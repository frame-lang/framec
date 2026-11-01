//! #211 — `push$ -> $S(7)` silently dropped its state args: the front end kept only
//! the target's NAME, so every backend emitted `__prepareEnter("S", [], [])` and the
//! program crashed when `$S` read its param (Python: IndexError), while the
//! identical `-> $S(7)` worked. The push node now carries the whole transition and
//! delegates to the transition expander.

mod common;
use common::{compile_source, find_tool};
use std::process::Command;

const SRC: &str = r#"
@@system P {
    interface:
        go()
        peek(): int

    machine:
        $A {
            go() { push$ -> $S(7) }
        }

        $S(value: int) {
            peek(): int { @@:(value) }
        }
}
"#;

#[test]
fn python_push_transition_keeps_state_args() {
    let code = compile_source(SRC, "python_3");
    assert!(
        code.contains(r#"__prepareEnter("S", [7], [])"#),
        "[#211/python] the push-transition must pass its state args\n{code}"
    );
    let Some(py) = find_tool("python3") else {
        eprintln!("skipped run: python3 not on PATH");
        return;
    };
    let dir = std::env::temp_dir().join(format!("framec_211_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("p.py");
    std::fs::write(&file, format!("{code}\ns = P()\ns.go()\nprint(s.peek())\n")).unwrap();
    let out = Command::new(py).arg("-I").arg(&file).output().unwrap();
    let _ = std::fs::remove_dir_all(&dir);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success() && stdout.trim() == "7",
        "[#211/python] expected 7\nstdout: {stdout}\nstderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn every_target_passes_the_push_state_arg() {
    // A literal that appears nowhere else: before the fix the front end dropped
    // the arg, so no target emitted it at all.
    let src = SRC.replace("$S(7)", "$S(424242)");
    for t in [
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
        "erlang",
    ] {
        let code = compile_source(&src, t);
        assert!(
            code.contains("424242"),
            "[#211/{t}] the push-transition's state arg must reach the emitted code\n{code}"
        );
    }
}
