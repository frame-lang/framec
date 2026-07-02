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

/// wasm32 RUNTIME gate for the C backend (#81) — compiles the
/// `17_float_roundtrip` fixture with Emscripten (32-bit pointers) and
/// executes it under node. This is the exact configuration the old
/// double-through-`void*` bit-pun silently corrupted: `sizeof(void*) == 4 <
/// sizeof(double) == 8`, so every float collapsed toward 0 while the
/// 64-bit native leg above stayed green. Doubles now travel as heap/stack
/// boxes, which is pointer-width independent — this leg locks that.
///
/// Skipped (not failed) when emcc or node is not on PATH, matching the
/// RFC-0034 toolchain-availability convention.
#[test]
fn issue81_float_roundtrip_runs_wasm32() {
    use std::process::Command;
    let emcc = match common::find_tool("emcc") {
        Some(p) => p,
        None => {
            eprintln!("#81 wasm32 runtime gate skipped: emcc not on PATH");
            return;
        }
    };
    let node = match common::find_tool("node") {
        Some(p) => p,
        None => {
            eprintln!("#81 wasm32 runtime gate skipped: node not on PATH");
            return;
        }
    };
    let code = compile_fixture("17_float_roundtrip", "c");
    let dir = tempfile::tempdir().expect("tempdir");
    let src = dir.path().join("17_float_roundtrip.c");
    let js = dir.path().join("rt.js");
    std::fs::write(&src, &code).expect("write tempfile");
    let build = Command::new(&emcc)
        .args(["-std=c11", "-o"])
        .arg(&js)
        .arg(&src)
        .output()
        .expect("emcc process");
    assert!(
        build.status.success(),
        "#81: float fixture failed to compile for wasm32.\n--- stderr ---\n{}\n--- generated source ---\n{}",
        String::from_utf8_lossy(&build.stderr),
        code
    );
    let run = Command::new(&node).arg(&js).output().expect("node process");
    assert!(
        run.status.success(),
        "#81: float fixture FAILED AT RUNTIME ON wasm32 (doubles corrupted through a 4-byte void* slot).\n--- stdout ---\n{}\n--- stderr ---\n{}",
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    );
}

/// RUNTIME gate for the C backend (#83 / RFC-0048) — float args on a `pop$`
/// transition, compiled AND executed. At a `pop$` the popped target state is
/// runtime-determined, so the declared `$>`/`<$` param type is statically
/// unknown; the old codegen pushed pop-args via `(void*)(intptr_t)(value)`,
/// which truncates a float and leaves the typed reader dereferencing a non-box
/// (crash). `{sys}_ARG_PUSH` now dispatches on the value's static type via
/// `_Generic`. The fixture exercises BOTH type-blind sites (enter-args and
/// exit-args on `pop$`) and asserts the values; only execution catches this
/// class. Compiled `-fno-exceptions` so the fix stays Godot-web-clean (#86).
///
/// Skipped (not failed) when no C compiler is on PATH.
#[test]
fn issue83_pop_float_args_runs() {
    use std::process::Command;
    let cc = match common::find_tool("gcc")
        .or_else(|| common::find_tool("clang"))
        .or_else(|| common::find_tool("cc"))
    {
        Some(p) => p,
        None => {
            eprintln!("#83 c runtime gate skipped: no C compiler on PATH");
            return;
        }
    };
    let code = compile_fixture("18_pop_float_args", "c");
    let dir = tempfile::tempdir().expect("tempdir");
    let src = dir.path().join("18_pop_float_args.c");
    let bin = dir.path().join("rt");
    std::fs::write(&src, &code).expect("write tempfile");
    let build = Command::new(&cc)
        .args(["-std=c11", "-fno-exceptions", "-o"])
        .arg(&bin)
        .arg(&src)
        .output()
        .expect("cc process");
    assert!(
        build.status.success(),
        "#83: pop-float-args fixture failed to compile.\n--- stderr ---\n{}\n--- generated source ---\n{}",
        String::from_utf8_lossy(&build.stderr),
        code
    );
    let run = Command::new(&bin).output().expect("run process");
    assert!(
        run.status.success(),
        "#83: pop-float-args fixture FAILED AT RUNTIME (float pop-arg truncated / non-box deref).\n--- stdout ---\n{}\n--- stderr ---\n{}",
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    );
}

/// wasm32 RUNTIME gate for #83 — same `pop$` float-args fixture under
/// Emscripten (32-bit pointers) + node, the configuration the old `void*`
/// bit-pun corrupted. Skipped when emcc or node is absent (RFC-0034).
#[test]
fn issue83_pop_float_args_runs_wasm32() {
    use std::process::Command;
    let emcc = match common::find_tool("emcc") {
        Some(p) => p,
        None => {
            eprintln!("#83 wasm32 runtime gate skipped: emcc not on PATH");
            return;
        }
    };
    let node = match common::find_tool("node") {
        Some(p) => p,
        None => {
            eprintln!("#83 wasm32 runtime gate skipped: node not on PATH");
            return;
        }
    };
    let code = compile_fixture("18_pop_float_args", "c");
    let dir = tempfile::tempdir().expect("tempdir");
    let src = dir.path().join("18_pop_float_args.c");
    let js = dir.path().join("rt.js");
    std::fs::write(&src, &code).expect("write tempfile");
    let build = Command::new(&emcc)
        .args(["-std=c11", "-o"])
        .arg(&js)
        .arg(&src)
        .output()
        .expect("emcc process");
    assert!(
        build.status.success(),
        "#83: pop-float-args fixture failed to compile for wasm32.\n--- stderr ---\n{}\n--- generated source ---\n{}",
        String::from_utf8_lossy(&build.stderr),
        code
    );
    let run = Command::new(&node).arg(&js).output().expect("node process");
    assert!(
        run.status.success(),
        "#83: pop-float-args fixture FAILED AT RUNTIME ON wasm32.\n--- stdout ---\n{}\n--- stderr ---\n{}",
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    );
}

// Note: C has no `async_attribute` snapshot — `@@[async]` on C is rejected at
// validation by E722 (no async runtime; #111 R4), so there is no casing/machine
// output to snapshot. That rejection is covered by `c_async_rejected_722.rs`.
