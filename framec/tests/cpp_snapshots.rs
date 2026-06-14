//! RFC-0027 in-tree snapshot tests — cpp backend.
//!
//! Mirrors python_snapshots.rs against the cpp target.
//! Re-bless workflow + corpus discipline documented in
//! CONTRIBUTING.md § "Snapshot tests (RFC-0027)".

mod common;

use common::compile_fixture;

#[test]
fn linear_fsm() {
    insta::assert_snapshot!(compile_fixture("01_linear_fsm", "cpp"));
}

#[test]
fn hsm() {
    insta::assert_snapshot!(compile_fixture("02_hsm", "cpp"));
}

#[test]
fn persist() {
    insta::assert_snapshot!(compile_fixture("03_persist", "cpp"));
}

#[test]
fn state_args() {
    insta::assert_snapshot!(compile_fixture("04_state_args", "cpp"));
}

#[test]
fn pushpop() {
    insta::assert_snapshot!(compile_fixture("05_pushpop", "cpp"));
}

#[test]
fn selfcall() {
    insta::assert_snapshot!(compile_fixture("06_selfcall", "cpp"));
}

#[test]
fn forward() {
    insta::assert_snapshot!(compile_fixture("07_forward", "cpp"));
}

#[test]
fn lifecycle() {
    insta::assert_snapshot!(compile_fixture("08_lifecycle", "cpp"));
}

#[test]
fn return_explicit() {
    insta::assert_snapshot!(compile_fixture("09_return_explicit", "cpp"));
}

#[test]
fn actions() {
    insta::assert_snapshot!(compile_fixture("10_actions", "cpp"));
}

#[test]
fn consts() {
    insta::assert_snapshot!(compile_fixture("11_consts", "cpp"));
}

#[test]
fn no_persist() {
    insta::assert_snapshot!(compile_fixture("12_no_persist", "cpp"));
}

#[test]
fn lifecycle_args() {
    insta::assert_snapshot!(compile_fixture("13_lifecycle_args", "cpp"));
}

/// Regression test for issue #69 — C++ `@@:self` lowering across all sections.
///
/// Snapshot tests only diff TEXT, so an unlowered `@@:self` could freeze into
/// a `.snap` undetected — the cpp backend has no compile-gate (unlike
/// js/lua/php/python/ruby/rust, which run `compile_check_all`; cf. #60). This
/// pipes the dedicated `16_self_member_lowering` fixture through
/// `g++ -fsyntax-only`. The fixture is valid C++ apart from its `@@:self`
/// references, so the only thing that can make the compiler reject it is an
/// unlowered `@@:self` — and it exercises every section the lowering must
/// reach: a handler body, an `operations:` body, an `actions:` body, a native
/// `return @@:self.x`, a `@@:(@@:self.x)` return-expr, and a cross-system
/// embed call (`@@:self.inner.ping()` → `this->inner->ping()`).
///
/// Skipped (not failed) when no C++ compiler is on PATH, matching the
/// RFC-0034 convention in the other backends' compile checks.
#[test]
fn issue69_self_member_lowering_compiles() {
    use std::process::Command;
    let cxx = match common::find_tool("g++")
        .or_else(|| common::find_tool("clang++"))
        .or_else(|| common::find_tool("c++"))
    {
        Some(p) => p,
        None => {
            eprintln!("issue #69 cpp compile check skipped: no C++ compiler on PATH");
            return;
        }
    };
    let code = compile_fixture("16_self_member_lowering", "cpp");
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("16_self_member_lowering.cpp");
    std::fs::write(&path, &code).expect("write tempfile");
    let out = Command::new(&cxx)
        .args(["-std=c++17", "-fsyntax-only"])
        .arg(&path)
        .output()
        .expect("c++ process");
    assert!(
        out.status.success(),
        "issue #69: framec emits C++ with an unlowered `@@:self` that the compiler rejects.\n--- stderr ---\n{}\n--- generated source ---\n{}",
        String::from_utf8_lossy(&out.stderr),
        code
    );
}

/// RUNTIME gate for the C++ backend (#78) — compiles AND EXECUTES the
/// `17_float_any_roundtrip` fixture. #77 (float literals stored as double
/// in std::any; declared-type any_cast<float> reads throw bad_any_cast)
/// compiled cleanly, so the -fsyntax-only gate (#60) blessed it; only
/// execution catches the erased-type round-trip class. The fixture's main
/// asserts the values and exits non-zero on any mismatch or uncaught cast.
///
/// Skipped (not failed) when no C++ compiler is on PATH.
#[test]
fn issue78_float_any_roundtrip_runs() {
    use std::process::Command;
    let cxx = match common::find_tool("g++")
        .or_else(|| common::find_tool("clang++"))
        .or_else(|| common::find_tool("c++"))
    {
        Some(p) => p,
        None => {
            eprintln!("#78 cpp runtime gate skipped: no C++ compiler on PATH");
            return;
        }
    };
    let code = compile_fixture("17_float_any_roundtrip", "cpp");
    let dir = tempfile::tempdir().expect("tempdir");
    let src = dir.path().join("17_float_any_roundtrip.cpp");
    let bin = dir.path().join("rt");
    std::fs::write(&src, &code).expect("write tempfile");
    let build = Command::new(&cxx)
        .args(["-std=c++17", "-o"])
        .arg(&bin)
        .arg(&src)
        .output()
        .expect("c++ process");
    assert!(
        build.status.success(),
        "#78: float-any fixture failed to compile.\n--- stderr ---\n{}\n--- generated source ---\n{}",
        String::from_utf8_lossy(&build.stderr),
        code
    );
    let run = Command::new(&bin).output().expect("run process");
    assert!(
        run.status.success(),
        "#77/#78: float-any fixture FAILED AT RUNTIME (the class -fsyntax-only cannot catch).\n--- stdout ---\n{}\n--- stderr ---\n{}",
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    );
}

/// #86 — C++ core dispatch must be EXCEPTION-FREE so output compiles under
/// `-fno-exceptions` (the condition Godot's web/wasm engine imposes). The
/// interface wrapper used to emit `try { __kernel; pop } catch(...) { pop;
/// throw }`; that dead catch path (Frame handlers never throw) created a hard
/// link-time dependency on the C++ exception runtime, so a GDExtension that ran
/// natively failed to link on web (`undefined symbol invoke_ji`). It is now an
/// RAII scope-guard (destructor pops on every exit). Two locks:
///
/// 1. Token assertion (always runs): a basic sync system's C++ output contains
///    zero `try` / `catch` / `throw`.
/// 2. Compile gate (skip if no C++ compiler): the same output compiles under
///    `-std=c++17 -fno-exceptions` — exactly what Godot web requires.
#[test]
fn issue86_cpp_dispatch_is_exception_free() {
    use std::process::Command;

    // A basic sync, non-persist, non-async system — the core dispatch path.
    let code = compile_fixture("01_linear_fsm", "cpp");

    // (1) Token assertion: no exception machinery in core output. Word-boundary
    // matched so identifiers like `throwaway` wouldn't false-positive (none
    // exist, but be precise).
    for tok in ["try", "catch", "throw"] {
        let needle_paren = format!("{tok} ");
        let needle_brace = format!("{tok}(");
        assert!(
            !code.contains(&needle_paren) && !code.contains(&needle_brace),
            "#86: core C++ dispatch must be exception-free, but emitted `{tok}`.\n--- generated source ---\n{code}"
        );
    }

    // (2) Compile gate under -fno-exceptions. Uses `16_self_member_lowering`
    // (the same compile-clean fixture issue69 syntax-checks — a sync system
    // whose native code is valid C++, unlike 01_linear_fsm whose hand-written
    // bodies omit trailing semicolons and so only round-trip as snapshots).
    let cxx = match common::find_tool("g++")
        .or_else(|| common::find_tool("clang++"))
        .or_else(|| common::find_tool("c++"))
    {
        Some(p) => p,
        None => {
            eprintln!("#86 -fno-exceptions gate skipped: no C++ compiler on PATH");
            return;
        }
    };
    let compile_code = compile_fixture("16_self_member_lowering", "cpp");
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("16_self_member_lowering.cpp");
    std::fs::write(&path, &compile_code).expect("write tempfile");
    let out = Command::new(&cxx)
        .args(["-std=c++17", "-fno-exceptions", "-fsyntax-only"])
        .arg(&path)
        .output()
        .expect("c++ process");
    assert!(
        out.status.success(),
        "#86: framec C++ output does not compile under -fno-exceptions (Godot web requirement).\n--- stderr ---\n{}\n--- generated source ---\n{}",
        String::from_utf8_lossy(&out.stderr),
        compile_code
    );
}

/// #87 / RFC-0049 — a PERSISTED C++ system must also compile under
/// `-fno-exceptions`, so persisted GDExtensions work on Godot web. Two framec
/// exception uses lived in the persist path: the `any_cast` `try/catch` type
/// PROBE (RFC-0049 R1: a query misusing exceptions as control flow) and the
/// E700 quiescence `throw` (R2: a proper precondition error). The probe is now
/// the non-throwing pointer `any_cast<T>(&v)`; the E700 throw and the tolerant
/// typed restore keep `throw`/`try` only behind
/// `#if defined(__cpp_exceptions)`, with an `abort`/null-guard fallback (R3).
///
/// 1. Token assertion (always runs): the save path uses the pointer-form
///    `any_cast<...>(&` (proves the R1 probe was de-thrown).
/// 2. Compile gate (skip if no C++ compiler / no nlohmann): the persist fixture
///    — which self-includes `<nlohmann/json.hpp>` — compiles under
///    `-std=c++17 -fno-exceptions`. nlohmann self-switches its own throws to
///    `abort` under `-fno-exceptions`, so the only thing that can fail this is a
///    residual UNGUARDED framec `try`/`catch`/`throw` in the persist codegen.
#[test]
fn issue87_persist_compiles_no_exceptions() {
    use std::process::Command;

    let code = compile_fixture("18_persist_noexcept", "cpp");

    // (1) Token assertion: the save-side probe is the non-throwing pointer form.
    assert!(
        code.contains("any_cast<int>(&") || code.contains("any_cast<float>(&"),
        "#87: persist save must probe std::any with the non-throwing pointer \
         any_cast<T>(&v) (RFC-0049 R1), but no pointer-form cast was emitted.\n\
         --- generated source ---\n{code}"
    );

    // (2) Compile gate under -fno-exceptions.
    let cxx = match common::find_tool("g++")
        .or_else(|| common::find_tool("clang++"))
        .or_else(|| common::find_tool("c++"))
    {
        Some(p) => p,
        None => {
            eprintln!("#87 -fno-exceptions persist gate skipped: no C++ compiler on PATH");
            return;
        }
    };
    // nlohmann is a system dep (the matrix's `nlohmann-json3-dev`); skip cleanly
    // where it isn't installed rather than fail on an unrelated missing header.
    let probe = tempfile::tempdir().expect("tempdir");
    let probe_src = probe.path().join("probe.cpp");
    std::fs::write(
        &probe_src,
        "#include <nlohmann/json.hpp>\nint main(){ nlohmann::json j; j[\"x\"]=1; return 0; }\n",
    )
    .expect("write probe");
    let probe_ok = Command::new(&cxx)
        .args(["-std=c++17", "-fsyntax-only"])
        .arg(&probe_src)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);
    if !probe_ok {
        eprintln!("#87 -fno-exceptions persist gate skipped: nlohmann/json.hpp not available");
        return;
    }

    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("18_persist_noexcept.cpp");
    std::fs::write(&path, &code).expect("write tempfile");
    let out = Command::new(&cxx)
        .args(["-std=c++17", "-fno-exceptions", "-fsyntax-only"])
        .arg(&path)
        .output()
        .expect("c++ process");
    assert!(
        out.status.success(),
        "#87: persisted C++ does not compile under -fno-exceptions (residual unguarded exception in the persist codegen).\n--- stderr ---\n{}\n--- generated source ---\n{}",
        String::from_utf8_lossy(&out.stderr),
        code
    );
}

/// #88 / RFC-0049 — an `@@[async]` C++ system must also compile under
/// `-fno-exceptions`. The RFC-0043 casing wrapped the busy-gate cleanup in
/// `try { co_await … } catch(...) { …; throw; }` and threw E703 unconditionally
/// — `throw`/`try` keywords that `-fno-exceptions` rejects. The cleanup is now
/// an RAII busy-guard (resets `busy`/`in_flight` on `co_return` AND unwind) and
/// the E703 precondition throw is behind `#if defined(__cpp_exceptions)` with an
/// `abort` fallback (R2+R3). The FrameTask's `std::rethrow_exception` is a
/// function call — legal with exceptions off, dead because handlers never throw.
///
/// 1. Token assertion (always runs): the casing emits the `__E703Guard` RAII
///    guard (proves the try/catch was replaced).
/// 2. Compile gate (skip unless a C++20 + `-fno-exceptions` compiler is on
///    PATH — coroutines need C++20, which the macOS system clang predates):
///    the async fixture compiles under `-std=c++20 -fno-exceptions`.
#[test]
fn issue88_async_compiles_no_exceptions() {
    use std::process::Command;

    let code = compile_fixture("19_async_noexcept", "cpp");

    // (1) Token assertion: the RAII busy-guard replaced the try/catch cleanup.
    assert!(
        code.contains("__E703Guard"),
        "#88: async casing must use the RAII busy-guard (not try/catch) for the \
         gate cleanup, but no __E703Guard was emitted.\n--- generated source ---\n{code}"
    );

    let cxx = match common::find_tool("g++")
        .or_else(|| common::find_tool("clang++"))
        .or_else(|| common::find_tool("c++"))
    {
        Some(p) => p,
        None => {
            eprintln!("#88 async -fno-exceptions gate skipped: no C++ compiler on PATH");
            return;
        }
    };
    // Coroutines require C++20; the macOS system clang predates it. Probe with a
    // trivial coroutine and skip cleanly where C++20 isn't supported (the gate
    // then runs on CI's newer g++).
    let probe = tempfile::tempdir().expect("tempdir");
    let probe_src = probe.path().join("coro.cpp");
    std::fs::write(
        &probe_src,
        "#include <coroutine>\nstruct T{struct promise_type{T get_return_object(){return{};}\
         std::suspend_never initial_suspend(){return{};}std::suspend_never final_suspend()noexcept{return{};}\
         void return_void(){}void unhandled_exception(){}};};\nint main(){return 0;}\n",
    )
    .expect("write probe");
    let probe_ok = Command::new(&cxx)
        .args(["-std=c++20", "-fsyntax-only"])
        .arg(&probe_src)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);
    if !probe_ok {
        eprintln!("#88 async -fno-exceptions gate skipped: no C++20 coroutine support");
        return;
    }

    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("19_async_noexcept.cpp");
    std::fs::write(&path, &code).expect("write tempfile");
    let out = Command::new(&cxx)
        .args(["-std=c++20", "-fno-exceptions", "-fsyntax-only"])
        .arg(&path)
        .output()
        .expect("c++ process");
    assert!(
        out.status.success(),
        "#88: async C++ does not compile under -fno-exceptions (residual unguarded exception in the casing).\n--- stderr ---\n{}\n--- generated source ---\n{}",
        String::from_utf8_lossy(&out.stderr),
        code
    );
}
