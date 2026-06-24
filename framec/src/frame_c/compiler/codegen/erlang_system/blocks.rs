//! Block-syntax transforms and statement joining for Erlang code
//! generation.
//!
//! Frame's handler bodies are written in C-family `if`/`else`/`case`
//! block syntax (`if cond { ... } else { ... }`) but Erlang uses
//! `case Cond of true -> ...; false -> ... end`. This module owns
//! the lowering — three passes that together convert spliced
//! handler text into syntactically-valid Erlang:
//!
//! 1. `erlang_lower_native_if` — rewrites user-authored
//!    native-Erlang `if Cond -> X ; true -> Y end` into the
//!    Frame-style `if Cond { X } else { Y }` so the C-family
//!    transform downstream has a uniform input. (Native Erlang
//!    `if` breaks the SSA renamer.)
//! 2. `erlang_transform_blocks` — the main `{ }` → `case ... of`
//!    lowering, with three sub-passes inside: block translation,
//!    sequential early-exit nesting (`erlang_nest_early_exits`),
//!    and trailing-`end`-comma insertion.
//! 3. `erlang_smart_join` — the statement-joiner that picks the
//!    right separator between two emitted lines (Erlang has three:
//!    `,` for expressions in a clause, `;` for case-arm separators,
//!    and bare newline for structural lines). The choice depends
//!    on context: ends-with-punctuation, case-block structural
//!    boundaries, and mid-expression continuations (where
//!    `paren_balance_unclosed` / `ends_with_binary_op` from
//!    `lexical` flag unfinished expressions).

use super::lexical::{ends_with_binary_op, paren_balance_unclosed};

/// Lowers native Erlang-style `if Cond -> Body ; true -> Body end` to
/// Frame's C-style `if Cond { Body } else { Body }` so the existing
/// `erlang_transform_blocks` pipeline can handle it. Without this pass,
/// native Erlang if syntax breaks the SSA renamer (each branch's
/// `__ReturnVal = X` gets a distinct `__ReturnVal_K` name, but Erlang
/// requires both arms to bind the same variable for it to be visible
/// after the `end`).
///
/// Recognises only the simple two-arm form: `if Cond ->` opener,
/// optional `;` arm separator, `true ->` else header, `end` closer.
/// Multi-arm `if A -> ; B -> ; true -> end` would need else-if
/// chaining; not yet handled.
pub(super) fn erlang_lower_native_if(text: &str) -> String {
    let lines: Vec<&str> = text.lines().collect();
    let mut out: Vec<String> = Vec::with_capacity(lines.len());
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        let t = line.trim();
        let is_native_if = t.starts_with("if ") && t.ends_with(" ->") && !t.ends_with('{');
        if !is_native_if {
            out.push(line.to_string());
            i += 1;
            continue;
        }
        let cond = t[3..t.len() - 2].trim().to_string();
        let indent = &line[..line.len() - line.trim_start().len()];
        let mut j = i + 1;
        let mut depth = 1;
        let mut sep_idx: Option<usize> = None;
        let mut end_idx: Option<usize> = None;
        while j < lines.len() {
            let lt = lines[j].trim();
            let opens = (lt.starts_with("if ") && lt.ends_with(" ->"))
                || ((lt.starts_with("case ") || lt.starts_with("case("))
                    && (lt.ends_with(" of") || lt.ends_with(" of,")));
            let closes = lt == "end" || lt == "end," || lt == "end;";
            if opens {
                depth += 1;
            } else if closes {
                depth -= 1;
                if depth == 0 {
                    end_idx = Some(j);
                    break;
                }
            } else if depth == 1 && lt == ";" {
                let mut k = j + 1;
                while k < lines.len() && lines[k].trim().is_empty() {
                    k += 1;
                }
                if k < lines.len() && lines[k].trim() == "true ->" {
                    sep_idx = Some(j);
                    j = k;
                }
            }
            j += 1;
        }
        let (Some(sep), Some(end)) = (sep_idx, end_idx) else {
            out.push(line.to_string());
            i += 1;
            continue;
        };
        let mut true_arm_start = sep + 1;
        while true_arm_start < end && lines[true_arm_start].trim() != "true ->" {
            true_arm_start += 1;
        }
        true_arm_start += 1;
        out.push(format!("{}if {} {{", indent, cond));
        for k in (i + 1)..sep {
            out.push(lines[k].to_string());
        }
        out.push(format!("{}}} else {{", indent));
        for k in true_arm_start..end {
            out.push(lines[k].to_string());
        }
        out.push(format!("{}}}", indent));
        i = end + 1;
    }
    out.join("\n")
}

/// Token-driven `{ }` → `case … of … end` lowering (#123). Drives the shared
/// `OutputBlockLexerFsm` (via `lex_blocks`, Erlang `%` comments) so structure is
/// recovered from real tokens — `if`/`else`/braces inside strings and comments
/// are never miscounted — and brace nesting is one principled stack pass rather
/// than accreted line special-cases. Emits the same intermediate shape the
/// downstream passes (`erlang_nest_early_exits`, comma insertion,
/// `erlang_smart_join`) consume; those passes trim, so exact indentation here is
/// cosmetic (Erlang is whitespace-insensitive). Leading whitespace passes
/// through verbatim, so the first emitted line inherits the source indent.
fn erlang_blocks_to_case(text: &str) -> String {
    use crate::frame_c::compiler::codegen::block_transform::lex_blocks;
    let (kinds, starts, ends) = lex_blocks(text, b'%', false);
    let bytes = text.as_bytes();
    let n = kinds.len();
    let mut out = String::new();
    // (kind, has_else): kind 0 = `if`, 1 = `elif` (the case an else-if opened).
    let mut stack: Vec<(u8, bool)> = Vec::new();

    // A `{`/`}` is a BLOCK brace only when it sits at a line-structural position
    // (`if … {` with the `{` ending the line, `}` starting the line). Erlang
    // tuple/map braces (`{call, From}`, `#{…}`) appear mid-line and must pass
    // through verbatim — the same whole-line discipline the old line scanner had,
    // now expressed over the FSM lexer's tokens.
    let hws = |k: usize| {
        kinds[k] == 11
            && bytes[starts[k]..ends[k]]
                .iter()
                .all(|&b| b == b' ' || b == b'\t')
    };
    // First meaningful token is on this line? (only horizontal whitespace before
    // it back to the previous NEWLINE.)
    let at_line_start = |ti: usize| -> bool {
        let mut k = ti;
        while k > 0 {
            k -= 1;
            if kinds[k] == 10 {
                return true;
            }
            if hws(k) {
                continue;
            }
            return false;
        }
        true
    };
    // Index of the NEWLINE ending `ti`'s line (or `n`).
    let line_end = |ti: usize| -> usize {
        let mut k = ti;
        while k < n && kinds[k] != 10 {
            k += 1;
        }
        k
    };
    // Last meaningful token on the line `(ti, le)` (or `ti` if none follow).
    let last_on_line = |ti: usize, le: usize| -> usize {
        let mut k = le;
        while k > ti {
            k -= 1;
            if hws(k) {
                continue;
            }
            return k;
        }
        ti
    };
    // First meaningful token after `from` within the line (or `le`).
    let next_on_line = |from: usize, le: usize| -> usize {
        let mut k = from;
        while k < le && hws(k) {
            k += 1;
        }
        k
    };

    let mut ti = 0;
    while ti < n {
        // `if <cond> {` — IF at line start, block `{` ending the line.
        if kinds[ti] == 1 && at_line_start(ti) {
            let le = line_end(ti);
            let k = last_on_line(ti + 1, le);
            if k > ti && kinds[k] == 6 {
                let cond = text[ends[ti]..starts[k]].trim();
                out.push_str(&format!("case ({cond}) of\n    true ->\n"));
                stack.push((0, false));
                ti = k + 1;
                continue;
            }
        }

        // `}` at line start → close / else / else-if.
        if kinds[ti] == 7 && at_line_start(ti) && !stack.is_empty() {
            let le = line_end(ti);
            let a = next_on_line(ti + 1, le);
            if a < le && kinds[a] == 3 {
                // `} else …` — block `{` ends the line.
                let k = last_on_line(a + 1, le);
                let b = next_on_line(a + 1, le);
                if b < le && kinds[b] == 1 && k > b && kinds[k] == 6 {
                    // `} else if <cond> {`
                    let cond = text[ends[b]..starts[k]].trim();
                    stack.pop();
                    out.push_str(&format!(
                        "    ; false ->\n        case ({cond}) of\n            true ->\n"
                    ));
                    stack.push((1, false));
                    stack.push((0, false));
                    ti = k + 1;
                    continue;
                }
                if kinds[k] == 6 {
                    // `} else {`
                    if let Some(last) = stack.last_mut() {
                        last.1 = true;
                    }
                    out.push_str("    ; false ->\n");
                    ti = k + 1;
                    continue;
                }
            } else if a >= le {
                // `}` alone on its line → close the case (+ enclosing elifs).
                let (ctx, has_else) = stack.pop().unwrap();
                if !has_else && ctx == 0 {
                    out.push_str("    ; false -> ok\n");
                }
                if out.trim_end().ends_with("->") {
                    out.push_str("    ok\n");
                }
                out.push_str("end");
                if ctx == 1 {
                    if !stack.is_empty() {
                        stack.pop();
                        out.push_str("\nend");
                    }
                } else {
                    while let Some(&(octx, _)) = stack.last() {
                        if octx != 1 {
                            break;
                        }
                        stack.pop();
                        if let Some(&(c, _)) = stack.last() {
                            if c == 0 {
                                stack.pop();
                            }
                        }
                        out.push_str("\nend");
                    }
                }
                out.push('\n');
                ti += 1;
                continue;
            }
            // Otherwise (content after `}` that isn't `else`): a tuple/expression
            // brace — fall through to verbatim.
        }

        // Default: emit the token's bytes verbatim (TEXT/STRING/COMMENT/NEWLINE/…).
        out.push_str(&text[starts[ti]..ends[ti]]);
        ti += 1;
    }

    out
}

/// Transform C-family `if/else { }` block syntax to Erlang `case/of/end`.
///
/// Runs on the spliced handler body text AFTER Frame statements have
/// been expanded. Only converts `{` that follows `if`/`else if`/`else`
/// keywords. Leaves other `{` alone (maps, tuples, records, gen_statem
/// return tuples).
pub(super) fn erlang_transform_blocks(text: &str) -> String {
    // Pass 1: brace → `case/of/end`, driven by the shared `OutputBlockLexerFsm`
    // (string/comment-safe scanning) rather than hand-rolled line matching — so
    // `if`/`else`/`{`/`}` inside strings and comments are never mistaken for
    // structure, and the brace nesting is tracked by one principled pass
    // instead of accreted special cases (#123).
    let result = erlang_blocks_to_case(text);

    // Second pass: nest sequential if-without-else blocks (early-exit pattern)
    let result_lines: Vec<&str> = result.lines().collect();
    let pass2 = erlang_nest_early_exits(&result_lines);

    // Third pass: add commas after `end` when followed by another expression
    let mut final_result = String::new();
    let pass2_lines: Vec<&str> = pass2.lines().collect();
    for (i, line) in pass2_lines.iter().enumerate() {
        final_result.push_str(line);
        if line.trim() == "end" && i + 1 < pass2_lines.len() {
            let next = pass2_lines[i + 1..].iter().find(|l| !l.trim().is_empty());
            if let Some(next_line) = next {
                let nt = next_line.trim();
                if !nt.starts_with("end") && !nt.starts_with(";") && !nt.is_empty() {
                    final_result.push(',');
                }
            }
        }
        final_result.push('\n');
    }

    final_result
}

/// Nest sequential if-without-else blocks into right-nested case expressions.
/// This converts the early-exit pattern (common in Frame handlers) into valid
/// Erlang where each function clause returns exactly one value.
fn erlang_nest_early_exits(lines: &[&str]) -> String {
    let mut output_lines: Vec<String> = lines.iter().map(|s| s.to_string()).collect();

    let mut changed = true;
    while changed {
        changed = false;
        let mut i = 0;
        while i < output_lines.len() {
            let is_false_ok = output_lines[i].trim() == "; false -> ok";

            if is_false_ok {
                let mut j = i + 1;
                while j < output_lines.len() && output_lines[j].trim().is_empty() {
                    j += 1;
                }

                let is_end = j < output_lines.len() && {
                    let t = output_lines[j].trim().to_string();
                    t == "end" || t == "end,"
                };
                if is_end {
                    let remaining_start = j + 1;
                    let mut remaining: Vec<String> = Vec::new();
                    for k in remaining_start..output_lines.len() {
                        if !output_lines[k].trim().is_empty() {
                            remaining.push(output_lines[k].clone());
                        }
                    }

                    let has_real_code = remaining.iter().any(|r| {
                        let t = r.trim();
                        !t.is_empty()
                            && t != "end"
                            && t != "end,"
                            && !t.starts_with("; false -> ok")
                            && !t.starts_with("; false ->")
                    });
                    if has_real_code {
                        let indent_len = output_lines[i].len() - output_lines[i].trim_start().len();
                        let indent = " ".repeat(indent_len);
                        output_lines[i] = format!("{}; false ->", indent);
                        let mut new_section: Vec<String> = Vec::new();
                        for r in &remaining {
                            new_section.push(format!("{}    {}", indent, r.trim()));
                        }
                        new_section.push(format!("{}end", indent));

                        output_lines.drain(j..);
                        let insert_pos = i + 1;
                        for (idx, new_line) in new_section.into_iter().enumerate() {
                            output_lines.insert(insert_pos + idx, new_line);
                        }

                        changed = true;
                        break;
                    }
                }
            }
            i += 1;
        }
    }

    output_lines.join("\n")
}

/// Join processed Erlang lines with proper comma/newline separators.
/// In Erlang, all expressions in a function clause are comma-separated except:
/// - Inside case blocks: branches are separated by `;`, values by comma only within a branch
/// - After `case ... of`, `true ->`, `; false ->` (structural, no comma)
/// - Before `end`, `; false`, `true ->` (structural, no comma)
/// - Lines already ending with `,` or `;` get a newline only
/// - Lines in the middle of an expression (unclosed parens or trailing
///   binary operator) — see `paren_balance_unclosed` and
///   `ends_with_binary_op`. The next line is the continuation and must
///   not be separated by `,`.
pub(super) fn erlang_smart_join(lines: &[String], code: &mut String) {
    let mut case_depth = 0i32;

    // Filter out comment-only lines — they contribute nothing to Erlang syntax
    // and break comma/semicolon placement logic when between code lines.
    let non_comment_lines: Vec<&String> = lines
        .iter()
        .filter(|l| {
            let t = l.trim();
            !t.starts_with('%') || t.is_empty()
        })
        .collect();

    for (idx, line) in non_comment_lines.iter().enumerate() {
        if idx > 0 {
            let lt = line.trim();
            let pt_full = non_comment_lines[idx - 1].trim();
            // Strip trailing % comment to get the code portion for punctuation checks
            let pt = {
                let mut in_string = false;
                let mut escape = false;
                let mut code_end = pt_full.len();
                for (i, c) in pt_full.char_indices() {
                    if escape {
                        escape = false;
                        continue;
                    }
                    if c == '\\' {
                        escape = true;
                        continue;
                    }
                    if c == '"' {
                        in_string = !in_string;
                        continue;
                    }
                    if c == '%' && !in_string {
                        code_end = i;
                        break;
                    }
                }
                pt_full[..code_end].trim_end()
            };

            if lt.starts_with("case ") || lt.starts_with("case(") {
                // case_depth will be incremented below
            }

            let prev_ends_punctuated = pt.ends_with(',') || pt.ends_with(';');

            let prev_is_case_head = pt.ends_with(" of");
            let prev_is_branch =
                pt.ends_with("->") || pt.starts_with("; false") || pt.starts_with("; true");

            let curr_is_end =
                lt == "end" || lt == "end," || lt.starts_with("end;") || lt.starts_with("end.");
            // A "branch" here is any case-arm header.
            let curr_is_branch = lt == ";"
                || lt.starts_with("true ->")
                || lt.starts_with("; false")
                || lt.starts_with("; true")
                || (lt.starts_with(';') && (lt.ends_with(" ->") || lt.ends_with("->")));

            let prev_is_structural_case = prev_is_case_head || prev_is_branch;
            let curr_is_structural_case = curr_is_end || curr_is_branch;

            // Multi-line expression continuation: when the previous
            // line is in the MIDDLE of an expression — has unbalanced
            // open parens/brackets/braces or ends with a binary
            // operator that requires a right operand — the current
            // line is the operand or continuation. Inserting `,\n`
            // would break the expression. Detect this and emit just
            // a newline.
            let prev_in_mid_expression = paren_balance_unclosed(pt) || ends_with_binary_op(pt);

            if prev_ends_punctuated
                || prev_is_structural_case
                || curr_is_structural_case
                || prev_in_mid_expression
            {
                code.push('\n');
            } else {
                code.push_str(",\n");
            }
        }

        let lt = line.trim();
        if (lt.starts_with("case ") || lt.contains(" case ") || lt.starts_with("case("))
            && lt.ends_with(" of")
        {
            case_depth += 1;
        }
        if lt == "end" || lt == "end," || lt.starts_with("end;") || lt.starts_with("end.") {
            case_depth = (case_depth - 1).max(0);
        }

        code.push_str(line);
    }
}
