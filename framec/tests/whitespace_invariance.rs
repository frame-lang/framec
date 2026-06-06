//! Tier-A whitespace invariance (coverage Stage 2).
//!
//! **Horizontal** whitespace between the tokens of a Frame statement is
//! insignificant: any permutation of spaces/tabs must produce
//! byte-identical generated code. The Tier-A `FILLERS` below stay
//! space/tab-only because not every gap is newline-tolerant (e.g. the gap
//! between `push$` and `->`, or before `=> $^`).
//!
//! Tier-B (FRAMEC_BUGS #43): the gap between a transition's `->` and its
//! `$State` target **is** newline-tolerant — `->` ⏎ `$State` lowers
//! identically to the one-line form. (It previously emitted `->` / `$B`
//! as native garbage with no diagnostic.) That property is pinned by
//! `transition_across_newline_matches_single_line` below rather than by
//! the blanket `FILLERS` sweep.
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

/// Tier-B (FRAMEC_BUGS #43): a transition written across a newline
/// (`->` ⏎ `$State`) must lower **identically** to the one-line `-> $State`.
/// Before the fix it silently emitted `->` / `$State` as native text — no
/// transition, no diagnostic — so the target state looked unreachable
/// (spurious W414) and the handler did nothing.
#[test]
fn transition_across_newline_matches_single_line() {
    let one_line = r#"
@@system R {
    interface:
        go()
        ev()
    machine:
        $A { go() { -> $B } }
        $B { ev() { -> $A } }
}
"#;
    let multi_line = r#"
@@system R {
    interface:
        go()
        ev()
    machine:
        $A { go() {
                ->
                $B } }
        $B { ev() { -> $A } }
}
"#;
    for backend in BACKENDS {
        assert_eq!(
            compile_source(multi_line, backend),
            compile_source(one_line, backend),
            "[transition-newline / {backend}] `->` ⏎ `$B` must lower like `-> $B`",
        );
    }
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
