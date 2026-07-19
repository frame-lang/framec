//! **The state-head reader, as a system, agrees with the hand scan-lets — proven by running.**
//! SCAFFOLDING (differential vs the new-factored hand oracle; conversion-internal — never
//! promoted; needs `@@[scan(u8)]`-on-`@@system` plus the hand oracle it is racing).
//!
//! `state_head_scan::scan` is generated from `state_head_scan.frs`, a `@@[scan(u8)]` TOTAL
//! register transducer (Item 3e, `_scratch/headreader_design.md`) — `$Dollar → $Name → $Params
//! → $ParentSeek → $ParentName → $ParentIdent → $SeekOpen → $Body → $Accept`, no `$Reject`
//! (the MachineWalk's `is_state_start` did the gating; malformedness is REGISTERS:
//! `params_unbalanced` = T-S6, `open_found == false` = T-S2, `body_clamped` = T-S1). This
//! proves — by running — that one fused machine pass equals the hand's INDEPENDENT scan-lets
//! (`state_head_hand`: `state_extent`'s name skip + open seek + clamp, `state()`'s params
//! scan and parent hunt, factored verbatim) as FULL `StateHeadParts` equality at EVERY
//! `(at, limit)` position pair with `at < limit <= len` (the callers' position contract,
//! T-S8 — the CONTENT at `at` is unconstrained and swept off-contract too), for all four
//! cleanroom targets, over realistic AND adversarial heads AND a deterministic fuzz corpus.
//! The parts carry OFFSETS, not Strings, so the T-S9 empty-parent (`Some("")`) case stays
//! distinguishable from no-parent. PHASE 1 IS BYTE-PARITY: the hand path's bugs are
//! REPRODUCED and PINNED; fixes are recorded Phase-2 deltas (D1 opaque-aware seeks, D2
//! params-skipping parent hunt, D3 limit-bounded probe).
//!
//! LEDGER ROSTER (design §5/§7 — one test per Phase-1 row):
//!   T-S1 unbalanced body clamps       -> `t_s1_unbalanced_body_clamps_pinned` (register +
//!        exact clamp; also records the H2 state-side close_node artifact: `close = end - 1`
//!        lands on a non-`}` byte).
//!   T-S2 no `{` before limit          -> `t_s2_no_brace_tail_pinned` (`open == end == limit`,
//!        `!open_found`; the driver's `(at, limit + 1)` header-span overrun is a DRIVER
//!        artifact, recorded in that test's comment — out of this lane's scope).
//!   T-S3 seek opaque-aware (D1 LANDED) -> `t_s3_opaque_seek_directed` (P2 directed; replaced
//!        `t_s3_brace_in_comment_pinned` + `pin_arrow_in_comment_phantom_parent`/H1). The seeks
//!        route through the shared `skip` (OpaqueScan) leaf; a `{`/`=>` in a comment/string no
//!        longer steers the head. Opacity is target-sensitive (Python has no `/* */`).
//!   T-S4 parent on a later line       -> `t_s4_parent_next_line_none_pinned` (grammar fact).
//!   T-S5 parent hunt scans the params -> `t_s5_arrow_in_params_pinned` (P1 pin, both faces:
//!        the phantom parent from a default AND the lost real parent after `)`; Phase-2 D2).
//!   T-S6 unbalanced params dropped    -> `t_s6_unbalanced_params_named_pinned` (register).
//!   T-S7 malformed parent stops hunt  -> `t_s7_malformed_parent_stops_hunt_pinned`.
//!   T-S8 off-contract `at`            -> no test (position precondition, documented in the
//!        mod docs; content-off-contract positions ARE swept by the rectangles).
//!   T-S9 limit-straddle parent probe  -> `t_s9_limit_straddle_pinned` (P1 pin; Phase-2 D3).

use frame_compiler::text::scan::literals::Target;
use frame_compiler::text::scan::machine::state_extent;
use frame_compiler::text::scan::state_head_scan::{scan, state_head_hand, StateHeadParts};

const TARGETS: [Target; 4] = [Target::C, Target::Java, Target::Rust, Target::Python3];

/// The differential: FULL `StateHeadParts` struct equality (offsets + flags) between the
/// fused machine pass and the hand's independent scan-lets, at this exact `(at, limit)`.
fn agree(bytes: &[u8], at: usize, limit: usize, target: Target) -> StateHeadParts {
    let machine = scan(bytes, at, limit, target);
    let hand = state_head_hand(bytes, at, limit, target);
    assert_eq!(
        machine,
        hand,
        "MISMATCH target {target:?} at={at} limit={limit} on {:?}",
        String::from_utf8_lossy(bytes),
    );
    machine
}

/// Every-position sweep over the callers' position contract: every `at` in `0..len`, every
/// `limit` in `at+1..=len` (T-S8: `at < limit <= len`; the CONTENT at `at` is unconstrained,
/// so positions mid-name, mid-params, mid-string and mid-comment are covered by construction).
fn sweep_all_positions(src: &str) {
    let b = src.as_bytes();
    let len = b.len();
    for target in TARGETS {
        for at in 0..len {
            for limit in (at + 1)..=len {
                agree(b, at, limit, target);
            }
        }
    }
}

// ===========================================================================
// The curated corpus. Each string is a MACHINE-SECTION fragment whose head the
// reader is pointed at (`at` = the `$` in production; the rectangle sweeps all
// positions). Enumerated from `state_head_scan.frs` + the hand `state()` /
// `state_extent`: `$Name`, optional ADJACENT `(params)`, optional first-line
// `=> $Parent`, the body `{` (crossing newlines), the balanced body.
// ===========================================================================

/// Realistic, well-formed state heads.
const REALISTIC: &[&str] = &[
    "$A { }",
    "$A{ }",
    "$Idle {\n    go() { }\n}",
    "$B(n: int) { }",
    "$B(n: int, m: str) => $A { }",
    "$Child => $Parent {\n    $>() { }\n}",
    "$S(f: int = 0) {\n    tick(): bool { return true }\n}",
    "$A\n{ }",
    "$A =>$P { }",
    "$Deep { go() { if p { q } else { r } } }",
    "$_x { }",
];

/// Adversarial heads — opaque decoys, unterminated bodies, arrow noise, malformed groups.
const ADVERSARIAL: &[&str] = &[
    // bare tails (T-S2)
    "$A",
    "$",
    "$A => $P",
    "$A(x",
    // unterminated bodies (T-S1)
    "$A { go() {",
    "$A => $P { x ",
    // opaque decoys steering the seeks (T-S3 / H1, carried Phase 1)
    "$A /* { */ { go(){} }",
    "$A /* => $X */ { }",
    "$A \"{\" { }",
    "$A // {\n{ }",
    // parent-hunt edges (T-S4, T-S5, T-S7, T-S9 fodder)
    "$A\n=> $P { }",
    "$S(f: cb = a => $b) { }",
    "$S(f: cb = a => b) => $Real { }",
    "$A => x => $B { }",
    "$A => $ { }",
    "$A => { }",
    "$A => $9 { }",
    "$A => $Px { }",
    "$A => \nx { }",
    // malformed groups (T-S6) and name edges
    "$A(( ) { go(){} }",
    "$1x { }",
    "$ { }",
    "$A ()) { }",
];

#[test]
fn differential_every_position_realistic() {
    for src in REALISTIC {
        sweep_all_positions(src);
    }
}

#[test]
fn differential_every_position_adversarial() {
    for src in ADVERSARIAL {
        sweep_all_positions(src);
    }
}

// ===========================================================================
// One directed test per Phase-1 ledger row. Every pin ALSO asserts machine ==
// hand at the pinned position (the pin is a statement about TODAY'S truth).
// ===========================================================================

/// T-S1 (carry-and-name): an unbalanced state body clamps `end` to `limit` — byte-exact with
/// the hand `unwrap_or(limit)` — and the clamp is now NAMED (`body_clamped`). Gate amendment
/// H2 (recorded): when this fires, the DRIVER (`state()`, unchanged) builds `close = end - 1`,
/// so `close_node` spans `[limit-1, limit)` over a byte that is NOT `}` — the state-side
/// analogue of the T-H5 artifact; the register makes it observable for the diagnostics pass.
#[test]
fn t_s1_unbalanced_body_clamps_pinned() {
    let src = b"$A { go() {";
    for t in TARGETS {
        let p = agree(src, 0, src.len(), t);
        assert!(p.open_found, "the body `{{` IS found for {t:?}");
        assert_eq!(p.open, 3);
        assert!(p.body_clamped, "the T-S1 register must fire for {t:?}");
        assert_eq!(p.end, src.len(), "the clamp value is limit, byte-exact for {t:?}");
        // The H2 driver artifact, demonstrated: close = end - 1 lands on a non-`}` byte.
        assert_ne!(src[p.end - 1], b'}', "the close_node byte is NOT a `}}` (H2, recorded)");
    }
    // Non-vacuity control: a balanced body does not fire it.
    for t in TARGETS {
        assert!(!agree(b"$A { go() {} }", 0, 14, t).body_clamped);
    }
}

/// T-S2 (carry-and-name): no `{` before `limit` -> `open == end == limit`, NAMED
/// (`open_found` stays false). The hand driver then builds `header_node` span
/// `(at, limit + 1)` — ONE PAST the section (a DRIVER artifact, recorded in the design;
/// fixing it would change the tree, out of this lane's scope).
#[test]
fn t_s2_no_brace_tail_pinned() {
    for src in [&b"$A"[..], &b"$A => $P"[..]] {
        for t in TARGETS {
            let p = agree(src, 0, src.len(), t);
            assert!(!p.open_found, "no `{{` -> open_found stays false for {t:?}");
            assert_eq!(p.open, src.len(), "open == limit for {t:?}");
            assert_eq!(p.end, src.len(), "end == limit for {t:?}");
            assert!(!p.body_clamped, "the clamp register is for FOUND opens only");
        }
    }
    // `$A => $P` still reads the parent even with no body (the hunt ran first).
    for t in TARGETS {
        let p = agree(b"$A => $P", 0, 8, t);
        assert!(p.has_parent);
        assert_eq!((p.parent_start, p.parent_end), (7, 8));
    }
}

/// T-S3 + H1 (fix P2 D1): the `{`/`=>` seeks are now OPAQUE-AWARE — they route through the
/// shared `skip` (OpaqueScan) leaf, so a `{` or `=>`/`$` trapped in a comment/string no longer
/// steers the head. Replaces `t_s3_brace_in_comment_pinned` and `pin_arrow_in_comment_phantom_parent`.
/// The machine and the oracle move together (shared `skip`), so the differential stays LOCKED;
/// this test pins the NEW behavior. Opacity is TARGET-SENSITIVE (the OpaqueScan policy): a
/// double-quoted string is opaque in EVERY target, but `/* */` block comments exist only in
/// C/Java/Rust — Python3 has none — so the comment cases are asserted per target, which
/// documents the policy honestly.
#[test]
fn t_s3_opaque_seek_directed() {
    // A `{` AND a `=> $X` trapped in a STRING literal — opaque in every target — no longer
    // steer the head: the seek skips the whole string and opens at the REAL `{`.
    let s = b"$A \"{ => $X\" { }";
    for t in TARGETS {
        let p = agree(s, 0, s.len(), t);
        assert!(!p.has_parent, "the in-string `=> $X` is NOT read for {t:?}");
        assert_eq!(p.open, 13, "open = the real `{{` (the string's `{{` is skipped) for {t:?}");
        assert!(p.open_found && !p.body_clamped, "the real body balances cleanly for {t:?}");
        assert_eq!(p.end, 16);
    }
    // A `{` trapped in a `/* */` block comment: skipped in C/Java/Rust (body opens at the real
    // `{`); Python3 has no block comments, so the text's first `{` (byte 6) wins and the
    // mis-open never balances (the old pinned behavior, now Python-only).
    let c = b"$A /* { */ { }";
    for t in TARGETS {
        let p = agree(c, 0, c.len(), t);
        if t == Target::Python3 {
            assert_eq!(p.open, 6, "no block comments in Python — the text's `{{` (6) wins");
            assert!(p.body_clamped, "the mis-open never balances for Python3");
        } else {
            assert_eq!(p.open, 11, "the comment is skipped; the body opens at the real `{{`");
            assert!(!p.body_clamped);
            assert_eq!(p.end, 14);
        }
    }
    // H1: a `=> $X` trapped in a block comment. C/Java/Rust skip it (NO phantom parent);
    // Python3 (no block comment) still reads the phantom `X`. The body opener (15) is the same
    // either way — only the parent outcome differs by target.
    let a = b"$A /* => $X */ { }";
    for t in TARGETS {
        let p = agree(a, 0, a.len(), t);
        assert_eq!((p.open, p.end), (15, 18), "the real body opener is target-invariant for {t:?}");
        if t == Target::Python3 {
            assert!(p.has_parent, "Python has no block comment — the arrow IS read");
            assert_eq!(&a[p.parent_start..p.parent_end], b"X", "phantom parent `X` for Python3");
        } else {
            assert!(!p.has_parent, "the in-comment arrow is skipped for {t:?} — NO phantom parent");
        }
    }
}

/// T-S4 (carry): the parent arrow lives on the header's FIRST line — `$A\n=> $P { }` has NO
/// parent (the hunt stops at the newline; the `{` seek then crosses it). Grammar fact, pinned.
#[test]
fn t_s4_parent_next_line_none_pinned() {
    let src = b"$A\n=> $P { }";
    for t in TARGETS {
        let p = agree(src, 0, src.len(), t);
        assert!(!p.has_parent, "a later-line arrow is silently no-parent for {t:?}");
        assert_eq!(p.open, 9, "the `{{` seek crosses the newline");
        assert_eq!(p.end, 12);
    }
}

/// T-S5 (carry P1 -> fix P2 D2): the parent hunt starts at `name_end` and scans THROUGH the
/// params group. Both faces pinned: (a) `=> $b` inside a param default is read as a PHANTOM
/// parent `b`; (b) the in-params arrow consumes the hunt's single `break`, so a REAL
/// `=> $Real` after the `)` is LOST. Phase-2 D2 starts the hunt at `params_close`.
#[test]
fn t_s5_arrow_in_params_pinned() {
    // (a) the phantom parent from inside the default.
    let a = b"$S(f: cb = a => $b) { }";
    for t in TARGETS {
        let p = agree(a, 0, a.len(), t);
        assert!(p.has_params, "the group itself is balanced for {t:?}");
        assert_eq!((p.params_open, p.params_close), (2, 19));
        assert!(p.has_parent, "the in-params arrow IS read for {t:?} (pinned)");
        assert_eq!(&a[p.parent_start..p.parent_end], b"b");
    }
    // (b) the real parent after the group is never seen.
    let b_ = b"$S(f: cb = a => b) => $Real { }";
    for t in TARGETS {
        let p = agree(b_, 0, b_.len(), t);
        assert!(p.has_params);
        assert!(
            !p.has_parent,
            "the in-params arrow consumed the hunt; `=> $Real` is LOST for {t:?} (pinned)"
        );
    }
}

/// T-S6 (carry-and-name): an unbalanced params group is silently DROPPED (the hand if-let's
/// missing else arm) — `has_params` stays false, byte-identical — and the fork is now NAMED
/// (`params_unbalanced`).
#[test]
fn t_s6_unbalanced_params_named_pinned() {
    let src = b"$A(( ) { go(){} }";
    for t in TARGETS {
        let p = agree(src, 0, src.len(), t);
        assert!(!p.has_params, "the unbalanced group is dropped for {t:?}");
        assert!(p.params_unbalanced, "the T-S6 register must fire for {t:?}");
        assert_eq!(p.open, 7, "the `{{` seek is independent of the params outcome");
        assert_eq!(p.end, 17, "braces balance from the open");
    }
    // Non-vacuity control: a balanced group does not fire it.
    for t in TARGETS {
        let p = agree(b"$B(n: int) { }", 0, 14, t);
        assert!(p.has_params && !p.params_unbalanced);
    }
}

/// T-S7 (carry): `=>` followed by anything but `$name` yields NO parent AND the hunt STOPS
/// (the hand's unconditional `break` — a second arrow is never tried): `$A => x => $B { }`
/// has no parent.
#[test]
fn t_s7_malformed_parent_stops_hunt_pinned() {
    let src = b"$A => x => $B { }";
    for t in TARGETS {
        let p = agree(src, 0, src.len(), t);
        assert!(!p.has_parent, "the hunt stopped at `x`; `=> $B` is never tried for {t:?}");
        assert_eq!(p.open, 14);
        assert_eq!(p.end, 17);
    }
}

/// T-S9 (carry P1 -> fix P2 D3): the parent name-start probe is LEN-bounded (the hand `.get`)
/// while every other scan is LIMIT-bounded — a span cut right after `=> $` with a name byte
/// beyond `limit` reads ONE byte past `limit` and yields an EMPTY parent extent
/// (`has_parent` with `parent_start == parent_end` — the `Some("")` the offset-carrying parts
/// keep distinguishable). Phase-2 D3 bounds the probe by `limit` (-> no-parent).
#[test]
fn t_s9_limit_straddle_pinned() {
    let src = b"$A => $Px { }"; // limit 7 cuts between the `$` and the `P`
    for t in TARGETS {
        let p = agree(src, 0, 7, t);
        assert!(p.has_parent, "the len-bounded probe reads past limit for {t:?} (pinned)");
        assert_eq!(
            (p.parent_start, p.parent_end),
            (7, 7),
            "the parent extent is EMPTY (the `Some(\"\")` case) for {t:?}"
        );
        assert!(!p.open_found, "no `{{` inside the cut");
        assert_eq!((p.open, p.end), (7, 7));
        // Control: cut ON the `$` (limit 6) and the probe cannot fire at all.
        let q = agree(src, 0, 6, t);
        assert!(!q.has_parent, "no straddle at limit 6 for {t:?}");
    }
}

// ===========================================================================
// Teeth — every named register fires, non-vacuously, by RUNNING the system
// (not the oracle) over the corpus. A differential whose corpus never fires a
// register proves nothing about it. SCAFFOLDING.
// ===========================================================================

#[test]
fn heads_teeth() {
    let mut has_params = 0usize;
    let mut has_parent = 0usize;
    let mut params_unbalanced = 0usize;
    let mut body_clamped = 0usize;
    let mut no_open = 0usize;
    let mut plain_accept = 0usize; // open found, body balanced — the clean head
    for src in REALISTIC.iter().chain(ADVERSARIAL.iter()) {
        let b = src.as_bytes();
        for t in TARGETS {
            let p = scan(b, 0, b.len(), t);
            if p.has_params {
                has_params += 1;
            }
            if p.has_parent {
                has_parent += 1;
            }
            if p.params_unbalanced {
                params_unbalanced += 1;
            }
            if p.body_clamped {
                body_clamped += 1;
            }
            if !p.open_found {
                no_open += 1;
            }
            if p.open_found && !p.body_clamped {
                plain_accept += 1;
            }
        }
    }
    assert!(has_params > 0, "has_params never fired");
    assert!(has_parent > 0, "has_parent never fired");
    assert!(params_unbalanced > 0, "params_unbalanced (T-S6) never fired");
    assert!(body_clamped > 0, "body_clamped (T-S1) never fired");
    assert!(no_open > 0, "the no-brace tail (T-S2, !open_found) never fired");
    assert!(plain_accept > 0, "the plain clean accept never fired");
}

// ===========================================================================
// Deterministic fuzz arm — whole-token fragments (heads, params with arrows,
// comment/string `{` decoys, `=>` noise) assembled by an inline xorshift64*,
// random position pairs + the full span, full-struct differential, 4 targets.
// A divergence reproduces from its seed. SCAFFOLDING.
// ===========================================================================

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

/// Whole-token fragments so the generator forms head-shaped inputs (names, param groups with
/// arrows trapped in defaults, opaque `{`/`=>` decoys, parent arrows, unbalanced noise).
const FRAGMENTS: &[&str] = &[
    "$A", "$B", "$_x", "$", "$1",
    "(n: int)", "(", ")", "(f = a => $b)", "(( )",
    "=>", "=> $P", "=> $", "=> x", "$P",
    " ", "\t", "\n",
    "{", "}", "{ }", "{ go(){} }", "{ x ",
    "/* { */", "/* => $X */", "\"{\"", "// {\n", "*/",
    "x", "_id", "9",
];

fn gen_head_ish(rng: &mut Rng, max_frags: usize) -> String {
    let n = rng.below(max_frags + 1);
    let mut s = String::new();
    for _ in 0..n {
        s.push_str(FRAGMENTS[rng.below(FRAGMENTS.len())]);
    }
    s
}

#[test]
fn fuzz_frame_ish_every_position() {
    for seed in 0u64..3000 {
        let mut rng = Rng::new(seed ^ 0x5EAD_0001);
        let src = gen_head_ish(&mut rng, 6);
        let b = src.as_bytes();
        let len = b.len();
        if len == 0 {
            continue;
        }
        for _ in 0..4 {
            let at = rng.below(len);
            let limit = at + 1 + rng.below(len - at);
            for target in TARGETS {
                agree(b, at, limit, target); // random position pair (may cut mid-anything)
            }
        }
        for target in TARGETS {
            agree(b, 0, len, target); // the full span
        }
    }
}

/// The fuzz generator must reach BOTH parent outcomes, BOTH clamp outcomes, params (balanced
/// and unbalanced), and the no-brace tail — many times over the seed range; a generator that
/// never reached a register leaves its arm untested. Run the SYSTEM; require diversity.
#[test]
fn fuzz_has_teeth() {
    use std::collections::HashSet;
    let mut distinct = HashSet::new();
    let mut parent = 0usize;
    let mut no_parent = 0usize;
    let mut clamped = 0usize;
    let mut clean = 0usize;
    let mut params = 0usize;
    let mut unbalanced = 0usize;
    let mut no_open = 0usize;
    for seed in 0u64..3000 {
        let mut rng = Rng::new(seed ^ 0x5EAD_0001);
        let src = gen_head_ish(&mut rng, 6);
        let b = src.as_bytes();
        if b.is_empty() {
            continue;
        }
        distinct.insert(src.clone());
        for t in TARGETS {
            let p = scan(b, 0, b.len(), t);
            if p.has_parent {
                parent += 1;
            } else {
                no_parent += 1;
            }
            if p.body_clamped {
                clamped += 1;
            }
            if p.open_found && !p.body_clamped {
                clean += 1;
            }
            if p.has_params {
                params += 1;
            }
            if p.params_unbalanced {
                unbalanced += 1;
            }
            if !p.open_found {
                no_open += 1;
            }
        }
    }
    assert!(distinct.len() > 1200, "fuzz generator not diverse: {} distinct", distinct.len());
    assert!(parent > 100, "too few parents ({parent})");
    assert!(no_parent > 100, "too few no-parent heads ({no_parent})");
    assert!(clamped > 50, "too few clamped bodies ({clamped}) — T-S1 barely exercised");
    assert!(clean > 100, "too few clean bodies ({clean})");
    assert!(params > 100, "too few param groups ({params})");
    assert!(unbalanced > 20, "too few unbalanced groups ({unbalanced}) — T-S6 barely exercised");
    assert!(no_open > 50, "too few no-brace tails ({no_open}) — T-S2 barely exercised");
}

// ===========================================================================
// Self-contained spec — exact register values for hand-verified heads. These
// survive the oracle's eventual retirement (facts, not comparisons).
// ===========================================================================

#[test]
fn known_heads_self_contained() {
    for t in TARGETS {
        // `$A { }` — the minimal head.
        let p = scan(b"$A { }", 0, 6, t);
        assert_eq!((p.name_end, p.open, p.end), (2, 3, 6));
        assert!(!p.has_params && !p.has_parent && p.open_found && !p.body_clamped);

        // `$B(n: int) => $A { x }` — params + parent + body, every register on.
        let src = b"$B(n: int) => $A { x }";
        let p = scan(src, 0, src.len(), t);
        assert_eq!(p.name_end, 2);
        assert!(p.has_params && !p.params_unbalanced);
        assert_eq!((p.params_open, p.params_close), (2, 10));
        assert!(p.has_parent);
        assert_eq!(&src[p.parent_start..p.parent_end], b"A");
        assert_eq!((p.open, p.end), (17, 22));
        assert!(p.open_found && !p.body_clamped);

        // `$A\n{ }` — the Allman open: the seek crosses the newline.
        let p = scan(b"$A\n{ }", 0, 6, t);
        assert_eq!((p.name_end, p.open, p.end), (2, 3, 6));
        assert!(p.open_found && !p.has_parent);
    }
}

// ===========================================================================
// I1 byte-partition through the REAL pipeline + the anti-drift gate. The
// production driver runs the WIRED system path; these prove the
// SYSTEM's boundary equals the node extents production built — the executable
// form of the single-source discipline — and they hold unchanged across the
// M-wire flip. SCAFFOLDING (real pipeline + internal tree entry).
// ===========================================================================

fn wrap_system(states: &str) -> String {
    format!(
        "@@system S {{\n    interface:\n        go()\n    machine:\n{}\n}}\n",
        states
    )
}

/// Head-focused well-formed states, brace-balanced on every target (decoys live in string
/// literals so Python agrees; comment decoys are covered by the differential above).
const WELL_FORMED_STATES: &[&str] = &[
    "        $A {\n            go() { }\n        }",
    "        $Child(n: int) => $Parent {\n            $>() { }\n            go() { }\n        }\n        $Parent {\n            tick() { }\n        }",
    "        $A\n        {\n            go() { }\n        }",
    "        $A {\n            go() {\n                s = \"=> $Fake { }\"\n            }\n        }",
];

#[test]
fn real_pipeline_partition_covers_every_byte() {
    use frame_compiler::scan::segment;
    use frame_compiler::tree::{check_total, Defect, Node};
    use frame_compiler::Source;

    let mut checked = 0usize;
    for target in [Target::C, Target::Rust, Target::Python3] {
        for body in WELL_FORMED_STATES {
            let text = wrap_system(body);
            let bytes = text.as_bytes().to_vec();
            let src = Source::new("state_head_partition.frm", bytes.clone()).expect("utf8 source");
            let ast = segment(&src, target).expect("segment should succeed");

            ast.check_coverage()
                .unwrap_or_else(|d| panic!("check_coverage failed for {target:?} on:\n{text}\n  => {d}"));

            let rebuilt = ast.unparse(&bytes);
            assert_eq!(rebuilt, bytes, "unparse != source for {target:?} on:\n{text}");

            match check_total(&ast as &dyn Node) {
                Ok(()) => {}
                Err(Defect::UndecomposedBlob { .. }) => {}
                Err(d) => panic!("recursive partition BROKEN for {target:?} on:\n{text}\n  => {d}"),
            }
            checked += 1;
        }
    }
    assert!(checked >= WELL_FORMED_STATES.len() * 3);
}

/// THE ANTI-DRIFT GATE: for every state the real pipeline recorded, the SYSTEM's one run
/// yields the walk boundary AND every head field of the node — `end` == the state span end
/// == `state_extent(..).2` (the MachineWalk leaf's source), `open + 1` == the header span
/// end, the name/parent slices == the node's values. Makes the single-source discipline
/// executable, and survives the M-wire flip unchanged.
#[test]
fn walk_boundary_equals_node_extent() {
    use frame_compiler::scan::segment;
    use frame_compiler::tree::{Item, MachineMember, Section};
    use frame_compiler::Source;

    let mut states_checked = 0usize;
    for target in [Target::C, Target::Rust, Target::Python3] {
        for body in WELL_FORMED_STATES {
            let text = wrap_system(body);
            let bytes = text.as_bytes().to_vec();
            let src = Source::new("state_head_boundary.frm", bytes.clone()).unwrap();
            let ast = segment(&src, target).unwrap();

            for item in &ast.items {
                let Item::System(sys) = item else { continue };
                for sec in &sys.sections {
                    let Section::Machine(m) = sec else { continue };
                    for member in &m.members {
                        let MachineMember::State(s) = member else { continue };
                        let at = s.span.start;
                        let limit = m.span.end; // the driver's limit (machine_section)
                        let p = scan(&bytes, at, limit, target);
                        // Boundary: system == node span == the shared extent projection.
                        assert_eq!(p.end, s.span.end, "system end != state span end");
                        assert_eq!(
                            state_extent(&bytes, at, limit, target),
                            (p.name_end, p.open, p.end),
                            "system != state_extent (the MachineWalk leaf source)"
                        );
                        // Node fields: projections of the same run.
                        assert_eq!(p.open + 1, s.header_node.span.end, "header span end");
                        assert_eq!(
                            String::from_utf8_lossy(&bytes[at + 1..p.name_end]),
                            s.name,
                            "name slice"
                        );
                        let parent = if p.has_parent {
                            Some(String::from_utf8_lossy(&bytes[p.parent_start..p.parent_end]).into_owned())
                        } else {
                            None
                        };
                        assert_eq!(parent, s.parent, "parent slice");
                        states_checked += 1;
                    }
                }
            }
        }
    }
    assert!(states_checked >= 5 * 3, "expected every state x target to be checked");
}
