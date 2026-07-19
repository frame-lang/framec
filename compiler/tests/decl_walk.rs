//! **The decl-section walk, as a system, agrees with the hand walk — proven by running.**
//! SCAFFOLDING (differential vs the factored hand oracle + the internal `Source`/`segment` entry
//! and tree spans; conversion-internal — never promoted; needs `@@[scan(u8)]`-on-`@@system`, a
//! cleanroom-only capability today, plus the hand oracle it is racing).
//!
//! `decl_walk::decl_starts` is generated from `decl_walk.frs`, a `@@[scan(u8)]` Frame system —
//! the FOURTH section walk (Item 3d, `_scratch/declwalk_design.md`). It walks a decl-section
//! span and accumulates the declaration-start offsets (plus the `unterminated_body` register,
//! ledger T2), skipping opaque regions, whitespace, and `@@[` attribute lines, and jumping each
//! decl's whole extent (line → eol; body → past the matching `}`, DelimBalance, clamped to
//! `limit` when unbalanced). This proves — by running — that the offsets match the hand
//! `decl_section` boundary loop (`decl_starts_hand`, the factored differential oracle) at EVERY
//! `(from, limit)` position, for all four cleanroom targets, for BOTH `with_bodies` values, over
//! realistic AND adversarial decl sections AND a deterministic fuzz corpus. The two
//! implementations share the SAME leaves (`skip_opaque`, `is_attr`, `to_end_of_line`,
//! `decl_end` → `machine::decl_extent`), so what the differential proves is the WALK — the thing
//! being converted. PHASE 1 IS BYTE-PARITY: the hand path's bugs are REPRODUCED and PINNED here
//! (T13's string-blind body fork), never fixed on the sly — fixes are Phase-B deltas.
//!
//! LEDGER ROSTER (design §6 — one test per walk-side Phase-1 row):
//!   T1  SectionEnd / trailing trivia  -> `t1_section_end_and_trailing_trivia` + the I1
//!       real-pipeline test (driver-side trivia flush).
//!   T2  Unbalanced-body clamp+swallow -> `t2_unterminated_body_register_and_swallow` (register
//!       fires non-vacuously; the rest of the section is swallowed into one extent).
//!   T3  `@@[` attribute-line refusal  -> `t3_attr_line_is_not_a_decl` (incl. `@@[` at EOF
//!       without newline).
//!   T4  Opaque-refusal fallthrough    -> `t4_unterminated_literal_starts_a_decl` (the quote
//!       byte becomes a decl start — pinned).
//!   T5  Whitespace arm                -> `t5_whitespace_only_section` + every-position sweeps.
//!   T6  Step-budget breaker           -> unreachable by the total-progress proof stated in
//!       `decl_walk.frs`'s header; witnessed here by `t6_long_pathological_input_terminates`
//!       (and by every sweep terminating).
//!   T13 String-blind body fork        -> `t13_brace_in_string_default_misforks_pinned`
//!       (Phase-A parity pin; the Phase-B fix will move this to a directed-fix test).
//!   T14 Allman-brace mis-split        -> `t14_allman_head_splits_into_two_decls` (the second,
//!       empty-named decl's read-side register is pinned in tests/decl_read.rs).
//!   T15 `saturating_sub(1)` clamp     -> `t15_open_brace_as_final_byte_walk_level` + the
//!       STATEMENT below.
//!
//! T15 STATEMENT (gate amendment 2026-07-18): this battery demonstrates panic-freedom of the
//! MACHINE CHAIN (`decl_starts`/`decl_starts_hand`/`decl_extent`) over its entire window sweep —
//! including the `{`-as-final-byte inputs — in debug builds (every `Span`/slice assert live).
//! DRIVER-level windows are KNOWINGLY EXCLUDED: `decl_section` (now the M-wired driver over
//! these systems) carries the hand path's `Span::new(open + 1, end - 1)` construction VERBATIM —
//! exactly the T15 hazard the ledger CARRIES with a written reachability argument (full-pipeline
//! reachability believed blocked upstream by `close_brace`'s strict balance; the pre-conversion
//! hand `decl_section` panicked identically on the same direct-call input, so parity holds).
//! Calling `decl_section` directly on a `{`-final-byte section stays out of this battery by that
//! recorded argument; the walk/read systems themselves never construct that span.

use frame_compiler::text::scan::decl_walk::{decl_starts, decl_starts_hand};
use frame_compiler::text::scan::literals::Target;

const TARGETS: [Target; 4] = [Target::C, Target::Java, Target::Rust, Target::Python3];

/// The differential: the system and the factored hand oracle must return the byte-identical
/// `Vec<usize>` of decl-start offsets for these exact `(from, limit, with_bodies)` arguments —
/// plus oracle-independent sanity on the machine's output: strictly increasing, in range, and
/// never at a whitespace byte (the ws arm consumes those; a recorded ws start would be a
/// dispatch-order break even if the oracle agreed).
fn agree(bytes: &[u8], from: usize, limit: usize, with_bodies: bool, target: Target) {
    let (machine, _unterm) = decl_starts(bytes, from, limit, with_bodies, target);
    let hand = decl_starts_hand(bytes, from, limit, with_bodies, target);
    assert_eq!(
        machine, hand,
        "MISMATCH target {target:?} from={from} limit={limit} with_bodies={with_bodies} on {:?}:\n  machine={machine:?}\n  hand  ={hand:?}",
        String::from_utf8_lossy(bytes),
    );
    let mut prev: Option<usize> = None;
    for &s in &machine {
        assert!(
            s >= from && s < limit,
            "start {s} out of [{from},{limit}) on {:?}",
            String::from_utf8_lossy(bytes)
        );
        if let Some(p) = prev {
            assert!(
                s > p,
                "starts not strictly increasing ({p} then {s}) on {:?}",
                String::from_utf8_lossy(bytes)
            );
        }
        prev = Some(s);
        assert!(
            !bytes[s].is_ascii_whitespace(),
            "start {s} points at whitespace on {:?}",
            String::from_utf8_lossy(bytes)
        );
    }
}

/// Suffix self-consistency, oracle-INDEPENDENT: the walk is memoryless (position-determined),
/// so re-running it FROM any recorded start must reproduce exactly the tail of the list from
/// that start — a start "inside a prior extent" would break this, because the walk jumps whole
/// extents. (The design's "never inside a prior extent" sanity, checkable without the oracle.)
fn suffix_consistent(bytes: &[u8], limit: usize, with_bodies: bool, target: Target) {
    let (all, _) = decl_starts(bytes, 0, limit, with_bodies, target);
    for (k, &s) in all.iter().enumerate() {
        let (tail, _) = decl_starts(bytes, s, limit, with_bodies, target);
        assert_eq!(
            tail,
            all[k..].to_vec(),
            "suffix walk from start {s} diverges on {:?} (with_bodies={with_bodies}, {target:?})",
            String::from_utf8_lossy(bytes)
        );
    }
}

/// Every-position sweep: agree for EVERY `from` in `0..=len` and EVERY `limit` in `from..=len`,
/// for all 4 targets × both `with_bodies` values. Exhaustive over the whole rectangle, so
/// mid-decl `from`, mid-extent `limit`, and mid-opaque `limit` are covered by construction —
/// not spot checks. (This sweep is also the T15 walk-level panic-freedom witness: it runs in
/// debug builds with every `Span`/slice assert live.)
fn sweep_all_positions(src: &str) {
    let b = src.as_bytes();
    let len = b.len();
    for target in TARGETS {
        for wb in [false, true] {
            for from in 0..=len {
                for limit in from..=len {
                    agree(b, from, limit, wb, target);
                }
            }
            suffix_consistent(b, len, wb, target);
        }
    }
}

// ===========================================================================
// The curated corpus. Each string is a DECL-SECTION span — the bytes between a
// section keyword's `:` and the next section (what `decl_section` walks as
// `[kw.end, span.end)`). Enumerated from `decl_walk.frs` + the hand
// `decl_section` (machine.rs): a decl start is any non-ws, non-opaque,
// non-`@@[` byte; its extent is the line (interface/domain) or the balanced
// body (actions/operations, `with_bodies`). Opaque regions, ws, and attribute
// lines are trivia.
// ===========================================================================

/// Realistic, well-formed decl sections — the shapes real `.frm` sections enclose.
const REALISTIC: &[&str] = &[
    // plain interface decls, one per line
    "\n    go()\n    stop()\n",
    // typed + initialized domain decls
    "\n    n: int = 0\n    cache: Cache = Cache\n",
    // a typed signature with params and a return type
    "\n    fetch(key: String): String\n",
    // the async modifier (T10's walk-side shape — one decl, extent to eol)
    "\n    async fetch(key: String): String\n",
    // system-field initializers, both forms
    "\n    sys: Inner = @@Inner(1)\n    other = @@!Sys()\n",
    // an attribute line between decls (T3)
    "\n    @@[export]\n    go()\n",
    // actions-style body decls, incl. nested braces (with_bodies exercises the fork)
    "\n    greet(name: str): str {\n        if x { return a } else { return b }\n    }\n    add(a: int, b: int): int { return a + b }\n",
    // adjacent body decls, no trivia between them
    "a() { x }b() { y }",
    // comments between decls (C-family; the sweep runs all targets — parity, not well-formedness)
    "\n    // a comment\n    go()\n    /* block */\n    stop()\n",
    // python comment flavor
    "\n    # a comment\n    go()\n",
    // a decl at EOF without a trailing newline
    "\n    go()",
];

/// Adversarial decl sections — the long tail the walk must get right.
const ADVERSARIAL: &[&str] = &[
    // empty section
    "",
    // whitespace only (T5)
    "   \n\t \n",
    // decl-looking text inside a `//` comment (C/Java/Rust) — not a decl
    "// go() fake: int = 0\nreal()\n",
    // decl-looking text inside a `#` comment (Python) — not a decl
    "# go() fake\nreal()\n",
    // decl-looking text inside a string — the literal is trivia, not a decl
    "\"go() fake\"\nreal()\n",
    // an unterminated literal — skip_opaque refuses, the quote byte STARTS a decl (T4)
    "\"never closed\nx: int = 0\n",
    // an unbalanced body decl — the clamp swallows the rest of the section (T2)
    "go() {\n    x = 1\nnext()\n",
    // a `{` inside a string default on an actions line (T13, Phase-A parity pin)
    "greet(pre: String = \"{\") { x }\n",
    // a `)` inside a string default (T9's walk-side shape: the line extent is eol regardless)
    "f(s: str = \")\")\ng()\n",
    // Allman-style split head (T14)
    "go()\n{ x = 1 }\n",
    // `@@[` at EOF without a newline (T3 edge)
    "@@[attr]",
    // a `{` as the section's final byte (T15's walk-level shape)
    "x() {",
    // stray punctuation decls (empty-name family, T7's walk-side reach)
    ";\n$.\n",
    // decls hidden ONLY inside opaque — zero real decls (C flavor)
    "// go() a: int = 0\n/* stop() b() { } */",
    // decls hidden ONLY inside opaque — zero real decls (python flavor)
    "# go() a: int\n\"stop() b() { }\"",
];

#[test]
fn realistic_sections_agree_every_position() {
    for src in REALISTIC {
        sweep_all_positions(src);
    }
}

#[test]
fn adversarial_sections_agree_every_position() {
    for src in ADVERSARIAL {
        sweep_all_positions(src);
    }
}

// ===========================================================================
// Teeth — the corpus is non-trivial. A differential over inputs that all yield
// the empty vector proves nothing (the #232 lie). Assert, by RUNNING the system
// (not the oracle), that the corpus spans the outcome space: some input yields
// >=2 decls; opaque-only inputs yield 0; the body-decl fork is actually TAKEN
// (with_bodies flips the outcome); the `unterminated_body` register actually
// FIRES. SCAFFOLDING.
// ===========================================================================

#[test]
fn corpus_has_teeth() {
    let mut max_decls = 0usize;
    let mut unterminated_fired = 0usize;
    for src in REALISTIC.iter().chain(ADVERSARIAL.iter()) {
        let b = src.as_bytes();
        for target in TARGETS {
            for wb in [false, true] {
                let (starts, unterm) = decl_starts(b, 0, b.len(), wb, target);
                max_decls = max_decls.max(starts.len());
                if unterm {
                    unterminated_fired += 1;
                }
            }
        }
    }
    assert!(
        max_decls >= 2,
        "no corpus input yields >=2 decls — the differential is toothless (max={max_decls})"
    );
    assert!(
        unterminated_fired > 0,
        "the unterminated_body register NEVER fired over the corpus — T2 is vacuously agreed"
    );

    // Decl-looking tokens ONLY inside opaque must yield ZERO decls, on the target whose
    // comment/string syntax actually swallows them.
    let only_c_opaque = "// go() a: int = 0\n/* stop() b() { } */";
    assert_eq!(
        decl_starts(only_c_opaque.as_bytes(), 0, only_c_opaque.len(), true, Target::C).0.len(),
        0,
        "decl-looking tokens only inside C comments must produce ZERO decls"
    );
    let only_py_opaque = "# go() a: int\n\"stop() b() { }\"";
    assert_eq!(
        decl_starts(only_py_opaque.as_bytes(), 0, only_py_opaque.len(), true, Target::Python3).0.len(),
        0,
        "decl-looking tokens only inside a `#` comment / string (python) must produce ZERO decls"
    );

    // The body-decl fork is actually TAKEN: with_bodies flips the outcome on a body section.
    // with_bodies=true: `a` swallows its body -> [a, c]. with_bodies=false: line decls -> the
    // interior `x()` and the bare `}` line each start a decl -> [a, x, }, c].
    let body = "a() {\n    x()\n}\nc()\n";
    for t in TARGETS {
        let (with_b, _) = decl_starts(body.as_bytes(), 0, body.len(), true, t);
        let (without_b, _) = decl_starts(body.as_bytes(), 0, body.len(), false, t);
        assert_eq!(with_b.len(), 2, "with_bodies swallows the body for {t:?}");
        assert_eq!(without_b.len(), 4, "line-decl mode splits the body lines for {t:?}");
        assert_ne!(
            with_b, without_b,
            "with_bodies did not change the outcome for {t:?} — the fork is never taken"
        );
    }
}

// ===========================================================================
// One directed, self-contained test per walk-side ledger row (design §6).
// These pin KNOWN offsets/registers for hand-verified inputs and survive the
// oracle's eventual retirement. SCAFFOLDING (pins carried Phase-A behavior).
// ===========================================================================

/// T1: the normal terminal — the walk stops at `limit`; trailing trivia after the last decl is
/// not a start (the driver flushes it as Trivia; driver-side proof = the I1 test below).
/// Trailing trivia is target-neutral here (ws + a string literal — opaque on all four targets);
/// the `//`-vs-`#` comment flavors are covered by the sweeps, where parity, not shape, is the
/// claim.
#[test]
fn t1_section_end_and_trailing_trivia() {
    let src = "\n    go()\n    \n  \"tail\"  \n";
    for t in TARGETS {
        let (starts, unterm) = decl_starts(src.as_bytes(), 0, src.len(), false, t);
        assert_eq!(starts, vec![5], "exactly the `go()` start for {t:?}");
        assert!(!unterm);
    }
}

/// T2: an unbalanced body decl clamps to `limit`, SWALLOWS the rest of the section (the
/// following `next()` line is inside the clamped extent, so it is NOT a second decl), and the
/// clamp is now REPORTED by the `unterminated_body` register — carried semantics, named.
#[test]
fn t2_unterminated_body_register_and_swallow() {
    let src = "go() {\n    x = 1\nnext()\n";
    for t in TARGETS {
        let (starts, unterm) = decl_starts(src.as_bytes(), 0, src.len(), true, t);
        assert_eq!(starts, vec![0], "the clamp swallows `next()` for {t:?}");
        assert!(unterm, "the T2 register must fire for {t:?}");
        // Same bytes WITHOUT the body fork (interface mode): three line decls, no clamp.
        let (line_starts, line_unterm) = decl_starts(src.as_bytes(), 0, src.len(), false, t);
        assert_eq!(line_starts.len(), 3, "line mode sees go/x/next for {t:?}");
        assert!(!line_unterm, "no body fork, no clamp for {t:?}");
    }
}

/// T3: an `@@[attr]` line is trivia, not a declaration (the `public Object ;` guard), including
/// the `@@[`-at-EOF-without-newline edge (attr_end = limit -> the walk exits via T1).
#[test]
fn t3_attr_line_is_not_a_decl() {
    let src = "@@[export]\ngo()\n";
    for t in TARGETS {
        for wb in [false, true] {
            let (starts, _) = decl_starts(src.as_bytes(), 0, src.len(), wb, t);
            assert_eq!(starts, vec![11], "only `go()` is a decl for {t:?}");
        }
    }
    let eof = "@@[attr]";
    for t in TARGETS {
        for wb in [false, true] {
            let (starts, _) = decl_starts(eof.as_bytes(), 0, eof.len(), wb, t);
            assert_eq!(starts, Vec::<usize>::new(), "`@@[` at EOF is trivia for {t:?}");
        }
    }
}

/// T4: an unterminated literal is REFUSED by the shared opaque skip (M7 policy), so its opening
/// quote byte falls through and STARTS a decl — carried, now visible (its read yields
/// `empty_name`, pinned in tests/decl_read.rs).
#[test]
fn t4_unterminated_literal_starts_a_decl() {
    let src = "\"never closed\nx: int = 0\n";
    for t in TARGETS {
        let (starts, _) = decl_starts(src.as_bytes(), 0, src.len(), false, t);
        assert_eq!(starts, vec![0, 14], "the quote byte is a decl start for {t:?}");
        assert_eq!(src.as_bytes()[starts[0]], b'"');
    }
}

/// T5: the whitespace arm — a ws-only section yields zero decls (steady consumption).
#[test]
fn t5_whitespace_only_section() {
    let src = "   \n\t \n";
    for t in TARGETS {
        for wb in [false, true] {
            let (starts, unterm) = decl_starts(src.as_bytes(), 0, src.len(), wb, t);
            assert!(starts.is_empty(), "ws-only section has no decls for {t:?}");
            assert!(!unterm);
        }
    }
}

/// T6: the generated engine's step budget (`len*4+64`) is unreachable — every arm strictly
/// advances (the proof is in `decl_walk.frs`'s header). Witness: a long pathological input
/// (single-byte decls, each one step + one extent jump) terminates and agrees. A budget break
/// would truncate `starts` and the differential would catch it.
#[test]
fn t6_long_pathological_input_terminates() {
    let src = ";\n".repeat(600);
    let b = src.as_bytes();
    for t in TARGETS {
        for wb in [false, true] {
            agree(b, 0, b.len(), wb, t);
            let (starts, _) = decl_starts(b, 0, b.len(), wb, t);
            assert_eq!(starts.len(), 600, "every `;` line is a (garbage) decl for {t:?}");
        }
    }
}

/// T13 (PHASE-A PARITY PIN — the string-blind body fork, machine.rs `(i..eol).find(b'{')`):
/// a `{` inside a string default on an actions line mis-forks the decl into a body opening
/// MID-LITERAL; DelimBalance then starts inside a string it cannot know it is in, never
/// balances, and the T2 clamp fires. Phase 1 REPRODUCES this bug byte-for-byte (verified: the
/// full-rectangle sweep + this pin). The Phase-B fix (`body_open_at`: opaque-aware + params-
/// group-aware) will replace this pin with directed-fix tests per target.
#[test]
fn t13_brace_in_string_default_misforks_pinned() {
    let bug = "greet(pre: String = \"{\") { x }\n";
    for t in TARGETS {
        let (starts, unterm) = decl_starts(bug.as_bytes(), 0, bug.len(), true, t);
        assert_eq!(
            starts,
            vec![0],
            "Phase-A pin for {t:?}: the mis-fork swallows the whole section into one decl"
        );
        assert!(
            unterm,
            "Phase-A pin for {t:?}: the mis-forked body never balances -> the T2 clamp fires"
        );
    }
    // Control: the SAME line without a `{` in the string forks correctly — two decls, no clamp.
    let ok = "greet(pre: String = \"x\") { x }\nnext()\n";
    for t in TARGETS {
        let (starts, unterm) = decl_starts(ok.as_bytes(), 0, ok.len(), true, t);
        assert_eq!(starts, vec![0, 31], "control: a clean body fork + next() for {t:?}");
        assert!(!unterm);
    }
}

/// T14: an Allman-style head (`go()` on one line, `{` on the next) splits into TWO decls — a
/// bodyless line decl and a second, EMPTY-NAMED body decl starting at the `{` (its `empty_name`
/// register is pinned in tests/decl_read.rs). Carried: same-line `{` is Frame's canonical
/// style; a multi-line head is a grammar-phase question.
#[test]
fn t14_allman_head_splits_into_two_decls() {
    let src = "go()\n{ x = 1 }\n";
    for t in TARGETS {
        let (starts, unterm) = decl_starts(src.as_bytes(), 0, src.len(), true, t);
        assert_eq!(starts, vec![0, 5], "line decl at 0, `{{` decl at 5 for {t:?}");
        assert_eq!(src.as_bytes()[starts[1]], b'{');
        assert!(!unterm, "the `{{ x = 1 }}` body balances");
    }
}

/// T15 (walk-level): a `{` as the section's FINAL byte — the machine chain records one decl,
/// clamps its extent to `limit`, fires the register, and is PANIC-FREE (this test and the
/// full-rectangle sweep of the same input in ADVERSARIAL run with debug asserts live). The
/// DRIVER-level `Span::new(open+1, end-1)` inversion hazard is knowingly excluded — see the
/// T15 STATEMENT in this file's header.
#[test]
fn t15_open_brace_as_final_byte_walk_level() {
    let src = "x() {";
    for t in TARGETS {
        let (starts, unterm) = decl_starts(src.as_bytes(), 0, src.len(), true, t);
        assert_eq!(starts, vec![0]);
        assert!(unterm, "open==limit-1 clamps and reports for {t:?}");
    }
}

// ===========================================================================
// Deterministic fuzz arm. Assemble frame-ish decl sections from decl / body /
// attr / comment / string / noise fragments, draw random `from`/`limit`, run
// the differential for all 4 targets × both with_bodies. Determinism: inline
// xorshift64* over a fixed seed range — no system randomness. A divergence
// panics with the source and arguments and reproduces from its seed.
// SCAFFOLDING.
// ===========================================================================

/// Inline deterministic PRNG (xorshift64*). Mirrors the sibling walks' prior art.
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

/// Whole-token fragments so the generator forms real decls (every form), real body decls
/// (nested, unbalanced, string-trapped), real attribute lines, and real opaque regions carrying
/// decoy decl-looking tokens — instead of relying on single bytes lining up.
const FRAGMENTS: &[&str] = &[
    // line decls of every form
    "go()\n",
    "n: int = 0\n",
    "fetch(key: String): String\n",
    "async f(): T\n",
    "s: Inner = @@Inner(1)\n",
    "t = @@!Sys()\n",
    "x\n",
    // body decls (balanced, nested, adjacent, unbalanced, string-trapped)
    "a() { r }\n",
    "g(a: int): int {\n    if p { q } else { r }\n}\n",
    "b() { y }c() { z }",
    "open() {\n",
    "}\n",
    "greet(pre: String = \"{\") { x }\n",
    "f(s: str = \")\")\n",
    // attribute lines
    "@@[export]\n",
    "@@[attr]",
    // opaque carrying decoys
    "// go() fake: int\n",
    "# stop() fake\n",
    "/* a() { } b: int */",
    "\"c() { } d: str\"",
    "'e()'",
    "\"never closed\n",
    // trivia + noise
    " ",
    "\n",
    "\t",
    ";\n",
    "$.\n",
    ":",
    "=",
    "(",
    ")",
    "{",
    "}",
];

fn gen_frame_ish(rng: &mut Rng, max_frags: usize) -> String {
    let n = rng.below(max_frags + 1);
    let mut s = String::new();
    for _ in 0..n {
        s.push_str(FRAGMENTS[rng.below(FRAGMENTS.len())]);
    }
    s
}

#[test]
fn fuzz_frame_ish_every_position_all_targets() {
    for seed in 0u64..3000 {
        let mut rng = Rng::new(seed ^ 0x3D3D_FFFF);
        let src = gen_frame_ish(&mut rng, 10);
        let b = src.as_bytes();
        let len = b.len();
        // Random from/limit (clamped so from <= limit <= len) — may land mid-decl, mid-extent,
        // or mid-opaque — PLUS the full span, per target × with_bodies.
        let from = if len == 0 { 0 } else { rng.below(len + 1) };
        let limit = if len == 0 { 0 } else { from + rng.below(len - from + 1) };
        for target in TARGETS {
            for wb in [false, true] {
                agree(b, from, limit, wb, target); // random window
                agree(b, 0, len, wb, target); // full span
            }
        }
    }
}

/// The fuzz generator must reach BOTH empty and non-empty results, multi-decl results, fired
/// `unterminated_body` registers, AND inputs where `with_bodies` actually changes the outcome
/// (the fork is exercised) — a generator that never reached these would test half the machine.
/// Assert, by running the system, that each occurs many times, and that the corpus is diverse.
#[test]
fn fuzz_has_teeth() {
    use std::collections::HashSet;
    let mut distinct = HashSet::new();
    let mut empty_results = 0usize;
    let mut nonempty_results = 0usize;
    let mut multi_decl_results = 0usize;
    let mut unterminated_fired = 0usize;
    let mut fork_changed_outcome = 0usize;
    for seed in 0u64..3000 {
        let mut rng = Rng::new(seed ^ 0x3D3D_FFFF);
        let src = gen_frame_ish(&mut rng, 10);
        let b = src.as_bytes();
        distinct.insert(src.clone());
        for target in TARGETS {
            let (with_b, unterm) = decl_starts(b, 0, b.len(), true, target);
            let (without_b, _) = decl_starts(b, 0, b.len(), false, target);
            if with_b.is_empty() {
                empty_results += 1;
            } else {
                nonempty_results += 1;
            }
            if with_b.len() >= 2 {
                multi_decl_results += 1;
            }
            if unterm {
                unterminated_fired += 1;
            }
            if with_b != without_b {
                fork_changed_outcome += 1;
            }
        }
    }
    assert!(distinct.len() > 1500, "fuzz generator not diverse: {} distinct", distinct.len());
    assert!(empty_results > 0, "fuzz never produced an EMPTY result — the zero-decl path is untested");
    assert!(nonempty_results > 100, "fuzz produced too few NON-empty results ({nonempty_results})");
    assert!(multi_decl_results > 50, "fuzz produced too few multi-decl results ({multi_decl_results})");
    assert!(
        unterminated_fired > 20,
        "fuzz fired unterminated_body only {unterminated_fired} times — the T2 register is barely exercised"
    );
    assert!(
        fork_changed_outcome > 50,
        "with_bodies changed the outcome only {fork_changed_outcome} times — the body fork is barely exercised"
    );
}

// ===========================================================================
// I1 byte-partition through the REAL pipeline. Drive `segment()` on full `.frm`
// files with interface/domain/actions sections, then assert the tree covers
// every byte: `check_coverage` (top-level partition), byte-identical `unparse`
// round-trip, recursive `check_total` (into each section's Member/WithBody/
// Trivia decls). This now exercises the M-WIRED path: `decl_section` is the
// native driver over DeclWalk/`decl_extent`/DeclRead, so a one-byte partition
// error in the driver surfaces as Gap/Overlap here — never silently. An
// `UndecomposedBlob` is expected and tolerated; a Gap/Overlap fails.
// SCAFFOLDING (real pipeline + internal tree entry; conversion-internal).
// ===========================================================================

/// Well-formed full systems whose decl sections carry every decl shape (brace-balanced on every
/// target; comment decoys live in the differential above, not here, so Python parses too).
const WELL_FORMED_SYSTEMS: &[&str] = &[
    "@@system S {\n    interface:\n        go()\n        fetch(key: String): String\n        async poll(): int\n    machine:\n        $A {\n            go() { }\n        }\n    domain:\n        n: int = 0\n        cache: Cache = Cache\n        sys: Inner = @@Inner(1)\n}\n",
    "@@system S {\n    interface:\n        @@[export]\n        go()\n    machine:\n        $A {\n            go() { }\n        }\n    actions:\n        greet(name: str): str {\n            if x { return a }\n            return b\n        }\n        add(a: int, b: int): int { return a + b }\n    domain:\n        other = @@!Sys()\n}\n",
    "@@system S {\n    interface:\n        go()\n    machine:\n        $A {\n            $.count: int = 0\n            go() { }\n        }\n    operations:\n        reset() {\n            n = 0\n        }\n}\n",
];

#[test]
fn real_pipeline_partition_covers_every_byte() {
    use frame_compiler::scan::segment;
    use frame_compiler::tree::{check_total, Defect, Node};
    use frame_compiler::Source;

    let mut checked = 0usize;
    for target in [Target::C, Target::Rust, Target::Python3] {
        for text in WELL_FORMED_SYSTEMS {
            let bytes = text.as_bytes().to_vec();
            let src = Source::new("decl_walk_partition.frm", bytes.clone()).expect("utf8 source");
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
    assert!(checked >= WELL_FORMED_SYSTEMS.len() * 3);
}
