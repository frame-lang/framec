//! **ArgScan agrees with the hand `parse_inst_args` everywhere the ledger says CARRY —
//! and provably DISAGREES on every FIX ruling — proven by running.**
//!
//! `arg_scan::parse` is generated from `arg_scan.frs` (a `@@[scan(u8)]` TWO-counter
//! automaton, design record §11 Option C) and replaces the hand `parse_inst_args` +
//! `split_top_commas` + `split_top_eq` (parts.rs M6). The battery:
//!
//!   * PIN tests (§7.2 as amended §11.7, 22) — CARRY ledger rows, hand-independent
//!     expected values (they survive oracle deletion at C-final), each also cross-checked
//!     `== hand` while the oracle lives, and `angles == Inert`;
//!   * FIX tests (§7.3 as amended, 18) — every FIX ledger row asserts the new behavior
//!     AND `!= hand` (teeth built in — the recorded D4-shape exception: fix-at-landing
//!     via the partitioned carry/fix differential);
//!   * FORK tests (§11.7, 9) — the dual-counter angle fork: Forked/Operators/Inert
//!     outcomes, digraph guards, depth-0 independence, refusal suppression, and the
//!     scaffolding differential `fork_g_matches_hand_on_balanced_angles` (the hand's
//!     comma alphabet IS hypothesis G);
//!   * DIFFERENTIAL (2) — the carry-domain corpus sweep + `oracle_stayed_buggy` (pins the
//!     oracle's Bug A/Bug B so the fix teeth can never go vacuous via an oracle "repair");
//!   * FUZZ (3) — deterministic xorshift64*: a carry-domain-by-construction differential
//!     arm, a full-alphabet invariants arm, and the teeth gate;
//!   * MILESTONES (3) — the spec §1103/§1108 call sites through the wired production
//!     path (InstScan shape + ArgScan args), the `native_parts` route guard, and the fork
//!     reaching the tree.
//!
//! Every parity test here is SCAFFOLDING: it depends on the `#[doc(hidden)]`
//! `parse_inst_args_hand` oracle (and `parse_records`); it NEVER promotes to the
//! cross-language corpus and dies at C-final when the hand-independent pins take over.

use frame_compiler::text::scan::arg_scan::{self, AngleReading, ArgsOut, Refusal};
use frame_compiler::text::scan::inst_scan;
use frame_compiler::text::scan::literals::Target;
use frame_compiler::text::scan::parts::{native_parts, parse_inst_args_hand};
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

/// The hand oracle on the whole interior `[0, len)`.
fn hand(interior: &str) -> Quot {
    let b = interior.as_bytes();
    let (args, named) = parse_inst_args_hand(b, 0, b.len());
    quot_args(&args, named)
}
fn hand_bytes(interior: &[u8]) -> Quot {
    let (args, named) = parse_inst_args_hand(interior, 0, interior.len());
    quot_args(&args, named)
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

/// CARRY assertion: machine == the hand-independent expected values, == hand, and
/// `angles == Inert`, under EVERY target (carry rows are target-invariant).
fn pinned(interior: &str, rows: &[(i32, Option<&str>, &str)], named: bool) {
    let e = expect(rows, named);
    for &t in &TARGETS {
        let full = machine_full(interior, t);
        let m = quot_args(&full.primary.args, full.primary.named);
        assert_eq!(m, e, "pin wrong: {interior:?} under {t:?}");
        assert_eq!(m, hand(interior), "carry broke (!= hand): {interior:?} under {t:?}");
        assert_eq!(
            full.angles,
            AngleReading::Inert,
            "carry input not Inert: {interior:?} under {t:?}"
        );
    }
}

/// FIX assertion (teeth built in): machine == the expected NEW behavior and != the oracle.
fn fixed(interior: &str, t: Target, rows: &[(i32, Option<&str>, &str)], named: bool) {
    let m = machine(interior, t);
    assert_eq!(m, expect(rows, named), "fix wrong: {interior:?} under {t:?}");
    assert_ne!(
        m,
        hand(interior),
        "fix VACUOUS (oracle already agrees): {interior:?} under {t:?}"
    );
}
fn fixed_all(interior: &str, rows: &[(i32, Option<&str>, &str)], named: bool) {
    for &t in &TARGETS {
        fixed(interior, t, rows, named);
    }
}

// ============================================================================
// PIN battery — 22 tests (§7.2 as amended §11.7). CARRY rows: hand-independent
// expected values + `== hand` cross-check + `angles == Inert`.
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
    // BEYOND-DESIGN FIND (proven by running, recorded for the ledger): the hand's comma
    // splitter counts the `>` OF THE `$>(` SIGIL ITSELF (its parts.rs:400-402 alphabet),
    // so a non-final `$>(...)` group drives its depth negative and SILENCES every later
    // top-level comma — spec examples 4 and 6 are Bug-B territory, not carry (the same
    // §7.2 pin-input defect class the design's own L26 correction caught for `<=`). The
    // machine's spec-documented values stand, hand-independent; the hand cross-check
    // becomes a Bug-B TOOTH (`!= hand`): the oracle yields 2 args with the Enter value
    // `1000), "primary"`.
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
            assert_ne!(
                m,
                hand(interior),
                "enter-sigil Bug-B tooth vacuous (oracle repaired?): {interior:?}"
            );
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
        assert_eq!(m, hand("',' , x"), "carry broke: char comma under {t:?}");
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
        assert_eq!(m, hand_bytes(interior), "lossy carry broke under {t:?}");
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
            assert_ne!(m, hand(interior), "digraph tooth vacuous: {interior:?}");
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

#[test]
fn fork_g_matches_hand_on_balanced_angles() {
    // SCAFFOLDING differential: on digraph-free fork inputs with no `=` inside an angle
    // extent (gate amendment domain restriction — the hand names by O rules), the hand's
    // comma alphabet IS hypothesis G, so the G candidate == hand exactly.
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
            assert!(
                matches!(out.angles, AngleReading::Forked(_)),
                "{interior:?} expected to fork under {t:?}"
            );
            assert_eq!(g, hand(interior), "hand != hypothesis G on {interior:?} under {t:?}");
        }
    }
}

// ============================================================================
// DIFFERENTIAL — 2 tests (§7.4 as amended). Carry-domain sweep + the oracle-
// faithfulness pin that keeps the fix teeth honest.
// ============================================================================

#[test]
fn differential_carry_corpus() {
    // Every §7.2 carry input plus a curated widening. Whole-interior calls only (chopping
    // interiors manufactures fix-territory inputs); breadth comes from the fuzz carry arm.
    // Carry rows additionally assert `angles == Inert` (§11.7).
    let corpus = [
        // §7.2 inputs. (Spec examples 4 and 6 — non-final `$>(...)` — are EXCLUDED: the
        // hand counts the sigil's own `>` and silences the later commas, a Bug-B family
        // divergence proven by running; see pin_spec_examples. A FINAL `$>(...)` is
        // carry — `x, $>(9)` below keeps the enter group in the carry sweep.)
        "10",
        "$(7), \"R2D2\"",
        "$>(50)",
        "x, $>(9)",
        "$(x=7), name=\"R2D2\"",
        "",
        "   ",
        "a,",
        "a, b",
        "a,,b",
        ",",
        ",,",
        "a, ",
        "f(a, b), [c, d], {e, f}",
        "\"a=b\", x",
        "\"a,b\", c",
        "a, \"b, c",
        "a=b=c",
        "f(x)",
        "a==b, x",
        "c!=d, y",
        "$()",
        "$ (x)",
        "f($(x))",
        "$(x",
        "a], y",
        "$(x) junk",
        "x=1, 2",
        // curated widening: nested instantiations, adjacent groups, deep parens, carry
        // guard pairs, empty variants
        "@@Inner(), 3",
        "$(1), $(2), x",
        "f(g(h(1,2),3),4), y",
        "p != q, r == s",
        "x = a == b",
        "x = y = z",
        "$(1, 2)",
        "$(a + b), c",
        "'q', \"w\"",
        "  a  ,  b  ",
        "_x, y1",
    ];
    for interior in corpus {
        for &t in &TARGETS {
            let full = machine_full(interior, t);
            assert_eq!(
                quot_args(&full.primary.args, full.primary.named),
                hand(interior),
                "carry divergence on {interior:?} under {t:?}"
            );
            assert_eq!(
                full.angles,
                AngleReading::Inert,
                "carry input forked: {interior:?} under {t:?}"
            );
        }
    }
}

#[test]
fn oracle_stayed_buggy() {
    // The fix teeth (`assert_ne!` vs the oracle) go VACUOUS if anyone "repairs" the
    // oracle. Pin the oracle's verified bugs so that any repair shape is loud.
    // Bug A: the suffix trim eats the user's own closer.
    let (args, _) = {
        let b = b"$(g(1))";
        parse_inst_args_hand(b, 0, b.len())
    };
    assert_eq!(args.len(), 1);
    assert_eq!(args[0].value, "g(1", "oracle Bug A was fixed — the fix teeth are vacuous");
    // Bug B: a bare `<` silences every later top-level comma.
    let (args, _) = {
        let b = b"a < b, c";
        parse_inst_args_hand(b, 0, b.len())
    };
    assert_eq!(args.len(), 1, "oracle Bug B was fixed — the fix teeth are vacuous");
    // Bug B, enter-sigil instance (the wire-gate C2 pin): the comma splitter counts the
    // `>` OF THE `$>(` SIGIL ITSELF, so a non-final enter group drives depth negative and
    // silences the later commas — spec-4's input mangles to 2 args with the Enter value
    // `1000), "primary"`.
    let (args, named) = {
        let b = b"$(0), $>(1000), \"primary\"";
        parse_inst_args_hand(b, 0, b.len())
    };
    assert_eq!(
        args.len(),
        2,
        "oracle enter-sigil bug was fixed — the spec-4/6 teeth are vacuous"
    );
    assert_eq!(args[1].group, ParamGroup::Enter);
    assert_eq!(
        args[1].value, "1000), \"primary\"",
        "oracle enter-sigil bug changed shape — re-verify the spec-4/6 teeth"
    );
    assert!(!named);
    // Spec-6's named shape: the swallowed tail rides the Enter value through the eq
    // splitter — name `timeout`, value `1000), name="primary"`.
    let (args, named) = {
        let b = b"$(slot=0), $>(timeout=1000), name=\"primary\"";
        parse_inst_args_hand(b, 0, b.len())
    };
    assert_eq!(args.len(), 2);
    assert_eq!(args[1].group, ParamGroup::Enter);
    assert_eq!(args[1].name.as_deref(), Some("timeout"));
    assert_eq!(args[1].value, "1000), name=\"primary\"");
    assert!(named);
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
    fn chance(&mut self, one_in: usize) -> bool {
        self.below(one_in) == 0
    }
}

/// Carry-domain VALUE fragments: everything a positional value or a named value may be
/// built from without leaving the carry domain (no angles, no comments, no raw/triple
/// openers, no operator-adjacent `=`, no unbalanced brackets).
const CARRY_VALUES: &[&str] = &[
    "a", "x1", "_id", "foo", "0", "42", "a == b", "c != d", "(y)", "[i, j]", "{k}",
    "\"s,t\"", "'c'", "\"e\\\"q\"", "f(1, 2)", "g(h(1))", "n + m",
];
/// Carry-domain GROUP segments: fixed closed STATE sigil groups. No nested parens ENDING
/// at the closer (Bug A territory), no quotes (unterminated-in-group diverges on the hand
/// trim), and NO `$>(...)` here — the hand counts the enter sigil's own `>` (Bug-B family,
/// proven by running), so an enter group is carry only in FINAL position (appended by the
/// interior generator).
const CARRY_GROUPS: &[&str] = &["$(x)", "$(ab=cd)", "$()", "$(1, 2)", "$(a + b)"];
/// Names for carry named segments (proper identifiers only — the hand names ANY LHS, the
/// machine requires an ident; proper idents keep both in agreement).
const CARRY_NAMES: &[&str] = &["a", "slot", "x9", "_n"];

/// One carry-domain segment.
fn gen_carry_segment(rng: &mut Rng, out: &mut String) {
    match rng.below(4) {
        0 => {
            // named: ident = value [value]
            out.push_str(CARRY_NAMES[rng.below(CARRY_NAMES.len())]);
            out.push_str(" = ");
            out.push_str(CARRY_VALUES[rng.below(CARRY_VALUES.len())]);
        }
        1 => out.push_str(CARRY_GROUPS[rng.below(CARRY_GROUPS.len())]),
        _ => {
            out.push_str(CARRY_VALUES[rng.below(CARRY_VALUES.len())]);
            if rng.chance(3) {
                out.push(' ');
                out.push_str(CARRY_VALUES[rng.below(CARRY_VALUES.len())]);
            }
        }
    }
}

/// A whole carry-domain interior: segments joined by commas (with ws jitter), an optional
/// empty segment, an optional unterminated-quote tail (L5 is carry).
fn gen_carry_interior(rng: &mut Rng) -> String {
    let n = rng.below(5);
    let mut s = String::new();
    for i in 0..n {
        if i > 0 {
            s.push(',');
            if rng.chance(4) {
                s.push(','); // empty segment (dropped, carry)
            }
            if !rng.chance(3) {
                s.push(' ');
            }
        }
        gen_carry_segment(rng, &mut s);
        if rng.chance(5) {
            s.push(' ');
        }
    }
    if rng.chance(8) {
        if !s.is_empty() {
            s.push_str(", ");
        }
        s.push_str("\"tail, swallowed"); // unterminated tail — L5, carry
    } else if rng.chance(5) {
        // A FINAL-position enter group is carry (no later comma for the hand's counted
        // sigil-`>` to silence).
        if !s.is_empty() {
            s.push_str(", ");
        }
        s.push_str("$>(7)");
    }
    s
}

#[test]
fn fuzz_carry_differential() {
    // The fragment alphabet is CONSTRUCTED to stay in the carry domain, so machine == hand
    // (and Inert) on every case — breadth for the differential that the curated corpus
    // cannot reach.
    for seed in 0u64..8000 {
        let mut rng = Rng::new(seed ^ 0xA96C_0000);
        let interior = gen_carry_interior(&mut rng);
        let h = hand(&interior);
        for &t in &TARGETS {
            let full = machine_full(&interior, t);
            assert_eq!(
                quot_args(&full.primary.args, full.primary.named),
                h,
                "CARRY FUZZ DIVERGENCE seed {seed} target {t:?} interior {interior:?}"
            );
            assert_eq!(
                full.angles,
                AngleReading::Inert,
                "carry fuzz forked: seed {seed} {interior:?}"
            );
        }
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

/// Re-derive the O arg count and the G arg count from the raw records.
fn record_counts(recs: &[(i32, bool, usize, usize, usize, usize, bool)]) -> (usize, usize) {
    (recs.len(), recs.iter().filter(|r| r.6).count())
}

/// Does the ws byte at `pos` sit INSIDE an opaque extent opening within `[vs, pos]`? A
/// comment/string legitimately carries interior/trailing ws into the value span — the
/// walk sets `ve` to the opaque extent end VERBATIM (same clamp policy as the machine's
/// `opaque_skip`: comments clamp to the limit, literals overrunning it are not consumed).
fn ws_inside_opaque(b: &[u8], vs: usize, pos: usize, to: usize, t: Target) -> bool {
    use frame_compiler::text::scan::opaque_scan::{opaque_at, OpaqueAt};
    let mut k = vs;
    while k <= pos {
        let e = match opaque_at(b, k, t) {
            OpaqueAt::Comment(e) => Some(e.min(to)),
            OpaqueAt::Literal(e) if e <= to => Some(e),
            _ => None,
        };
        match e {
            Some(e) if e > k => {
                if e > pos {
                    return true;
                }
                k = e;
            }
            _ => k += 1,
        }
    }
    false
}

#[test]
fn fuzz_full_invariants() {
    // Machine-only structural invariants over the FULL alphabet — no oracle.
    for seed in 0u64..8000 {
        let mut rng = Rng::new(seed ^ 0x51D3_FFFF);
        let interior = gen_full_interior(&mut rng);
        let b = interior.as_bytes();
        for &t in &TARGETS {
            // (1) no panic (running at all) + (2) determinism.
            let out = arg_scan::parse(b, 0, b.len(), t);
            let out2 = arg_scan::parse(b, 0, b.len(), t);
            assert_eq!(out, out2, "nondeterminism: seed {seed} {t:?} {interior:?}");

            let (recs, (angle_touched, g_viable, refusal, _)) =
                arg_scan::parse_records(b, 0, b.len(), t);

            // (3) span sanity: within [0, len), ordered, non-overlapping, name span before
            // value span, no nonempty span starting/ending on ws.
            let is_ws = |i: usize| matches!(b[i], b' ' | b'\t' | b'\n' | b'\r');
            let mut prev_end = 0usize;
            for r in &recs {
                let (_, has_name, ns, ne, vs, ve, _) = *r;
                assert!(vs <= ve && ve <= b.len(), "value span oob: seed {seed} {interior:?}");
                if has_name {
                    assert!(ns < ne && ne <= vs, "name span disorder: seed {seed} {interior:?}");
                    assert!(
                        !is_ws(ns) && !is_ws(ne - 1),
                        "name span on ws: seed {seed} {interior:?}"
                    );
                    assert!(prev_end <= ns, "records overlap: seed {seed} {interior:?}");
                } else {
                    assert!(prev_end <= vs, "records overlap: seed {seed} {interior:?}");
                }
                if vs < ve {
                    assert!(!is_ws(vs), "value span starts on ws: seed {seed} {interior:?}");
                    assert!(
                        !is_ws(ve - 1) || ws_inside_opaque(b, vs, ve - 1, b.len(), t),
                        "value span ends on ws outside opaque: seed {seed} {t:?} {interior:?}"
                    );
                }
                prev_end = ve;
            }

            // (5) coverage: every interior byte outside the recorded spans is separator
            // material (ws, comma, the `=` of a named split, sigil bytes, group closer).
            let mut covered = vec![false; b.len()];
            for r in &recs {
                for c in covered.iter_mut().take(r.5).skip(r.4) {
                    *c = true;
                }
                if r.1 {
                    for c in covered.iter_mut().take(r.3).skip(r.2) {
                        *c = true;
                    }
                }
            }
            for i in 0..b.len() {
                if !covered[i] {
                    assert!(
                        matches!(b[i], b' ' | b'\t' | b'\n' | b'\r' | b',' | b'=' | b'$' | b'(' | b')' | b'>'),
                        "uncovered non-separator byte {:?} at {i}: seed {seed} {interior:?}",
                        b[i] as char
                    );
                }
            }

            // (4) refusal implications: fork suppressed; the verbatim tail is observable —
            // the last arg extends to the last non-ws byte of the interior.
            if out.refusal != Refusal::None {
                assert_eq!(out.angles, AngleReading::Inert, "(9) refusal forked: {interior:?}");
                let last_content = (0..b.len()).rev().find(|&i| !is_ws(i));
                if let (Some(lc), Some(last)) = (last_content, recs.last()) {
                    if last.4 < last.5 {
                        // Covers AT LEAST to the last non-ws byte (an opaque extent may
                        // legitimately clamp `ve` past it, to `to`) — nothing after the
                        // refusal is dropped.
                        assert!(
                            last.5 >= lc + 1,
                            "(4) refusal tail not verbatim-to-end: seed {seed} {t:?} {interior:?}"
                        );
                    }
                }
            }

            // (6)/(7)/(8): fork-shape invariants re-derived from the records.
            let (o_count, g_count) = record_counts(&recs);
            match &out.angles {
                AngleReading::Forked(alt) => {
                    assert_eq!(alt.args.len(), o_count, "alt is not O: seed {seed}");
                    assert_eq!(out.primary.args.len(), g_count, "primary is not G: seed {seed}");
                    assert!(
                        out.primary.args.len() < alt.args.len(),
                        "(6) fork without strict count reduction: seed {seed} {interior:?}"
                    );
                    assert!(g_viable && angle_touched && refusal == 0);
                }
                AngleReading::Operators => {
                    assert!(angle_touched && !g_viable, "Operators shape: seed {seed}");
                    assert_eq!(out.primary.args.len(), o_count);
                }
                AngleReading::Inert => {
                    // (8) Inert => the hypotheses agree: either no counted angles, or every
                    // boundary is shared (all g_end), or the list refused.
                    assert!(
                        !angle_touched || refusal != 0 || o_count == g_count,
                        "(8) Inert with diverged boundaries: seed {seed} {interior:?}"
                    );
                }
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
    let mut ne_hand = 0usize;
    let mut eq_hand = 0usize;
    for seed in 0u64..8000 {
        let mut rng = Rng::new(seed ^ 0x51D3_FFFF);
        let interior = gen_full_interior(&mut rng);
        let b = interior.as_bytes();
        let h = hand(&interior);
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
            if quot_args(&out.primary.args, out.primary.named) == h {
                eq_hand += 1;
            } else {
                ne_hand += 1;
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
    assert!(ne_hand > 100, "the fixes are barely exercised ({ne_hand})");
    assert!(eq_hand > 500, "the carries are barely exercised ({eq_hand})");
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
