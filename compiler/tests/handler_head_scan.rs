//! **The handler-head reader, as a system, agrees with the hand reader — proven by running.**
//! SCAFFOLDING (differential vs the factored hand oracle; conversion-internal — never
//! promoted; needs `@@[scan(u8)]`-on-`@@system` plus the hand oracle it is racing).
//!
//! `handler_head_scan::scan` is generated from `handler_head_scan.frs`, a `@@[scan(u8)]`
//! register transducer WITH refusal (Item 3e, `_scratch/headreader_design.md`) — `$Name →
//! $NameIdent → $WsBeforeParen → $Params → $AfterParams → $RetType → $SeekBrace → $Body →
//! $Accept | $Reject`. The FOUR not-a-handler refusals of the hand code (no name / no `(` on
//! the head line / unbalanced params — the exit the old three-way description missed / no `{`
//! on the head line) share ONE `$Reject` (identical futures; the scan(u8) pump halts only on
//! `Accept`/`Reject`), with the CAUSE in the `reject_reason` register (1..4). This proves —
//! by running — that the machine's `Option<HandlerHeadParts>` is well-formed (on Accept the
//! offsets are ordered within the window: `i <= name_start`, `open < end <= limit`) at EVERY
//! `(i, limit)` position pair with `i < limit <= len` (the callers' position contract, T-H9),
//! for all four cleanroom targets, over realistic AND adversarial heads AND a deterministic fuzz
//! corpus. The EXACT parts/registers for each ledger row are pinned self-contained by the
//! directed T-H* tests and `known_heads_self_contained`.
//!
//! LEDGER ROSTER (design §5/§7 — one test per Phase-1 row):
//!   T-H1 no event name           -> `t_h1_reject_no_name_directed` (reason 1).
//!   T-H2 no `(` on the head line -> `t_h2_reject_no_paren_pinned` (reason 2; incl. the
//!        paren-on-the-next-line grammar-fact pin and the `$>` two-byte probe straddling
//!        `limit` — output-equivalent, lands here exactly as the hand returns `None`).
//!   T-H3 unbalanced params       -> `t_h3_reject_unbalanced_params_directed` (reason 3 —
//!        the FOURTH exit the inventory's three-way merge missed, now named).
//!   T-H4 no `{` on the head line -> `t_h4_brace_not_on_head_line_pinned` (reason 4 —
//!        `go()\n{ }` is NOT a handler: grammar fact, pinned).
//!   T-H5 unbalanced body clamps  -> `t_h5_unterminated_body_clamps_pinned` (register + exact
//!        clamp; records the driver artifact: `close = end - 1` -> `close_node` spans
//!        `[limit-1, limit)` over a non-`}` byte and the body loses its final byte).
//!   T-H6 empty return type       -> `t_h6_empty_return_type_pinned` (`has_return` observable
//!        for the first time; trim/empty->None stays adapter-side value work).
//!   T-H7 `{` inside the type     -> `t_h7_brace_in_return_type_pinned` (carried grammar
//!        limit — native-type parsing framec does not do; LEAVE-LATENT plea recorded).
//!   T-H8 head-line seeks opaque-aware (D1 LANDED) -> `t_h8_directed` (P2 directed; replaced
//!        `t_h8_comment_brace_head_line_pinned`). $RetType/$SeekBrace route through the shared
//!        `skip` (OpaqueScan) leaf; a `{`/`:` in a comment/string no longer opens the body.
//!        Opacity is target-sensitive (Python has no `/* */`).
//!   T-H9 `bytes[i]` panic        -> no test (position precondition, documented in the mod
//!        docs; the sweep respects `i < limit <= len`).

use frame_compiler::text::scan::handler_head_scan::{scan, scan_shape, HandlerHeadParts};
use frame_compiler::text::scan::literals::Target;

const TARGETS: [Target; 4] = [Target::C, Target::Java, Target::Rust, Target::Python3];

/// Run the reader and assert the standalone well-formedness invariant on any Accept: the recorded
/// offsets are ordered and inside the window `[i, limit]`. Returns the machine output so the
/// directed tests can assert exact values. No oracle.
fn agree(bytes: &[u8], i: usize, limit: usize, target: Target) -> Option<HandlerHeadParts> {
    let machine = scan(bytes, i, limit, target);
    if let Some(p) = &machine {
        assert!(
            i <= p.name_start && p.name_end <= limit,
            "name span [{},{}) escapes [{i},{limit}] on {:?}",
            p.name_start,
            p.name_end,
            String::from_utf8_lossy(bytes),
        );
        assert!(
            i <= p.open && p.open < p.end && p.end <= limit,
            "body span open={} end={} escapes [{i},{limit}] on {:?}",
            p.open,
            p.end,
            String::from_utf8_lossy(bytes),
        );
    }
    machine
}

/// Every-position sweep over the callers' position contract: every `i` in `0..len`, every
/// `limit` in `i+1..=len` (T-H9: `i < limit <= len`; the CONTENT at `i` is unconstrained —
/// positions mid-name, mid-params, mid-string are covered by construction, and all four
/// reject exits fire across the rectangle).
fn sweep_all_positions(src: &str) {
    let b = src.as_bytes();
    let len = b.len();
    for target in TARGETS {
        for i in 0..len {
            for limit in (i + 1)..=len {
                agree(b, i, limit, target);
            }
        }
    }
}

// ===========================================================================
// The curated corpus. Each string is a STATE-BODY fragment whose head the
// reader is pointed at. Enumerated from `handler_head_scan.frs` + the hand
// `handler_head`: `$>` / `<$` / identifier, ws, the REQUIRED `(params)`,
// optional same-line `: T`, the same-line `{`, the balanced body.
// ===========================================================================

/// Realistic, well-formed handler heads.
const REALISTIC: &[&str] = &[
    "go() { }",
    "$>() { }",
    "<$() { }",
    "tick(a: int): bool { return true }",
    "go () { }",
    "fetch(key: String) : String { body }",
    "go(): { }",
    "go() {\n    x\n}",
    "_under(x) { }",
    "go(s: str = \")\") { }",
    "e() {}",
];

/// Adversarial heads — every reject exit, opaque decoys, unterminated bodies, straddles.
const ADVERSARIAL: &[&str] = &[
    // reason 1: no event name
    "(x) { }",
    "123() { }",
    "$.x: int = 0",
    "$",
    "<",
    " go() { }",
    // reason 2: no `(` on the head line
    "go { }",
    "go\n() { }",
    "$>{ }",
    "$>",
    "<$",
    // reason 3: unbalanced params (the fourth exit)
    "go(( ) {",
    "go((x) { }",
    // NOT unbalanced: DelimBalance is opaque-aware, so trapped delimiters in strings are
    // skipped and these ACCEPT (unlike decl_read's Phase-A string-blind counter)
    "go(s = \"(\") { }",
    // reason 4: no `{` on the head line
    "go()\n{ }",
    "go()",
    "go() : T\n{ }",
    // clamp + type + decoys
    "go() { x",
    "f(): S { x } { b }",
    "go() /* { */ { }",
    "go() : { }",
    "go() \"{\" { }",
];

#[test]
fn differential_every_position() {
    for src in REALISTIC.iter().chain(ADVERSARIAL.iter()) {
        sweep_all_positions(src);
    }
}

// ===========================================================================
// One directed test per Phase-1 ledger row. Each pin asserts the exact parts/registers the
// machine produces at the pinned position (self-contained; `agree` also checks well-formedness).
// ===========================================================================

/// T-H1 (carry-and-name): no event name (not `$>` / `<$` / identifier-start) -> `None`,
/// `reject_reason` 1. The `Option` interface stays merged (the callers' semantics is only
/// "no member starts here"); the fork is named in-register.
#[test]
fn t_h1_reject_no_name_directed() {
    for src in [&b"(x) { }"[..], &b"123() { }"[..], &b"$.x: int = 0"[..]] {
        for t in TARGETS {
            assert_eq!(agree(src, 0, src.len(), t), None);
            let s = scan_shape(src, 0, src.len(), t);
            assert!(!s.accepted);
            assert_eq!(s.reject_reason, 1, "reason 1 (no name) for {t:?} on {src:?}");
        }
    }
}

/// T-H2 (carry-and-name): a `(` must follow the name on the HEAD LINE (ws = space/tab only)
/// -> `None`, reason 2. Grammar-fact pin: `go\n() {{ }}` is NOT a handler. Includes the
/// `$>`/`<$` two-byte probe straddling `limit` (LEN-bounded like the hand `.get` —
/// output-equivalent: the straddle lands HERE exactly as the hand returns `None`).
#[test]
fn t_h2_reject_no_paren_pinned() {
    for src in [&b"go { }"[..], &b"go\n() { }"[..], &b"$>{ }"[..]] {
        for t in TARGETS {
            assert_eq!(agree(src, 0, src.len(), t), None);
            let s = scan_shape(src, 0, src.len(), t);
            assert_eq!(s.reject_reason, 2, "reason 2 (no paren) for {t:?} on {src:?}");
        }
    }
    // The T-H2 straddle: `$>() { }` cut at limit 1 — the len-bounded probe reads the `>`
    // one byte past `limit`, then the missing `(` inside the window rejects with reason 2,
    // exactly where the hand returns `None`.
    let src = b"$>() { }";
    for t in TARGETS {
        assert_eq!(agree(src, 0, 1, t), None);
        let s = scan_shape(src, 0, 1, t);
        assert_eq!(s.reject_reason, 2, "the straddle lands in reason 2 for {t:?}");
        assert_eq!(s.name_kind, 1, "the probe DID read the `$>` (past limit — pinned)");
    }
}

/// T-H3 (carry-and-name): an unbalanced params group -> `None`, reason 3 — the FOURTH exit
/// (the hand `?` on `delim_balance::balanced`) the inventory's three-way description missed.
#[test]
fn t_h3_reject_unbalanced_params_directed() {
    for src in [&b"go(( ) {"[..], &b"go((x) { }"[..]] {
        for t in TARGETS {
            assert_eq!(agree(src, 0, src.len(), t), None);
            let s = scan_shape(src, 0, src.len(), t);
            assert_eq!(s.reject_reason, 3, "reason 3 (unbalanced params) for {t:?} on {src:?}");
        }
    }
    // Controls: delimiters hidden in a string do NOT unbalance (DelimBalance is
    // opaque-aware — unlike decl_read's Phase-A string-blind counter): both accept.
    for t in TARGETS {
        assert!(agree(b"go(s: str = \")\") { }", 0, 20, t).is_some());
        assert!(agree(b"go(s = \"(\") { }", 0, 15, t).is_some());
    }
}

/// T-H4 (carry-and-name): the `{` must be on the head line — newline or limit first ->
/// `None`, reason 4. Grammar-fact pin: `go()\n{ }` (Allman) is NOT a handler today.
#[test]
fn t_h4_brace_not_on_head_line_pinned() {
    for src in [&b"go()\n{ }"[..], &b"go()"[..], &b"go() : T\n{ }"[..]] {
        for t in TARGETS {
            assert_eq!(agree(src, 0, src.len(), t), None);
            let s = scan_shape(src, 0, src.len(), t);
            assert_eq!(s.reject_reason, 4, "reason 4 (no head-line brace) for {t:?} on {src:?}");
        }
    }
}

/// T-H5 (carry-and-name): an unbalanced handler body clamps `end` to `limit` — byte-exact
/// with the hand `unwrap_or(limit)` — NAMED (`body_clamped`). The DRIVER artifact, recorded:
/// `handler_at` (unchanged) builds `close = end - 1`, so `close_node` spans `[limit-1, limit)`
/// over a byte that is NOT `}` and the body loses its final byte; the register is the
/// diagnostics-pass hook.
#[test]
fn t_h5_unterminated_body_clamps_pinned() {
    let src = b"go() { x";
    for t in TARGETS {
        let p = agree(src, 0, src.len(), t).expect("the clamped head still ACCEPTS");
        assert_eq!(p.open, 5);
        assert!(p.body_clamped, "the T-H5 register must fire for {t:?}");
        assert_eq!(p.end, src.len(), "the clamp value is limit, byte-exact for {t:?}");
        // The driver artifact, demonstrated: close = end - 1 lands on a non-`}` byte.
        assert_ne!(src[p.end - 1], b'}', "the close_node byte is NOT a `}}` (recorded)");
    }
    // Non-vacuity control: a balanced body does not fire it.
    for t in TARGETS {
        assert!(!agree(b"go() { x }", 0, 10, t).unwrap().body_clamped);
    }
}

/// T-H6 (carry-and-name): `f(): {` — a `:` with an empty-after-trim type. The adapter maps it
/// to `return_text = None` (value work, unchanged); the `has_return` register distinguishes
/// it from `f() {` for the first time.
#[test]
fn t_h6_empty_return_type_pinned() {
    let src = b"f(): { }";
    for t in TARGETS {
        let p = agree(src, 0, src.len(), t).expect("accepts");
        assert!(p.has_return, "the `:` IS seen for {t:?}");
        let raw = std::str::from_utf8(&src[p.ret_start..p.ret_end]).unwrap();
        assert!(raw.trim().is_empty(), "the type text trims to empty (-> None in the adapter)");
        assert_eq!(p.open, 5);
        assert_eq!(p.end, 8);
    }
    // Control: no `:` at all -> has_return stays false.
    for t in TARGETS {
        assert!(!agree(b"f() { }", 0, 7, t).unwrap().has_return);
    }
}

/// T-H7 (carry, LEAVE-LATENT plea recorded): a `{` INSIDE the return-type text truncates the
/// type there and opens the body — `f(): S { x } { b }` has type `S`, body ` x `, and the
/// trailing `{ b }` is water. Distinguishing a type's `{` from the body opener would be
/// native-type parsing, which framec does not do. Pinned as today's truth.
#[test]
fn t_h7_brace_in_return_type_pinned() {
    let src = b"f(): S { x } { b }";
    for t in TARGETS {
        let p = agree(src, 0, src.len(), t).expect("accepts");
        assert!(p.has_return);
        assert_eq!(
            std::str::from_utf8(&src[p.ret_start..p.ret_end]).unwrap().trim(),
            "S",
            "the type is truncated at its `{{` for {t:?} (pinned)"
        );
        assert_eq!(p.open, 7, "the body opens at the type's `{{`");
        assert_eq!(p.end, 12, "the body is ` x ` — the trailing `{{ b }}` is water");
    }
}

/// T-H8 (fix P2 D1): the head-line seeks ($RetType, $SeekBrace) are now OPAQUE-AWARE — they
/// route through the shared `skip` (OpaqueScan) leaf, so a `{`/`:` trapped in a comment/string
/// no longer opens the body. Replaces `t_h8_comment_brace_head_line_pinned`. Machine and oracle
/// move together (shared `skip`), so the differential stays LOCKED; this pins the NEW behavior.
/// Opacity is TARGET-SENSITIVE: a string is opaque in every target, `/* */` only in C/Java/Rust
/// (Python3 has no block comments).
#[test]
fn t_h8_directed() {
    // A `{` trapped in a STRING literal (opaque in every target) on the head line no longer
    // opens the body: the seek skips the string and opens at the REAL `{`.
    let s = b"go() \"{\" { }";
    for t in TARGETS {
        let p = agree(s, 0, s.len(), t).expect("accepts at the real brace");
        assert_eq!(p.open, 9, "open = the real `{{` (the string's `{{` is skipped) for {t:?}");
        assert!(!p.body_clamped, "the real body balances cleanly for {t:?}");
        assert_eq!(p.end, 12);
    }
    // A `{` in a `/* */` block comment on the head line: skipped in C/Java/Rust (body opens at
    // the real `{`); Python3 has no block comments, so the text's first `{` (byte 8) wins and
    // the mis-open never balances (the old pinned behavior, now Python-only).
    let c = b"go() /* { */ { }";
    for t in TARGETS {
        let p = agree(c, 0, c.len(), t).expect("accepts");
        if t == Target::Python3 {
            assert_eq!(p.open, 8, "no block comments in Python — the text's `{{` (8) wins");
            assert!(p.body_clamped, "the mis-open never balances for Python3");
        } else {
            assert_eq!(p.open, 13, "the comment is skipped; the body opens at the real `{{`");
            assert!(!p.body_clamped);
            assert_eq!(p.end, 16);
        }
    }
}

// ===========================================================================
// Teeth — every named register fires, non-vacuously, by RUNNING the system
// over the corpus: all four reject reasons, accept, the clamp, has_return in
// BOTH values, and all three name kinds. SCAFFOLDING.
// ===========================================================================

#[test]
fn reject_reason_teeth() {
    let mut reasons = [0usize; 5]; // [accepted, r1, r2, r3, r4]
    let mut clamped = 0usize;
    let mut ret_true = 0usize;
    let mut ret_false = 0usize;
    let mut kinds = [0usize; 3]; // ident, $>, <$
    for src in REALISTIC.iter().chain(ADVERSARIAL.iter()) {
        let b = src.as_bytes();
        for t in TARGETS {
            let s = scan_shape(b, 0, b.len(), t);
            if s.accepted {
                reasons[0] += 1;
                kinds[s.name_kind as usize] += 1;
                if s.has_return {
                    ret_true += 1;
                } else {
                    ret_false += 1;
                }
                if s.body_clamped {
                    clamped += 1;
                }
            } else {
                reasons[s.reject_reason as usize] += 1;
            }
        }
    }
    assert!(reasons[0] > 0, "accept never fired");
    assert!(reasons[1] > 0, "reject_reason 1 (no name) never fired");
    assert!(reasons[2] > 0, "reject_reason 2 (no paren) never fired");
    assert!(reasons[3] > 0, "reject_reason 3 (unbalanced params, T-H3) never fired");
    assert!(reasons[4] > 0, "reject_reason 4 (no head-line brace) never fired");
    assert!(clamped > 0, "body_clamped (T-H5) never fired");
    assert!(ret_true > 0 && ret_false > 0, "has_return must fire BOTH ways");
    assert!(kinds[0] > 0, "ident heads never accepted");
    assert!(kinds[1] > 0, "`$>` heads never accepted");
    assert!(kinds[2] > 0, "`<$` heads never accepted");
}

// ===========================================================================
// Deterministic fuzz arm — whole-token fragments (event names incl. `$>`/`<$`,
// param groups incl. trapped delimiters, `: T`, head-line decoys), random
// position pairs + the full span, Option<Parts> differential, 4 targets.
// SCAFFOLDING.
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

const FRAGMENTS: &[&str] = &[
    "go", "tick", "_x", "e", "9", "$>", "<$", "$.",
    "(", ")", "(a: int)", "(( )", "(s = \")\")", "()",
    " ", "\t", "\n",
    ":", ": bool", ": ",
    "{", "}", "{ }", "{ x ", "{ return true }",
    "/* { */", "\"{\"", "->", "=",
    // accept-friendly compounds, so the generator reaches whole valid heads often
    "go()", "() {", "(): T {", ") { }",
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
        let mut rng = Rng::new(seed ^ 0x4EAD_0002);
        let src = gen_head_ish(&mut rng, 6);
        let b = src.as_bytes();
        let len = b.len();
        if len == 0 {
            continue;
        }
        for _ in 0..4 {
            let i = rng.below(len);
            let limit = i + 1 + rng.below(len - i);
            for target in TARGETS {
                agree(b, i, limit, target); // random position pair (may cut mid-anything)
            }
        }
        for target in TARGETS {
            agree(b, 0, len, target); // the full span
        }
    }
}

/// The fuzz generator must reach `Some` and `None` both, spread across the reject reasons,
/// and fire the clamp and return registers — many times over the seed range. Run the SYSTEM;
/// require diversity.
#[test]
fn fuzz_has_teeth() {
    use std::collections::HashSet;
    let mut distinct = HashSet::new();
    let mut accepted = 0usize;
    let mut rejected = 0usize;
    let mut reasons = [0usize; 5];
    let mut clamped = 0usize;
    let mut has_return = 0usize;
    for seed in 0u64..3000 {
        let mut rng = Rng::new(seed ^ 0x4EAD_0002);
        let src = gen_head_ish(&mut rng, 6);
        let b = src.as_bytes();
        if b.is_empty() {
            continue;
        }
        distinct.insert(src.clone());
        for t in TARGETS {
            let s = scan_shape(b, 0, b.len(), t);
            if s.accepted {
                accepted += 1;
                if s.body_clamped {
                    clamped += 1;
                }
                if s.has_return {
                    has_return += 1;
                }
            } else {
                rejected += 1;
                reasons[s.reject_reason as usize] += 1;
            }
        }
    }
    assert!(distinct.len() > 1200, "fuzz generator not diverse: {} distinct", distinct.len());
    assert!(accepted > 100, "too few accepted heads ({accepted})");
    assert!(rejected > 100, "too few rejected heads ({rejected})");
    assert!(reasons[1] > 50, "reason 1 barely exercised ({})", reasons[1]);
    assert!(reasons[2] > 50, "reason 2 barely exercised ({})", reasons[2]);
    assert!(reasons[3] > 10, "reason 3 (T-H3) barely exercised ({})", reasons[3]);
    assert!(reasons[4] > 10, "reason 4 barely exercised ({})", reasons[4]);
    assert!(clamped > 10, "the clamp (T-H5) barely exercised ({clamped})");
    assert!(has_return > 10, "has_return barely exercised ({has_return})");
}

// ===========================================================================
// Self-contained spec — exact register values for hand-verified heads incl.
// `$>` / `<$`. These survive the oracle's eventual retirement.
// ===========================================================================

#[test]
fn known_heads_self_contained() {
    for t in TARGETS {
        // `go() { }` — the minimal handler.
        let p = scan(b"go() { }", 0, 8, t).expect("accepts");
        assert_eq!(p.name_kind, 0);
        assert_eq!((p.name_start, p.name_end), (0, 2));
        assert_eq!((p.params_open, p.params_close), (2, 4));
        assert!(!p.has_return);
        assert_eq!((p.open, p.end), (5, 8));
        assert!(!p.body_clamped);

        // `$>() { }` — the enter-event form.
        let p = scan(b"$>() { }", 0, 8, t).expect("accepts");
        assert_eq!(p.name_kind, 1);
        assert_eq!((p.name_start, p.name_end), (0, 2));
        assert_eq!((p.params_open, p.params_close), (2, 4));
        assert_eq!((p.open, p.end), (5, 8));

        // `<$() { }` — the exit-event form.
        let p = scan(b"<$() { }", 0, 8, t).expect("accepts");
        assert_eq!(p.name_kind, 2);
        assert_eq!((p.name_start, p.name_end), (0, 2));
        assert_eq!((p.open, p.end), (5, 8));

        // `tick(a: int): bool { return true }` — params + return type + body.
        let src = b"tick(a: int): bool { return true }";
        let p = scan(src, 0, src.len(), t).expect("accepts");
        assert_eq!(p.name_kind, 0);
        assert_eq!((p.name_start, p.name_end), (0, 4));
        assert_eq!((p.params_open, p.params_close), (4, 12));
        assert!(p.has_return);
        assert_eq!((p.ret_start, p.ret_end), (13, 19));
        assert_eq!(std::str::from_utf8(&src[p.ret_start..p.ret_end]).unwrap().trim(), "bool");
        assert_eq!((p.open, p.end), (19, 34));
    }
}

// ===========================================================================
// Integration + the anti-drift gate, through the REAL pipeline. The production
// driver runs the WIRED system path; these prove the SYSTEM's boundary
// equals the member extents production built — and they hold unchanged across
// the M-wire flip. SCAFFOLDING (real pipeline + internal tree entry).
// ===========================================================================

fn wrap_system(states: &str) -> String {
    format!(
        "@@system S {{\n    interface:\n        go()\n    machine:\n{}\n}}\n",
        states
    )
}

/// Head-focused well-formed states, brace-balanced on every target (decoys in strings).
const WELL_FORMED_STATES: &[&str] = &[
    "        $A {\n            go() { }\n        }",
    "        $Work {\n            $.count: int = 0\n            $>() { }\n            go(a: int, b: str): bool { return true }\n            <$() { }\n        }",
    "        $A {\n            go() {\n                s = \"fake() { } <$() { }\"\n            }\n            tick () { }\n        }",
    "        $B {\n            f() : T { }\n        }",
];

/// A milestone-validation test: a state DECOMPOSES into exactly its StateVar + Handler
/// members end-to-end through `segment()` — the `handler_end` seam intact — and
/// handler-looking tokens buried in a string do NOT spawn phantom members. A regression in
/// the handler-head chain fails THIS named test.
#[test]
fn state_decomposes_into_the_right_members() {
    use frame_compiler::scan::segment;
    use frame_compiler::tree::{Item, MachineMember, Section, StateMember};
    use frame_compiler::Source;

    let text = wrap_system(
        "        $Work {\n            $.count: int = 0\n            $>() { }\n            go(): bool {\n                s = \"fake() { } <$() { }\"\n            }\n            <$() { }\n        }",
    );
    let src = Source::new("handler_head_members.frm", text.as_bytes().to_vec()).unwrap();
    let ast = segment(&src, Target::Rust).unwrap();

    let sys = ast
        .items
        .iter()
        .find_map(|i| match i {
            Item::System(s) => Some(s),
            _ => None,
        })
        .expect("a system");
    let machine = sys
        .sections
        .iter()
        .find_map(|sec| match sec {
            Section::Machine(m) => Some(m),
            _ => None,
        })
        .expect("a machine section");
    let state = machine
        .members
        .iter()
        .find_map(|m| match m {
            MachineMember::State(s) => Some(s),
            _ => None,
        })
        .expect("a state");

    let mut kinds: Vec<String> = Vec::new();
    let mut go_return = None;
    for m in &state.members {
        match m {
            StateMember::StateVar(d) => kinds.push(format!("var:{}", d.name)),
            StateMember::Handler(h) => {
                if h.event == "go" {
                    go_return = h.return_text.clone();
                }
                kinds.push(format!("handler:{}", h.event));
            }
            StateMember::DefaultForward(_) => kinds.push("forward".to_string()),
            StateMember::Trivia(_) => {}
        }
    }
    assert_eq!(
        kinds,
        vec!["var:count", "handler:$>", "handler:go", "handler:<$"],
        "exactly $.count, $>, go, <$; the tokens inside the string are NOT members"
    );
    assert_eq!(go_return.as_deref(), Some("bool"), "go's return type survives the chain");
}

/// THE ANTI-DRIFT GATE: for every handler the real pipeline recorded, the SYSTEM's one run
/// yields the member boundary AND every head field of the node — `end` == the handler span
/// end (what the StateWalk's `handler_end` leaf found), `open + 1` == the header span end,
/// the params/event/return projections == the node's values. Makes the single-source
/// discipline executable, and survives the M-wire flip unchanged.
#[test]
fn walk_boundary_equals_node_extent() {
    use frame_compiler::scan::segment;
    use frame_compiler::tree::{Item, MachineMember, Section, StateMember};
    use frame_compiler::Source;

    let mut handlers_checked = 0usize;
    for target in [Target::C, Target::Rust, Target::Python3] {
        for body in WELL_FORMED_STATES {
            let text = wrap_system(body);
            let bytes = text.as_bytes().to_vec();
            let src = Source::new("handler_head_boundary.frm", bytes.clone()).unwrap();
            let ast = segment(&src, target).unwrap();

            for item in &ast.items {
                let Item::System(sys) = item else { continue };
                for sec in &sys.sections {
                    let Section::Machine(m) = sec else { continue };
                    for member in &m.members {
                        let MachineMember::State(st) = member else { continue };
                        // The driver's limit for members: the state's closing `}` offset.
                        let close = st.close_node.span.start;
                        for sm in &st.members {
                            let StateMember::Handler(h) = sm else { continue };
                            let p = scan(&bytes, h.span.start, close, target)
                                .expect("production recorded a handler here");
                            // Boundary: system == node span (the handler_end leaf's answer).
                            assert_eq!(p.end, h.span.end, "system end != handler span end");
                            // Node fields: projections of the same run.
                            assert_eq!(p.open + 1, h.header_node.span.end, "header span end");
                            let event = match p.name_kind {
                                1 => "$>".to_string(),
                                2 => "<$".to_string(),
                                _ => String::from_utf8_lossy(&bytes[p.name_start..p.name_end])
                                    .into_owned(),
                            };
                            assert_eq!(event, h.event, "event name");
                            assert_eq!(
                                String::from_utf8_lossy(
                                    &bytes[p.params_open + 1..p.params_close - 1]
                                ),
                                h.params_text,
                                "params interior"
                            );
                            let ret = if p.has_return {
                                let t = String::from_utf8_lossy(&bytes[p.ret_start..p.ret_end])
                                    .trim()
                                    .to_string();
                                if t.is_empty() {
                                    None
                                } else {
                                    Some(t)
                                }
                            } else {
                                None
                            };
                            assert_eq!(ret, h.return_text, "return text projection");
                            handlers_checked += 1;
                        }
                    }
                }
            }
        }
    }
    assert!(handlers_checked >= 7 * 3, "expected every handler x target to be checked");
}
