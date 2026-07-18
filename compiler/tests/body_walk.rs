//! **The handler-BODY statement dispatch walk, as a system, agrees with the hand walk — proven by
//! running.** SCAFFOLDING (differential vs the retired hand oracle + the internal `Source`/
//! `segment` entry and tree spans; conversion-internal — never promoted; needs `@@[scan(u8)]`-on-
//! `@@system`, a cleanroom-only capability today, plus the hand oracle it is racing).
//!
//! `body_walk::stmt_starts` is generated from `body_walk.frs`, a `@@[scan(u8)]` Frame system that
//! now DRIVES `machine::body()`. It is the FIRST system to fuse a segmenter-style ACCUMULATOR
//! (`starts`) with a DelimBalance-style running COUNTER (`depth`): it walks a handler body and
//! records each Frame-statement START **paired with the brace depth at that point**, while running
//! a saturating `{`/`}` counter over the native water (opaque-skipped), and returns the FINAL depth
//! at `limit` (for the trailing native gap).
//!
//! This test proves — by running — that BOTH the `Vec<(usize, u32)>` of `(start, depth)` pairs AND
//! the final `u32` depth match the pre-conversion hand loop (`stmt_starts_hand`, kept ONLY as the
//! differential oracle) at EVERY (`from`, `limit`) position, for all four cleanroom targets, over
//! realistic AND adversarial handler bodies (every Frame-statement form; native code with NESTED
//! braces so depth VARIES; a `@@:`/`$.x=`/`->`/`{`/`}` buried in a comment or string; unbalanced
//! braces so the final depth is nonzero; empty body) AND a deterministic frame-ish fuzz corpus.
//!
//! The two implementations share the SAME leaves (`stmt_end` = the drift-safe `frame_call_end` /
//! `frame_assign_end` / `stmt_scan::classify` heads, `skip` = OpaqueScan) exactly as the sibling
//! `StateWalk`/`MachineWalk` walks share theirs, so what the differential proves is the WALK — the
//! dispatch order AND the running depth counter, the thing being converted. A MISMATCH here is a
//! real machine/oracle divergence and is reproducible from its printed inputs (or seed).
//!
//! TEETH: a differential whose inputs all yield the empty vector at depth 0 proves nothing (the
//! #232 lie), and a depth counter that is a no-op is worse. Dedicated teeth assert, by running the
//! SYSTEM (not the oracle), that recorded depths VARY (a statement at depth 0 AND one at depth >=1),
//! that the final depth is nonzero for an unbalanced body, and that a `{`/`}` inside a comment or
//! string does NOT perturb depth — so the counter is proven load-bearing.

use frame_compiler::text::scan::body_walk::{stmt_starts, stmt_starts_hand};
use frame_compiler::text::scan::literals::Target;

const TARGETS: [Target; 4] = [Target::C, Target::Java, Target::Rust, Target::Python3];

/// A recorded statement start must point at the first byte of a recognized Frame-statement form.
/// This is the CLOSED set of bytes such a form can begin with — hand-derived from the recognizers
/// (`frame_call_parse` → `@@:` = `@`; `frame_assign_parse` LHS is a frame ref = `$` or `@`;
/// `frame_stmt_hand` = `push$`/`pop$` = `p`, `->` = `-`, `(exit) ->` = `(`, `=> $^` = `=`), NOT from
/// the oracle. Oracle-INDEPENDENT: even if the oracle and the machine agreed on a wrong offset, a
/// start pointing at (say) a `{` would still be caught here.
fn is_valid_stmt_start_byte(b: u8) -> bool {
    b == b'@' || b == b'$' || b == b'-' || b == b'(' || b == b'p' || b == b'='
}

/// The differential: the system and the retired hand oracle must return the byte-identical
/// `(Vec<(usize, u32)>, u32)` — BOTH the `(start, depth)` pairs AND the final depth — for these
/// exact `(from, limit)` arguments, plus an oracle-independent partition sanity check on the
/// machine's output.
fn agree(bytes: &[u8], from: usize, limit: usize, target: Target) {
    let machine = stmt_starts(bytes, from, limit, target);
    let hand = stmt_starts_hand(bytes, from, limit, target);
    assert_eq!(
        machine, hand,
        "MISMATCH target {target:?} from={from} limit={limit} on {:?}:\n  machine={machine:?}\n  hand  ={hand:?}",
        String::from_utf8_lossy(bytes),
    );

    // Partition sanity, independent of the oracle: the recorded STARTS are strictly increasing
    // (the depths are a running counter and need NOT be monotone), each start is in [from, limit),
    // and each points at a byte a real Frame statement can begin with.
    let (starts, _final_depth) = &machine;
    let mut prev: Option<usize> = None;
    for &(s, _d) in starts {
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
            is_valid_stmt_start_byte(bytes[s]),
            "start {s} points at {:?} — not a Frame-statement-start byte, on {:?}",
            bytes[s] as char,
            String::from_utf8_lossy(bytes)
        );
    }
}

/// Every-position sweep: agree for EVERY `from` in `0..=len` and EVERY `limit` in `from..=len`.
/// Exhaustive over the whole `(from, limit)` rectangle, so mid-statement `from`, mid-statement
/// `limit`, mid-opaque `limit`, and mid-brace-run `limit` are all covered by construction — not
/// spot checks.
fn sweep_all_positions(src: &str) {
    let b = src.as_bytes();
    let len = b.len();
    for target in TARGETS {
        for from in 0..=len {
            for limit in from..=len {
                agree(b, from, limit, target);
            }
        }
    }
}

// ===========================================================================
// The curated corpus. Each string is a handler-BODY span — the bytes a handler's
// `{ … }` encloses (what `body()` receives as `bytes[open+1..close]`). Enumerated
// from `body_walk.frs` + the shared extent heads (`frame_call_parse`,
// `frame_assign_parse`, `frame_stmt_hand`): a statement start is a `@@:…` call
// (`@@:(e)` / `@@:return(e)` / `@@:self.m(a)`), a `$.x = e` / `@@:… = e` frame
// assignment, or a `frame_stmt` (`-> $S`, `push$ -> $S`, `-> pop$`, `pop$`, `=> $^`,
// `(exit) -> $S`). Braces of NATIVE water increment/decrement a saturating depth;
// opaque regions (comments/strings) are skipped whole and their braces do NOT count.
// ===========================================================================

/// Realistic, well-formed handler bodies — every Frame-statement form, interleaved with native
/// code (incl. NESTED braces so the recorded depth VARIES), single-line natives, and blank lines.
const REALISTIC: &[&str] = &[
    // each Frame-statement form on its own
    "@@:(x + 1)\n",                 // concise return call
    "@@:return(y * 2)\n",           // explicit return call
    "@@:self.report(a, b)\n",       // reentrant self call
    "$.count = 0\n",                // frame assignment
    "@@:self.factor = 3\n",         // frame assignment with an `@@:self` LHS
    "-> $Next\n",                   // transition, no args
    "-> $Next(a, b)\n",             // transition with state args
    "push$ -> $Sub\n",              // push + transition
    "-> pop$\n",                    // pop-and-restore
    "pop$\n",                       // bare pop (discard)
    "=> $^\n",                      // forward
    "(quit) -> $Bye\n",             // transition with EXIT args preceding the arrow (start byte `(`)
    "(reason) -> pop$\n",           // pop-and-restore carrying exit args (start byte `(`)
    "-> (enter) $Bye(s)\n",         // transition with enter args + state args
    // adjacent Frame statements, minimal trivia
    "$.a = 1\n$.b = 2\n-> $C\n",
    // a realistic mix: assignment, a native if-block (nested braces), a transition inside it,
    // an else-block, a self call inside it — depth VARIES across the recorded statements
    "$.count = 0\nif ready() {\n    -> $Run\n} else {\n    @@:self.wait()\n}\n",
    // multi-level nesting with a Frame statement buried deep
    "while go {\n    if x {\n        -> $Deep\n    }\n}\n$.done = 1\n",
    // single-line natives + blank lines interleaved with Frame statements
    "x = f(1)\n\n$.y = 2\n\nfoo(bar)\n-> $Q\n",
    // a return-call inside a nested block, then a top-level transition after the block closes
    "if done {\n    @@:return(result)\n}\n-> $End\n",
    // every form once, all interleaved with native braces
    "$.a = 1\n{\n    @@:(a)\n    push$ -> $P\n}\n=> $^\n@@:self.go(1)\n-> pop$\n",
];

/// Adversarial handler bodies — the long tail the walk (and the depth counter) must get right.
const ADVERSARIAL: &[&str] = &[
    // empty body
    "",
    // whitespace only
    "   \n\t ",
    // `@@:` / `$.x=` / `->` / `{` / `}` buried in a STRING — must NOT be recorded, braces must NOT
    // count (opaque on every target)
    "$.real = 1\ns = \"@@:self.x() -> $Fake } { $.q = 9\"\n-> $Done\n",
    // the same buried in a `//` LINE comment (C/Java/Rust)
    "$.real = 1\n// @@:self.x() -> $Fake } { $.q = 9\n-> $Done\n",
    // the same buried in a `#` LINE comment (Python)
    "$.real = 1\n# @@:self.x() -> $Fake } { $.q = 9\n-> $Done\n",
    // the same buried in a BLOCK comment (C/Java/Rust)
    "/* @@:x } { -> $Z $.a = 1 */\n$.real = 2\n",
    // UNBALANCED braces (opens with no closes) so the final depth is NONZERO
    "{\n    {\n        $.x = 1\n",
    // extra CLOSES so the saturating counter clamps at 0 (never underflows)
    "}\n}\n$.x = 1\n}\n",
    // a brace-heavy STRING adjacent to a bare-brace run — the string braces must be ignored while
    // the bare braces count
    "s = \"{ { {\"\n{\n-> $A\n",
    // adjacent statements with no blank lines between them
    "$.a = 1\n-> $B\n=> $^\n",
    // incomplete `@@:` forms that are NOT statements (no paren / no `=`): none recorded, one real
    "@@:\n@@:self\n@@:self.x\n$.real = 1\n",
    // a native arrow `(*p)->field` — the walk classifies whatever `frame_stmt` does; machine and
    // oracle share it, so they must AGREE (this exercises the tricky `(exit)`/`->` dispatch)
    "x = (*p)->field\n$.y = 2\n",
    // a Frame statement whose EXTENT runs to `limit` because the line never ends (transition = EOL)
    "$.a = 1\n-> $Unterminated no newline here",
    // ONLY buried tokens inside opaque — ZERO real statements (C/Java/Rust flavor)
    "// $.x = 1 -> $A @@:(z)\n/* push$ -> $B } { */",
    // ONLY buried tokens inside opaque — ZERO real statements (python flavor)
    "# $.x = 1 -> $A\n\"push$ -> $B } { $.q = 1\"",
    // deeply nested native braces with NO Frame statement — final depth exercised, zero starts
    "{ { { { } } }",
    // a `@@:self.m(` whose paren spans a nested `(...)` — extent = balanced close
    "@@:self.emit(f(g(1), h(2)), k)\n$.after = 1\n",
];

#[test]
fn realistic_bodies_agree_every_position() {
    for src in REALISTIC {
        sweep_all_positions(src);
    }
}

#[test]
fn adversarial_bodies_agree_every_position() {
    for src in ADVERSARIAL {
        sweep_all_positions(src);
    }
}

// ===========================================================================
// Teeth — the corpus is non-trivial AND the depth counter is load-bearing.
// Assert, by RUNNING the system (not the oracle):
//   (1) the corpus spans the start-count outcome space (some body >= 2 starts;
//       an opaque-only body 0 starts);
//   (2) recorded depths VARY — some statement at depth 0 AND some at depth >= 1;
//   (3) the final depth is NONZERO for an unbalanced-brace body;
//   (4) a `{`/`}` inside a comment/string does NOT change depth (brace-in-string
//       body vs a bare-brace body differ in exactly the way a live counter would).
// SCAFFOLDING.
// ===========================================================================

#[test]
fn corpus_has_teeth() {
    let mut max_starts = 0usize;
    for src in REALISTIC.iter().chain(ADVERSARIAL.iter()) {
        let b = src.as_bytes();
        for target in TARGETS {
            let (starts, _d) = stmt_starts(b, 0, b.len(), target);
            max_starts = max_starts.max(starts.len());
        }
    }
    assert!(
        max_starts >= 2,
        "no corpus body yields >=2 statement starts — the differential is toothless (max={max_starts})"
    );

    // Buried tokens ONLY inside opaque must yield ZERO statements, on the target whose comment/
    // string syntax actually swallows them.
    let only_c_opaque = "// $.x = 1 -> $A @@:(z)\n/* push$ -> $B } { */";
    let (c_starts, _) = stmt_starts(only_c_opaque.as_bytes(), 0, only_c_opaque.len(), Target::C);
    assert_eq!(
        c_starts.len(),
        0,
        "statement-looking tokens only inside C comments must produce ZERO statements"
    );
    let only_py_opaque = "# $.x = 1 -> $A\n\"push$ -> $B } { $.q = 1\"";
    let (py_starts, _) =
        stmt_starts(only_py_opaque.as_bytes(), 0, only_py_opaque.len(), Target::Python3);
    assert_eq!(
        py_starts.len(),
        0,
        "statement-looking tokens only inside a `#` comment / string (python) must produce ZERO statements"
    );
}

/// TEETH (2): recorded depths must VARY over a nested-brace body — the counter is not pinned to 0.
/// Hand-computed extents (self-contained, survives the oracle's retirement):
///
/// ```text
/// $.a = 1        assign  @ depth 0
/// {              depth -> 1
///     -> $B      transition @ depth 1
///     {          depth -> 2
///         pop$   bare pop @ depth 2
///     }          depth -> 1
/// }              depth -> 0
/// -> $C          transition @ depth 0
/// ```
#[test]
fn depth_teeth_recorded_depths_vary() {
    let body = "$.a = 1\n{\n    -> $B\n    {\n        pop$\n    }\n}\n-> $C\n";
    for t in TARGETS {
        let (starts, final_depth) = stmt_starts(body.as_bytes(), 0, body.len(), t);
        let depths: Vec<u32> = starts.iter().map(|&(_, d)| d).collect();
        assert_eq!(
            depths,
            vec![0, 1, 2, 0],
            "expected recorded depths [0,1,2,0] for {t:?} — the counter must climb with native braces"
        );
        assert!(
            depths.iter().any(|&d| d == 0) && depths.iter().any(|&d| d >= 1),
            "recorded depths do NOT vary for {t:?} ({depths:?}) — the depth counter is a no-op"
        );
        assert_eq!(final_depth, 0, "balanced body must end at depth 0 for {t:?}");
    }
}

/// TEETH (3): the final depth must be NONZERO for an unbalanced-brace body — the counter survives
/// to `limit`. Hand-computed: `{` -> 1, `{` -> 2, then `$.x = 1` recorded @ depth 2, no closes.
#[test]
fn depth_teeth_final_depth_nonzero_when_unbalanced() {
    let body = "{\n    {\n        $.x = 1\n";
    for t in TARGETS {
        let (starts, final_depth) = stmt_starts(body.as_bytes(), 0, body.len(), t);
        assert_eq!(final_depth, 2, "unbalanced `{{ {{` must leave final depth 2 for {t:?}");
        assert_eq!(starts.len(), 1, "exactly one statement (`$.x = 1`) for {t:?}");
        assert_eq!(starts[0].1, 2, "`$.x = 1` recorded at depth 2 for {t:?}");
    }

    // The saturating counter never underflows: extra closes clamp at 0.
    let over_closed = "}\n}\n$.x = 1\n}\n";
    for t in TARGETS {
        let (starts, final_depth) = stmt_starts(over_closed.as_bytes(), 0, over_closed.len(), t);
        assert_eq!(final_depth, 0, "extra `}}` must saturate at depth 0 for {t:?}");
        assert_eq!(starts[0].1, 0, "`$.x = 1` recorded at depth 0 (saturated) for {t:?}");
    }
}

/// TEETH (4): a `{`/`}` inside a comment/string does NOT change depth. Compare a body whose braces
/// live in a STRING against the same braces written BARE — a live counter reacts to one and not the
/// other, so the two outcomes must differ in exactly that way. Self-contained (no oracle).
#[test]
fn depth_teeth_brace_in_opaque_does_not_count() {
    // Braces inside a string: opaque, NOT counted -> `-> $A` at depth 0, final depth 0.
    let in_string = "$.x = 1\ns = \"{ { { }\"\n-> $A\n";
    // The SAME braces written bare: counted -> `-> $A` at depth 2 (`{`,`{`,`{`,`}` => 2), final 2.
    let bare = "$.x = 1\n{ { { }\n-> $A\n";
    for t in TARGETS {
        let (s_starts, s_final) = stmt_starts(in_string.as_bytes(), 0, in_string.len(), t);
        let (b_starts, b_final) = stmt_starts(bare.as_bytes(), 0, bare.len(), t);

        // Same number of statements ($.x and -> $A) either way — the string spawns none.
        assert_eq!(s_starts.len(), 2, "string-brace body has 2 statements for {t:?}");
        assert_eq!(b_starts.len(), 2, "bare-brace body has 2 statements for {t:?}");

        // The DEPTH is where they diverge: string braces ignored, bare braces counted.
        assert_eq!(s_final, 0, "braces in a string must NOT raise the depth for {t:?}");
        assert_eq!(b_final, 2, "bare braces MUST raise the depth for {t:?}");
        assert_eq!(s_starts[1].1, 0, "`-> $A` after a string of braces is at depth 0 for {t:?}");
        assert_eq!(b_starts[1].1, 2, "`-> $A` after bare braces is at depth 2 for {t:?}");
        assert_ne!(
            s_final, b_final,
            "brace-in-string vs bare-brace produced the SAME final depth for {t:?} — the counter ignored the distinction"
        );
    }

    // The same argument for a `//` comment on a brace target.
    let in_comment = "$.x = 1\n// { { { }\n-> $A\n";
    for t in [Target::C, Target::Java, Target::Rust] {
        let (starts, final_depth) = stmt_starts(in_comment.as_bytes(), 0, in_comment.len(), t);
        assert_eq!(final_depth, 0, "braces in a `//` comment must NOT count for {t:?}");
        assert_eq!(starts[1].1, 0, "`-> $A` after a comment of braces is at depth 0 for {t:?}");
    }
}

/// A focused self-contained spec: KNOWN (start-count, final-depth) for hand-verified bodies. This
/// survives the oracle's eventual retirement — it asserts the facts directly, not by comparison.
#[test]
fn known_counts_self_contained() {
    // Every Frame-statement form once, no nesting: 8 statements, final depth 0.
    // @@:(a) / @@:return(b) / @@:self.m(c) / $.x=1 / -> $S / push$ -> $P / -> pop$ / => $^
    let all_forms = "@@:(a)\n@@:return(b)\n@@:self.m(c)\n$.x = 1\n-> $S\npush$ -> $P\n-> pop$\n=> $^\n";
    for t in TARGETS {
        let (starts, final_depth) = stmt_starts(all_forms.as_bytes(), 0, all_forms.len(), t);
        assert_eq!(starts.len(), 8, "all 8 Frame-statement forms recognized for {t:?}");
        assert!(
            starts.iter().all(|&(_, d)| d == 0),
            "no nesting => every form at depth 0 for {t:?}"
        );
        assert_eq!(final_depth, 0, "no braces => final depth 0 for {t:?}");
    }

    // Buried decoys hide everything but the two real statements (C/Java/Rust, `//` comment).
    let hidden = "// @@:self.x() -> $Fake } { $.q = 9\n$.real = 1\n-> $Done\n";
    for t in [Target::C, Target::Java, Target::Rust] {
        let (starts, final_depth) = stmt_starts(hidden.as_bytes(), 0, hidden.len(), t);
        assert_eq!(starts.len(), 2, "only $.real and -> $Done are statements for {t:?}");
        assert_eq!(final_depth, 0, "the comment's braces do not count for {t:?}");
    }
}

// ===========================================================================
// Deterministic fuzz arm. Assemble frame-ish handler bodies from statement /
// native-brace / comment / string / noise fragments, draw random `from`/`limit`,
// run the differential for all 4 targets. Determinism: inline xorshift64* over a
// fixed seed range — no Date/system-random. A divergence panics with the source and
// arguments and reproduces from its seed.
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

/// Whole-token fragments so the generator forms real Frame statements (every form), real native
/// brace runs (so `depth` moves), and real opaque regions carrying decoy statement-looking tokens —
/// instead of relying on single bytes lining up.
const FRAGMENTS: &[&str] = &[
    // Frame statements (each ended by a newline: transitions/pop/push/forward run to end-of-line)
    "@@:(a)\n",
    "@@:return(b)\n",
    "@@:self.m(1)\n",
    "$.a = 1\n",
    "@@:self.f = 2\n",
    "-> $S\n",
    "-> $T(1, 2)\n",
    "push$ -> $P\n",
    "-> pop$\n",
    "pop$\n",
    "=> $^\n",
    // native brace runs (move `depth`)
    "{",
    "}",
    "{ }",
    "{ { } }",
    "if x {\n",
    "} else {\n",
    "}\n",
    // single-line natives + blank lines
    "y = f(1)\n",
    "foo(bar)\n",
    "\n",
    "  \n",
    // opaque carrying decoys (comments/strings)
    "// @@:self.x() -> $Fake } {\n",
    "# -> $Z $.q = 1\n",
    "/* $.a = 1 } { push$ -> $B */",
    "\"@@:x } { $.z = 1 -> $F\"",
    "'push$ -> $G } {'",
    // noise / partial tokens
    " ",
    "\t",
    ";",
    "@@:",
    "$.",
    "->",
    "$",
    "(*p)->q\n",
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
        let mut rng = Rng::new(seed ^ 0x5A5A_FFFF);
        let src = gen_frame_ish(&mut rng, 12);
        let b = src.as_bytes();
        let len = b.len();
        // Random from/limit (clamped so from <= limit <= len) — may land mid-statement, mid-opaque,
        // or mid-brace-run — PLUS the full span, for each target.
        let from = if len == 0 { 0 } else { rng.below(len + 1) };
        let limit = if len == 0 { 0 } else { from + rng.below(len - from + 1) };
        for target in TARGETS {
            agree(b, from, limit, target); // random window
            agree(b, 0, len, target); // full span
        }
    }
}

/// The fuzz generator must reach BOTH empty and non-empty results, multi-statement results, AND —
/// crucially — bodies where the depth counter actually MOVES (a recorded depth >= 1) and where it
/// survives unbalanced to a nonzero FINAL depth. A generator that only ever produced depth-0
/// results would leave the fused counter untested. Assert, by running the system, that all of these
/// occur many times over the seed range, and that the corpus is diverse.
#[test]
fn fuzz_has_teeth() {
    use std::collections::HashSet;
    let mut distinct = HashSet::new();
    let mut empty_results = 0usize;
    let mut nonempty_results = 0usize;
    let mut multi_stmt_results = 0usize;
    let mut nonzero_recorded_depth = 0usize;
    let mut nonzero_final_depth = 0usize;
    for seed in 0u64..3000 {
        let mut rng = Rng::new(seed ^ 0x5A5A_FFFF);
        let src = gen_frame_ish(&mut rng, 12);
        let b = src.as_bytes();
        distinct.insert(src.clone());
        for target in TARGETS {
            let (starts, final_depth) = stmt_starts(b, 0, b.len(), target);
            if starts.is_empty() {
                empty_results += 1;
            } else {
                nonempty_results += 1;
            }
            if starts.len() >= 2 {
                multi_stmt_results += 1;
            }
            if starts.iter().any(|&(_, d)| d >= 1) {
                nonzero_recorded_depth += 1;
            }
            if final_depth >= 1 {
                nonzero_final_depth += 1;
            }
        }
    }
    assert!(distinct.len() > 1500, "fuzz generator not diverse: {} distinct", distinct.len());
    assert!(empty_results > 0, "fuzz never produced an EMPTY result — the zero-statement path is untested");
    assert!(nonempty_results > 100, "fuzz produced too few NON-empty results ({nonempty_results}) — lacks teeth");
    assert!(multi_stmt_results > 50, "fuzz produced too few multi-statement results ({multi_stmt_results})");
    assert!(
        nonzero_recorded_depth > 50,
        "fuzz produced too few bodies with a recorded depth >= 1 ({nonzero_recorded_depth}) — the depth counter is barely exercised"
    );
    assert!(
        nonzero_final_depth > 50,
        "fuzz produced too few bodies with a nonzero FINAL depth ({nonzero_final_depth}) — the surviving counter is barely exercised"
    );
}

// ===========================================================================
// I1 byte-partition through the REAL pipeline. Drive `segment()` on full `.frm`
// files whose handlers contain a MIX of Frame statements + native code with nested
// braces (well-formed per target), then assert the tree covers every byte.
// `check_coverage` is the top-level partition; `unparse` round-trip is the
// constructive form; `check_total` recurses INTO each handler's Body — the
// Native/Frame-statement/Trivia stmts the native driver builds over `stmt_starts` —
// so a broken body partition surfaces as a `Gap`/`Overlap`, never silently. An
// `UndecomposedBlob` (an un-parsed native-part leaf) is EXPECTED and tolerated; a
// Gap/Overlap is a real bug and fails the test.
// SCAFFOLDING (real pipeline + internal tree entry; conversion-internal).
// ===========================================================================

fn wrap_system(states: &str) -> String {
    format!(
        "@@system S {{\n    interface:\n        go()\n    machine:\n{}\n}}\n",
        states
    )
}

/// Well-formed states whose handlers mix Frame statements with native NESTED braces, brace-balanced
/// on every target (incl. Python, where `//` is not a comment — so decoys live only inside string
/// literals here; the `//`/`#`-comment cases are covered by the differential above).
const WELL_FORMED_STATES: &[&str] = &[
    // an assignment + a native if-block (nested braces) + a transition inside it
    "        $A {\n            go() {\n                $.count = 0\n                if $.count > 0 {\n                    -> $B\n                }\n            }\n        }\n        $B {\n            go() { }\n        }",
    // every Frame-statement form interleaved with native braces, all in one handler
    "        $A {\n            go() {\n                @@:self.setup()\n                if ready {\n                    @@:(done)\n                }\n                $.n = 1\n                -> $B\n            }\n        }\n        $B {\n            e() {\n                push$ -> $A\n            }\n            f() {\n                -> pop$\n            }\n        }",
    // a string decoy inside a handler body carrying statement-looking tokens + braces — must not
    // spawn statements. (Braces kept BALANCED `{ }` so the file parses on Python too, where a lone
    // `{` in a string is an f-string hole opener; the unbalanced-brace-in-string case is covered by
    // the differential + the depth teeth, which run on raw body spans and need no full parse.)
    "        $A {\n            go() {\n                $.real = 1\n                log = \"@@:self.x() -> $Fake { } $.q = 9\"\n                -> $B\n            }\n        }\n        $B {\n            go() { }\n        }",
    // deep nesting: a Frame statement two blocks down
    "        $A {\n            go() {\n                while a {\n                    if b {\n                        -> $B\n                    }\n                }\n            }\n        }\n        $B {\n            go() { }\n        }",
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
            let src = Source::new("body_walk_partition.frm", bytes.clone()).expect("utf8 source");
            let ast = segment(&src, target).expect("segment should succeed");

            // I1 top-level: items partition [0, len).
            ast.check_coverage()
                .unwrap_or_else(|d| panic!("check_coverage failed for {target:?} on:\n{text}\n  => {d}"));

            // I1 constructive: byte-identical round-trip.
            let rebuilt = ast.unparse(&bytes);
            assert_eq!(rebuilt, bytes, "unparse != source for {target:?} on:\n{text}");

            // I1 RECURSIVE: traverse into each handler's Body — the Native/Frame-statement/Trivia
            // stmts built by the native driver over `stmt_starts`. A broken body partition (a start
            // off by a byte, an overlap, a dropped trivia gap) is a Gap/Overlap here. An
            // UndecomposedBlob is an un-parsed native-part leaf — expected.
            match check_total(&ast as &dyn Node) {
                Ok(()) => {}
                Err(Defect::UndecomposedBlob { .. }) => {}
                Err(d) => panic!("recursive partition BROKEN for {target:?} on:\n{text}\n  => {d}"),
            }
            checked += 1;
        }
    }
    assert!(
        checked >= WELL_FORMED_STATES.len() * 3,
        "expected every body x target to be checked"
    );
}

/// A milestone-validation test: a handler body actually DECOMPOSES into exactly its expected
/// Native/Frame-statement sequence WITH the right brace depth, end-to-end through `segment()`, and a
/// `@@:`/`$.x=`/brace buried in a native string spawns NO phantom statement and does NOT perturb
/// depth. This is the observable outcome of the `BodyWalk` system driving `body()` — a regression in
/// the fused start+depth walk fails THIS named test.
#[test]
fn handler_body_decomposes_with_correct_depth() {
    use frame_compiler::scan::segment;
    use frame_compiler::tree::body::Stmt;
    use frame_compiler::tree::{Item, MachineMember, Section, StateMember};
    use frame_compiler::Source;

    // go's body (interior between its braces):
    //   $.count = 0                                assign  @ depth 0
    //   if ready {                                 native, `{` -> depth 1
    //       -> $Done                               transition @ depth 1
    //   }                                          native, `}` -> depth 0
    //   log = "@@:self.oops() } { $.fake = 1"      native @ depth 0; the string is opaque, so its
    //                                              `@@:self.oops()` / `$.fake = 1` / braces are NOT
    //                                              statements and do NOT move the depth.
    let text = wrap_system(
        "        $Work {\n            go() {\n                $.count = 0\n                if ready {\n                    -> $Done\n                }\n                log = \"@@:self.oops() } { $.fake = 1\"\n            }\n        }\n        $Done {\n            go() { }\n        }",
    );
    let src = Source::new("body_walk_members.frm", text.as_bytes().to_vec()).unwrap();
    // Rust: a brace target, so `block_depth` is knowable (Some) and the depth tooth is observable.
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
    let work = machine
        .members
        .iter()
        .find_map(|m| match m {
            MachineMember::State(s) if s.name == "Work" => Some(s),
            _ => None,
        })
        .expect("the $Work state");
    let handler = work
        .members
        .iter()
        .find_map(|m| match m {
            StateMember::Handler(h) if h.event == "go" => Some(h),
            _ => None,
        })
        .expect("the go handler");

    // Tally the body's statements (Trivia ignored). Prove the string spawns nothing.
    let mut transitions = 0usize;
    let mut transition_depth: Option<u32> = None;
    let mut assigns = 0usize;
    let mut self_calls = 0usize;
    let mut return_calls = 0usize;
    let mut native_depths: Vec<u32> = Vec::new();
    for st in &handler.body.stmts {
        match st {
            Stmt::Transition(t) => {
                transitions += 1;
                transition_depth = Some(t.depth);
            }
            Stmt::Assign(_) => assigns += 1,
            Stmt::SelfCall(_) => self_calls += 1,
            Stmt::ReturnCall(_) => return_calls += 1,
            Stmt::Native(n) => {
                if let Some(d) = n.block_depth {
                    native_depths.push(d);
                }
            }
            _ => {}
        }
    }

    // Exactly one transition — at brace depth 1 (inside the native `if` block). THE DEPTH TOOTH.
    assert_eq!(transitions, 1, "exactly one transition (`-> $Done`)");
    assert_eq!(
        transition_depth,
        Some(1),
        "the transition is nested inside `if ready {{` so its recorded depth is 1"
    );

    // Exactly one assignment (`$.count`); the `$.fake = 1` inside the string is NOT one.
    assert_eq!(assigns, 1, "only $.count is an assignment; the string's `$.fake = 1` is NOT");
    // The `@@:self.oops()` inside the string is NOT a self call, and there is no return call.
    assert_eq!(self_calls, 0, "the string's `@@:self.oops()` must NOT become a SelfCall");
    assert_eq!(return_calls, 0, "no return call in this body");

    // The native gaps carry the recorded depth, and it VARIES: the `if ready {` gap is at depth 1
    // (the following statement's depth), the trailing `}` + `log = "…"` gap is at depth 0. A live
    // counter produces both; a no-op counter would produce only 0.
    assert!(
        native_depths.contains(&0),
        "some native gap must be at depth 0 (got {native_depths:?})"
    );
    assert!(
        native_depths.contains(&1),
        "some native gap must be at depth 1 — the counter must have climbed into the `if` block (got {native_depths:?})"
    );
}
