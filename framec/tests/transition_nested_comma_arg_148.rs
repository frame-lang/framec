//! Issue #148 — a transition argument that contains a nested comma (a comma
//! inside a call/bracket in the value, e.g. `-> $S(clamp(1, 2), 9)`) must be
//! kept whole. The old codegen split the captured arg blob on *every* comma
//! (`blob.split(',')`), tearing `clamp(1, 2)` into `clamp(1` and `2)` and
//! emitting an invalid argument list. The fix routes every transition
//! arg-blob split through the depth/string-aware `codegen_utils::arg_values` /
//! `split_top_level_args` primitives.

mod common;
use common::compile_source;

const REPRO: &str = r#"
@@[target("python_3")]
@@[main]
@@system G {
    interface: go()
    machine:
        $S {
            go() { -> $Active(clamp(1, 2), 9) }
        }
        $Active(pt: int, hi: int) {
            $>() { }
        }
}
"#;

/// The nested-comma value must survive intact — never split into `clamp(1` /
/// `2)` — across every backend's transition arg emission.
#[test]
fn nested_comma_transition_arg_kept_whole_all_backends() {
    let backends = [
        "python_3",
        "typescript",
        "javascript",
        "rust",
        "go",
        "c",
        "cpp",
        "csharp",
        "java",
        "kotlin",
        "swift",
        "dart",
        "php",
        "ruby",
        "lua",
        "erlang",
        "gdscript",
    ];
    for lang in backends {
        // Re-point the target so each backend compiles the same machine.
        let src = REPRO.replace("python_3", lang);
        let code = compile_source(&src, lang);
        assert!(
            code.contains("clamp(1, 2)"),
            "[#148/{lang}] nested-comma arg `clamp(1, 2)` must be kept whole:\n{code}"
        );
        // The tell-tale broken split: a `clamp(1` not immediately followed by
        // `, 2)`. (Guards against a regression that re-splits the blob.)
        assert!(
            !code.contains("clamp(1,\n")
                && !code.contains("clamp(1;")
                && !code.contains("clamp(1)"),
            "[#148/{lang}] arg was split mid-call:\n{code}"
        );
    }
}
