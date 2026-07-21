//! **ParamSplit agrees with the retired string-BLIND hand comma loop everywhere the ledger
//! says CARRY — and provably DIVERGES, correctly, wherever a `"…"`-string is in play —
//! proven by running.**
//!
//! `paramsplit::split` is generated from `paramsplit.frs` (a `@@[scan(u8)]` merged-Dyck COUNTER
//! automaton, STRING-AWARE by composition with StringScan). It replaces the string-BLIND hand
//! depth-0 comma loop that used to live in `scan/mod.rs::split_system_params` — the top-level
//! comma-split of a `@@system Name(...)` param-list interior. From the interior bytes it emits the
//! top-level comma-split EXTENTS `(start, end)`; a `,` separates only at merged `()[]<>{}` depth 0
//! AND outside a `"…"`-string default.
//!
//! The differential oracle here — `split_extents_hand` — reproduces the EXACT pre-conversion
//! loop: track `()[]<>{}` depth, split at a depth-0 `,`, plus the `start < len` tail, with NO
//! string model at all. The machine's alphabet is byte-for-byte identical (same five bracket
//! kinds, same comma), so the SOLE intended divergence is string-awareness: on any input with no
//! `"` the two agree exactly (CARRIED); when a `,`/bracket is buried in a `"…"`-string the machine
//! (correctly) skips it and the oracle does not (a FIXED row).
//!
//! Coverage strategy (the `delim_balance`/`arg_scan` fix-with-teeth + fuzz pattern):
//!   * a CARRIED corpus (no `"`) asserting `split == split_extents_hand` byte-for-byte — the
//!     intentional-identity claim on the shared merged-Dyck alphabet;
//!   * a partition-aware DIFFERENTIAL over a curated `"`-bearing corpus + a deterministic
//!     xorshift fuzz: for each input, `split == hand` (CARRIED) OR they diverge (a FIXED row),
//!     where the machine's extents must still be WELL-FORMED (ordered, contiguous, comma-
//!     separated, jointly partitioning the interior) — checked hand-independently;
//!   * TEETH: the fuzz must reach many FIXED rows (else the partition arm is vacuous);
//!   * THE NAMED FIX CASE — `$(a: T), $>(b: T = "x,y"), c: Map<K,V> = d` — explicit 3-vs-4;
//!   * the CARRIED-GAP PIN — `= ')'` single-quote defaults are STILL miscounted (`"`-only), with
//!     the double-quote contrast proving the boundary is exactly the `"`-only StringScan;
//!   * a MILESTONE through the real `segment()` pipeline: a `@@system` header whose param default
//!     carries a comma inside a `"…"`-string splits into the correct param count.
//!
//! Every differential/fix/carried test here is SCAFFOLDING: it depends on the string-blind hand
//! oracle `split_extents_hand`, or on the cleanroom-only `@@[scan(u8)]`-on-`@@system` capability
//! (`paramsplit::split`) / the internal `segment()` tree API. It NEVER promotes to the
//! cross-language corpus and dies at C-final when the hand-independent well-formedness pins take
//! over.

use frame_compiler::text::scan::literals::Target;
use frame_compiler::text::scan::{paramsplit, segment, string_scan};
use frame_compiler::tree::Item;
use frame_compiler::Source;

// ============================================================================
// THE DIFFERENTIAL ORACLE — the exact pre-conversion string-BLIND depth-0 comma loop.
// Independent of the FSM (it is the retired hand code, a distinct code path). Deleted at
// C-final; until then it keeps the fix teeth honest.
// ============================================================================

/// The retired string-BLIND top-level comma-split: track `()[]<>{}` depth, split at a depth-0
/// `,` into extents `[part_start, comma)`, then a final `[part_start, len)` tail iff nonempty.
/// NO string model — a `,`/bracket inside a `"…"` is counted like any other byte (the bug the
/// conversion fixes). Byte-for-byte the pre-conversion `split_system_params` loop.
#[doc(hidden)]
fn split_extents_hand(bytes: &[u8]) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut depth = 0i32;
    let mut part_start = 0usize;
    let mut i = 0usize;
    while i < bytes.len() {
        let b = bytes[i];
        if b == b',' && depth == 0 {
            out.push((part_start, i));
            part_start = i + 1;
        }
        if b == b'(' || b == b'[' || b == b'<' || b == b'{' {
            depth += 1;
        }
        if b == b')' || b == b']' || b == b'>' || b == b'}' {
            depth -= 1;
        }
        i += 1;
    }
    if part_start < bytes.len() {
        out.push((part_start, bytes.len()));
    }
    out
}

// ============================================================================
// HAND-INDEPENDENT WELL-FORMEDNESS — the invariant the extents must satisfy no matter what,
// derived from the split's own contract (survives oracle deletion). The extents, in order,
// with a single separating `,` between consecutive ones, reconstruct a prefix of the interior;
// the only permitted uncovered suffix is empty or a single trailing comma (the dropped empty
// tail). This alone proves: ordered, non-overlapping, contiguous, `s <= e <= len`, comma-
// separated, jointly partitioning. Panics with the offending input on any violation.
// ============================================================================

fn assert_well_formed(bytes: &[u8], extents: &[(usize, usize)], ctx: &str) {
    let len = bytes.len();
    let mut pos = 0usize;
    for (idx, &(s, e)) in extents.iter().enumerate() {
        assert!(
            s <= e && e <= len,
            "extent {idx} out of range ({s},{e}) len {len}: {ctx} of {bytes:?}"
        );
        assert_eq!(
            s, pos,
            "extent {idx} not contiguous (start {s} != expected {pos}): {ctx} of {bytes:?}"
        );
        pos = e;
        if idx + 1 < extents.len() {
            assert!(
                pos < len && bytes[pos] == b',',
                "gap after extent {idx} is not a `,` (byte at {pos}): {ctx} of {bytes:?}"
            );
            pos += 1; // step over the separating comma
        }
    }
    assert!(
        pos == len || (pos + 1 == len && bytes[pos] == b','),
        "uncovered tail [{pos},{len}) is neither empty nor a lone trailing comma: {ctx} of {bytes:?}"
    );
}

/// Partition-aware differential: `split == hand` (CARRIED — no `"` in play) OR they diverge (a
/// FIXED row — a `,`/bracket buried in a `"…"`-string), where the machine's extents must still be
/// WELL-FORMED. Returns true on a FIXED row (for the teeth counters).
fn agree_or_fixed(bytes: &[u8], ctx: &str) -> bool {
    let m = paramsplit::split(bytes);
    let h = split_extents_hand(bytes);
    // The machine is well-formed on EVERY input, carried or fixed.
    assert_well_formed(bytes, &m, ctx);
    if m == h {
        return false;
    }
    true
}

/// A no-`"` input MUST agree with the oracle byte-for-byte (the intentional-identity claim on the
/// shared merged-Dyck alphabet), and be well-formed.
fn assert_carried(bytes: &[u8]) {
    assert!(
        !bytes.contains(&b'"'),
        "assert_carried given a `\"`-bearing input {bytes:?} — use agree_or_fixed"
    );
    let m = paramsplit::split(bytes);
    let h = split_extents_hand(bytes);
    assert_well_formed(bytes, &m, "carried");
    assert_eq!(
        m, h,
        "CARRIED divergence on the shared alphabet (no `\"` present): {bytes:?}"
    );
}

// ============================================================================
// CARRIED corpus — no `"`, so machine == hand byte-for-byte. Nested groups, unbalanced both
// ways, the five bracket kinds, sigil forms, empty/EOF edges, adversarial depth.
// ============================================================================

#[test]
fn carried_no_string_agrees_byte_for_byte() {
    for s in [
        "",                       // empty
        ",",                      // lone separator (empty middle recorded, empty tail dropped)
        ",,",                     // two separators
        "a",                      // single tail, no comma
        "a,",                     // trailing comma (empty tail dropped)
        ",a",                     // leading comma (empty middle recorded)
        "a,b",                    // two parts
        "a,,b",                   // empty middle recorded
        "a, b, c",                // whitespace jitter (extents untrimmed)
        "  a  ,  b  ",            // more jitter
        "f(a, b), g(c, d)",       // () protects interior commas
        "[a, b], {c, d}",         // [] and {} protect
        "a<b, c>d",               // <> counted: `<` opens, matched `>` closes → both commas split? see below
        "Map<K, V>",              // angle-protected comma (one part)
        "f(g(h(1,2),3),4), y",    // deep nesting
        "(((",                    // unbalanced opens
        ")))",                    // unbalanced closes (depth negative silences nothing here)
        "a)b,c",                  // stray closer drives depth negative → later comma silenced
        "a(b,c",                  // unbalanced open → comma protected, no tail split
        "$(x), $>(y), z",         // the sigil forms (note $> counts the `>`)
        "$()",                    // empty state group
        "a: T = 0, b: U = 1",     // realistic domain params
        "n: N = 0",               // single typed default
    ] {
        assert_carried(s.as_bytes());
    }
}

// ============================================================================
// FIXED corpus — a `,`/bracket buried in a `"…"`-string. Machine skips it (correct), the
// string-blind oracle does not. Every one must be a FIXED row with a WELL-FORMED machine result.
// ============================================================================

#[test]
fn fixed_string_bearing_diverges_and_is_well_formed() {
    // MUST-DIVERGE: a `,` or an UNBALANCED bracket buried in a depth-0 `"…"`-string. The machine
    // skips the string (correct); the string-blind oracle counts through it → a FIXED row.
    for s in [
        "\"a,b\"",          // a whole interior that is one string with a comma
        "x, \"y,z\", w",    // a string-protected comma between two real ones
        "\"(\" , a",        // an unbalanced `(` hidden in a string raises the oracle's depth
        "\")\" , b",        // an unbalanced `)` hidden in a string drives the oracle negative
        "a = \"p,q\", b",   // a named default carrying a comma
        "\"a\\\"b, c\", d", // an escaped quote inside; the real comma is after the close
        "\"a,b\", \"c,d\"", // two string-protected commas
    ] {
        let b = s.as_bytes();
        assert!(
            agree_or_fixed(b, "must-diverge"),
            "expected a FIXED (string-aware) divergence on {s:?}, but machine == string-blind hand"
        );
    }

    // MUST-CARRY (documented boundary): a string whose comma is ALSO protected by in-string
    // balanced brackets, or that sits inside a REAL `()` group (oracle depth already >= 1), or is
    // UNTERMINATED (skip_string declines) — the string skip changes nothing, so machine == hand.
    // These pin WHERE string-awareness is inert, so the fix's scope is recorded, not assumed.
    for s in [
        "\"[,]\", c",  // in-string brackets balance → oracle never splits inside either
        "f(\"x,y\"), z", // string comma inside a real () group → oracle depth 1 protects it
        "a, \"b,c",    // UNTERMINATED string → skip_string declines → machine is string-blind here
    ] {
        let b = s.as_bytes();
        assert!(
            !agree_or_fixed(b, "must-carry"),
            "expected CARRIED (machine == hand) on {s:?}, but they diverged"
        );
    }
}

// ============================================================================
// THE NAMED FIX CASE (the whole point) — `$(a: T), $>(b: T = "x,y"), c: Map<K,V> = d`.
// The machine (string-aware) yields exactly 3 extents; the string-blind hand yields 4,
// corruptly splitting inside `"x,y"`. Asserted with the exact extents AND explicitly by the
// string-aware content (the machine keeps `"x,y"` whole; the hand places a boundary inside it).
//
// NOTE on the extent boundaries: this scanner's merged-Dyck alphabet counts the `$>` sigil's own
// `>` and the `Map<K,V>` angle bytes exactly as the retired hand loop did (that is the CARRIED
// half — same alphabet), so the middle boundary lands at the `,` inside `Map<K,V>`, not at the
// clean end of `$>(b: T = "x,y")`. The ONLY difference between machine and hand on this input is
// the string skip — the machine's middle extent (8,35) is precisely the hand's two string-
// straddling parts (8,21)+(22,35) fused because the `"x,y"` comma was not counted.
// ============================================================================

#[test]
fn named_fix_case_string_aware_three_vs_four() {
    let s = r#"$(a: T), $>(b: T = "x,y"), c: Map<K,V> = d"#;
    let b = s.as_bytes();
    let m = paramsplit::split(b);
    let h = split_extents_hand(b);

    // The `"x,y"` string occupies bytes [19,24) — pinned hand-independently via StringScan.
    assert_eq!(string_scan::scan(b, 19), Some(24), "the `\"x,y\"` string extent moved");
    let (str_lo, str_hi) = (19usize, 24usize);

    // String-AWARE: exactly 3 parts, with the exact fused extents.
    assert_eq!(
        m,
        vec![(0, 7), (8, 35), (36, 42)],
        "machine (string-aware) extents wrong"
    );
    // String-BLIND hand: exactly 4 parts — the comma inside `"x,y"` is corruptly split.
    assert_eq!(
        h,
        vec![(0, 7), (8, 21), (22, 35), (36, 42)],
        "hand (string-blind) extents wrong"
    );
    assert_eq!(m.len(), 3, "machine must yield exactly 3 parts");
    assert_eq!(h.len(), 4, "hand must yield exactly 4 parts");

    // The fix is NON-VACUOUS: the two disagree, and the machine is well-formed.
    assert_ne!(m, h, "fix VACUOUS — machine and string-blind hand agree");
    assert_well_formed(b, &m, "named-fix");

    // Explicit string-aware result: NO machine boundary falls strictly inside `"x,y"`; the middle
    // machine extent contains the WHOLE `"x,y"` literal verbatim.
    for &(s0, e0) in &m {
        assert!(!(str_lo < s0 && s0 < str_hi), "machine start {s0} inside the string [19,24)");
        assert!(!(str_lo < e0 && e0 < str_hi), "machine end {e0} inside the string [19,24)");
    }
    assert!(
        s[m[1].0..m[1].1].contains("\"x,y\""),
        "the middle machine part must carry the intact `\"x,y\"`: got {:?}",
        &s[m[1].0..m[1].1]
    );

    // The corruption, pinned on the hand: it DOES place a boundary (position 21) strictly inside
    // `"x,y"`, and the machine's fused extent equals the hand's two straddling parts joined.
    assert!(
        h.iter().any(|&(_, e)| str_lo < e && e < str_hi),
        "the string-blind hand must corruptly split inside `\"x,y\"`"
    );
    assert_eq!((h[1].0, h[2].1), (m[1].0, m[1].1), "machine fused == hand's straddling pair");
}

// ============================================================================
// CARRIED-GAP PIN — the `"`-only boundary, recorded not assumed. A single-quote `= ')'` char
// default is STILL miscounted: StringScan is `"`-only, so the `)` inside `')'` drives merged
// depth negative and SILENCES the following top-level comma. Both machine and hand miss it
// identically (CARRIED). The double-quote form of the SAME structure IS fixed — proving the gap
// is exactly the single-quote/`"`-only limitation, not a deeper defect.
// ============================================================================

#[test]
fn carried_gap_single_quote_still_miscounts() {
    // `a, b = ')', c` — the correct (single-quote-aware) split is 3 parts; the `)` inside `')'`
    // silences the comma after it, so BOTH the machine and the string-blind hand yield 2 parts.
    let sq = b"a, b = ')', c";
    let m = paramsplit::split(sq);
    let h = split_extents_hand(sq);
    assert_well_formed(sq, &m, "single-quote gap");
    assert_eq!(
        m, h,
        "single-quote gap must be CARRIED (machine == hand) — StringScan is `\"`-only"
    );
    assert_eq!(
        m.len(),
        2,
        "the `)` in `')'` silences the top-level comma — a semantic MISS, carried by design"
    );

    // Contrast: the SAME structure with DOUBLE quotes is FIXED — the `")"` is skipped, the comma
    // splits (machine 3 parts), while the string-blind hand still miscounts (2 parts).
    let dq = b"a, b = \")\", c";
    let dm = paramsplit::split(dq);
    let dh = split_extents_hand(dq);
    assert_well_formed(dq, &dm, "double-quote fix");
    assert_eq!(dm.len(), 3, "double-quote `)` is skipped — the comma splits (FIXED)");
    assert_eq!(dh.len(), 2, "string-blind hand still miscounts the double-quote `)`");
    assert_ne!(dm, dh, "the double-quote fix must be non-vacuous");
}

// ============================================================================
// DETERMINISTIC FUZZ — xorshift64*, random param-interior strings from a fragment alphabet that
// includes `"…"`-strings carrying commas/brackets (so FIXED rows are reached). Partition-aware
// differential on every case; a failing seed reproduces from its number. Plus a machine-only
// determinism + well-formedness invariant, and the teeth gate (FIXED rows > 0).
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

/// Whole-token fragments: carried structure (idents, `:`, `=`, all five bracket kinds, commas,
/// sigils, whitespace) PLUS terminated `"…"`-strings carrying commas/brackets/escapes — the only
/// way to reach a FIXED row — PLUS a lone `"` to occasionally form an unterminated tail.
const FRAGMENTS: &[&[u8]] = &[
    b"a", b"bb", b"x1", b"_id", b"0", b"42", b":", b" = ", b", ", b",", b" ", b"\t",
    b"(", b")", b"[", b"]", b"<", b">", b"{", b"}", b"()", b"[]", b"<>", b"{}",
    b"Map<K, V>", b"f(1, 2)", b"$(", b"$>(", b"$(x)", b"$>(y)", b"T", b"K", b"V",
    // string fragments that DRIVE the FIXED class:
    b"\"a,b\"", b"\"(\"", b"\")\"", b"\"[,]\"", b"\"x,y\"", b"\"\"", b"\"a\\\"b\"",
    b"= \"p,q\"", b"\", \"", b"\"<,>\"", b"\"{,}\"",
    // an unterminated-string driver (skip_string declines → carried):
    b"\"", b"\"tail",
];

fn gen_interior(rng: &mut Rng, max_frags: usize) -> Vec<u8> {
    let n = rng.below(max_frags + 1);
    let mut v: Vec<u8> = Vec::new();
    for _ in 0..n {
        v.extend_from_slice(FRAGMENTS[rng.below(FRAGMENTS.len())]);
    }
    v
}

#[test]
fn fuzz_partition_aware_differential() {
    let mut fixed = 0usize;
    let mut carried = 0usize;
    for seed in 0u64..12000 {
        let mut rng = Rng::new(seed ^ 0x9A5C_0000);
        let b = gen_interior(&mut rng, 10);
        // Determinism: two runs must agree (a leaked register would break this).
        let m1 = paramsplit::split(&b);
        let m2 = paramsplit::split(&b);
        assert_eq!(m1, m2, "nondeterminism: seed {seed} of {b:?}");
        // Partition-aware differential + machine well-formedness on every case.
        if agree_or_fixed(&b, &format!("FUZZ seed {seed}")) {
            fixed += 1;
        } else {
            carried += 1;
        }
    }
    // TEETH: the fuzz MUST reach the FIXED class (a string-buried `,`/bracket the machine skips
    // and the string-blind oracle does not) — else the partition arm is vacuous. It must also
    // reach the CARRIED class (agreement is exercised, not merely asserted on divergence).
    assert!(
        fixed > 200,
        "fuzz reached too few FIXED rows ({fixed}) — the string-aware divergence is vacuous"
    );
    assert!(
        carried > 200,
        "fuzz reached too few CARRIED rows ({carried}) — agreement is not exercised"
    );
}

// ============================================================================
// MILESTONE — end to end through the real `segment()` pipeline. A `@@system` header whose param
// default carries a `,` inside a `"…"`-string must split into the CORRECT param count: the string
// comma is protected. Under the retired string-blind loop the header would split into 4 domain
// params, corrupting the middle default. This is the paramsplit-specific end-to-end capability
// (distinct from the existing `paren_balance` header-close string test in tests/segmenter.rs).
//
// SCAFFOLDING: asserts the internal `segment()`/tree API (`SystemParams`), not emitted-code
// behavior — not promotable to the cross-language corpus.
// ============================================================================

#[test]
fn milestone_system_header_comma_in_string_default_is_protected() {
    // Three domain params; the middle one's default carries a top-level-looking `,` inside a
    // "…"-string. String-aware → 3 params, middle default intact. String-blind → 4, corrupted.
    let text = "@@system S(a: A, msg: String = \"x,y\", c: C) {\n\
                \x20   interface:\n\
                \x20       go()\n\
                \x20   machine:\n\
                \x20       $A { go() { } }\n\
                }\n";
    let src = Source::new("t.frm", text.as_bytes().to_vec()).unwrap();
    let ast = segment(&src, Target::Rust).unwrap();

    let sys = ast
        .items
        .iter()
        .find_map(|it| match it {
            Item::System(s) => Some(s),
            _ => None,
        })
        .expect("expected exactly one @@system");
    assert_eq!(sys.name, "S", "system name mis-parsed");

    // THE TEETH: exactly THREE domain params (not four), and the middle default is the INTACT
    // "…"-string INCLUDING the interior comma. Under the string-blind loop the split would land
    // inside `"x,y"`, yielding four params and a truncated `"x` default.
    assert!(sys.params.state.is_empty(), "no $() state params expected");
    assert!(sys.params.enter.is_empty(), "no $>() enter params expected");
    assert_eq!(
        sys.params.domain.len(),
        3,
        "param split corrupted (string comma not protected): {:#?}",
        sys.params.domain
    );
    assert_eq!(sys.params.domain[0].name, "a");
    assert_eq!(sys.params.domain[0].ty.as_deref(), Some("A"));
    assert_eq!(sys.params.domain[1].name, "msg");
    assert_eq!(sys.params.domain[1].ty.as_deref(), Some("String"));
    assert_eq!(
        sys.params.domain[1].default.as_deref(),
        Some("\"x,y\""),
        "the middle default was truncated — the string comma was miscounted (string-blind)"
    );
    assert_eq!(sys.params.domain[2].name, "c");
    assert_eq!(sys.params.domain[2].ty.as_deref(), Some("C"));
}
