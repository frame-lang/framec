//! Case-arm structural classification + per-arm gen_statem reply
//! injection.
//!
//! Erlang's gen_statem callbacks must return one of a closed set of
//! tuples — `{keep_state, Data, Actions}` / `{next_state, ..., ...}`
//! / `frame_transition__(...)`. The Frame source's portable
//! conditional shape (`if/else if/else`) lowers to a `case ... of`
//! whose arms may or may not transition. This module classifies
//! each top-level case arm and rewrites the case so every arm
//! produces a valid gen_statem return:
//!
//! - **AllTerminal** — every arm transitions; the case is itself
//!   the handler's terminal return.
//! - **NoTerminal** — no arm transitions; the case body hoists
//!   `__ReturnVal` and the outer handler emits a single
//!   `{keep_state, Data, [{reply, From, __ReturnVal}]}`.
//! - **Mixed** — some arms transition, some don't. Per-arm rewrite:
//!   transition arms keep their `frame_transition__(...)` shape
//!   (with the arm-local `@@:return` spliced into the reply slot);
//!   non-transition arms get a `{keep_state, Data, [{reply, From,
//!   <return val>}]}` tuple injected.
//!
//! `erlang_inject_orphan_reply_tuples` is the depth>1 sibling pass —
//! `rewrite_mixed_case_arms` only descends one level, so nested
//! cases (produced by nested `if/else`) need separate orphan
//! handling for their leaf `__ReturnVal = ...` writes that would
//! otherwise leak bare values into the gen_statem return.

/// Information about a single arm in a case...end block
pub(super) struct CaseArmInfo {
    /// Index of the arm header line (e.g., "true ->" or "; false ->") in processed lines
    pub header_idx: usize,
    /// Line indices of body content (after header, before next arm or end)
    pub body_start: usize,
    pub body_end: usize,
    /// Whether this arm contains a frame_transition__() call
    pub has_transition: bool,
    /// The __ReturnVal expression if one was assigned in this arm
    pub return_val: Option<String>,
    /// The last DataN variable in this arm (for {keep_state, DataN, ...})
    pub final_data_var: Option<String>,
}

/// Classification of a case block's arm behaviors
pub(super) enum CaseBlockClassification {
    /// All arms have frame_transition__() — case is terminal, use as handler return
    AllTerminal,
    /// No arms have frame_transition__() — hoist __ReturnVal, append {keep_state,...}
    NoTerminal,
    /// Mixed: some arms transition, some don't — per-arm rewrite needed
    Mixed,
}

/// Analyze a case block in processed handler lines.
/// Returns (classification, arms, case_start_line, case_end_line).
///
/// When a handler body contains MULTIPLE top-level case blocks
/// (e.g., consecutive `if` statements), this analyzes the LAST
/// one. That's the case which typically contains the handler's
/// terminal — its transition arm or final return value — while
/// earlier cases are just intermediate logic. The rewriter emits
/// pre-case lines verbatim, rewrites the last case in place, and
/// trailing-line emission handles anything after.
pub(super) fn analyze_case_arms(
    processed: &[String],
) -> Option<(CaseBlockClassification, Vec<CaseArmInfo>, usize, usize)> {
    let mut case_start = None;
    let mut case_end = None;
    let mut depth = 0i32;
    let mut arms: Vec<CaseArmInfo> = Vec::new();
    let mut current_arm: Option<CaseArmInfo> = None;

    for (idx, line) in processed.iter().enumerate() {
        let t = line.trim();

        // Track case block depth
        if (t.starts_with("case ") || t.starts_with("case(")) && t.ends_with(" of") {
            depth += 1;
            if depth == 1 {
                // New top-level case begins. If we've already
                // analyzed an earlier sibling case, drop it — only
                // the LAST top-level case is the terminal.
                case_start = Some(idx);
                case_end = None;
                arms.clear();
                current_arm = None;
            }
            continue;
        }

        if t == "end" || t == "end," || t == "end;" {
            if depth == 1 {
                // Close current arm + record case end. Don't break:
                // a sibling case may follow, in which case we'll
                // discard the just-closed analysis on its first
                // header line above.
                if let Some(mut arm) = current_arm.take() {
                    arm.body_end = idx;
                    arms.push(arm);
                }
                case_end = Some(idx);
            }
            depth = (depth - 1).max(0);
            continue;
        }

        // Only analyze top-level arms (depth == 1)
        if depth != 1 {
            // Still track content for current arm at nested depths
            if let Some(ref mut arm) = current_arm {
                if t.starts_with("frame_transition__(")
                    || t.starts_with("frame_forward_transition__(")
                {
                    arm.has_transition = true;
                }
                if t.starts_with("__ReturnVal = ") {
                    let val = t
                        .trim_start_matches("__ReturnVal = ")
                        .trim_end_matches(',')
                        .to_string();
                    arm.return_val = Some(val);
                }
                // Track DataN variable assignments
                if t.starts_with("Data") && t.contains(" = ") && !t.contains("#data") {
                    if let Some(eq_pos) = t.find(" = ") {
                        let var = t[..eq_pos].trim().to_string();
                        if var.starts_with("Data") && var[4..].chars().all(|c| c.is_ascii_digit()) {
                            arm.final_data_var = Some(var);
                        }
                    }
                }
            }
            continue;
        }

        // Arm boundary detection at depth 1.
        //
        // Recognise both the canonical boolean-case shape (`true ->`,
        // `; false ->`, `; _ ->`) AND bare-pattern arms emitted by
        // `erlang_wrap_self_call_guards` (e.g. `s0 ->`, `_ ->`).
        // The wrap function runs AFTER the body processor's pre-pass
        // that normalises user-written subsequent arms to `; <pat> ->`,
        // so its emitted case can have either bare or `;`-prefixed
        // arms. Both must be detected here or the wrap-emitted case
        // gets misclassified and `rewrite_mixed_case_arms` drops the
        // `; _ ->` / `end` tail.
        let is_canonical_header =
            t.starts_with("true ->") || t.starts_with("; false") || t.starts_with("; _");
        let is_bare_pattern_header = !t.starts_with("case ")
            && !t.starts_with("case(")
            && !t.contains("({call, From},")
            && (t.ends_with(" ->") || t.ends_with("->"))
            && !t.starts_with("{");
        let is_general_semicolon_header =
            t.starts_with("; ") && (t.ends_with(" ->") || t.ends_with("->"));
        let is_arm_header =
            is_canonical_header || is_general_semicolon_header || is_bare_pattern_header;
        if is_arm_header {
            // Close previous arm
            if let Some(mut arm) = current_arm.take() {
                arm.body_end = idx;
                arms.push(arm);
            }
            // Start new arm
            current_arm = Some(CaseArmInfo {
                header_idx: idx,
                body_start: idx + 1,
                body_end: idx + 1, // updated when arm closes
                has_transition: false,
                return_val: None,
                final_data_var: None,
            });
            continue;
        }

        // Content within current arm
        if let Some(ref mut arm) = current_arm {
            if t.starts_with("frame_transition__(") || t.starts_with("frame_forward_transition__(")
            {
                arm.has_transition = true;
            }
            if t.starts_with("__ReturnVal = ") {
                let val = t
                    .trim_start_matches("__ReturnVal = ")
                    .trim_end_matches(',')
                    .to_string();
                arm.return_val = Some(val);
            }
            if t.starts_with("Data") && t.contains(" = ") && !t.contains("#data") {
                if let Some(eq_pos) = t.find(" = ") {
                    let var = t[..eq_pos].trim().to_string();
                    if var.starts_with("Data") && var[4..].chars().all(|c| c.is_ascii_digit()) {
                        arm.final_data_var = Some(var);
                    }
                }
            }
        }
    }

    let case_start = case_start?;
    let case_end = case_end?;
    if arms.is_empty() {
        return None;
    }

    // Classify
    let all_terminal = arms.iter().all(|a| a.has_transition);
    let none_terminal = arms.iter().all(|a| !a.has_transition);
    let classification = if all_terminal {
        CaseBlockClassification::AllTerminal
    } else if none_terminal {
        CaseBlockClassification::NoTerminal
    } else {
        CaseBlockClassification::Mixed
    };

    Some((classification, arms, case_start, case_end))
}

/// Rewrite a case block with mixed arms so each arm produces a gen_statem return tuple.
///
/// `default_reply` is the fallback value for non-transition arms that don't
/// have an in-arm `__ReturnVal = …` write — typically the SSA-renamed
/// `__ReturnVal_K` name from a top-level `@@:return` written *before* the
/// case block. Falls back to `"ok"` when no top-level return value exists.
pub(super) fn rewrite_mixed_case_arms(
    processed: &[String],
    arms: &[CaseArmInfo],
    case_start: usize,
    case_end: usize,
    default_data: &str,
    default_reply: &str,
) -> Vec<String> {
    let mut result = Vec::new();

    // Emit lines before case block
    for i in 0..case_start {
        result.push(processed[i].clone());
    }

    // Emit case header
    result.push(processed[case_start].clone());

    // Emit each arm
    for arm in arms {
        // Emit arm header — strip any inline content after "->"
        // (e.g., "; false -> ok" becomes "; false ->")
        let header = &processed[arm.header_idx];
        let clean_header = if let Some(arrow_pos) = header.find("->") {
            let after_arrow = &header[arrow_pos + 2..].trim();
            if after_arrow.is_empty() {
                header.clone()
            } else {
                header[..arrow_pos + 2].to_string()
            }
        } else {
            header.clone()
        };
        result.push(clean_header);

        // Emit arm body lines, filtering as needed. Track nested case
        // depth so we only strip `__ReturnVal = ...` at depth 0 (this arm's
        // top level). `__ReturnVal` inside nested cases belongs to the inner
        // case's own arm — the depth-≥2 injector already turned those into
        // complete reply tuples and stripping them here would leak an
        // unbound `__ReturnVal` reference.
        let mut nested_depth = 0i32;
        for i in arm.body_start..arm.body_end {
            let t = processed[i].trim();

            let opens = (t.starts_with("case ") || t.starts_with("case("))
                && (t.ends_with(" of") || t.ends_with(" of,"));
            let closes = t == "end" || t == "end," || t == "end;";

            if opens {
                result.push(processed[i].clone());
                nested_depth += 1;
                continue;
            }

            if t.starts_with("__ReturnVal = ") && nested_depth == 0 {
                // Top-level of this arm: drop. Captured via arm.return_val
                // and re-emitted in the injected reply tuple below.
                continue;
            }

            // Splice the arm's captured @@:return value into a
            // transition call's reply slot. The frame_expansion site
            // emits `frame_transition__(..., From, __ReturnVal)`; the
            // SSA pass + transition-finalize fallback resolved that
            // to a top-level SSA name or `ok`. But arm-local
            // `@@:return = X` writes don't reach the SSA pass (they
            // live at depth>0), so the transition call still has
            // `ok`. Substitute the arm-captured value here so a
            // transitioning arm with an in-arm @@:return preserves
            // the value through the gen_statem reply.
            if (t.starts_with("frame_transition__(")
                || t.starts_with("frame_forward_transition__("))
                && nested_depth == 0
            {
                if let Some(rv) = arm.return_val.as_deref() {
                    let line = &processed[i];
                    if line.contains(", ok)") {
                        let rewritten = line.replacen(", ok)", &format!(", {})", rv), 1);
                        result.push(rewritten);
                        continue;
                    }
                }
            }

            result.push(processed[i].clone());

            if closes {
                nested_depth = (nested_depth - 1).max(0);
            }
        }

        // For non-transition arms, inject the gen_statem return tuple.
        // Skip if the arm body already contains a reply tuple (the depth-≥2
        // injector may have planted one at a nested leaf that's the only
        // exit of this arm).
        if !arm.has_transition {
            let arm_has_reply = processed[arm.body_start..arm.body_end].iter().any(|l| {
                let t = l.trim();
                t.starts_with("{keep_state,") || t.starts_with("{next_state,")
            });
            if !arm_has_reply {
                let data = arm.final_data_var.as_deref().unwrap_or(default_data);
                let reply = arm.return_val.as_deref().unwrap_or(default_reply);
                result.push(format!(
                    "        {{keep_state, {}, [{{reply, From, {}}}]}}",
                    data, reply
                ));
            }
        }
    }

    // Emit end
    result.push("    end".to_string());

    // Emit any lines AFTER the case block. A handler with a sibling
    // `if`/`case` after this one (e.g., a non-transitioning `if` with
    // a follow-up transitioning `if`) needs those tail lines preserved
    // — without this, the analyzer's `break` at first-`end` truncates
    // the rewrite output. The original lines have already had their
    // `__ReturnVal` SSA-renamed by the body processor.
    for i in (case_end + 1)..processed.len() {
        result.push(processed[i].clone());
    }

    result
}

/// Post-process emitted handler lines so every arm of a **nested** case block
/// (depth ≥ 2) that reaches its close without transitioning yields a
/// `gen_statem` reply tuple.
///
/// A `gen_statem` state-function clause must return a status tuple on *every*
/// path. `rewrite_mixed_case_arms` guarantees this for the outermost case's
/// arms but only descends one level. When a handler nests `if/else` (or native
/// `case`), an inner branch that falls through — ending in a bare value like
/// `ok`, a `__ReturnVal = …` write, or a trailing `DataN = …` mutation, with no
/// transition — escapes the outer rewriter and leaks that value into the
/// gen_statem return, crashing with `bad_return_from_state_function`. This pass
/// closes that gap for arms at depth ≥ 2.
///
/// For each such fall-through leaf it appends
/// `{keep_state, <DataInScope>, [{reply, From, <ReturnVal>}]}`. `<DataInScope>`
/// is the `Data` SSA var live at the leaf: brace matching for `case … end` is a
/// pushdown problem, so we track it with a per-case stack — each `case … of`
/// pushes the current var, each arm header resets to that pushed var (a sibling
/// arm's `DataN` binding never leaks), and a `DataN = …` binding advances it.
/// `<ReturnVal>` is the in-scope `__ReturnVal[_K]` name, or `ok`.
///
/// Depth-1 arms are left to `rewrite_mixed_case_arms`; its `already_has_reply`
/// check skips any tuple this pass planted at a nested leaf.
pub(super) fn erlang_inject_orphan_reply_tuples(
    lines: &[String],
    default_data: &str,
) -> Vec<String> {
    fn is_terminal(t: &str) -> bool {
        t.starts_with("frame_transition__(")
            || t.starts_with("frame_forward_transition__(")
            || t.starts_with("{next_state,")
            || t.starts_with("{keep_state,")
            || t.starts_with("{repeat_state,")
            || t.starts_with("{stop,")
    }
    fn is_data_var(s: &str) -> bool {
        s.starts_with("Data") && s.len() > 4 && s[4..].chars().all(|c| c.is_ascii_digit())
    }
    // The SSA `Data` var bound by this line, if any. Handles both `DataN = …`
    // and the forward-unwrap tuple bind `{DataN, __FwdNext, __FwdReply} = …`
    // (the binding, not a `DataN#data…` read).
    fn data_binding(t: &str) -> Option<String> {
        let eq = t.find(" = ")?;
        let lhs = t[..eq].trim();
        if is_data_var(lhs) {
            return Some(lhs.to_string());
        }
        if let Some(inner) = lhs.strip_prefix('{') {
            let first = inner.split(',').next().unwrap_or("").trim();
            if is_data_var(first) {
                return Some(first.to_string());
            }
        }
        None
    }
    // The reply EXPRESSION live after a `__ReturnVal[_K] = <rhs>` write.
    // `rewrite_mixed_case_arms` strips the *bare* `__ReturnVal = …` binding
    // (hoisting it into its own reply tuple), so a nested leaf must not
    // reference the name — inline the bound `<rhs>` instead. SSA-renamed
    // `__ReturnVal_K` bindings survive, so reference those by name.
    fn return_value_expr(t: &str) -> Option<String> {
        let eq = t.find(" = ")?;
        let lhs = t[..eq].trim();
        if lhs == "__ReturnVal" {
            Some(t[eq + 3..].trim().trim_end_matches([',', ';']).to_string())
        } else if lhs.starts_with("__ReturnVal_") && lhs[12..].chars().all(|c| c.is_ascii_digit()) {
            Some(lhs.to_string())
        } else {
            None
        }
    }
    fn is_arm_header(t: &str) -> bool {
        if is_terminal(t) || t.starts_with("case ") || t.starts_with("case(") {
            return false;
        }
        let canonical =
            t.starts_with("true ->") || t.starts_with("; false") || t.starts_with("; _");
        let semi =
            t.starts_with("; ") && (t.ends_with(" ->") || t.ends_with("->") || t.contains(" -> "));
        let bare_first =
            !t.starts_with(';') && !t.starts_with('{') && (t.ends_with(" ->") || t.ends_with("->"));
        canonical || semi || bare_first
    }

    // Trimmed text of the next non-blank line.
    let next_meaningful = |from: usize| -> &str {
        let mut j = from + 1;
        while j < lines.len() && lines[j].trim().is_empty() {
            j += 1;
        }
        lines.get(j).map(|s| s.trim()).unwrap_or("")
    };
    // Does the arm containing line `idx` close right after it?
    let arm_closes_after = |idx: usize| -> bool {
        let nx = next_meaningful(idx);
        nx == "end"
            || nx == "end,"
            || nx == "end;"
            || (nx.starts_with("; ")
                && (nx.ends_with("->") || nx.ends_with(" ->") || nx.contains(" -> ")))
    };

    // Precompute, per `case … of` open-line, whether its matching `end` lacks a
    // trailing comma. A reply tuple may only be injected at a leaf whose whole
    // enclosing case chain is in TAIL position — i.e. the case's value IS the
    // handler's return. A case followed by more statements ends in `end,`
    // (Erlang's expression separator); injecting there would both return early
    // (skipping the trailing code) and reference a `DataN` not bound on that
    // path. `end` / `end;` means the case is in tail position locally; `end,`
    // means it is not.
    let mut locally_tail: std::collections::HashMap<usize, bool> = std::collections::HashMap::new();
    {
        let mut stack: Vec<usize> = Vec::new();
        for (i, line) in lines.iter().enumerate() {
            let t = line.trim();
            if (t.starts_with("case ") || t.starts_with("case("))
                && (t.ends_with(" of") || t.ends_with(" of,"))
            {
                stack.push(i);
            } else if t == "end" || t == "end," || t == "end;" {
                if let Some(open_i) = stack.pop() {
                    locally_tail.insert(open_i, t != "end,");
                }
            }
        }
    }

    let mut result: Vec<String> = Vec::with_capacity(lines.len());
    let mut depth: i32 = 0;
    // The handler input record is always `Data`; mutations advance the SSA name.
    // (`default_data` is the FINAL var, which `rewrite_mixed_case_arms` uses for
    // the outermost arm — but a nested leaf needs the var live at that point,
    // which starts from `Data`.)
    let _ = default_data;
    let mut cur_data = "Data".to_string();
    let mut cur_ret = "ok".to_string();
    let mut data_at_open: Vec<String> = Vec::new();
    let mut ret_at_open: Vec<String> = Vec::new();
    // Per-open-case tail flag (parent's tail AND this case's local tail). Only a
    // leaf whose innermost enclosing case is in tail position may be injected.
    let mut tail_stack: Vec<bool> = Vec::new();

    for (i, line) in lines.iter().enumerate() {
        let t = line.trim();
        let opens = (t.starts_with("case ") || t.starts_with("case("))
            && (t.ends_with(" of") || t.ends_with(" of,"));
        let closes = t == "end" || t == "end," || t == "end;";
        let lead = &line[..line.len() - line.trim_start().len()];

        // An arm header restores the in-scope vars to this case's entry scope,
        // so a sibling arm never inherits a prior arm's `DataN`/`__ReturnVal`.
        if depth >= 1 && is_arm_header(t) {
            if let Some(d) = data_at_open.last() {
                cur_data = d.clone();
            }
            if let Some(r) = ret_at_open.last() {
                cur_ret = r.clone();
            }
        }
        // Advance in-scope vars on bindings (before any leaf injection uses them).
        if let Some(v) = data_binding(t) {
            cur_data = v;
        }
        if let Some(r) = return_value_expr(t) {
            cur_ret = r;
        }

        if opens {
            data_at_open.push(cur_data.clone());
            ret_at_open.push(cur_ret.clone());
            let parent_tail = tail_stack.last().copied().unwrap_or(true);
            let locally = locally_tail.get(&i).copied().unwrap_or(true);
            tail_stack.push(parent_tail && locally);
            result.push(line.clone());
            depth += 1;
            continue;
        }
        if closes {
            result.push(line.clone());
            depth = (depth - 1).max(0);
            data_at_open.pop();
            ret_at_open.pop();
            tail_stack.pop();
            continue;
        }

        // A leaf may only become a return tuple if its enclosing case is in
        // tail position (its value reaches the handler return).
        let in_tail = tail_stack.last().copied().unwrap_or(false);

        let inject = |out: &mut Vec<String>, stmt: &str| {
            let stmt = stmt.trim_end_matches([',', ';']);
            out.push(format!("{}{},", lead, stmt));
            out.push(format!(
                "{}{{keep_state, {}, [{{reply, From, {}}}]}}",
                lead, cur_data, cur_ret
            ));
        };

        // Inline arm-header body: `; false -> ok`.
        if depth >= 2 && in_tail && is_arm_header(t) {
            if let Some(p) = t.find(" -> ") {
                let body = t[p + 4..].trim();
                if !body.is_empty() && !is_terminal(body) && arm_closes_after(i) {
                    let pat = &t[..p + 3]; // includes "->"
                    let stmt = format!("{} {}", pat, body);
                    inject(&mut result, &stmt);
                    continue;
                }
            }
            result.push(line.clone());
            continue;
        }

        // Standalone fall-through leaf at depth ≥ 2.
        if depth >= 2 && in_tail && !t.is_empty() && !is_terminal(t) && arm_closes_after(i) {
            inject(&mut result, t);
            continue;
        }

        result.push(line.clone());
    }

    result
}

#[cfg(test)]
mod orphan_reply_tests {
    use super::erlang_inject_orphan_reply_tuples;

    fn run(lines: &[&str], default_data: &str) -> Vec<String> {
        let owned: Vec<String> = lines.iter().map(|s| s.to_string()).collect();
        erlang_inject_orphan_reply_tuples(&owned, default_data)
    }

    // #119: a nested fall-through arm ending in bare `ok` must get a reply
    // tuple with the in-scope DataN (not the top-level default).
    #[test]
    fn nested_bare_ok_leaf_gets_tuple_with_scoped_data() {
        let out = run(
            &[
                "    case P of",
                "    \"ok\" ->",
                "    frame_transition__('active', Data, [], [], [], From, ok)",
                "    ; _ ->",
                "    Data1 = Data#data{failures = Data#data.failures + 1},",
                "    case Data1#data.failures >= 3 of",
                "    true ->",
                "    frame_transition__('locked', Data1, [], [], [], From, ok)",
                "    ; false ->",
                "    ok",
                "    end",
                "    end",
            ],
            "Data",
        )
        .join("\n");
        assert!(
            out.contains("{keep_state, Data1, [{reply, From, ok}]}"),
            "inner fall-through missing scoped tuple:\n{out}"
        );
        // The transition arms are untouched.
        assert!(out.contains("frame_transition__('locked', Data1"));
    }

    // Inline arm-body form: `; false -> ok` on one line.
    #[test]
    fn inline_arm_body_fall_through_gets_tuple() {
        let out = run(
            &[
                "    case (P == \"ok\") of",
                "    true ->",
                "    Data1 = Data#data{n = 1},",
                "    case (Data1#data.n >= 3) of",
                "    true ->",
                "    frame_transition__('locked', Data1, [], [], [], From, ok)",
                "    ; false -> ok",
                "    end",
                "    end",
            ],
            "Data",
        )
        .join("\n");
        assert!(
            out.contains("; false -> ok,")
                && out.contains("{keep_state, Data1, [{reply, From, ok}]}"),
            "inline fall-through not handled:\n{out}"
        );
    }

    // A sibling arm's DataN must not leak into another sibling's leaf.
    #[test]
    fn sibling_data_does_not_leak() {
        let out = run(
            &[
                "    case Tag of",
                "    a ->",
                "    Data1 = Data#data{x = 1},",
                "    case (Data1#data.x > 0) of",
                "    true -> frame_transition__('s', Data1, [], [], [], From, ok)",
                "    ; false -> ok",
                "    end",
                "    ; b ->",
                "    case (Data#data.y > 0) of",
                "    true -> frame_transition__('t', Data, [], [], [], From, ok)",
                "    ; false -> ok",
                "    end",
                "    end",
            ],
            "Data",
        )
        .join("\n");
        // arm a's inner false uses Data1; arm b's inner false uses Data (NOT Data1).
        assert!(
            out.contains("{keep_state, Data1, [{reply, From, ok}]}"),
            "arm a scope wrong:\n{out}"
        );
        assert!(
            out.contains("{keep_state, Data, [{reply, From, ok}]}"),
            "arm b leaked Data1:\n{out}"
        );
    }

    // A fully-terminal nested case (all arms transition) is left untouched.
    #[test]
    fn all_terminal_nested_unchanged() {
        let input = [
            "    case X of",
            "    true ->",
            "    case Y of",
            "    true -> frame_transition__('a', Data, [], [], [], From, ok)",
            "    ; false -> frame_transition__('b', Data, [], [], [], From, ok)",
            "    end",
            "    ; false -> frame_transition__('c', Data, [], [], [], From, ok)",
            "    end",
        ];
        let out = run(&input, "Data");
        assert_eq!(
            out.iter().filter(|l| l.contains("keep_state")).count(),
            0,
            "should not inject into all-terminal case"
        );
    }
}
