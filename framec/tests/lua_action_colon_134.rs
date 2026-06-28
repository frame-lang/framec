//! Issue #134 — a bare action invocation (and a Python-style cross-system
//! method call) must lower to a Lua COLON call, matching the colon definition
//! framec emits for the callee.
//!
//! Root cause: framec emits actions and embedded-system methods with Lua's
//! implicit-`self` colon syntax (`function S:note(s)` / `function Sensor:bump()`).
//! A handler that invokes one as native passthrough writes a DOT call —
//! `self.note("hi;")` or `$.sensor.bump()` (which lowers to
//! `…state_vars["sensor"].bump()`). In Lua a dot call does NOT pass the
//! receiver as `self`: `self.note("hi;")` binds `self = "hi;"` and `s = nil`,
//! so the action body's `self.trace` is a nil-field crash at run time even
//! though the file compiles. The blessed `@@:self.note(…)` form already lowers
//! to colon; only the bare / `$.`-cross-system passthrough path emitted dot.
//!
//! Fix: `lua_fixup_method_calls` rewrites a native-passthrough method CALL from
//! dot to colon when the callee is colon-defined — `self.<action>(` for any
//! declared action, and `state_vars["<field>"].<method>(` when `<field>` is an
//! embedded system. Only the CALL form (`recv.name(`) changes; value reads
//! (`self.trace`, `state_vars["x"]` without a call) and string/comment text are
//! untouched, preserving the type-ignorant passthrough contract.

mod common;
use common::{compile_source, find_tool};
use std::process::Command;

const BARE_ACTION: &str = r#"
@@[target("lua")]
@@system S {
    interface:
        go()
        trace_of(): string
    machine:
        $A {
            go() { self.note("hi;") }
            trace_of(): string { @@:(self.trace) }
        }
    actions:
        note(s) { self.trace = self.trace .. s }
    domain:
        trace = ""
}
"#;

// Python-style `$.field.method()` cross-system call. The #134 colon fix
// applies here (the call site must be colon), but NOTE: `$.field` resolves the
// receiver via the compartment's `state_vars["field"]` slot, while an embedded
// SYSTEM domain field is stored at `self.field` (see `domain_lowering_note`).
// That storage mismatch is a SEPARATE pre-existing framec gap present in every
// backend including the Python reference — so this form is colon-correct but
// not run-correct. The runnable cross-system gate below uses the blessed
// RFC-0046 form, which both lowers to colon AND resolves the right receiver.
const CROSS_SYSTEM_STATE_ACCESSOR: &str = r#"
@@[target("lua")]
@@system Sensor {
    interface:
        bump()
        count_of(): int
    machine:
        $S {
            bump() { self.n = self.n + 1 }
            count_of(): int { @@:(self.n) }
        }
    domain:
        n: int = 0
}

@@[main]
@@system Controller {
    interface:
        tick()
        report(): int
    machine:
        $C {
            tick() { $.sensor.bump() }
            report(): int { @@:($.sensor.count_of()) }
        }
    domain:
        sensor: Sensor = @@Sensor()
}
"#;

// Blessed RFC-0046 cross-system form (`@@:self.field.method()`). This is the
// correct way to call a method on an embedded-system domain field: it lowers to
// `self.field:method(...)` (colon, correct receiver) and runs end to end.
const CROSS_SYSTEM: &str = r#"
@@[target("lua")]
@@system Sensor {
    interface:
        bump()
        count_of(): int
    machine:
        $S {
            bump() { self.n = self.n + 1 }
            count_of(): int { @@:(self.n) }
        }
    domain:
        n: int = 0
}

@@[main]
@@system Controller {
    interface:
        tick()
        report(): int
    machine:
        $C {
            tick() { @@:self.sensor.bump() }
            report(): int { @@:(@@:self.sensor.count_of()) }
        }
    domain:
        sensor: Sensor = @@Sensor()
}
"#;

/// Blessed RFC-0046 form — regression guard: must STILL be colon after the fix.
const BLESSED_SELF: &str = r#"
@@[target("lua")]
@@system S {
    interface:
        go()
        trace_of(): string
    machine:
        $A {
            go() { @@:self.note("hi;") }
            trace_of(): string { @@:(self.trace) }
        }
    actions:
        note(s) { self.trace = self.trace .. s }
    domain:
        trace = ""
}
"#;

/// The user-event handler method body (`function S:_…_hdl_user_<event>(…)`)
/// down to its closing `end`. Isolates the call site from runtime scaffolding.
fn handler_region(code: &str, event: &str) -> String {
    let needle = format!("hdl_user_{event}");
    let lines: Vec<&str> = code.lines().collect();
    let start = match lines
        .iter()
        .position(|l| l.contains("function") && l.contains(&needle))
    {
        Some(i) => i + 1,
        None => return String::new(),
    };
    let mut out = Vec::new();
    for l in &lines[start..] {
        if l.trim_start().starts_with("end") {
            break;
        }
        out.push(*l);
    }
    out.join("\n")
}

// ───────────────────────────── unit: call form ─────────────────────────────

#[test]
fn bare_action_call_uses_colon() {
    let code = compile_source(BARE_ACTION, "lua");
    let region = handler_region(&code, "go");
    assert!(
        region.contains("self:note(\"hi;\")"),
        "[#134] bare action call must use colon `self:note(...)`:\n{region}"
    );
    assert!(
        !region.contains("self.note("),
        "[#134] dot action call `self.note(` must not survive:\n{region}"
    );
    // The action definition stays colon-defined.
    assert!(
        code.contains("function S:note(s)"),
        "[#134] action must be colon-defined `function S:note(s)`:\n{code}"
    );
    // Value read of `self.trace` must be UNAFFECTED (still dot, no call).
    assert!(
        code.contains("self.trace = self.trace"),
        "[#134] `self.trace` value read must stay dot:\n{code}"
    );
}

#[test]
fn cross_system_state_accessor_call_uses_colon() {
    // The Python-style `$.sensor.bump()` form: #134 makes the CALL colon
    // (`state_vars["sensor"]:bump()`). (Receiver-storage correctness for an
    // embedded-system `$.field` is a separate gap — see `domain_lowering_note`.)
    let code = compile_source(CROSS_SYSTEM_STATE_ACCESSOR, "lua");
    let tick = handler_region(&code, "tick");
    assert!(
        tick.contains("state_vars[\"sensor\"]:bump()"),
        "[#134] `$.field` cross-system call must use colon `…:bump()`:\n{tick}"
    );
    assert!(
        !tick.contains("state_vars[\"sensor\"].bump("),
        "[#134] dot cross-system call must not survive:\n{tick}"
    );
    let report = handler_region(&code, "report");
    assert!(
        report.contains("state_vars[\"sensor\"]:count_of()"),
        "[#134] `$.field` cross-system read-call must use colon `…:count_of()`:\n{report}"
    );
}

#[test]
fn cross_system_blessed_call_uses_colon() {
    // The blessed RFC-0046 `@@:self.sensor.bump()` form lowers to
    // `self.sensor:bump()` — colon AND the right receiver (`self.sensor`).
    let code = compile_source(CROSS_SYSTEM, "lua");
    let tick = handler_region(&code, "tick");
    assert!(
        tick.contains("self.sensor:bump()"),
        "[#134] blessed cross-system call must be `self.sensor:bump()`:\n{tick}"
    );
    let report = handler_region(&code, "report");
    assert!(
        report.contains("self.sensor:count_of()"),
        "[#134] blessed cross-system read-call must be `self.sensor:count_of()`:\n{report}"
    );
    // The embedded system's methods stay colon-defined.
    assert!(
        code.contains("function Sensor:bump()"),
        "[#134] embedded-system method must be colon-defined:\n{code}"
    );
}

#[test]
fn blessed_self_call_still_colon() {
    // RFC-0046 `@@:self.note(...)` already lowered to colon; the fix must not
    // regress it (and must not double-rewrite it to `self::note`).
    let code = compile_source(BLESSED_SELF, "lua");
    let region = handler_region(&code, "go");
    assert!(
        region.contains("self:note(\"hi;\")"),
        "[#134] blessed `@@:self.note` must stay colon `self:note(...)`:\n{region}"
    );
    assert!(
        !region.contains("self::note"),
        "[#134] must not double-rewrite to `self::note`:\n{region}"
    );
}

// ───────────────────────── acceptance gate: luac -p ────────────────────────

fn luac_clean(src: &str, repro: &str) {
    let bin = match find_tool("luac") {
        Some(p) => p,
        None => {
            eprintln!("#134 luac parse-check skipped: `luac` not on PATH");
            return;
        }
    };
    let code = compile_source(src, "lua");
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join(format!("{repro}.lua"));
    std::fs::write(&path, &code).expect("write temp");
    let out = Command::new(&bin)
        .arg("-p")
        .arg(&path)
        .output()
        .expect("spawn luac");
    assert!(
        out.status.success(),
        "[#134/{repro}] luac rejected generated Lua:\n--- stderr ---\n{}\n--- source ---\n{}",
        String::from_utf8_lossy(&out.stderr),
        code
    );
}

#[test]
fn bare_action_luac_clean() {
    luac_clean(BARE_ACTION, "bare_action");
}

#[test]
fn cross_system_luac_clean() {
    luac_clean(CROSS_SYSTEM, "cross_system");
}

#[test]
fn cross_system_state_accessor_luac_clean() {
    // The `$.field` form is colon-correct and parses cleanly under luac even
    // though its receiver storage is the separate pre-existing gap.
    luac_clean(CROSS_SYSTEM_STATE_ACCESSOR, "cross_system_state");
}

/// Documents the SEPARATE pre-existing gap surfaced while fixing #134: a
/// Python-style `$.field` read of an embedded-SYSTEM domain field resolves the
/// receiver through `compartment.state_vars["field"]`, but the embedded system
/// is stored at `self.field` (the constructor writes `self.sensor = …`). The
/// `$.field` slot is therefore nil at run time. This is NOT a Lua-only or
/// colon issue — the Python reference emits the identical mismatch
/// (`compartment.state_vars["sensor"]` read vs `self.sensor` store). The
/// correct cross-system spelling is the blessed `@@:self.field.method()` form,
/// which this PR proves runs. This test pins the gap so a future fix is
/// deliberate; update it when `$.field` embedded-system access is addressed.
#[test]
fn domain_lowering_note_state_accessor_reads_state_vars_not_self() {
    let code = compile_source(CROSS_SYSTEM_STATE_ACCESSOR, "lua");
    // Store site: embedded system assigned to `self.sensor`.
    assert!(
        code.contains("self.sensor = Sensor"),
        "[#134-note] embedded system should be stored at self.sensor:\n{code}"
    );
    // Read site: `$.sensor` resolves through the state_vars slot (the mismatch).
    let tick = handler_region(&code, "tick");
    assert!(
        tick.contains("state_vars[\"sensor\"]"),
        "[#134-note] `$.sensor` reads state_vars (documents the gap):\n{tick}"
    );
}

// ─────────────────── acceptance gate: lua RUN + behavior ───────────────────

/// Compile `src`, append `driver`, run under `lua`, and return stdout.
/// Skipped (returns None) when `lua` is absent.
fn run_lua(src: &str, driver: &str, repro: &str) -> Option<String> {
    let bin = find_tool("lua")?;
    let mut code = compile_source(src, "lua");
    // The module returns `{ Name = Name, … }`; capture it via `package.preload`
    // so the driver can `require` it from the same file is overkill — instead we
    // strip the trailing `return { … }` and inline the driver after the defs.
    if let Some(pos) = code.rfind("\nreturn {") {
        code.truncate(pos + 1);
    }
    code.push('\n');
    code.push_str(driver);
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join(format!("{repro}_run.lua"));
    std::fs::write(&path, &code).expect("write temp");
    let out = Command::new(&bin).arg(&path).output().expect("spawn lua");
    assert!(
        out.status.success(),
        "[#134/{repro}] lua run failed:\n--- stderr ---\n{}\n--- stdout ---\n{}\n--- source ---\n{}",
        String::from_utf8_lossy(&out.stderr),
        String::from_utf8_lossy(&out.stdout),
        code
    );
    Some(String::from_utf8_lossy(&out.stdout).into_owned())
}

#[test]
fn bare_action_runs_and_accumulates_trace() {
    // Drive: go() twice, then assert trace == "hi;hi;".
    let driver = r#"
local s = S.new()
s:go()
s:go()
local t = s:trace_of()
assert(t == "hi;hi;", "expected 'hi;hi;', got '" .. tostring(t) .. "'")
print("PASS bare_action trace=" .. t)
"#;
    if let Some(stdout) = run_lua(BARE_ACTION, driver, "bare_action") {
        eprintln!("[#134 lua RUN bare_action] {}", stdout.trim());
        assert!(
            stdout.contains("PASS bare_action trace=hi;hi;"),
            "[#134] bare-action run output unexpected:\n{stdout}"
        );
    } else {
        eprintln!("#134 bare_action lua-run skipped: `lua` not on PATH");
    }
}

#[test]
fn cross_system_runs_and_increments_sensor() {
    // Drive: tick() three times, then assert report() == 3.
    let driver = r#"
local c = Controller.new()
c:tick()
c:tick()
c:tick()
local n = c:report()
assert(n == 3, "expected 3, got " .. tostring(n))
print("PASS cross_system count=" .. n)
"#;
    if let Some(stdout) = run_lua(CROSS_SYSTEM, driver, "cross_system") {
        eprintln!("[#134 lua RUN cross_system] {}", stdout.trim());
        assert!(
            stdout.contains("PASS cross_system count=3"),
            "[#134] cross-system run output unexpected:\n{stdout}"
        );
    } else {
        eprintln!("#134 cross_system lua-run skipped: `lua` not on PATH");
    }
}
