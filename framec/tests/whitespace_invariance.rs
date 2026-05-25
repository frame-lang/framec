//! Tier-A whitespace invariance (coverage Stage 2).
//!
//! **Horizontal** whitespace between the tokens of a Frame statement is
//! insignificant: any permutation of spaces/tabs must produce
//! byte-identical generated code. Verified scope, learned the hard way
//! (see below): Frame statements are **line-oriented** — a newline ends a
//! statement, so newlines are NOT free to inject mid-statement (`->\n$B`
//! is not a transition; it lowers to native `->` / `$B` lines). Thus the
//! Tier-A invariant covers space/tab variants only; newline behavior is a
//! separate (Tier-B) property.
//!
//! This is adjacent to — but not the same as — FRAMEC_BUGS #42. #42 was
//! two *different but equivalent* segmentations (`push$`⏎`-> $S`
//! [bare-push + transition] vs inline `push$ -> $S` [push-with-transition])
//! that must lower the same; that semantic-equivalence is covered by the
//! #42 fix + the p15 fuzz pattern. This file covers the orthogonal axis:
//! horizontal-whitespace invariance of a single statement.
//!
//! The generator marks each gap in a seed with `~` and substitutes the
//! fillers below, asserting every variant transpiles to the same bytes as
//! the single-space canonical. Oracle: transpile-and-diff (no toolchains).

mod common;
use common::compile_source;

/// Horizontal-whitespace fillers substituted at each `~` gap. No newlines:
/// Frame is line-oriented, so a newline mid-statement is a *different*
/// program, not a whitespace variant of the same one.
const FILLERS: &[&str] = &[" ", "  ", "\t", " \t ", "\t ", "   ", "\t\t"];

/// Backends to check. Includes the four #42 broke on (python_3,
/// javascript, typescript, gdscript) plus a structural-helper backend
/// (rust) and two more compiled ones.
const BACKENDS: &[&str] = &[
    "python_3",
    "javascript",
    "typescript",
    "gdscript",
    "rust",
    "csharp",
    "java",
];

/// For one seed (with `~` gap markers), assert every whitespace filler
/// transpiles to the same bytes as the single-space canonical, on every
/// backend.
fn assert_invariant(label: &str, seed: &str) {
    for backend in BACKENDS {
        let canonical = compile_source(&seed.replace('~', " "), backend);
        for filler in FILLERS {
            let variant = compile_source(&seed.replace('~', filler), backend);
            assert_eq!(
                variant, canonical,
                "[{label} / {backend}] whitespace filler {filler:?} changed the generated output",
            );
        }
    }
}

/// `push$ -> $B`: horizontal-whitespace invariance of push-with-transition. Inline and separated forms must be
/// identical. Gaps: around `push$` and `->`.
#[test]
fn push_with_transition_is_whitespace_invariant() {
    assert_invariant(
        "push-transition",
        r#"
@@system R {
    interface:
        go()
        back()
    machine:
        $A { go() { push$~->~$B } }
        $B { back() { -> pop$ } }
}
"#,
    );
}

/// Plain transition `-> $B`. Gap: around `->`.
#[test]
fn plain_transition_is_whitespace_invariant() {
    assert_invariant(
        "transition",
        r#"
@@system R {
    interface:
        go()
        ev()
    machine:
        $A { go() {~->~$B } }
        $B { ev() { } }
}
"#,
    );
}

/// Forward transition `-> => $B`. Gaps: around `->` and `=>`.
#[test]
fn forward_transition_is_whitespace_invariant() {
    assert_invariant(
        "forward",
        r#"
@@system R {
    interface:
        go()
        ev()
    machine:
        $A { go() {~->~=>~$B } }
        $B { ev() { } }
}
"#,
    );
}

/// Default forward `=> $^` from an HSM child. Gap: around `=>`.
#[test]
fn default_forward_is_whitespace_invariant() {
    assert_invariant(
        "default-forward",
        r#"
@@system R {
    interface:
        ev()
    machine:
        $Parent { ev() { } }
        $Child => $Parent {
            ev() {~=>~$^ }
        }
}
"#,
    );
}

/// Pop transition `-> pop$`. Gap: around `->`.
#[test]
fn pop_transition_is_whitespace_invariant() {
    assert_invariant(
        "pop-transition",
        r#"
@@system R {
    interface:
        go()
        back()
    machine:
        $A { go() { push$ -> $B } }
        $B { back() {~->~pop$ } }
}
"#,
    );
}
