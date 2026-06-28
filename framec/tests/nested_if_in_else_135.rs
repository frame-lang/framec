//! Issue #135 — a nested `if` placed directly inside an `else { }` block must
//! lower like any other block-nested `if`.
//!
//! Scope note: Frame's brace-form control flow (`if c { … } else { … }`) is
//! lowered to native syntax ONLY for the targets that opt into block
//! transformation — **Lua** (`if/then/elseif/else/end`, via
//! `codegen/output_block_parser.frs`) and **Erlang** (`case … of … end`, via
//! `codegen/output_block_parser_erlang.frs`). For Python / JavaScript /
//! TypeScript / Ruby / C the user writes target-native control flow (see the
//! documented `excluded_for` list in `tests/common/mod.rs`); framec passes it
//! through verbatim and does not translate braces. The #135 regression
//! therefore lives in the brace-lowering path, and these tests pin it on the
//! two targets that exercise it.
//!
//! Root cause: the Lua `output_block_parser.frs` collapsed `} else { if c {`
//! into an `elseif` ladder unconditionally. That collapse is only valid when
//! the inner `if` is the SOLE content of the `else` block (a genuine `else if`
//! ladder). When the inner `if` has its own `else` (or trailing statements),
//! the collapse dropped a block level and leaked a stray brace into the
//! generated Lua — a `luac` syntax error. The collapse is now guarded by a
//! sole-content check; non-ladder nested ifs lower as ordinary blocks, and the
//! ladder's redundant `else`-block close brace is swallowed.

mod common;
use common::{compile_source, find_tool};
use std::process::Command;

/// Compile `src` for `target`, write the output to a temp file, and run
/// `tool args… <file>` over it. Asserts the tool accepts the generated source.
/// Skipped (not failed) when the tool is absent, mirroring the snapshot suite.
fn parse_check(src: &str, target: &str, ext: &str, tool: &str, args: &[&str]) {
    let bin = match find_tool(tool) {
        Some(p) => p,
        None => {
            eprintln!("#135 {target} parse-check skipped: `{tool}` not on PATH");
            return;
        }
    };
    let code = compile_source(src, target);
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join(format!("repro135.{ext}"));
    std::fs::write(&path, &code).expect("write temp");
    let out = Command::new(&bin)
        .args(args)
        .arg(&path)
        .output()
        .unwrap_or_else(|e| panic!("spawn {tool}: {e}"));
    assert!(
        out.status.success(),
        "[{target}] generated source rejected by `{tool}`:\n--- stderr ---\n{}\n--- source ---\n{}",
        String::from_utf8_lossy(&out.stderr),
        code
    );
}

/// Lines of generated code that carry the branch payloads — the lowered
/// conditional, isolated from surrounding machinery (which legitimately uses
/// braces in some backends).
fn payload_lines(code: &str, payloads: &[&str]) -> String {
    code.lines()
        .filter(|l| payloads.iter().any(|p| l.contains(&format!("\"{p}\""))))
        .collect::<Vec<&str>>()
        .join("\n")
}

/// Extract a backend's user-event handler method body (`…_hdl_user_<event>…`)
/// up to the next blank line — the lowered native block lives here, isolated
/// from the dispatch/runtime scaffolding.
fn handler_region(code: &str, event: &str) -> String {
    let needle = format!("hdl_user_{event}");
    let lines: Vec<&str> = code.lines().collect();
    // Match the method DEFINITION (a `function`/`def`/`fn` line carrying the
    // needle), not the dispatch call site that also references it.
    let start = match lines.iter().position(|l| {
        l.contains(&needle)
            && (l.contains("function") || l.trim_start().starts_with("def ") || l.contains("fn "))
    }) {
        Some(i) => i + 1,
        None => return String::new(),
    };
    let mut out = Vec::new();
    for l in &lines[start..] {
        if l.trim().is_empty() {
            break;
        }
        out.push(*l);
    }
    out.join("\n")
}

fn assert_clean_lua(region: &str, payloads: &[&str], label: &str) {
    assert!(
        !region.contains('{') && !region.contains('}'),
        "[lua/{label}] stray Frame brace survived lowering:\n{region}"
    );
    assert!(
        !region.contains("@@") && !region.contains('$'),
        "[lua/{label}] leaked Frame marker (`@@`/`$`):\n{region}"
    );
    for p in payloads {
        assert!(
            region.contains(p),
            "[lua/{label}] branch payload {p:?} missing:\n{region}"
        );
    }
    // Lua control flow is `then`/`else`/`end`; the lowered single-line chain
    // must contain `then` and balanced `end`s.
    assert!(
        region.contains("then"),
        "[lua/{label}] no `then`:\n{region}"
    );
}

/// Minimal repro: nested `if`/`else` directly inside the outer `else { }`,
/// single-line.
#[test]
fn repro_single_line_nested_if_in_else_lua() {
    let src = r#"
@@[target("lua")]
@@system S {
    interface:
        classify(n)
    machine:
        $A {
            classify(n) {
                if n < 0 { @@:("neg") } else { if n == 0 { @@:("zero") } else { @@:("pos") } }
            }
        }
}
"#;
    let code = compile_source(src, "lua");
    let region = payload_lines(&code, &["neg", "zero", "pos"]);
    assert_clean_lua(&region, &["neg", "zero", "pos"], "repro-1line");
    // Two `if`s (outer + inner) → two `then`, closed by matching `end`s.
    let then_n = region.matches("then").count();
    let end_n = region.matches("end").count();
    assert_eq!(
        then_n, end_n,
        "[lua/repro-1line] unbalanced then/end ({then_n} vs {end_n}):\n{region}"
    );
}

/// Multi-line variant of the repro.
#[test]
fn repro_multiline_nested_if_in_else_lua() {
    let src = r#"
@@[target("lua")]
@@system S {
    interface:
        classify(n)
    machine:
        $A {
            classify(n) {
                if n < 0 {
                    @@:("neg")
                } else {
                    if n == 0 {
                        @@:("zero")
                    } else {
                        @@:("pos")
                    }
                }
            }
        }
}
"#;
    let code = compile_source(src, "lua");
    // Multi-line: the lowered block spans several lines, so capture the whole
    // user-handler method body (`_hdl_user_classify`) for inspection.
    let region = handler_region(&code, "classify");
    assert_clean_lua(&region, &["neg", "zero", "pos"], "repro-multiline");
    // The handler region includes the method's own trailing `end`, so the
    // conditional's `end`s are `then` count + 1 (outer if + inner if, plus the
    // method terminator). The decisive check is the absence of stray braces
    // (asserted in `assert_clean_lua`) — a leaked brace was the #135 symptom.
    let then_n = region.matches("then").count();
    let end_n = region.matches(" end").count() + region.matches("\nend").count();
    assert_eq!(
        end_n,
        then_n + 1,
        "[lua/repro-multiline] expected {} `end`s (2 ifs + method close), got {end_n}:\n{region}",
        then_n + 1
    );
}

/// Deeper nesting: else { if { } else { if { } else { } } }.
#[test]
fn deep_nested_if_in_else_lua() {
    let src = r#"
@@[target("lua")]
@@system S {
    interface:
        classify(a, b, c)
    machine:
        $A {
            classify(a, b, c) {
                if a { @@:("br_a") } else { if b { @@:("br_b") } else { if c { @@:("br_c") } else { @@:("br_d") } } }
            }
        }
}
"#;
    let code = compile_source(src, "lua");
    let payloads = ["br_a", "br_b", "br_c", "br_d"];
    let region = payload_lines(&code, &payloads);
    assert_clean_lua(&region, &payloads, "deep");
}

/// Erlang lowers brace-form conditionals to `case … of … end`. The multi-line
/// nested-if-in-else must produce nested `case`s with no leaked braces. (Erlang
/// brace-lowering is line-based, so it covers the multi-line authoring form.)
#[test]
fn nested_if_in_else_erlang() {
    let src = r#"
@@[target("erlang")]
@@system S {
    interface:
        classify(n)
    machine:
        $A {
            classify(n) {
                if n < 0 {
                    @@:("neg")
                } else {
                    if n == 0 {
                        @@:("zero")
                    } else {
                        @@:("pos")
                    }
                }
            }
        }
}
"#;
    let code = compile_source(src, "erlang");
    let region = payload_lines(&code, &["neg", "zero", "pos"]);
    assert!(
        !region.contains('{') && !region.contains('}'),
        "[erlang] stray Frame brace survived lowering:\n{region}"
    );
    for p in ["neg", "zero", "pos"] {
        assert!(
            region.contains(p),
            "[erlang] payload {p:?} missing:\n{region}"
        );
    }
    // Nested conditional → two `case … of` opened (outer + inner).
    assert_eq!(
        code.matches("case (").count(),
        2,
        "[erlang] expected two nested cases:\n{code}"
    );
}

/// Regression guard: a genuine `else if` ladder (inner `if` is the SOLE content
/// of the `else` block, no inner `else`) must STILL collapse to `elseif` in Lua.
#[test]
fn else_if_ladder_still_collapses_lua() {
    let src = r#"
@@[target("lua")]
@@system S {
    interface:
        classify(a, b)
    machine:
        $A {
            classify(a, b) {
                if a { @@:("br_a") } else { if b { @@:("br_b") } }
            }
        }
}
"#;
    let code = compile_source(src, "lua");
    let region = payload_lines(&code, &["br_a", "br_b"]);
    assert!(
        region.contains("elseif"),
        "[lua/ladder] genuine `else if` ladder must collapse to elseif:\n{region}"
    );
    assert!(
        !region.contains('{') && !region.contains('}'),
        "[lua/ladder] stray brace:\n{region}"
    );
}

// ---- Parse-check acceptance gate: the generated Lua must compile under
// `luac -p`. Before the fix, the leaked brace made `luac` report
// `unexpected symbol near '}'`. ----

const REPRO_LUA_SINGLE: &str = r#"
@@[target("lua")]
@@system S {
    interface:
        classify(n)
    machine:
        $A {
            classify(n) {
                if n < 0 { @@:("neg") } else { if n == 0 { @@:("zero") } else { @@:("pos") } }
            }
        }
}
"#;

const REPRO_LUA_DEEP: &str = r#"
@@[target("lua")]
@@system S {
    interface:
        classify(a, b, c)
    machine:
        $A {
            classify(a, b, c) {
                if a { @@:("br_a") } else { if b { @@:("br_b") } else { if c { @@:("br_c") } else { @@:("br_d") } } }
            }
        }
}
"#;

#[test]
fn repro_single_line_luac_clean() {
    parse_check(REPRO_LUA_SINGLE, "lua", "lua", "luac", &["-p"]);
}

#[test]
fn repro_deep_luac_clean() {
    parse_check(REPRO_LUA_DEEP, "lua", "lua", "luac", &["-p"]);
}

#[test]
fn else_if_ladder_luac_clean() {
    let src = r#"
@@[target("lua")]
@@system S {
    interface:
        classify(a, b)
    machine:
        $A {
            classify(a, b) {
                if a { @@:("br_a") } else { if b { @@:("br_b") } }
            }
        }
}
"#;
    parse_check(src, "lua", "lua", "luac", &["-p"]);
}
