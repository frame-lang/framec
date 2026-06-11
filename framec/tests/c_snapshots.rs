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
