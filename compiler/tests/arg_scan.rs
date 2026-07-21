//! **ArgScan's standalone spec — the ledger's CARRY values, FIX rulings, and dual-counter
//! angle fork, proven by running (no hand oracle).**
//!
//! `arg_scan::parse` is generated from `arg_scan.frs` (a `@@[scan(u8)]` TWO-counter
//! automaton, design record §11 Option C) and replaces the hand `parse_inst_args` +
//! `split_top_commas` + `split_top_eq` (parts.rs M6). The battery:
//!
//!   * PIN tests (§7.2 as amended §11.7) — CARRY ledger rows, hand-independent expected
//!     values, and `angles == Inert`;
//!   * FIX tests (§7.3 as amended) — every FIX ledger row asserts the new behavior directly;
//!   * FORK tests (§11.7) — the dual-counter angle fork: Forked/Operators/Inert outcomes,
//!     digraph guards, depth-0 independence, refusal suppression;
//!   * FUZZ (2) — deterministic xorshift64*: a full-alphabet PUBLIC-invariants arm
//!     (determinism + fork strict-count-reduction) and a teeth gate over the outcome classes;
//!   * MILESTONES (3) — the spec §1103/§1108 call sites through the wired production path
//!     (InstScan shape + ArgScan args), the `native_parts` route guard, and the fork reaching
//!     the tree.
//!
//! Every test here is SCAFFOLDING (white-box on the internal `arg_scan`/`inst_scan`); it NEVER
//! promotes to the cross-language corpus.

use frame_compiler::text::scan::arg_scan::{self, AngleReading, ArgsOut, Refusal};
use frame_compiler::text::scan::inst_scan;
use frame_compiler::text::scan::literals::Target;
use frame_compiler::text::scan::parts::native_parts;
use frame_compiler::tree::body::{ArgAngles, InstArg, NativePart, ParamGroup};

const TARGETS: [Target; 4] = [Target::C, Target::Java, Target::Rust, Target::Python3];

/// The comparable quotient of both sides: (group, name, value) triples + the named flag.
/// group: 0 Domain / 1 State / 2 Enter.
type Quot = (Vec<(i32, Option<String>, String)>, bool);

fn quot_args(args: &[InstArg], named: bool) -> Quot {
    (
        args.iter()
            .map(|a| {
                let g = match a.group {
                    ParamGroup::State => 1,
                    ParamGroup::Enter => 2,
                    ParamGroup::Domain => 0,
                };
                (g, a.name.clone(), a.value.clone())
            })
            .collect(),
        named,
    )
}

/// The machine's PRIMARY candidate quotient.
fn machine(interior: &str, t: Target) -> Quot {
    let out = machine_full(interior, t);
    quot_args(&out.primary.args, out.primary.named)
}
fn machine_full(interior: &str, t: Target) -> ArgsOut {
    let b = interior.as_bytes();
    arg_scan::parse(b, 0, b.len(), t)
}
fn machine_full_bytes(interior: &[u8], t: Target) -> ArgsOut {
    arg_scan::parse(interior, 0, interior.len(), t)
}

/// Build the expected quotient from literals.
fn expect(rows: &[(i32, Option<&str>, &str)], named: bool) -> Quot {
    (
        rows.iter()
            .map(|(g, n, v)| (*g, n.map(str::to_string), v.to_string()))
            .collect(),
        named,
    )
}

/// CARRY assertion: machine == the hand-independent expected values, and `angles == Inert`,
/// under EVERY target (carry rows are target-invariant).
fn pinned(interior: &str, rows: &[(i32, Option<&str>, &str)], named: bool) {
    let e = expect(rows, named);
    for &t in &TARGETS {
        let full = machine_full(interior, t);
        let m = quot_args(&full.primary.args, full.primary.named);
        assert_eq!(m, e, "pin wrong: {interior:?} under {t:?}");
        assert_eq!(
            full.angles,
            AngleReading::Inert,
            "carry input not Inert: {interior:?} under {t:?}"
        );
    }
}

/// FIX assertion: machine == the expected NEW behavior (asserted directly, self-contained).
fn fixed(interior: &str, t: Target, rows: &[(i32, Option<&str>, &str)], named: bool) {
    let m = machine(interior, t);
    assert_eq!(m, expect(rows, named), "fix wrong: {interior:?} under {t:?}");
}
fn fixed_all(interior: &str, rows: &[(i32, Option<&str>, &str)], named: bool) {
    for &t in &TARGETS {
        fixed(interior, t, rows, named);
    }
}

// ============================================================================
// PIN battery (§7.2 as amended §11.7). CARRY rows: hand-independent expected
// values (asserted directly) + `angles == Inert`.
// ============================================================================

#[test]
fn pin_spec_examples() {
    // The spec §1103/§1108 call sites (docs/frame_language.md).
    pinned("10", &[(0, None, "10")], false);
    pinned("$(7), \"R2D2\"", &[(1, None, "7"), (0, None, "\"R2D2\"")], false);
    pinned("$>(50)", &[(2, None, "50")], false);
    pinned(
        "$(x=7), name=\"R2D2\"",
        &[(1, Some("x"), "7"), (0, Some("name"), "\"R2D2\"")],
        true,
    );
    // The non-final `$>(...)` spec examples 4 and 6: the machine's spec-documented values stand
    // (a non-final enter group does NOT silence later commas — the two-counter automaton counts
    // the sigil's `>` correctly). Asserted directly, self-contained.
    for (interior, rows, named) in [
        (
            "$(0), $>(1000), \"primary\"",
            vec![(1, None, "0"), (2, None, "1000"), (0, None, "\"primary\"")],
            false,
        ),
        (
            "$(slot=0), $>(timeout=1000), name=\"primary\"",
            vec![
                (1, Some("slot"), "0"),
                (2, Some("timeout"), "1000"),
                (0, Some("name"), "\"primary\""),
            ],
            true,
        ),
    ] {
        for &t in &TARGETS {
            let full = machine_full(interior, t);
            let m = quot_args(&full.primary.args, full.primary.named);
            assert_eq!(m, expect(&rows, named), "spec pin wrong: {interior:?} under {t:?}");
            assert_eq!(full.angles, AngleReading::Inert, "{interior:?} under {t:?}");
        }
    }
}

#[test]
fn pin_empty_interior() {
    pinned("", &[], false);
}

#[test]
fn pin_ws_only_interior() {
    pinned("   ", &[], false);
    pinned(" \t\n ", &[], false);
}

#[test]
fn pin_trailing_comma() {
    pinned("a,", &[(0, None, "a")], false);
}

#[test]
fn pin_tail_segment() {
    pinned("a, b", &[(0, None, "a"), (0, None, "b")], false);
}

#[test]
fn pin_empty_segment_drop() {
    // Empty segments are dropped silently (L15, carried) — but COUNTED (`dropped_empty`).
    pinned("a,,b", &[(0, None, "a"), (0, None, "b")], false);
    pinned(",", &[], false);
    pinned(",,", &[], false);
    for &t in &TARGETS {
        assert_eq!(machine_full("a,,b", t).dropped_empty, 1);
        assert_eq!(machine_full(",", t).dropped_empty, 1);
        assert_eq!(machine_full(",,", t).dropped_empty, 2);
    }
}

#[test]
fn pin_ws_tail_after_comma() {
    // §11.10: `dropped_empty` counts COMMA-delimited empties only. A ws tail after a
    // trailing comma ends at end-of-interior, not at a comma — NOT counted (the hand also
    // drops that tail, via its trim-empty `continue`; behavior equal, register true).
    pinned("a, ", &[(0, None, "a")], false);
    for &t in &TARGETS {
        assert_eq!(machine_full("a, ", t).dropped_empty, 0);
    }
}

#[test]
fn pin_bracket_protected_comma() {
    pinned(
        "f(a, b), [c, d], {e, f}",
        &[(0, None, "f(a, b)"), (0, None, "[c, d]"), (0, None, "{e, f}")],
        false,
    );
}

#[test]
fn pin_eq_in_string() {
    // A `=` inside a string never names (L29) — the ONE opacity model covers `=` too.
    pinned("\"a=b\", x", &[(0, None, "\"a=b\""), (0, None, "x")], false);
}

#[test]
fn pin_string_comma() {
    // A `,` inside a string never splits (all four targets share the `"` form).
    pinned("\"a,b\", c", &[(0, None, "\"a,b\""), (0, None, "c")], false);
    // Char-quoted comma under the C-family (the table's targeted rows).
    for t in [Target::C, Target::Java] {
        let m = machine("',' , x", t);
        assert_eq!(m, expect(&[(0, None, "','"), (0, None, "x")], false));
    }
}

#[test]
fn pin_unterminated_quote_tail() {
    // L5: the hand swallowed to end inside an unterminated quote — CARRIED byte-for-byte,
    // but the state is NAMED (refusal 1) and the fork is suppressed.
    pinned("a, \"b, c", &[(0, None, "a"), (0, None, "\"b, c")], false);
    for &t in &TARGETS {
        let full = machine_full("a, \"b, c", t);
        assert_eq!(full.refusal, Refusal::UnterminatedOpaque);
        assert_eq!(full.angles, AngleReading::Inert);
    }
}

#[test]
fn pin_eq_first_wins() {
    // L24: the FIRST qualifying `=` names; the rest of the segment is the value.
    pinned("a=b=c", &[(0, Some("a"), "b=c")], true);
}

#[test]
fn pin_positional() {
    pinned("f(x)", &[(0, None, "f(x)")], false);
}

#[test]
fn pin_eq_guards() {
    // L26, input corrected per §11.6: `e<=f`/`g>=h` were never in the carry domain (the
    // hand's comma splitter counts the `<` of `<=`) — the digraph comma behavior lives in
    // `fork_guard_digraphs`. These are the genuinely-carry guard rows.
    pinned("a==b, x", &[(0, None, "a==b"), (0, None, "x")], false);
    pinned("c!=d, y", &[(0, None, "c!=d"), (0, None, "y")], false);
}

#[test]
fn pin_empty_group_kept() {
    // L17: `$()` KEEPS an empty State arg (asymmetric with L15's drop — documented).
    pinned("$()", &[(1, None, "")], false);
}

#[test]
fn pin_sigil_space_fallthrough() {
    // L18: `$ (x)` is NOT a sigil (exact-prefix match only) — silent Domain fallthrough,
    // which is also what keeps `$ (x)` a legal Java call (`$` is a Java identifier).
    pinned("$ (x)", &[(0, None, "$ (x)")], false);
}

#[test]
fn pin_sigil_only_at_start() {
    // L23: a sigil mid-segment is content.
    pinned("f($(x))", &[(0, None, "f($(x))")], false);
}

#[test]
fn pin_unclosed_group() {
    // L20: `$(x` accepted silently with the value-so-far — behavior-identical to the hand,
    // now observable (refusal 4, routed through the zero-byte $VerbatimTail).
    pinned("$(x", &[(1, None, "x")], false);
    for &t in &TARGETS {
        let full = machine_full("$(x", t);
        assert_eq!(full.refusal, Refusal::UnclosedGroup);
        assert_eq!(full.angles, AngleReading::Inert);
    }
}

#[test]
fn pin_stray_closer_simple() {
    // L9 (carry half): a simple stray closer merges the rest verbatim — value-equal to the
    // hand's silenced-comma merge; the state is named (refusal 2).
    pinned("a], y", &[(0, None, "a], y")], false);
    for &t in &TARGETS {
        assert_eq!(machine_full("a], y", t).refusal, Refusal::StrayCloser);
    }
}

#[test]
fn pin_trailing_after_group() {
    // L19 (carry half): `$(x) junk` keeps the hand's garbage value `x) junk`, named
    // (refusal 3).
    pinned("$(x) junk", &[(1, None, "x) junk")], false);
    for &t in &TARGETS {
        assert_eq!(machine_full("$(x) junk", t).refusal, Refusal::TrailingAfterGroup);
    }
}

#[test]
fn pin_named_flag_any() {
    // L22: `named` = ANY arg named — the mixed form is carried (its rejection is §1167's
    // deferred job; per-arg namedness is preserved so it can be detected later).
    pinned("x=1, 2", &[(0, Some("x"), "1"), (0, None, "2")], true);
}

#[test]
fn pin_lossy_value() {
    // L12/L31: invalid UTF-8 in a value materializes as U+FFFD (the held file-wide
    // `from_utf8_lossy` policy; the machine's spans keep the raw bytes recoverable).
    let interior: &[u8] = b"a, \xFF";
    for &t in &TARGETS {
        let full = machine_full_bytes(interior, t);
        let m = quot_args(&full.primary.args, full.primary.named);
        assert_eq!(
            m,
            expect(&[(0, None, "a"), (0, None, "\u{FFFD}")], false),
            "lossy pin under {t:?}"
        );
    }
}

// ============================================================================
// FIX battery — 18 tests (§7.3 as amended §11.7). Every one asserts the NEW
// behavior AND `!= hand` (teeth).
// ============================================================================

#[test]
fn fix_bug_a_nested_call() {
    // L16 / Bug A: the group closer is the BALANCED closer found by the walk — the hand's
    // `trim_end_matches(')')` ate the user's own `)` (`$(g(1))` -> `g(1`).
    fixed_all("$(g(1))", &[(1, None, "g(1)")], false);
}

#[test]
fn fix_bug_a_inner_parens() {
    fixed_all("$(f())", &[(1, None, "f()")], false);
    fixed_all("$((x))", &[(1, None, "(x)")], false);
}

#[test]
fn fix_bug_a_enter_group() {
    fixed_all("$>(h(2)), k", &[(2, None, "h(2)"), (0, None, "k")], false);
}

#[test]
fn fix_bug_b_bare_lt() {
    // L6 / Bug B(i): a bare `<` no longer silences every later comma. Option C shape: the
    // unclosed `<` drives hypothesis G nonviable -> the SOLE O reading (Operators).
    fixed_all("a < b, c", &[(0, None, "a < b"), (0, None, "c")], false);
    for &t in &TARGETS {
        assert_eq!(machine_full("a < b, c", t).angles, AngleReading::Operators);
    }
}

#[test]
fn fix_bug_b_ge() {
    // L7 / Bug B(ii): `>=` is guard-excluded (never counted) — the hand decremented depth
    // through zero and silenced the comma. No angle question at all: Inert.
    fixed_all("x >= 1, y", &[(0, None, "x >= 1"), (0, None, "y")], false);
    for &t in &TARGETS {
        assert_eq!(machine_full("x >= 1, y", t).angles, AngleReading::Inert);
    }
}

#[test]
fn fix_bug_b_shift() {
    // A shift's angles are counted but never balance -> G nonviable -> sole O reading.
    fixed_all("a << 2, b", &[(0, None, "a << 2"), (0, None, "b")], false);
    for &t in &TARGETS {
        assert_eq!(machine_full("a << 2, b", t).angles, AngleReading::Operators);
    }
}

#[test]
fn fix_comment_blind_split_c() {
    // L10: a `,` inside a comment never splits (the hand had no comment model).
    fixed("a /* , */ , b", Target::C, &[(0, None, "a /* , */"), (0, None, "b")], false);
}

#[test]
fn fix_comment_blind_split_java() {
    fixed("a /* , */ , b", Target::Java, &[(0, None, "a /* , */"), (0, None, "b")], false);
}

#[test]
fn fix_comment_blind_split_rust() {
    fixed("a /* , */ , b", Target::Rust, &[(0, None, "a /* , */"), (0, None, "b")], false);
}

#[test]
fn fix_comment_blind_split_python() {
    fixed(
        "a # x, y\n , b",
        Target::Python3,
        &[(0, None, "a # x, y"), (0, None, "b")],
        false,
    );
}

#[test]
fn fix_triple_split_python() {
    // L11: the hand's `"`-pair model splits mid-triple.
    fixed(
        "\"\"\"a\"b,c\"\"\", d",
        Target::Python3,
        &[(0, None, "\"\"\"a\"b,c\"\"\""), (0, None, "d")],
        false,
    );
}

#[test]
fn fix_raw_split_rust() {
    // L11: the hand mis-pairs the quotes of a Rust raw string.
    fixed(
        "r#\"a\", b\"#, c",
        Target::Rust,
        &[(0, None, "r#\"a\", b\"#"), (0, None, "c")],
        false,
    );
}

#[test]
fn fix_stray_closer_resurrection() {
    // L9 (fix half): the hand's negative depth was RESURRECTED by a later `(` and split
    // mid-group; ArgScan yields one verbatim arg (refusal 2) — deterministic degradation.
    fixed_all("a], (b, c)", &[(0, None, "a], (b, c)")], false);
    for &t in &TARGETS {
        assert_eq!(machine_full("a], (b, c)", t).refusal, Refusal::StrayCloser);
    }
}

#[test]
fn fix_trailing_paren_not_eaten() {
    // L19 (fix half): trailing junk after a group is SURFACED verbatim, never trimmed
    // away (`$(x))` -> hand `x`, machine `x))`).
    fixed_all("$(x))", &[(1, None, "x))")], false);
    for &t in &TARGETS {
        assert_eq!(machine_full("$(x))", t).refusal, Refusal::TrailingAfterGroup);
    }
}

#[test]
fn fix_compound_assign() {
    // L27: a named split requires an IDENTIFIER name — `x +=` no longer names `x +`.
    fixed_all("x += 1", &[(0, None, "x += 1")], false);
    fixed_all("y -= 2, z", &[(0, None, "y -= 2"), (0, None, "z")], false);
}

#[test]
fn fix_empty_name() {
    // L28: `=v` no longer yields the empty name `Some("")` — ident validation fails on the
    // empty span, so the `=` is content and the arg is positional.
    fixed_all("=v", &[(0, None, "=v")], false);
}

#[test]
fn fix_alphabet_consistency() {
    // L30: ONE walk, one alphabet — the comma decision AND the eq decision come from the
    // same registers. `m[k<1], n=2` under C: the `<` sits at bracket depth 1 (never
    // counted), the `,` splits, the `=` names. The hand's two siblings disagreed: its
    // comma splitter counted the `<` (comma silenced -> one segment), then its eq splitter
    // ignored angles and named the garbage LHS `m[k<1], n`.
    fixed(
        "m[k<1], n=2",
        Target::C,
        &[(0, None, "m[k<1]"), (0, Some("n"), "2")],
        true,
    );
}

#[test]
fn fix_unicode_ws_trim() {
    // L32 (micro-delta, recorded): spans trim BYTE ws only — the hand's `str::trim` also
    // trimmed Unicode-exotic ws (NBSP).
    fixed_all(
        "\u{a0}x\u{a0}, y",
        &[(0, None, "\u{a0}x\u{a0}"), (0, None, "y")],
        false,
    );
}

// ============================================================================
// FORK battery — 9 tests (§11.7). The dual-counter fork: both hypotheses from one
// walk, divergence carried explicitly, adjudicated downstream — never guessed.
// ============================================================================

/// Unwrap a Forked reading's payload (the O candidate) or panic.
fn forked_alt(out: &ArgsOut, ctx: &str) -> Quot {
    match &out.angles {
        AngleReading::Forked(alt) => quot_args(&alt.args, alt.named),
        other => panic!("{ctx}: expected Forked, got {other:?}"),
    }
}

#[test]
fn fork_generic_java() {
    // The mechanism is TARGET-BLIND (no generics table): the same fork on all 4 targets.
    for &t in &TARGETS {
        let out = machine_full("new HashMap<Integer, String>(), z", t);
        let g = quot_args(&out.primary.args, out.primary.named);
        assert_eq!(
            g,
            expect(&[(0, None, "new HashMap<Integer, String>()"), (0, None, "z")], false),
            "G candidate under {t:?}"
        );
        let o = forked_alt(&out, "fork_generic_java");
        assert_eq!(
            o,
            expect(
                &[(0, None, "new HashMap<Integer"), (0, None, "String>()"), (0, None, "z")],
                false
            ),
            "O candidate under {t:?}"
        );
        assert!(g.0.len() < o.0.len(), "Lemma 3(ii) invariant under {t:?}");
    }
}

#[test]
fn fork_generic_rust_turbofish() {
    for &t in &TARGETS {
        let out = machine_full("Vec::<u8, A>::new(), n", t);
        assert_eq!(
            quot_args(&out.primary.args, out.primary.named),
            expect(&[(0, None, "Vec::<u8, A>::new()"), (0, None, "n")], false)
        );
        assert_eq!(
            forked_alt(&out, "turbofish"),
            expect(&[(0, None, "Vec::<u8"), (0, None, "A>::new()"), (0, None, "n")], false)
        );
    }
}

#[test]
fn fork_matched_comparison() {
    // The classic ambiguity: `a<b, c>d` — one generic-bracketed arg, or two comparisons.
    for &t in &TARGETS {
        let out = machine_full("a<b, c>d", t);
        assert_eq!(
            quot_args(&out.primary.args, out.primary.named),
            expect(&[(0, None, "a<b, c>d")], false)
        );
        assert_eq!(
            forked_alt(&out, "matched_comparison"),
            expect(&[(0, None, "a<b"), (0, None, "c>d")], false)
        );
    }
}

#[test]
fn fork_classic_cpp() {
    // `a<b, c>(d)` — the old AngleProbe design mis-grouped this permanently (its follower
    // `(` confirmed a generic). Option C carries both readings; arity decides either way.
    for &t in &TARGETS {
        let out = machine_full("a<b, c>(d)", t);
        assert_eq!(
            quot_args(&out.primary.args, out.primary.named),
            expect(&[(0, None, "a<b, c>(d)")], false)
        );
        assert_eq!(
            forked_alt(&out, "classic_cpp"),
            expect(&[(0, None, "a<b"), (0, None, "c>(d)")], false)
        );
    }
}

#[test]
fn fork_angles_inside_parens_inert() {
    // The depth-0-only rule: angles inside `()` are never counted — no probe needed, no
    // fork noise. (The hand silenced the comma here: its `(`+`<` both raised depth.)
    for &t in &TARGETS {
        let out = machine_full("f(a<b), g(c>d)", t);
        assert_eq!(out.angles, AngleReading::Inert, "under {t:?}");
        assert_eq!(
            quot_args(&out.primary.args, out.primary.named),
            expect(&[(0, None, "f(a<b)"), (0, None, "g(c>d)")], false)
        );
    }
}

#[test]
fn fork_guard_digraphs() {
    // L35: `<=` `>=` `->` `=>` are guard-excluded — ordinary content under BOTH
    // hypotheses, so these are fork-free (Inert) and split at the comma. Each is also a
    // Bug-B-family tooth: the hand counted the digraphs' angle bytes and silenced the
    // comma (one arg).
    for &t in &TARGETS {
        for (interior, rows, named) in [
            (
                "a <= b, c >= d",
                vec![(0, None, "a <= b"), (0, None, "c >= d")],
                false,
            ),
            ("p->x, y", vec![(0, None, "p->x"), (0, None, "y")], false),
            // `=>`: the `=` qualifies as a named split under the CARRIED eq guard (the
            // hand names it too); the guarded `>` is content — 2 args, comma alive.
            (
                "a => b, c",
                vec![(0, Some("a"), "> b"), (0, None, "c")],
                true,
            ),
        ] {
            let out = machine_full(interior, t);
            assert_eq!(out.angles, AngleReading::Inert, "{interior:?} under {t:?}");
            let m = quot_args(&out.primary.args, out.primary.named);
            assert_eq!(m, expect(&rows, named), "{interior:?} under {t:?}");
        }
    }
}

#[test]
fn fork_independence_nested() {
    // §11.5: an inner list's angles live at outer depth >= 1 and cannot leak into the
    // outer fork. The OUTER `c>d` here is a bare counted `>` -> G nonviable -> Operators;
    // the inner `a<b` contributes nothing.
    for &t in &TARGETS {
        let out = machine_full("@@Inner(a<b), c>d", t);
        assert_eq!(out.angles, AngleReading::Operators, "under {t:?}");
        assert_eq!(
            quot_args(&out.primary.args, out.primary.named),
            expect(&[(0, None, "@@Inner(a<b)"), (0, None, "c>d")], false)
        );
    }
}

#[test]
fn fork_suppressed_on_refusal() {
    // L36 + gate amendment: a refusal clears g_viable and the wrapper reports Inert — a
    // malformed list adjudicates nothing, even with counted angles present.
    for &t in &TARGETS {
        let out = machine_full("a<b, \"unterm c>d", t);
        assert_eq!(out.refusal, Refusal::UnterminatedOpaque, "under {t:?}");
        assert_eq!(out.angles, AngleReading::Inert, "refusal must suppress the fork");
        assert_eq!(
            quot_args(&out.primary.args, out.primary.named),
            expect(&[(0, None, "a<b"), (0, None, "\"unterm c>d")], false)
        );
    }
}

/// The fork ACTUALLY forks on the classic ambiguous inputs, and the two candidates differ by a
/// strict count reduction (Lemma 3(ii)) — self-contained, no oracle.
#[test]
fn fork_forks_with_strict_count_reduction() {
    let corpus = [
        "new HashMap<Integer, String>(), z",
        "Vec::<u8, A>::new(), n",
        "a<b, c>d",
        "a<b, c>(d)",
        "pair<K, V> p, q",
    ];
    for interior in corpus {
        for &t in &TARGETS {
            let out = machine_full(interior, t);
            let g = quot_args(&out.primary.args, out.primary.named);
            match &out.angles {
                AngleReading::Forked(alt) => {
                    let o = quot_args(&alt.args, alt.named);
                    assert!(
                        g.0.len() < o.0.len(),
                        "primary (G) must be strictly smaller than alt (O) on {interior:?} ({t:?})"
                    );
                }
                other => panic!("{interior:?} expected to fork under {t:?}, got {other:?}"),
            }
        }
    }
}

// ============================================================================
// FUZZ — 3 tests (§7.5 as amended §11.7). Deterministic xorshift64*; a failing
// seed reproduces from its number.
// ============================================================================

struct Rng(u64);
impl Rng {
    fn new(seed: u64) -> Rng {
        let mut s = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15).wrapping_add(0x1234_5678);
        if s == 0 {
            s = 0xDEAD_BEEF;
        }
        Rng(s)
    }
    fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next_u64() % (n as u64)) as usize
    }
}

/// Full-alphabet fragments: the carry alphabet PLUS angles, digraphs, comments, exotic
/// strings, bare sigils, stray closers, compound assigns.
const FULL_FRAGMENTS: &[&str] = &[
    "a", "bb", "x1", "0", "42", ", ", ",", " ", "\t", "\n", "==", "!=", " = ", "(y)",
    "[i]", "{j}", "\"s,t\"", "'c'", "$(x)", "$(ab=cd)", "$>(7)", "$()", "$(x) j", "$(q",
    "<", ">", " < ", " > ", "<=", ">=", "->", "=>", "<<", ">>", "Map<K, V>",
    "new HashMap<Integer, String>(), z", "a<b", "c>d", "/* c, */", "// c\n", "# p\n",
    "\"\"\"t\"\"\"", "r#\"r\"#", "\"unterm", ")", "]", "}", "+=", "pair<K, V> p, q",
];

fn gen_full_interior(rng: &mut Rng) -> String {
    let n = rng.below(10) + 1;
    let mut s = String::new();
    for _ in 0..n {
        s.push_str(FULL_FRAGMENTS[rng.below(FULL_FRAGMENTS.len())]);
    }
    s
}

#[test]
fn fuzz_full_invariants() {
    // PUBLIC-`parse()` invariants over the FULL alphabet — no oracle, no internal records.
    for seed in 0u64..8000 {
        let mut rng = Rng::new(seed ^ 0x51D3_FFFF);
        let interior = gen_full_interior(&mut rng);
        let b = interior.as_bytes();
        for &t in &TARGETS {
            // (1) no panic (running at all) + (2) determinism.
            let out = arg_scan::parse(b, 0, b.len(), t);
            let out2 = arg_scan::parse(b, 0, b.len(), t);
            assert_eq!(out, out2, "nondeterminism: seed {seed} {t:?} {interior:?}");

            // (3) a refusal suppresses the fork (a malformed list adjudicates nothing).
            if out.refusal != Refusal::None {
                assert_eq!(
                    out.angles,
                    AngleReading::Inert,
                    "refusal forked: seed {seed} {t:?} {interior:?}"
                );
            }

            // (4) a Forked reading carries a STRICT count reduction: primary (G) < alt (O).
            if let AngleReading::Forked(alt) = &out.angles {
                assert!(
                    out.primary.args.len() < alt.args.len(),
                    "fork without strict count reduction: seed {seed} {t:?} {interior:?}"
                );
            }
        }
    }
}

#[test]
fn fuzz_has_teeth() {
    // The #232-lie gate: a corpus that never reaches an outcome class proves nothing.
    // Every bound below must hold or the corpus lies.
    let mut multi_arg = 0usize;
    let mut named_args = 0usize;
    let mut group_args = 0usize;
    let mut refusal_counts = [0usize; 5];
    let mut forked = 0usize;
    let mut operators = 0usize;
    for seed in 0u64..8000 {
        let mut rng = Rng::new(seed ^ 0x51D3_FFFF);
        let interior = gen_full_interior(&mut rng);
        let b = interior.as_bytes();
        for &t in &TARGETS {
            let out = arg_scan::parse(b, 0, b.len(), t);
            if out.primary.args.len() > 1 {
                multi_arg += 1;
            }
            named_args += out.primary.args.iter().filter(|a| a.name.is_some()).count();
            group_args += out
                .primary
                .args
                .iter()
                .filter(|a| a.group != ParamGroup::Domain)
                .count();
            refusal_counts[match out.refusal {
                Refusal::None => 0,
                Refusal::UnterminatedOpaque => 1,
                Refusal::StrayCloser => 2,
                Refusal::TrailingAfterGroup => 3,
                Refusal::UnclosedGroup => 4,
            }] += 1;
            match out.angles {
                AngleReading::Forked(_) => forked += 1,
                AngleReading::Operators => operators += 1,
                AngleReading::Inert => {}
            }
        }
    }
    assert!(multi_arg > 200, "too few multi-arg outcomes ({multi_arg})");
    assert!(named_args > 200, "too few named args ({named_args})");
    assert!(group_args > 200, "too few group args ({group_args})");
    assert!(
        refusal_counts[1] > 50,
        "too few UnterminatedOpaque refusals ({})",
        refusal_counts[1]
    );
    assert!(refusal_counts[2] > 50, "too few StrayCloser refusals ({})", refusal_counts[2]);
    assert!(
        refusal_counts[3] > 50,
        "too few TrailingAfterGroup refusals ({})",
        refusal_counts[3]
    );
    assert!(forked > 50, "too few Forked outcomes ({forked})");
    assert!(operators > 200, "too few Operators outcomes ({operators})");
}

// ============================================================================
// MILESTONES — 3 tests (§7.7 as amended §11.7). End-to-end through the WIRED
// production path: `inst_scan::scan_node` (InstScan shape + ArgScan args — the
// one production seat, the same call `native_parts` makes), and the
// `native_parts` route guard.
// ============================================================================

#[test]
fn milestone_scan_node_spec_examples() {
    // The spec §1103/§1108 call sites through the real path (InstScan shape + ArgScan
    // args), all 4 targets.
    let cases: [(&str, &[(i32, Option<&str>, &str)], bool); 6] = [
        ("let a = @@Service(10);", &[(0, None, "10")], false),
        (
            "let b = @@Service($(7), \"R2D2\");",
            &[(1, None, "7"), (0, None, "\"R2D2\"")],
            false,
        ),
        ("let c = @@Service($>(50));", &[(2, None, "50")], false),
        (
            "let d = @@Service($(0), $>(1000), \"primary\");",
            &[(1, None, "0"), (2, None, "1000"), (0, None, "\"primary\"")],
            false,
        ),
        (
            "let e = @@Service($(x=7), name=\"R2D2\");",
            &[(1, Some("x"), "7"), (0, Some("name"), "\"R2D2\"")],
            true,
        ),
        (
            "let f = @@Service($(slot=0), $>(timeout=1000), name=\"primary\");",
            &[
                (1, Some("slot"), "0"),
                (2, Some("timeout"), "1000"),
                (0, Some("name"), "\"primary\""),
            ],
            true,
        ),
    ];
    for (water, rows, named) in cases {
        let bytes = water.as_bytes();
        let at = water.find("@@").unwrap();
        for &t in &TARGETS {
            let node = inst_scan::scan_node(bytes, at, t)
                .unwrap_or_else(|| panic!("no instantiation in {water:?}"));
            assert_eq!(node.name, "Service");
            assert_eq!(
                quot_args(&node.args, node.named),
                expect(rows, named),
                "spec example through the real path: {water:?} under {t:?}"
            );
            assert_eq!(node.angles, ArgAngles::Inert);
            assert_eq!(&water[node.span.start..node.span.end], &water[at..water.len() - 1]);
        }
    }
}

#[test]
fn milestone_native_parts_route() {
    // `native_parts` still produces the same (kind, span) partition on
    // instantiation-bearing water — guards the parts.rs:59 seam across the staging.
    let water = "a = @@Counter(10); b";
    for &t in &TARGETS {
        let bytes = water.as_bytes();
        let parts: Vec<(i32, usize, usize)> = native_parts(bytes, 0, bytes.len(), t)
            .iter()
            .map(|p| match p {
                NativePart::Text(x) => (0, x.span.start, x.span.end),
                NativePart::Literal(l) => (1, l.span.start, l.span.end),
                NativePart::Ref(r) => (2, r.span.start, r.span.end),
                NativePart::Instantiate(i) => (3, i.span.start, i.span.end),
                NativePart::EmbedCall(e) => (4, e.span.start, e.span.end),
            })
            .collect();
        assert_eq!(
            parts,
            vec![(0, 0, 4), (3, 4, 17), (0, 17, 20)],
            "native_parts partition moved under {t:?}"
        );
    }
}

#[test]
fn milestone_fork_reaches_tree() {
    // The fork RIDES THE TREE: through the real path the node carries both candidates and
    // the declared-arity adjudicator downstream can pick.
    let water = "x = @@M(new HashMap<Integer, String>(), z);";
    let bytes = water.as_bytes();
    let at = water.find("@@").unwrap();
    for &t in &TARGETS {
        let node = inst_scan::scan_node(bytes, at, t).expect("instantiation");
        assert_eq!(node.name, "M");
        assert_eq!(node.args.len(), 2, "primary (G) count under {t:?}");
        match &node.angles {
            ArgAngles::Forked {
                alt_args,
                alt_named,
            } => {
                assert_eq!(alt_args.len(), 3, "alt (O) count under {t:?}");
                assert!(!alt_named);
            }
            other => panic!("expected Forked in the tree, got {other:?}"),
        }
    }
}
