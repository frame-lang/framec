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
//! 2. `erlang_transform_blocks` — the `{ }` → `case ... of` lowering.
//!    Pass 1 (scan + emit, incl. early-exit deferred-`end` nesting) is the
//!    dogfooded `OutputBlockLexerFsm` + `ErlangBlockParserFsm`; this module
//!    keeps only the trailing-`end`-comma formatting pass (#123).
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

/// Transform C-family `if/else { }` block syntax to Erlang `case/of/end`.
///
/// Runs on the spliced handler body text AFTER Frame statements have
/// been expanded. Only converts `{` that follows `if`/`else if`/`else`
/// keywords. Leaves other `{` alone (maps, tuples, records, gen_statem
/// return tuples).
pub(super) fn erlang_transform_blocks(text: &str) -> String {
    // Pass 1: brace → `case/of/end`, scanned by the shared `OutputBlockLexerFsm`
    // and emitted by the dogfooded `ErlangBlockParserFsm` — both Frame state
    // machines, no hand-rolled text scanning (#123).
    let result = crate::frame_c::compiler::codegen::block_transform::erlang_blocks_to_case(text);

    // Early-exit nesting is now done structurally inside ErlangBlockParserFsm
    // (the deferred-end fold), replacing the hand-rolled `erlang_nest_early_exits`
    // post-pass — which also mis-nested trailing code into the wrong arm (#123).
    let pass2 = result;

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
