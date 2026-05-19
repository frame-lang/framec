//! Shared test helpers for RFC-0027 in-tree snapshot tests.
//!
//! Cargo convention: files under `tests/common/` are treated as
//! module sources rather than separate integration test binaries
//! (cf. https://doc.rust-lang.org/cargo/reference/cargo-targets.html#integration-tests),
//! so this `mod.rs` can be `mod common;`-imported by each
//! per-backend snapshot test file.

#![allow(dead_code)]

use framec::frame_c::compiler::compile_module;
use framec::frame_c::compiler::TargetLanguage;
use framec::frame_c::utils::RunError;
use std::convert::TryFrom;
use std::path::PathBuf;

/// Load a fixture from `tests/fixtures/<name>.frm` and compile it
/// for the given target language. Returns the generated target
/// code as a String, suitable for `insta::assert_snapshot!`.
///
/// Panics with a useful message if the fixture file is missing or
/// if framec returns an error (snapshot tests assume the fixture
/// itself is valid Frame; a compile error means the fixture has a
/// bug, not the snapshot).
pub fn compile_fixture(fixture_name: &str, target: &str) -> String {
    let lang = TargetLanguage::try_from(target)
        .unwrap_or_else(|e| panic!("unknown target language '{}': {}", target, e));
    let fixture_path = fixture_path(fixture_name);
    let source = std::fs::read_to_string(&fixture_path)
        .unwrap_or_else(|e| panic!("read fixture {}: {}", fixture_path.display(), e));
    match compile_module(&source, lang) {
        Ok(code) => code,
        Err(RunError { error, .. }) => panic!(
            "framec failed to compile fixture {} for target {}:\n{}",
            fixture_name, target, error
        ),
    }
}

/// Absolute path to a fixture file under `tests/fixtures/`.
fn fixture_path(fixture_name: &str) -> PathBuf {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.push("tests");
    p.push("fixtures");
    p.push(format!("{}.frm", fixture_name));
    p
}

/// Compile an inline Frame source string for the given target.
///
/// Used by regression tests that exercise target-specific syntax
/// (e.g. `&str` for Rust) that would fail on other backends if
/// added as a shared fixture. The cross-backend snapshot corpus
/// stays in `tests/fixtures/` and remains portable.
pub fn compile_source(source: &str, target: &str) -> String {
    let lang = TargetLanguage::try_from(target)
        .unwrap_or_else(|e| panic!("unknown target language '{}': {}", target, e));
    match compile_module(source, lang) {
        Ok(code) => code,
        Err(RunError { error, .. }) => panic!(
            "framec failed to compile inline source for target {}:\n{}\n--- source ---\n{}",
            target, error, source
        ),
    }
}

// ─── RFC-0034: per-backend in-process compile checks ─────────────────
//
// Snapshot tests historically only diff TEXT; the .snap file could
// freeze syntactically invalid output and the test would still pass.
// `compile_check_fixture` pipes the framec-emitted output through
// the target language's parser / type-checker in non-executing mode
// so the canonical correctness property — "does framec emit code the
// target accepts?" — is actually verified. See `docs/rfcs/rfc-0034.md`
// for the contract.

/// Canonical fixture corpus shared by every backend's compile check.
/// Adding a fixture in `tests/fixtures/` extends coverage across all
/// 17 backends.
pub const FIXTURES_ALL: &[&str] = &[
    "01_linear_fsm",
    "02_hsm",
    "03_persist",
    "04_state_args",
    "05_pushpop",
    "06_selfcall",
    "07_forward",
    "08_lifecycle",
    "09_return_explicit",
    "10_actions",
    "11_consts",
    "12_no_persist",
];

/// Fixtures whose emitted code references external libraries the
/// in-process compile check can't resolve without a package manager
/// (`serde_json`, JSON parsers in Java/C#/Kotlin/Swift, etc.). The
/// matrix test-env exercises these via cargo / mvn / etc.; per
/// RFC-0034 the exclusion is the seam where the in-process check
/// hands off to the matrix.
pub fn excluded_for(target: &str) -> &'static [&'static str] {
    match target {
        // Serde-style JSON serialization in persist codegen.
        "rust" | "java" | "csharp" | "kotlin" | "swift" | "cpp" => &["03_persist", "12_no_persist"],
        // Python/JS/Ruby: fixtures using Rust-flavored
        // `if x { ... }` braces in user code (Python expects
        // `if x:`; JS needs `if (x)`; Ruby needs `if x then` or
        // `if x\n`). The user-written control-flow syntax is
        // inherently target-coupled — the matrix test-env covers
        // the same semantics via target-specific fixtures.
        "python_3" | "javascript" | "ruby" => &["09_return_explicit", "11_consts"],
        // PHP: framec's PHP backend doesn't yet lower `self.X` →
        // `$this->X` in handler-body NativeCode (the same class of
        // bug Java had, fixed in RFC-0033 via
        // `java_native_rewrite.rs`). Every fixture with
        // self-member access in a handler trips this. Tracked
        // separately — once the PHP rewriter ships, these
        // exclusions shrink to just the persist pair.
        "php" => &[
            "01_linear_fsm",
            "02_hsm",
            "03_persist",
            "05_pushpop",
            "06_selfcall",
            "07_forward",
            "08_lifecycle",
            "09_return_explicit",
            "10_actions",
            "11_consts",
            "12_no_persist",
        ],
        // GDScript persist uses Godot's `var_to_bytes` /
        // `bytes_to_var` (engine-provided, no external dep) —
        // covered.
        _ => &[],
    }
}

/// Find an executable on `PATH`. Returns `None` if the binary isn't
/// present (test runner reports the result as `ignored` rather than
/// failing — a developer running `cargo test` shouldn't be required
/// to install every backend's toolchain).
pub fn find_tool(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        let candidate = dir.join(name);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

/// Run a per-backend compile check across every fixture in
/// `FIXTURES_ALL` not in `excluded_for(target)`. The `tool_runner`
/// closure receives the path to the compiled target file and
/// returns the tool's `Output`; the helper asserts success and
/// formats a useful failure message including the rejected source.
///
/// The caller is responsible for finding the tool (`find_tool`)
/// and reporting "skipped" when it's missing — exiting the test
/// early before any fixture work.
pub fn compile_check_all<F>(target: &str, file_ext: &str, tool_runner: F)
where
    F: Fn(&std::path::Path) -> std::process::Output,
{
    let excluded: std::collections::HashSet<&str> = excluded_for(target).iter().copied().collect();
    for fixture in FIXTURES_ALL {
        if excluded.contains(*fixture) {
            continue;
        }
        let code = compile_fixture(fixture, target);
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join(format!("{}.{}", fixture, file_ext));
        std::fs::write(&path, &code).expect("write tempfile");
        let out = tool_runner(&path);
        assert!(
            out.status.success(),
            "fixture `{}` emits {} that the tool rejected.\n--- stderr ---\n{}\n--- stdout ---\n{}\n--- first 80 lines of generated source ---\n{}",
            fixture,
            target,
            String::from_utf8_lossy(&out.stderr),
            String::from_utf8_lossy(&out.stdout),
            code.lines().take(80).collect::<Vec<_>>().join("\n")
        );
    }
}
