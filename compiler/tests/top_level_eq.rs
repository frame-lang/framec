//! **TopLevelEq — the top-level `=` finder, proven by running.**
//!
//! `top_level_eq::find` is generated from `top_level_eq.frs` (a `@@[scan(u8)]` counter automaton:
//! Dyck-1 depth over `()[]{}` + a digraph-guarded angle counter, `"`-only by composition with
//! StringScan). It is the shared correct-class primitive that retires the byte-blind `split_once('=')`
//! (B2 `parse_one_param`), the `while != b'='` leaf (B9 `decl_read::eq_or_end`), and — through
//! ParamScan — the emit-side `params_split`/`param_names` (B1).
//!
//! Coverage strategy (the `param_scan.rs`/`opaque_scan.rs` precedent):
//!   * a DIRECTED semantic battery — the actual #249 fixtures (associated-type binding, generic,
//!     string-hidden `=`, bracket-hidden `=`, `->`/`=>`/`==` digraphs, no-`=`, `from > 0`);
//!   * an INDEPENDENT hand oracle `hand_find` (a second implementation of the same walk, composing
//!     the SAME public `string_scan` opacity primitive), asserted `find ≡ hand_find` at every
//!     position across a directed corpus AND deterministic fuzz over an alphabet that exercises
//!     angles, all three bracket kinds, `"`-strings, the digraphs, and `=`.
//!
//! Every test here is SCAFFOLDING (it depends on the internal `top_level_eq`/`string_scan` entry
//! points); it NEVER promotes to the cross-language corpus.

use frame_compiler::text::scan::{string_scan, top_level_eq};

// ============================================================================
// INDEPENDENT HAND ORACLE — a second implementation of the walk. It composes the SAME public
// `string_scan` opacity primitive (already independently tested), and re-states the O(1) digraph
// guards; agreement therefore checks the generated DISPATCH + the two counters against a hand walk.
// ============================================================================

/// Replicates `arg_scan::angle_guard`: a `<`/`>` that is part of `<= >= -> =>` is NOT counted.
fn angle_guard_hand(src: &[u8], i: usize, from: usize, to: usize) -> bool {
    if src[i] == b'<' {
        return i + 1 < to && src[i + 1] == b'=';
    }
    (i + 1 < to && src[i + 1] == b'=') || (i > from && (src[i - 1] == b'-' || src[i - 1] == b'='))
}

/// Replicates `top_level_eq::eq_is_sep`: a lone assignment `=`, not part of `== <= >= != =>`.
fn eq_is_sep_hand(src: &[u8], i: usize, from: usize, to: usize) -> bool {
    let prev_ok = i == from || !matches!(src[i - 1], b'=' | b'!' | b'<' | b'>');
    let next_ok = i + 1 >= to || !matches!(src[i + 1], b'=' | b'>');
    prev_ok && next_ok
}

/// The reference: first `=` at bracket-depth 0 AND angle-depth 0, `"`-opaque, digraph-excluded; or `to`.
fn hand_find(bytes: &[u8], from: usize, to: usize) -> usize {
    let mut depth = 0i32;
    let mut adepth = 0i32;
    let mut i = from;
    while i < to {
        // Opacity FIRST — mirror the machine's `skip_string` leaf exactly (only `"` triggers).
        let sk = if bytes[i] == b'"' {
            string_scan::scan(bytes, i).unwrap_or(i)
        } else {
            i
        };
        if sk > i {
            i = sk;
            continue;
        }
        let b = bytes[i];
        if (b == b'<' || b == b'>') && depth == 0 {
            if !angle_guard_hand(bytes, i, from, to) {
                if b == b'<' {
                    adepth += 1;
                } else if adepth > 0 {
                    adepth -= 1;
                }
            }
            i += 1;
            continue;
        }
        if b == b'(' || b == b'[' || b == b'{' {
            depth += 1;
            i += 1;
            continue;
        }
        if b == b')' || b == b']' || b == b'}' {
            if depth > 0 {
                depth -= 1;
            }
            i += 1;
            continue;
        }
        if b == b'=' && depth == 0 && adepth == 0 && eq_is_sep_hand(bytes, i, from, to) {
            return i;
        }
        i += 1;
    }
    to
}

fn find(s: &str) -> usize {
    top_level_eq::find(s.as_bytes(), 0, s.len())
}

// ============================================================================
// DIRECTED SEMANTIC BATTERY — the #249 fixtures.
// ============================================================================

#[test]
fn plain_default_found() {
    // `n: int = 5` — the `=` is at offset 7 (the real separator).
    assert_eq!(find("n: int = 5"), 7);
    assert_eq!("n: int = 5".as_bytes()[7], b'=');
}

#[test]
fn no_default_returns_to() {
    // `amount: int` / `m: Map<K, V>` — no top-level `=`, so `find == len`.
    assert_eq!(find("amount: int"), "amount: int".len());
    assert_eq!(find("m: Map<K, V>"), "m: Map<K, V>".len());
}

#[test]
fn associated_type_binding_eq_is_not_the_separator() {
    // B2: `x: impl Iterator<Item = u8>` — the `=` is inside `<…>`, NOT a separator. `find == len`.
    let s = "x: impl Iterator<Item = u8>";
    assert_eq!(find(s), s.len(), "the `=` inside `<Item = u8>` must NOT be taken");
    // The SAME type WITH a real default: the separator is the top-level `=`, past the angles.
    let d = "x: impl Iterator<Item = u8> = def";
    let eq = find(d);
    assert_eq!(d.as_bytes()[eq], b'=');
    assert_eq!(&d[..eq].trim_end(), &"x: impl Iterator<Item = u8>");
    assert_eq!(&d[eq + 1..].trim(), &"def");
}

#[test]
fn nested_generic_eq_protected() {
    // `x: Box<dyn Iterator<Item = u8>> = 0` — nested angles; separator is the top-level `=`.
    let s = "x: Box<dyn Iterator<Item = u8>> = 0";
    let eq = find(s);
    assert_eq!(&s[..eq].trim_end(), &"x: Box<dyn Iterator<Item = u8>>");
    assert_eq!(&s[eq + 1..].trim(), &"0");
}

#[test]
fn eq_inside_string_default_is_after_the_separator() {
    // `s: str = "a=b"` — the FIRST `=` (the separator) precedes the string; the `=` inside `"a=b"`
    // is never reached. And a `,`/`=` hidden in a string never fools the walk.
    let s = r#"s: str = "a=b""#;
    let eq = find(s);
    assert_eq!(s.as_bytes()[eq], b'=');
    assert_eq!(&s[..eq].trim_end(), &"s: str");
    assert_eq!(&s[eq + 1..].trim(), &r#""a=b""#);
}

#[test]
fn bracket_hidden_eq_protected() {
    // A `=` inside a `[...]`/`(...)`/`{...}` in the type is not the separator.
    assert_eq!(find("x: [u8; N == 4]"), "x: [u8; N == 4]".len());
    let s = "cb: fn(a = 1) = g";
    let eq = find(s);
    assert_eq!(&s[..eq].trim_end(), &"cb: fn(a = 1)");
    assert_eq!(&s[eq + 1..].trim(), &"g");
}

#[test]
fn digraphs_are_not_separators() {
    // `->` `=>` `==` `<=` `>=` `!=` never count as the separator `=`.
    let s = "f: Fn() -> R = g"; // the `->` `>` is guarded; the lone `=` is the separator
    let eq = find(s);
    assert_eq!(&s[..eq].trim_end(), &"f: Fn() -> R");
    assert_eq!(&s[eq + 1..].trim(), &"g");
    // A `=>` is not a separator; the following lone `=` is.
    let t = "k: T = a => b";
    let eq2 = find(t);
    assert_eq!(&t[..eq2].trim_end(), &"k: T");
    assert_eq!(&t[eq2 + 1..].trim(), &"a => b");
    // `==` alone has no separator.
    assert_eq!(find("a == b"), "a == b".len());
}

#[test]
fn from_offset_respected() {
    // `find` with `from > 0` (the decl_read `eq_or_end` call shape: scan the TYPE window only).
    let s = "x: impl Iterator<Item = u8> = def";
    // Start scanning at the type (past `x: `, offset 3). The angle `=` is still protected; the
    // real separator is found.
    let eq = top_level_eq::find(s.as_bytes(), 3, s.len());
    assert_eq!(s.as_bytes()[eq], b'=');
    assert_eq!(&s[3..eq].trim_end(), &"impl Iterator<Item = u8>");
    // A window that ENDS before the real `=` finds nothing.
    let cut = "x: Map<K, V>".len();
    assert_eq!(top_level_eq::find(s.as_bytes(), 3, cut), cut);
}

// ============================================================================
// DIRECTED DIFFERENTIAL — `find ≡ hand_find` on a handpicked corpus (every position).
// ============================================================================

const CORPUS: &[&str] = &[
    "",
    "=",
    "==",
    "a = b",
    "a == b",
    "n: int = 5",
    "amount: int",
    "m: Map<K, V>",
    "m: Map<K, V> = d",
    "x: impl Iterator<Item = u8>",
    "x: impl Iterator<Item = u8> = def",
    "x: Box<dyn Iterator<Item = u8>> = 0",
    "s: str = \"a=b\"",
    "s: str = \"=\"",
    "cb: fn(a = 1) = g",
    "x: [u8; N == 4]",
    "f: Fn() -> R = g",
    "k: T = a => b",
    "a <= b = c",
    "a >= b = c",
    "a != b = c",
    "Vec::<u8>::new() = z",
    "<>=",
    "((=))",
    "[{<=>}]=",
    "\"unterminated = x",
    "'c' = q",
];

#[test]
fn directed_differential_agrees() {
    for &s in CORPUS {
        let b = s.as_bytes();
        assert_eq!(
            top_level_eq::find(b, 0, b.len()),
            hand_find(b, 0, b.len()),
            "find != hand_find on {s:?}"
        );
        // Every non-empty window offset must also agree with the hand oracle.
        for from in 0..=b.len() {
            assert_eq!(
                top_level_eq::find(b, from, b.len()),
                hand_find(b, from, b.len()),
                "find != hand_find on {s:?} from {from}"
            );
        }
    }
}

// ============================================================================
// DETERMINISTIC FUZZ — xorshift64*, random interiors over an alphabet that exercises angles, all
// three bracket kinds, `"`-strings, the digraphs, and `=`. Every case must (a) be deterministic
// and (b) agree with the independent hand oracle. A coverage gate ensures the `found` arm is hit.
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

const FRAGMENTS: &[&[u8]] = &[
    b"a", b"bb", b"x1", b"_id", b"0", b"42", b": ", b" = ", b"=", b"==", b"<", b">", b"<=", b">=",
    b"->", b"=>", b"!=", b"(", b")", b"[", b"]", b"{", b"}", b"<T>", b"Map<K, V>", b"Item = u8",
    b" ", b"\t", b"\"", b"\"x=y\"", b",",
];

fn gen(rng: &mut Rng, max_frags: usize) -> Vec<u8> {
    let n = rng.below(max_frags + 1);
    let mut v: Vec<u8> = Vec::new();
    for _ in 0..n {
        v.extend_from_slice(FRAGMENTS[rng.below(FRAGMENTS.len())]);
    }
    v
}

#[test]
fn fuzz_differential() {
    let mut found = 0usize;
    for seed in 0u64..20000 {
        let mut rng = Rng::new(seed ^ 0x51ED_0000);
        let b = gen(&mut rng, 12);
        // Determinism (a leaked register would break this).
        let a = top_level_eq::find(&b, 0, b.len());
        let c = top_level_eq::find(&b, 0, b.len());
        assert_eq!(a, c, "nondeterminism: seed {seed} of {b:?}");
        // Differential against the independent hand oracle.
        assert_eq!(a, hand_find(&b, 0, b.len()), "find != hand_find: seed {seed} of {b:?}");
        if a < b.len() {
            found += 1;
        }
    }
    assert!(found > 500, "fuzz reached too few `found` cases ({found}) — the arm is thin");
}
