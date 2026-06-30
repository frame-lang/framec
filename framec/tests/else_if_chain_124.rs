//! Issue #124 — a direct `else if` chain in a Lua handler body must lower to
//! Lua's single `elseif … then` keyword, not `end else if … then`.
//!
//! Scope note: like #135, this lives in the brace-lowering path that only the
//! **Lua** backend exercises (`codegen/output_block_parser.frs`). The user
//! writes Frame's brace-form control flow `if c { … } else if d { … } else
//! { … }`; framec lowers it to `if … then … elseif … then … else … end`.
//!
//! Root cause: the parser handled the *one-word* `elseif` token and the
//! `} else { if c { … } }` ladder (#135), but NOT the *two-word* `} else if c
//! {` form. That form reached the plain-RBRACE path, emitting `end`, then the
//! bare `else`/`if` passed through and the `if` independently became `if … then`
//! — producing the invalid `end else if … then`. The fix adds an
//! `RBRACE ELSE IF … LBRACE` arm that collapses the link to `elseif … then`,
//! leaving `block_depth` untouched (the consumed `}` and the reopened `{`
//! cancel), exactly like the one-word `elseif` arm. It generalises to N-arm
//! chains (each link is an independent match) and composes with #135 (the
//! `else { if }` form still routes through the dedicated ladder/nested path
//! because it has an intervening `{`).

mod common;
use common::{compile_source, find_tool};
use std::process::Command;

/// Extract the lowered user-event handler body (`…_hdl_user_<event>…`) up to
/// the next blank line — the native conditional lives here, isolated from
/// dispatch scaffolding.
fn handler_region(code: &str, event: &str) -> String {
    let needle = format!("hdl_user_{event}");
    let lines: Vec<&str> = code.lines().collect();
    let start = match lines
        .iter()
        .position(|l| l.contains(&needle) && l.contains("function"))
    {
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

/// Run `luac -p` over the generated Lua. Skipped (not failed) when luac is
/// absent, mirroring the snapshot suite.
fn luac_check(code: &str, label: &str) {
    let bin = match find_tool("luac") {
        Some(p) => p,
        None => {
            eprintln!("#124 {label} luac-check skipped: `luac` not on PATH");
            return;
        }
    };
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("repro124.lua");
    std::fs::write(&path, code).expect("write temp");
    let out = Command::new(&bin)
        .arg("-p")
        .arg(&path)
        .output()
        .unwrap_or_else(|e| panic!("spawn luac: {e}"));
    assert!(
        out.status.success(),
        "[{label}] generated Lua rejected by `luac -p`:\n--- stderr ---\n{}\n--- source ---\n{}",
        String::from_utf8_lossy(&out.stderr),
        code
    );
}

/// Compile `src` to Lua, splice `driver` in just before the module's trailing
/// `return { … }` export, run the chunk under `lua`, and return its stdout.
/// Returns `None` (test should treat as skip) when `lua` is absent.
///
/// The generated module is `local M = { … } … return { M = M }`. `M` is a
/// chunk-local, so the driver must run *inside* the chunk (before its `return`)
/// to reach `M.new()`.
fn run_lua(src: &str, driver: &str) -> Option<String> {
    let bin = find_tool("lua")?;
    let compiled = compile_source(src, "lua");
    // Find the module's final top-level `return {` export and inject the driver
    // immediately before it so the driver shares the chunk's scope.
    let marker = compiled
        .rfind("\nreturn {")
        .expect("generated Lua module must end with a `return { … }` export");
    let mut code = String::new();
    code.push_str(&compiled[..marker]);
    code.push('\n');
    code.push_str(driver);
    code.push('\n');
    code.push_str(&compiled[marker..]);
    code.push('\n');
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("run124.lua");
    std::fs::write(&path, &code).expect("write temp");
    let out = Command::new(&bin)
        .arg(&path)
        .output()
        .unwrap_or_else(|e| panic!("spawn lua: {e}"));
    assert!(
        out.status.success(),
        "lua run failed:\n--- stderr ---\n{}\n--- source ---\n{}",
        String::from_utf8_lossy(&out.stderr),
        code
    );
    Some(String::from_utf8_lossy(&out.stdout).to_string())
}

/// The exact issue repro: a 3-arm `if / else if / else` chain.
const REPRO_3ARM: &str = r#"
@@[target("lua")]
@@system M {
    interface: pick(e: number): number
    machine: $S { pick(e: number): number {
        local r = 0
        if e == 0 { r = 10 } else if e == 1 { r = 20 } else { r = 30 }
        @@:(r)
    } }
}
"#;

#[test]
fn repro_3arm_lowers_to_elseif() {
    let code = compile_source(REPRO_3ARM, "lua");
    let region = handler_region(&code, "pick");
    // The bug emitted `end else if`; the fix emits `elseif` with no preceding
    // `end` on the chain link.
    assert!(
        region.contains("elseif"),
        "[3arm] direct `else if` must lower to `elseif`:\n{region}"
    );
    assert!(
        !region.contains("end else"),
        "[3arm] stray `end else` (the #124 bug) survived:\n{region}"
    );
    assert!(
        !region.contains('{') && !region.contains('}'),
        "[3arm] stray Frame brace survived lowering:\n{region}"
    );
    // One opening `if … then`, one `elseif … then`, one `else`, closed by a
    // single `end`.
    assert_eq!(
        region.matches("then").count(),
        2,
        "[3arm] expected exactly two `then` (if + elseif):\n{region}"
    );
}

#[test]
fn repro_3arm_luac_clean() {
    luac_check(&compile_source(REPRO_3ARM, "lua"), "3arm");
}

#[test]
fn repro_3arm_runs_correct_values() {
    let driver = r#"
local m = M.new()
print(m:pick(0))
print(m:pick(1))
print(m:pick(2))
"#;
    match run_lua(REPRO_3ARM, driver) {
        Some(out) => {
            let got: Vec<&str> = out.split_whitespace().collect();
            assert_eq!(
                got,
                vec!["10", "20", "30"],
                "[3arm/run] pick(0..2) must be 10/20/30, got: {out:?}"
            );
        }
        None => eprintln!("#124 3arm run skipped: `lua` not on PATH"),
    }
}

/// A 4-arm chain: `if / else if / else if / else`. Confirms the fix generalises
/// to arbitrary chain length (each `} else if c {` is an independent match).
const CHAIN_4ARM: &str = r#"
@@[target("lua")]
@@system M {
    interface: grade(e: number): number
    machine: $S { grade(e: number): number {
        local r = 0
        if e == 0 { r = 100 } else if e == 1 { r = 200 } else if e == 2 { r = 300 } else { r = 400 }
        @@:(r)
    } }
}
"#;

#[test]
fn chain_4arm_lowers_to_elseif_ladder() {
    let code = compile_source(CHAIN_4ARM, "lua");
    let region = handler_region(&code, "grade");
    assert_eq!(
        region.matches("elseif").count(),
        2,
        "[4arm] two `else if` links must each become `elseif`:\n{region}"
    );
    assert!(
        !region.contains("end else"),
        "[4arm] stray `end else`:\n{region}"
    );
    assert!(
        !region.contains('{') && !region.contains('}'),
        "[4arm] stray brace:\n{region}"
    );
    // 1 `if then` + 2 `elseif then` = 3 `then`; closed by one `end`.
    assert_eq!(
        region.matches("then").count(),
        3,
        "[4arm] expected three `then`:\n{region}"
    );
}

#[test]
fn chain_4arm_luac_clean() {
    luac_check(&compile_source(CHAIN_4ARM, "lua"), "4arm");
}

#[test]
fn chain_4arm_runs_correct_values() {
    let driver = r#"
local m = M.new()
print(m:grade(0))
print(m:grade(1))
print(m:grade(2))
print(m:grade(3))
"#;
    match run_lua(CHAIN_4ARM, driver) {
        Some(out) => {
            let got: Vec<&str> = out.split_whitespace().collect();
            assert_eq!(
                got,
                vec!["100", "200", "300", "400"],
                "[4arm/run] grade(0..3) must be 100/200/300/400, got: {out:?}"
            );
        }
        None => eprintln!("#124 4arm run skipped: `lua` not on PATH"),
    }
}

/// A direct `else if` chain whose final `else` arm contains a NESTED `if`
/// (the #135 form, `else { if c { … } else { … } }`). Confirms the #124 fix
/// composes with #135: the `else if` links collapse to `elseif`, while the
/// nested-in-else conditional lowers as an ordinary block (its own `end`).
const MIX_124_135: &str = r#"
@@[target("lua")]
@@system M {
    interface: route(e: number): number
    machine: $S { route(e: number): number {
        local r = 0
        if e == 0 { r = 1 } else if e == 1 { r = 2 } else { if e == 2 { r = 3 } else { r = 4 } }
        @@:(r)
    } }
}
"#;

#[test]
fn mix_124_135_composes() {
    let code = compile_source(MIX_124_135, "lua");
    let region = handler_region(&code, "route");
    // The `else if` link → exactly one `elseif`.
    assert_eq!(
        region.matches("elseif").count(),
        1,
        "[mix] the `else if` link must yield one `elseif`:\n{region}"
    );
    assert!(
        !region.contains("end else"),
        "[mix] stray `end else`:\n{region}"
    );
    assert!(
        !region.contains('{') && !region.contains('}'),
        "[mix] stray brace (would be the #135 leak):\n{region}"
    );
    // Outer `if`, the `elseif`, and the nested-in-else `if` → three `then`.
    assert_eq!(
        region.matches("then").count(),
        3,
        "[mix] expected three `then` (if + elseif + nested if):\n{region}"
    );
    // The single conditional line carries two `end`s: the nested-in-else `if`'s
    // own `end` (#135: it stays an ordinary block, NOT collapsed into the
    // chain) and the outer chain's terminating `end`.
    let cond_line = region
        .lines()
        .find(|l| l.contains("elseif"))
        .expect("conditional line");
    assert_eq!(
        cond_line.matches("end").count(),
        2,
        "[mix] conditional line needs two `end` (nested if + outer chain):\n{cond_line}"
    );
    // The nested `if` is preserved as `else if … end` (a real block), confirming
    // it did NOT collapse into the chain's `elseif`.
    assert!(
        cond_line.contains("else if e == 2 then"),
        "[mix] nested-in-else `if` must lower as an ordinary block:\n{cond_line}"
    );
}

#[test]
fn mix_124_135_luac_clean() {
    luac_check(&compile_source(MIX_124_135, "lua"), "mix");
}

#[test]
fn mix_124_135_runs_correct_values() {
    let driver = r#"
local m = M.new()
print(m:route(0))
print(m:route(1))
print(m:route(2))
print(m:route(3))
"#;
    match run_lua(MIX_124_135, driver) {
        Some(out) => {
            let got: Vec<&str> = out.split_whitespace().collect();
            assert_eq!(
                got,
                vec!["1", "2", "3", "4"],
                "[mix/run] route(0..3) must be 1/2/3/4, got: {out:?}"
            );
        }
        None => eprintln!("#124 mix run skipped: `lua` not on PATH"),
    }
}
