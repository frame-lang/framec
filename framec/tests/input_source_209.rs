//! #209 — an `@@system` can borrow its input. A header param typed with an
//! `@@fsm` alphabet type (`text: bytes`) is the system's borrowed input source:
//! the caller's buffer is held by reference through a per-system adapter
//! (`<Sys>Input`, with `get(i)` / `len()`), so a positioned probe no longer copies
//! the whole buffer on every call. Targets with no adapter reject it with E620.

mod common;
use common::{compile_expect_error, compile_source, find_tool};
use std::process::Command;

/// Minimal borrowing system: body-free, so it is valid on every target.
const MINIMAL: &str = r#"
@@system Scan(text: bytes) {
    interface:
        go()
    machine:
        $S {
            go() {}
        }
}
"#;

const CODE_TARGETS: &[&str] = &[
    "python_3",
    "typescript",
    "javascript",
    "java",
    "csharp",
    "kotlin",
    "swift",
    "dart",
    "go",
    "ruby",
    "php",
    "lua",
    "gdscript",
    "cpp",
    "c",
    "rust",
];

#[test]
fn every_code_target_emits_the_input_adapter() {
    for t in CODE_TARGETS {
        let c = compile_source(MINIMAL, t);
        assert!(
            c.contains("ScanInput"),
            "[#209/{t}] a borrowing system must emit its `ScanInput` adapter\n{c}"
        );
    }
}

#[test]
fn a_system_without_an_input_source_emits_no_adapter() {
    let src = MINIMAL.replace("Scan(text: bytes)", "Scan");
    for t in CODE_TARGETS {
        let c = compile_source(&src, t);
        assert!(
            !c.contains("ScanInput"),
            "[#209/{t}] an ordinary system must emit exactly as before (no adapter)\n{c}"
        );
    }
}

#[test]
fn e620_names_the_reason_per_target() {
    let g = compile_expect_error(MINIMAL, "graphviz");
    assert!(
        g.contains("E620") && g.contains("nothing to borrow"),
        "[#209/graphviz] E620 must say a diagram has nothing to borrow\n{g}"
    );
    let e = compile_expect_error(MINIMAL, "erlang");
    assert!(
        e.contains("E620") && e.contains("Erlang target has no input adapter"),
        "[#209/erlang] E620 must name the Erlang reason\n{e}"
    );
    assert!(
        !e.contains("GraphViz") && !e.contains("diagram"),
        "[#209/erlang] the Erlang message must not carry the Graphviz reason\n{e}"
    );
}

/// The Python output, RUN: the adapter holds the caller's buffer itself (no copy),
/// and the machine scans it correctly from a given position.
const SKIP_STRING_PY: &str = r#"
@@system SkipString(text: bytes) {
    interface:
        skip_at(i)

    machine:
        $Scan {
            skip_at(i) {
                self.ok = False
                self.result = 0
                self.pos = i
                if self.pos >= self.text.len():
                    return
                if self.text.get(self.pos) != 34:
                    return
                self.pos = self.pos + 1
                -> $InString
            }
        }
        $InString {
            $>() {
                while self.pos < self.text.len():
                    b = self.text.get(self.pos)
                    self.pos = self.pos + 1
                    if b == 92:
                        self.pos = self.pos + 1
                    elif b == 34:
                        self.ok = True
                        self.result = self.pos
                        break
                -> $Scan
            }
        }

    domain:
        ok = False
        result = 0
        pos = 0
}
"#;

const DRIVER_PY: &str = r#"
buf = bytearray(b'x "he\\"llo" y')
s = SkipString(buf)
assert s.text._b is buf, "adapter copied the buffer instead of borrowing it"
s.skip_at(2)
assert (s.ok, s.result) == (True, 11), (s.ok, s.result)
s.skip_at(0)
assert (s.ok, s.result) == (False, 0), (s.ok, s.result)
print("OK")
"#;

#[test]
fn python_borrows_and_scans_at_runtime() {
    let Some(py) = find_tool("python3") else {
        eprintln!("skipped: python3 not on PATH");
        return;
    };
    let code = compile_source(SKIP_STRING_PY, "python_3");
    let dir = std::env::temp_dir().join(format!("framec_209_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("skip_string.py");
    std::fs::write(&file, format!("{code}\n{DRIVER_PY}")).unwrap();
    let out = Command::new(py).arg("-I").arg(&file).output().unwrap();
    let _ = std::fs::remove_dir_all(&dir);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success() && stdout.trim() == "OK",
        "[#209/python] run failed\nstdout: {stdout}\nstderr: {}\n--- generated ---\n{code}",
        String::from_utf8_lossy(&out.stderr)
    );
}
