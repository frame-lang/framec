//! **The state-member walk, as a system, agrees with the hand walk — proven by running.**
//! SCAFFOLDING (differential vs the retired hand oracle + the internal `Source`/`segment` entry
//! and tree spans; conversion-internal — never promoted; needs `@@[scan(u8)]`-on-`@@system`, a
//! cleanroom-only capability today, plus the hand oracle it is racing).
//!
//! `state_walk::member_starts` is generated from `state_walk.frs`, a `@@[scan(u8)]` Frame system
//! that now DRIVES `machine::state()`'s member loop. This proves — by running — that the
//! member-start offsets it accumulates (a `$.x` state variable, or a handler head `ev(...) {` /
//! `$>() {` / `<$() {`) match the pre-conversion hand loop (`member_starts_hand`, kept ONLY as the
//! differential oracle) at EVERY (`from`, `close`) position, for all four cleanroom targets, over
//! realistic state bodies AND adversarial ones (a `}` inside a string/comment in a handler body; a
//! `$.`/handler-looking token buried in opaque; nested `{}`; adjacent members; unterminated body;
//! state var at EOF; empty body) AND a deterministic frame-ish fuzz corpus.
//!
//! The two implementations share the SAME leaves (`skip_opaque`, `to_end_of_line`, `handler_end`)
//! exactly as `MachineWalk`/`Segmenter` share theirs, so what the differential proves is the WALK —
//! the thing being converted. A MISMATCH here is a real machine/oracle divergence and is
//! reproducible from its printed inputs (or seed).

use frame_compiler::text::scan::literals::Target;
use frame_compiler::text::scan::state_walk::{member_starts, member_starts_hand};

const TARGETS: [Target; 4] = [Target::C, Target::Java, Target::Rust, Target::Python3];

/// A recorded member start must point at the first byte of a recognized member form: `$` (a `$.x`
/// state var, or an `$>` enter handler), `<` (a `<$` exit handler), or an identifier start byte
/// `[A-Za-z_]` (a named handler). Oracle-INDEPENDENT — this is the closed set the recognizer can
/// possibly emit, hand-derived from `handler_head`/`is_statevar` in `machine.rs`, not from the
/// oracle.
fn is_valid_member_start_byte(b: u8) -> bool {
    b == b'$' || b == b'<' || b.is_ascii_alphabetic() || b == b'_'
}

/// The differential: the system and the retired hand oracle must return the byte-identical
/// `Vec<usize>` of member-start offsets for these exact `(from, close)` arguments — plus an
/// oracle-independent partition sanity check on the machine's output.
fn agree(bytes: &[u8], from: usize, close: usize, target: Target) {
    let machine = member_starts(bytes, from, close, target);
    let hand = member_starts_hand(bytes, from, close, target);
    assert_eq!(
        machine, hand,
        "MISMATCH target {target:?} from={from} close={close} on {:?}:\n  machine={machine:?}\n  hand  ={hand:?}",
        String::from_utf8_lossy(bytes),
    );
    // Partition sanity, independent of the oracle: strictly increasing, each in [from, close),
    // each pointing at a byte a real member form can begin with.
    let mut prev: Option<usize> = None;
    for &s in &machine {
        assert!(
            s >= from && s < close,
            "start {s} out of [{from},{close}) on {:?}",
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
            is_valid_member_start_byte(bytes[s]),
            "start {s} points at {:?} — not a member-start byte, on {:?}",
            bytes[s] as char,
            String::from_utf8_lossy(bytes)
        );
    }
}

/// Every-position sweep: agree for EVERY `from` in `0..=len` and EVERY `close` in `from..=len`.
/// Exhaustive over the whole `(from, close)` rectangle, so mid-member `from`, mid-member `close`,
/// and mid-opaque `close` are all covered by construction — not spot checks.
fn sweep_all_positions(src: &str) {
    let b = src.as_bytes();
    let len = b.len();
    for target in TARGETS {
        for from in 0..=len {
            for close in from..=len {
                agree(b, from, close, target);
            }
        }
    }
}

// ===========================================================================
// The curated corpus. Each string is a STATE-BODY span — the bytes a state's
// `{ … }` encloses (what `state()` passes as `bytes[open+1..close]`). Enumerated
// from `state_walk.frs` + `machine.rs`'s `handler_head`/`is_statevar`/`member_end`
// (the forms the walk records): a member start is a `$.` state var (extent to
// end-of-line), or a handler head — a name / `$>` / `<$`, then `(...)`, an optional
// `: T`, then `{`, its body (nesting balanced) skipped whole. Opaque regions are
// skipped whole.
// ===========================================================================

/// Realistic, well-formed state bodies — the shapes a real state encloses.
const REALISTIC: &[&str] = &[
    // a single state variable
    "$.count: int = 0\n",
    // multiple state variables, various shapes
    "$.a: int = 0\n$.b: str = \"x\"\n$.c = 3\n",
    // a plain handler
    "go() { x = 1 }",
    // a handler with a typed parameter
    "set(a: int) { $.a = a }",
    // a handler with a return type
    "check(): bool { return true }",
    // enter and exit handlers
    "$>() { init() }\n<$() { cleanup() }",
    // the whole mix: state var, enter, named handler with a transition, exit
    "$.n: int = 0\n$>() { }\ngo() {\n    if done() { -> $End }\n}\n<$() { }",
    // nested `{}` inside a handler body — the inner braces must not end the handler early
    "$.x: int = 0\ngo() {\n    if p { q } else { r }\n}\ntick() { }",
    // adjacent handlers, NO trivia between them
    "a() {}b() {}c() {}",
    // a state var directly followed by a handler, minimal trivia
    "$.k = 0\ngo(){}",
];

/// Adversarial state bodies — the long tail the walk must get right.
const ADVERSARIAL: &[&str] = &[
    // empty body
    "",
    // whitespace only
    "   \n\t ",
    // a `}` inside a STRING in a handler body — must not close the handler early
    "go() { s = \"a } b { c\" }\ntick() { }",
    // a `}` inside a `//` COMMENT in a handler body (C/Java/Rust) — must not close early
    "go() {\n    x = 1 // } close?\n    y = 2\n}\ntick() { }",
    // a handler-looking token buried in a `//` line comment (C/Java/Rust) — NOT a member
    "// go() { should not count }\n$.real: int = 0\ntick() { }",
    // a handler-looking token buried in a `#` line comment (Python) — NOT a member
    "# go() { should not count }\n$.real: int = 0\ntick() { }",
    // member-looking tokens buried in a block comment (C/Java/Rust) — NONE count
    "/* $.fake: int = 0  go() { }  $>() { }  <$() { } */\nreal() { }",
    // a `$.`/handler-looking token inside a top-level STRING literal — NOT a member
    "\"$.fake go() { } <$() { }\"\nreal() { }",
    // `$`-noise that is NOT a member: `$` at EOF, `$` + digit, `$` + punctuation,
    // `$>` / `<$` with no `()`, a bare identifier with no `()`
    "$ $1 $+ $> <$ bareName\ngo() { }",
    // a state var with NO trailing newline at EOF — extent runs to `close`
    "go() { }\n$.tail: int = 0",
    // an unterminated handler body — never closes; extent runs to `close`, one member
    "$.x = 0\ngo() { unterminated",
    // an unterminated string carrying a handler-looking token — swallowed to `close`, no member
    "s = \"open go() {  $.q\ntick() {",
    // `$.` alone (the minimal state-var form) and `$_x`-named handler
    "$.\n_x() { }",
    // ONLY member-looking tokens inside opaque — zero real members (C/Java/Rust flavor)
    "// go() { } $.x: int = 0\n/* $>() { } <$() { } */",
    // ONLY member-looking tokens inside opaque — zero real members (python flavor)
    "# go() { } $.x\n\"$.y go() { }\"",
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
// Teeth — the corpus is non-trivial. A differential over inputs that all yield
// the empty vector proves nothing (the #232 lie). Assert, by RUNNING the system
// (not the oracle), that the corpus spans the outcome space: some input yields
// >=2 members; some input with only opaque-buried member-looking tokens yields 0.
// SCAFFOLDING.
// ===========================================================================

#[test]
fn corpus_has_teeth() {
    let mut max_members = 0usize;

    for src in REALISTIC.iter().chain(ADVERSARIAL.iter()) {
        let b = src.as_bytes();
        for target in TARGETS {
            let starts = member_starts(b, 0, b.len(), target);
            max_members = max_members.max(starts.len());
        }
    }

    // Member-looking tokens that exist ONLY inside opaque must yield ZERO members, on the target
    // whose comment/string syntax actually swallows them.
    let only_c_opaque = "// go() { } $.x: int = 0\n/* $>() { } <$() { } */";
    assert_eq!(
        member_starts(only_c_opaque.as_bytes(), 0, only_c_opaque.len(), Target::C).len(),
        0,
        "member-looking tokens only inside C comments must produce ZERO members"
    );
    let only_py_opaque = "# go() { } $.x\n\"$.y go() { }\"";
    assert_eq!(
        member_starts(only_py_opaque.as_bytes(), 0, only_py_opaque.len(), Target::Python3).len(),
        0,
        "member-looking tokens only inside a `#` comment / string (python) must produce ZERO members"
    );
    assert!(
        max_members >= 2,
        "no corpus input yields >=2 members — the differential is toothless (max={max_members})"
    );
}

/// A focused self-contained spec: KNOWN member counts for hand-verified inputs, per the target
/// whose opaque syntax applies. This survives the oracle's eventual retirement — it asserts the
/// count directly, not by comparison. (The offsets themselves are position-dependent and left to
/// the differential; the COUNT is the language-level fact.)
#[test]
fn known_member_counts_self_contained() {
    // state var + enter + named handler + exit = four members; the `//`/string decoys hide nothing
    // real here (there are none inside opaque), so exactly four.
    let src = "$.n: int = 0\n$>() { }\ngo() { x = 1 }\n<$() { }";
    for t in TARGETS {
        assert_eq!(
            member_starts(src.as_bytes(), 0, src.len(), t).len(),
            4,
            "expected $.n,$>,go,<$ for {t:?}"
        );
    }
    // adjacent handlers, no trivia: three members.
    let adj = "a() {}b() {}c() {}";
    for t in TARGETS {
        assert_eq!(
            member_starts(adj.as_bytes(), 0, adj.len(), t).len(),
            3,
            "three adjacent handlers for {t:?}"
        );
    }
    // nested braces + a `}` inside a string inside a handler body: exactly ONE handler member.
    let nested = "go() {\n    if x { y { z } }\n    s = \"} { }\"\n}";
    for t in TARGETS {
        assert_eq!(
            member_starts(nested.as_bytes(), 0, nested.len(), t).len(),
            1,
            "nested braces + string-brace => one handler for {t:?}"
        );
    }
    // `$`-noise, `$>`/`<$` without `()`, bare name without `()` are NOT members: only $.x and go().
    let noise = "$.x = 0\n$ $1 $+ $> <$ bare\ngo() { }";
    for t in TARGETS {
        assert_eq!(
            member_starts(noise.as_bytes(), 0, noise.len(), t).len(),
            2,
            "only $.x and go() are members for {t:?}"
        );
    }
    // member-looking tokens ONLY inside a C/Java/Rust `//` line comment: exactly one real member.
    let hidden = "// go() { } $.x: int = 0\nreal(): bool { return true }";
    for t in [Target::C, Target::Java, Target::Rust] {
        assert_eq!(
            member_starts(hidden.as_bytes(), 0, hidden.len(), t).len(),
            1,
            "only real() is a member (the comment hides the rest) for {t:?}"
        );
    }
}

// ===========================================================================
// Deterministic fuzz arm. Assemble frame-ish state bodies from state-var /
// handler / comment / string / noise fragments, draw random `from`/`close`, run
// the differential for all 4 targets. Determinism: inline xorshift64* over a fixed
// seed range — no Date/system-random. A divergence panics with the source and
// arguments and reproduces from its seed.
// SCAFFOLDING.
// ===========================================================================

/// Inline deterministic PRNG (xorshift64*). Mirrors the opaque_scan/delim_balance prior art.
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

/// Whole-token fragments so the generator forms real state vars, real handlers (incl. nested
/// bodies), and real opaque regions carrying decoy member-looking tokens — instead of relying on
/// single bytes lining up.
const FRAGMENTS: &[&str] = &[
    // state vars
    "$.a: int = 0\n",
    "$.b = 3\n",
    "$.\n",
    // handlers of every shape
    "go() { }",
    "set(x: int) { $.a = x }",
    "chk(): bool { return true }",
    "$>() { }",
    "<$() { }",
    "nest() { if p { q } else { r } }",
    "brace() { s = \"} { }\" }",
    // opaque carrying decoys
    "// go() { } $.x\n",
    "# <$() { }\n",
    "/* $>() { } $.z */",
    "\"$.q go() { }\"",
    "'$.w <$() {}'",
    // trivia + noise
    " ",
    "\n",
    "\t",
    "$",
    "$1",
    "$+",
    "$>",
    "<$",
    "bare",
    ";",
    "( )",
    "-> $Q",
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
        // Random from/close (clamped so from <= close <= len), PLUS the full span, per target.
        let from = if len == 0 { 0 } else { rng.below(len + 1) };
        let close = if len == 0 { 0 } else { from + rng.below(len - from + 1) };
        for target in TARGETS {
            agree(b, from, close, target); // random window (may land mid-member / mid-opaque)
            agree(b, 0, len, target); // full span
        }
    }
}

/// The fuzz generator must reach BOTH empty and non-empty results — a generator that only ever
/// produces the empty vector (or only ever finds members) tests half the machine. Assert, by
/// running the system, that both outcomes occur many times over the seed range, that multi-member
/// results occur, and that the corpus is diverse (not one string repeated).
#[test]
fn fuzz_has_teeth() {
    use std::collections::HashSet;
    let mut distinct = HashSet::new();
    let mut empty_results = 0usize;
    let mut nonempty_results = 0usize;
    let mut multi_member_results = 0usize;
    for seed in 0u64..3000 {
        let mut rng = Rng::new(seed ^ 0x5A5A_FFFF);
        let src = gen_frame_ish(&mut rng, 12);
        let b = src.as_bytes();
        distinct.insert(src.clone());
        for target in TARGETS {
            let starts = member_starts(b, 0, b.len(), target);
            if starts.is_empty() {
                empty_results += 1;
            } else {
                nonempty_results += 1;
            }
            if starts.len() >= 2 {
                multi_member_results += 1;
            }
        }
    }
    assert!(distinct.len() > 1500, "fuzz generator not diverse: {} distinct", distinct.len());
    assert!(empty_results > 0, "fuzz never produced an EMPTY result — the zero-member path is untested");
    assert!(nonempty_results > 100, "fuzz produced too few NON-empty results ({nonempty_results}) — lacks teeth");
    assert!(multi_member_results > 50, "fuzz produced too few multi-member results ({multi_member_results})");
}

// ===========================================================================
// I1 byte-partition through the REAL pipeline. Drive `segment()` on full `.frm`
// files whose states contain state vars + handlers, then assert the tree covers
// every byte. `check_coverage` is the top-level partition; `unparse` round-trip is
// the constructive form; `check_total` recurses INTO each state's StateVar/Handler/
// Trivia members (built by the native driver over `member_starts`), so a broken
// member partition surfaces as a `Gap`/`Overlap` — never silently. An
// `UndecomposedBlob` (un-parsed handler-body statement) is EXPECTED and tolerated;
// a Gap/Overlap is a real bug and fails the test.
// SCAFFOLDING (real pipeline + internal tree entry; conversion-internal).
// ===========================================================================

fn wrap_system(states: &str) -> String {
    // A complete, well-formed system whose `machine:` section is `states`.
    format!(
        "@@system S {{\n    interface:\n        go()\n    machine:\n{}\n}}\n",
        states
    )
}

/// Well-formed states (state var + handlers), brace-balanced on every target (incl. Python, where
/// `//` is not a comment — so decoys live only inside string literals here; the `//`/`#`-comment
/// cases are covered by the differential above).
const WELL_FORMED_STATES: &[&str] = &[
    "        $A {\n            $.count: int = 0\n            go() { }\n        }",
    "        $A {\n            $.n: int = 0\n            $>() { }\n            go() { }\n            <$() { }\n        }\n        $B {\n            tick() { }\n        }",
    "        $A {\n            $.x: int = 0\n            go() {\n                if p { q } else { r }\n            }\n            tick(): bool { return true }\n        }",
    // a string decoy inside a handler body carrying member-looking tokens — must not spawn members
    "        $A {\n            $.real: int = 0\n            go() {\n                s = \"$.fake go() { } <$() { }\"\n            }\n            $>() { }\n        }\n        $B {\n            done() { }\n        }",
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
            let src = Source::new("state_walk_partition.frm", bytes.clone()).expect("utf8 source");
            let ast = segment(&src, target).expect("segment should succeed");

            // I1 top-level: items partition [0, len).
            ast.check_coverage()
                .unwrap_or_else(|d| panic!("check_coverage failed for {target:?} on:\n{text}\n  => {d}"));

            // I1 constructive: byte-identical round-trip.
            let rebuilt = ast.unparse(&bytes);
            assert_eq!(rebuilt, bytes, "unparse != source for {target:?} on:\n{text}");

            // I1 RECURSIVE: traverse into each state's StateVar/Handler/Trivia members. A broken
            // member partition (a start off by a byte, an overlap, a dropped trivia gap) is a
            // Gap/Overlap here. An UndecomposedBlob is a still-unparsed handler-body stmt — expected.
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

/// A milestone-validation test: a state actually DECOMPOSES into exactly its StateVar + Handler
/// members end-to-end through `segment()`, and member-looking tokens buried in a string in a
/// handler body do NOT spawn phantom members. This is the observable outcome of the `StateWalk`
/// system driving `state()`'s member loop — a regression fails THIS named test.
#[test]
fn state_decomposes_into_the_right_members() {
    use frame_compiler::scan::segment;
    use frame_compiler::tree::{Item, MachineMember, Section, StateMember};
    use frame_compiler::Source;

    let text = wrap_system(
        "        $Work {\n            $.count: int = 0\n            $>() { }\n            go() {\n                s = \"$.fake go() { } <$() { }\"\n            }\n            <$() { }\n        }",
    );
    let src = Source::new("state_walk_members.frm", text.as_bytes().to_vec()).unwrap();
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
    assert_eq!(state.name, "Work");

    // The non-trivia members, in order: a StateVar `count`, then handlers `$>`, `go`, `<$`. The
    // `$.fake`/`go()`/`<$()` inside the string in `go`'s body are NOT members.
    let mut kinds: Vec<String> = Vec::new();
    for m in &state.members {
        match m {
            StateMember::StateVar(d) => kinds.push(format!("var:{}", d.name)),
            StateMember::Handler(h) => kinds.push(format!("handler:{}", h.event)),
            StateMember::Trivia(_) => {}
        }
    }
    assert_eq!(
        kinds,
        vec!["var:count", "handler:$>", "handler:go", "handler:<$"],
        "exactly $.count, $>, go, <$; the tokens inside the string are NOT members"
    );
}
