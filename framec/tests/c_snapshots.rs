//! RFC-0027 in-tree snapshot tests — c backend.
//!
//! Mirrors python_snapshots.rs against the c target.
//! Re-bless workflow + corpus discipline documented in
//! CONTRIBUTING.md § "Snapshot tests (RFC-0027)".

mod common;

use common::compile_fixture;

#[test]
fn linear_fsm() {
    insta::assert_snapshot!(compile_fixture("01_linear_fsm", "c"));
}

#[test]
fn hsm() {
    insta::assert_snapshot!(compile_fixture("02_hsm", "c"));
}

#[test]
fn persist() {
    insta::assert_snapshot!(compile_fixture("03_persist", "c"));
}

#[test]
fn state_args() {
    insta::assert_snapshot!(compile_fixture("04_state_args", "c"));
}

#[test]
fn pushpop() {
    insta::assert_snapshot!(compile_fixture("05_pushpop", "c"));
}

#[test]
fn selfcall() {
    insta::assert_snapshot!(compile_fixture("06_selfcall", "c"));
}

#[test]
fn forward() {
    insta::assert_snapshot!(compile_fixture("07_forward", "c"));
}

#[test]
fn lifecycle() {
    insta::assert_snapshot!(compile_fixture("08_lifecycle", "c"));
}

#[test]
fn return_explicit() {
    insta::assert_snapshot!(compile_fixture("09_return_explicit", "c"));
}

#[test]
fn actions() {
    insta::assert_snapshot!(compile_fixture("10_actions", "c"));
}

#[test]
fn consts() {
    insta::assert_snapshot!(compile_fixture("11_consts", "c"));
}

#[test]
fn no_persist() {
    insta::assert_snapshot!(compile_fixture("12_no_persist", "c"));
}

#[test]
fn lifecycle_args() {
    insta::assert_snapshot!(compile_fixture("13_lifecycle_args", "c"));
}

/// Compile gate for the C backend (#60) — the gap that let #72 (float/
/// struct void* marshalling) and #73 (pointer-typed embed calls) ship as
/// uncompilable text that snapshots happily blessed. Pipes the dedicated
/// `16_marshal_embed` fixture (valid C apart from its Frame constructs)
/// through `gcc -std=c11 -fsyntax-only`: the only thing that can make the
/// compiler reject it is a marshalling or embed-lowering regression.
///
/// Skipped (not failed) when no C compiler is on PATH, matching the
/// RFC-0034 convention in the other backends' compile checks.
#[test]
fn issue60_marshal_embed_compiles() {
    use std::process::Command;
    let cc = match common::find_tool("gcc")
        .or_else(|| common::find_tool("clang"))
        .or_else(|| common::find_tool("cc"))
    {
        Some(p) => p,
        None => {
            eprintln!("#60 c compile gate skipped: no C compiler on PATH");
            return;
        }
    };
    let code = compile_fixture("16_marshal_embed", "c");
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("16_marshal_embed.c");
    std::fs::write(&path, &code).expect("write tempfile");
    let out = Command::new(&cc)
        .args(["-std=c11", "-fsyntax-only"])
        .arg(&path)
        .output()
        .expect("cc process");
    assert!(
        out.status.success(),
        "#60/#72/#73: framec emits C the compiler rejects.\n--- stderr ---\n{}\n--- generated source ---\n{}",
        String::from_utf8_lossy(&out.stderr),
        code
    );
}

/// RUNTIME gate for the C backend (#78) — compiles AND EXECUTES the
/// `17_float_roundtrip` fixture. The C float state-var path truncated
/// values through `(void*)(intptr_t)` while compiling cleanly (#77's C
/// sibling); only execution catches the round-trip class. The fixture's
/// main asserts the values and exits non-zero on any mismatch.
///
/// Skipped (not failed) when no C compiler is on PATH.
#[test]
fn issue78_float_roundtrip_runs() {
    use std::process::Command;
    let cc = match common::find_tool("gcc")
        .or_else(|| common::find_tool("clang"))
        .or_else(|| common::find_tool("cc"))
    {
        Some(p) => p,
        None => {
            eprintln!("#78 c runtime gate skipped: no C compiler on PATH");
            return;
        }
    };
    let code = compile_fixture("17_float_roundtrip", "c");
    let dir = tempfile::tempdir().expect("tempdir");
    let src = dir.path().join("17_float_roundtrip.c");
    let bin = dir.path().join("rt");
    std::fs::write(&src, &code).expect("write tempfile");
    let build = Command::new(&cc)
        .args(["-std=c11", "-o"])
        .arg(&bin)
        .arg(&src)
        .output()
        .expect("cc process");
    assert!(
        build.status.success(),
        "#78: float fixture failed to compile.\n--- stderr ---\n{}\n--- generated source ---\n{}",
        String::from_utf8_lossy(&build.stderr),
        code
    );
    let run = Command::new(&bin).output().expect("run process");
    assert!(
        run.status.success(),
        "#77/#78: float fixture FAILED AT RUNTIME (values truncated through the void* slot).\n--- stdout ---\n{}\n--- stderr ---\n{}",
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    );
}
