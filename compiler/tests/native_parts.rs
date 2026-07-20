//! **Item 4 — the native-water decomposition, machine vs hand, proven structurally.**
//!
//! Production `native_parts` is a construction driver over the `NativePartsScan`
//! `@@[scan(u8)]` system (walk = island boundaries + kinds + text runs; nodes = fold +
//! `opaque_probe` registers + hole recursion). This battery is the Phase-1 parity gate:
//!
//! - **B-13, the structural recursive differential**: the FULL `Vec<NativePart>` tree
//!   (spans, Literal delim, Content/Hole spans, Hole parts, Ref kind/name, Inst
//!   name/args/named, Embed fields) compared as `format!("{:?}")` against the verbatim
//!   hand walk `native_parts_hand` — every `from`, every window, 4 targets, corpus + fuzz.
//! - **One pin per Phase-1 ledger row** (the carried swallows and glosses, asserted as
//!   CURRENT behavior — each names the Phase-2 delta that will flip it).
//! - **I1, recursively**: parts partition `[from, to)` exactly, and each Hole's parts
//!   partition its span.
//! - **Teeth**: the fuzz corpus must actually produce literals, holes, unterminated
//!   bodies, islands-inside-interiors, and clamped comments — counted, not hoped.
//!
//! Target scope is the R3 gate (4 cleanroom targets). The TS/JS fixtures that
//! tests/holes.rs carried beyond that gate are re-pinned there on 4-target equivalents;
//! their originals live HERE against the hand oracle until C-final.

use frame_compiler::text::scan::lex::Lexer;
use frame_compiler::text::scan::literals::Target;
use frame_compiler::text::scan::opaque_scan::{opaque_at, OpaqueAt};
use frame_compiler::text::scan::parts::{native_parts, native_parts_hand};
use frame_compiler::tree::body::{LiteralNode, LiteralPart, NativePart, RefKind};
use frame_compiler::tree::{check_total, Node};

const TARGETS: [Target; 4] = [Target::C, Target::Java, Target::Rust, Target::Python3];

fn machine(b: &[u8], from: usize, to: usize, t: Target) -> Vec<NativePart> {
    native_parts(b, from, to, t)
}
fn hand(b: &[u8], from: usize, to: usize, t: Target) -> Vec<NativePart> {
    native_parts_hand(&Lexer::new(b, t), b, from, to)
}

/// The structural differential on ONE window: full-tree Debug equality (the tree derives
/// `Debug` only, and Debug formatting prints every field of every node recursively).
fn agree_window(b: &[u8], from: usize, to: usize, t: Target) {
    let m = format!("{:?}", machine(b, from, to, t));
    let h = format!("{:?}", hand(b, from, to, t));
    assert_eq!(
        m, h,
        "structural disagreement on {:?} [{from},{to}) ({t:?})",
        String::from_utf8_lossy(b)
    );
}

/// Sweep: every `from` with `to = len`, every `to` with `from = 0` — subsumes the curated
/// island-straddling / comment-straddling / `to = hole.end` windows, since every boundary
/// value of `to` occurs.
fn agree_sweep(src: &str, t: Target) {
    let b = src.as_bytes();
    for from in 0..=b.len() {
        agree_window(b, from, b.len(), t);
    }
    for to in 0..=b.len() {
        agree_window(b, 0, to, t);
    }
}

/// Full `(from, to)` cross-product — for the short adversarial corpus. Partition-aware
/// (Δ1/Δ2): a divergence is a string-aware-hole / no-phantom-brace FIXED row, not a regression.
/// Returns the count of FIXED rows.
fn agree_all_windows(src: &str, t: Target) -> usize {
    let b = src.as_bytes();
    let mut fixed = 0usize;
    for from in 0..=b.len() {
        for to in from..=b.len() {
            fixed += agree_or_fixed_window(b, from, to, t) as usize;
        }
    }
    fixed
}

/// Partition-aware differential (the Phase-2 fix-with-teeth machinery): the machine agrees
/// with the hand oracle (a CARRIED row) OR it diverges (a FIXED row — the hand oracle stays
/// buggy: `hole_at` is string-blind, `native_parts_hand` still swallows unterminated interiors
/// and fabricates comment delims). On a fixed row the machine must still partition `[from,to)`
/// exactly and be recursively total (well-formedness — no regression into garbage); the EXACT
/// fixed-row trees are pinned by the directed B-tests and the oracle's own bug by the
/// `oracle_stayed_buggy` pins. Returns true on a fixed row (for the non-vacuity teeth).
fn agree_or_fixed_window(b: &[u8], from: usize, to: usize, t: Target) -> bool {
    let m = machine(b, from, to, t);
    if format!("{m:?}") == format!("{:?}", hand(b, from, to, t)) {
        return false;
    }
    check_partition(
        &m,
        from,
        to,
        &format!("fixed row {:?} [{from},{to}) {t:?}", String::from_utf8_lossy(b)),
    );
    true
}

/// Partition-aware sweep: every `from` (to = len) and every `to` (from = 0). Returns the count
/// of FIXED rows encountered (carried rows return 0).
fn agree_or_fixed_sweep(src: &str, t: Target) -> usize {
    let b = src.as_bytes();
    let mut fixed = 0usize;
    for from in 0..=b.len() {
        fixed += agree_or_fixed_window(b, from, b.len(), t) as usize;
    }
    for to in 0..=b.len() {
        fixed += agree_or_fixed_window(b, 0, to, t) as usize;
    }
    fixed
}

/// I1, recursive: the parts partition `[from, to)` exactly, each node passes
/// `check_total`, and each Hole's parts partition the hole's span (explicit recursion).
fn check_partition(parts: &[NativePart], from: usize, to: usize, ctx: &str) {
    let mut cursor = from;
    for p in parts {
        let s = (p as &dyn Node).span();
        assert_eq!(s.start, cursor, "gap/overlap in {ctx}");
        check_total(p as &dyn Node).expect("recursive totality");
        if let NativePart::Literal(l) = p {
            for lp in &l.parts {
                if let LiteralPart::Hole(h) = lp {
                    check_partition(&h.parts, h.span.start, h.span.end, ctx);
                }
            }
        }
        cursor = s.end;
    }
    assert_eq!(cursor, to, "parts must cover {ctx} to the last byte");
}

/// Every ref text found anywhere in the tree (any depth).
fn ref_texts(parts: &[NativePart], src: &str) -> Vec<String> {
    fn walk(ps: &[NativePart], src: &str, out: &mut Vec<String>) {
        for p in ps {
            match p {
                NativePart::Ref(r) => out.push(src[r.span.start..r.span.end].to_string()),
                NativePart::Literal(l) => {
                    for lp in &l.parts {
                        if let LiteralPart::Hole(h) = lp {
                            walk(&h.parts, src, out);
                        }
                    }
                }
                _ => {}
            }
        }
    }
    let mut out = Vec::new();
    walk(parts, src, &mut out);
    out
}

/// The first Literal node (any kind) in the parts, if one exists.
fn first_literal(parts: &[NativePart]) -> Option<&LiteralNode> {
    parts.iter().find_map(|p| match p {
        NativePart::Literal(l) => Some(l),
        _ => None,
    })
}

// ------------------------------------------------------------------ the ledger pins

/// B-1 (T-N1 — FLIPPED at Δ3, DP-1): an UNTERMINATED block comment's interior is no longer
/// scanned for islands — the rescued interior becomes ONE plain Text run to `to`. The lexer's
/// refusal is honored (content, not code); `native_parts` grows no diagnostics channel. FIXED
/// row — the hand oracle still island-scans the interior (oracle_stayed_buggy).
#[test]
fn b1_unterminated_comment_interior_is_one_text_run() {
    let src = "x /* never closes $.n";
    for t in [Target::C, Target::Java, Target::Rust] {
        // Teeth: the input really is on the `Err`/Unterminated path (a refusal, not a miss).
        assert_eq!(
            opaque_at(src.as_bytes(), 2, t),
            OpaqueAt::Unterminated,
            "the fixture must exercise the swallow ({t:?})"
        );
        let parts = machine(src.as_bytes(), 0, src.len(), t);
        // Pin (Δ3): no comment node, no `$.n` ref — the interior is ONE Text run to `to`.
        assert!(
            !parts.iter().any(
                |p| matches!(p, NativePart::Literal(l) if l.delim == b'/')
            ),
            "no comment node may be built from an unterminated comment ({t:?})"
        );
        assert!(
            ref_texts(&parts, src).is_empty(),
            "the unterminated interior is content, not code — no island ({t:?})"
        );
        assert_eq!(
            parts.len(),
            1,
            "the whole range is ONE plain Text run ({t:?})"
        );
        assert!(matches!(&parts[0], NativePart::Text(x) if x.span.start == 0 && x.span.end == src.len()));
        // FIXED row: the machine diverges from the still-island-scanning hand oracle.
        assert_ne!(
            format!("{parts:?}"),
            format!("{:?}", hand(src.as_bytes(), 0, src.len(), t)),
            "Δ3 fix VACUOUS: the hand oracle already agrees (it must still island-scan)"
        );
        check_partition(&parts, 0, src.len(), src);
    }
}

/// B-2 (T-N2 — FLIPPED at Δ3, DP-1): an unterminated LITERAL's interior is no longer
/// island-scanned — the `$.x` / `@@Sub()` inside the user's mis-terminated string (the
/// #224/#215 corruption class) is content, not code. The rescued interior becomes ONE plain
/// Text run. FIXED row — the hand oracle still splices those phantom islands (oracle_stayed_buggy).
#[test]
fn b2_unterminated_literal_interior_is_one_text_run() {
    let src = "a = \"unterminated $.x and @@Sub() ";
    for t in TARGETS {
        assert_eq!(
            opaque_at(src.as_bytes(), 4, t),
            OpaqueAt::Unterminated,
            "the fixture must exercise the literal swallow ({t:?})"
        );
        let parts = machine(src.as_bytes(), 0, src.len(), t);
        assert!(
            ref_texts(&parts, src).is_empty(),
            "NO ref is found inside the unterminated string's interior now ({t:?})"
        );
        assert!(
            !parts
                .iter()
                .any(|p| matches!(p, NativePart::Instantiate(i) if i.name == "Sub")),
            "NO instantiation is found inside the interior now ({t:?})"
        );
        // The whole range is ONE plain Text run to `to` (DP-1).
        assert_eq!(parts.len(), 1, "the interior is ONE plain Text run ({t:?})");
        assert!(matches!(&parts[0], NativePart::Text(x) if x.span.start == 0 && x.span.end == src.len()));
        // FIXED row: the machine diverges from the still-splicing hand oracle.
        assert_ne!(
            format!("{parts:?}"),
            format!("{:?}", hand(src.as_bytes(), 0, src.len(), t)),
            "Δ3 fix VACUOUS: the hand oracle already agrees (it must still island-scan)"
        );
        check_partition(&parts, 0, src.len(), src);
    }

    // The hole-interior variant (python): the unterminated `'` INSIDE a hole is likewise
    // text-run by the recursion — carried both before and after Δ3 (the interior has no
    // islands either way), so it stays a CARRIED row (machine == hand).
    let py = "f\"{ 'x } $.y \"";
    let parts = machine(py.as_bytes(), 0, py.len(), Target::Python3);
    let lit = first_literal(&parts).expect("the f-string is a literal");
    let hole = lit
        .parts
        .iter()
        .find_map(|lp| match lp {
            LiteralPart::Hole(h) => Some(h),
            _ => None,
        })
        .expect("the hole exists");
    assert!(
        hole.parts.iter().all(|p| matches!(p, NativePart::Text(_))),
        "the hole's unterminated `'x ` interior is one Text run (carried — no island either way)"
    );
    // `$.y` sits in the literal's CONTENT (after the hole) — not a ref, and that part is
    // correct forever: content is not code.
    assert!(ref_texts(&parts, py).is_empty());
    // Partition-aware: the FULL window is carried, but a sub-window that STARTS inside the
    // unterminated `'` is a Δ3 fixed row (the machine text-runs the interior + trailing `$.y`;
    // the string-blind hand splices the `$.y`).
    agree_or_fixed_sweep(py, Target::Python3);
}

/// `oracle_stayed_buggy` (Δ3 anti-vacuity): the fix teeth (`!= hand`) go VACUOUS if anyone
/// "repairs" the hand `native_parts_hand`. Pin that the hand STILL island-scans an unterminated
/// interior — it recognizes the `$.x` ref and the `@@Sub()` instantiation inside the user's
/// mis-terminated string — so any repair of the oracle is loud.
#[test]
fn oracle_stayed_buggy_unterminated_interior() {
    let comment = "x /* never closes $.n";
    let h = hand(comment.as_bytes(), 0, comment.len(), Target::C);
    assert_eq!(
        ref_texts(&h, comment),
        vec!["$.n"],
        "the hand oracle was fixed (no longer island-scans an unterminated comment) — Δ3 vacuous"
    );

    let lit = "a = \"unterminated $.x and @@Sub() ";
    let h2 = hand(lit.as_bytes(), 0, lit.len(), Target::C);
    assert_eq!(ref_texts(&h2, lit), vec!["$.x"], "the hand oracle still splices the phantom ref");
    assert!(
        h2.iter().any(|p| matches!(p, NativePart::Instantiate(i) if i.name == "Sub")),
        "the hand oracle still splices the phantom instantiation — else the Δ3 teeth are vacuous"
    );
}

/// B-3 (T-N3, carried): a literal whose extent crosses `to` is DEMOTED to water and its
/// interior island-scanned.
#[test]
fn b3_literal_straddling_to_demotes_to_water() {
    let src = "x = \"a $.b c\" y";
    for t in TARGETS {
        let b = src.as_bytes();
        // Full window: the string is a literal; `$.b` is content, NOT a ref.
        let full = machine(b, 0, b.len(), t);
        assert!(first_literal(&full).is_some());
        assert!(ref_texts(&full, src).is_empty());
        // `to` inside the string: demoted — no literal node, and the interior IS scanned.
        let to = 10; // mid-string, past `$.b`
        let cut = machine(b, 0, to, t);
        assert!(
            first_literal(&cut).is_none(),
            "a literal overrunning `to` must demote to water ({t:?})"
        );
        assert_eq!(
            ref_texts(&cut, &src[..to]),
            vec!["$.b"],
            "the demoted interior is island-scanned today ({t:?})"
        );
        agree_sweep(src, t);
    }
}

/// B-4 (T-N4, carried): the boundary asymmetry, both directions on the SAME input frame —
/// a comment crossing `to` is CLAMPED and still a comment node; a literal crossing `to`
/// demotes to water (B-3's policy). The hand code chose this silently; the walk's
/// `try_island` policy leaf now names it.
#[test]
fn b4_comment_clamps_where_literal_demotes() {
    for (src, comment_at_pos, t) in [
        ("a /* long comment */ b = \"sss\" c", 2, Target::C),
        ("a /* long comment */ b = \"sss\" c", 2, Target::Rust),
        ("a // tail comment\nb = \"sss\" c", 2, Target::Java),
        ("a # tail comment\nb = 'sss' c", 2, Target::Python3),
    ] {
        let b = src.as_bytes();
        // A `to` INSIDE the comment: clamped comment node, span ends exactly at `to`.
        let to_in_comment = comment_at_pos + 6;
        let parts = machine(b, 0, to_in_comment, t);
        let lit = first_literal(&parts).expect("clamped comment node exists");
        // Δ4: the comment delim is the real opener byte — `#` for Python, `/` for a `/`-comment.
        let expect_delim = if matches!(t, Target::Python3) { b'#' } else { b'/' };
        assert_eq!(lit.delim, expect_delim, "it is the comment kind ({t:?})");
        assert_eq!(
            lit.span.end, to_in_comment,
            "the comment is CLAMPED to `to`, not demoted ({t:?})"
        );
        // A `to` INSIDE the string: demoted (no STRING-LITERAL node at all — a string literal
        // has a quote delim `"`/`'`, distinct from a comment's `#`/`/`).
        let str_pos = src.find('=').unwrap() + 2;
        let to_in_string = str_pos + 2;
        let cut = machine(b, 0, to_in_string, t);
        assert!(
            !cut.iter()
                .any(|p| matches!(p, NativePart::Literal(l) if matches!(l.delim, b'"' | b'\''))),
            "the literal crossing `to` must DEMOTE ({t:?})"
        );
        // Partition-aware: the Python `#` comment is a Δ4 fixed row (machine `#` vs hand `/`).
        agree_or_fixed_sweep(src, t);
    }
}

/// B-5 (T-N5 — FLIPPED at Δ4): a comment node's `delim` is now the ACTUAL opener byte, sourced
/// from the probe — `#` for a Python `#` comment, `/` for a C/Java/Rust `//`|`/*`. FIXED row on
/// Python — the hand oracle still fabricates `b'/'` (oracle_stayed_buggy).
#[test]
fn b5_python_hash_comment_delim_is_the_real_opener() {
    let src = "x = 1 # a note";
    let parts = machine(src.as_bytes(), 0, src.len(), Target::Python3);
    let lit = first_literal(&parts).expect("the comment is a node");
    assert_eq!(lit.delim, b'#', "the Python comment delim is now the real opener `#` (Δ4)");
    // FIXED row: the machine diverges from the still-fabricating (`b'/'`) hand oracle.
    assert_ne!(
        format!("{parts:?}"),
        format!("{:?}", hand(src.as_bytes(), 0, src.len(), Target::Python3)),
        "Δ4 fix VACUOUS: the hand oracle already agrees (it must still fabricate `b'/'`)"
    );

    // A block/line comment on a `/`-comment target keeps delim `/` — carried (real opener == `/`).
    for (csrc, t) in [
        ("x = 1 // note", Target::Rust),
        ("a /* b */ c", Target::C),
    ] {
        let p = machine(csrc.as_bytes(), 0, csrc.len(), t);
        let l = first_literal(&p).expect("the comment is a node");
        assert_eq!(l.delim, b'/', "a `/`-comment's real opener is `/` ({t:?})");
    }
}

/// `oracle_stayed_buggy` (Δ4 anti-vacuity): the hand `native_parts_hand` still FABRICATES
/// `delim: b'/'` on every comment node, including a Python `#` comment. Any repair makes the
/// Δ4 fix teeth vacuous.
#[test]
fn oracle_stayed_buggy_comment_delim_fabrication() {
    let src = "x = 1 # a note";
    let h = hand(src.as_bytes(), 0, src.len(), Target::Python3);
    let lit = first_literal(&h).expect("the comment is a node");
    assert_eq!(
        lit.delim,
        b'/',
        "the hand oracle was fixed (no longer fabricates `b'/'`) — the Δ4 fix teeth are vacuous"
    );
}

/// B-6 (T-N7 / R6 — FLIPPED at Δ1): string-AWARE hole delimitation. In `f"{ d['}'] }"` the
/// hole now closes at the REAL `}` (position 11), not the one hidden inside the `'}'` string —
/// because `hole_skip` routes through the opaque-aware `DelimBalance`, which skips the in-string
/// `}`. Item 4 makes holes CODE nodes, so the CORRECT extent now shapes the parts tree: the
/// `'}'` inside the hole is recognized as a nested literal (content, not a delimiter). This is a
/// FIXED row — the hand `hole_at` stays string-blind (pinned by `oracle_stayed_buggy`), so the
/// machine and the oracle diverge here (fix-with-teeth).
#[test]
fn b6_string_aware_hole_extent_pinned() {
    let src = "f\"{ d['}'] }\"";
    //         0123456789...  — f=0 "=1 {=2 ␠=3 d=4 [=5 '=6 }=7 '=8 ]=9 ␠=10 }=11 "=12
    let parts = machine(src.as_bytes(), 0, src.len(), Target::Python3);
    let lit = first_literal(&parts).expect("the f-string is a literal");
    assert_eq!(lit.span.end, src.len(), "outer extent unperturbed on THIS input");
    let hole = lit
        .parts
        .iter()
        .find_map(|lp| match lp {
            LiteralPart::Hole(h) => Some(h),
            _ => None,
        })
        .expect("one hole");
    assert_eq!(
        (hole.span.start, hole.span.end),
        (3, 11),
        "the CORRECT extent — closes at the REAL `}}` (11), skipping the one hidden in `'}}'` (Δ1)"
    );
    // The `'}'` inside the hole is now recognized as a nested literal (content, NOT a hole
    // delimiter): the interior is no longer all-water.
    assert!(
        hole.parts
            .iter()
            .any(|p| matches!(p, NativePart::Literal(l) if l.delim == b'\'')),
        "the `'}}'` inside the hole is a nested char literal now (string-aware)"
    );
    // FIXED row: the machine diverges from the still-string-blind hand oracle.
    assert_ne!(
        format!("{parts:?}"),
        format!("{:?}", hand(src.as_bytes(), 0, src.len(), Target::Python3)),
        "Δ1 fix VACUOUS: the hand oracle already agrees (it should stay string-blind)"
    );
    check_partition(&parts, 0, src.len(), src);
}

/// `oracle_stayed_buggy` (Δ1 anti-vacuity): the fix teeth (`!= hand`) go VACUOUS if anyone
/// "repairs" the hand `Lexer::hole_at`. Pin that the hand oracle STILL delimits the hole
/// string-blind — `native_parts_hand` closes the `f"{ d['}'] }"` hole at the first `}` (7),
/// inside the `'}'` string — so any repair of the oracle is loud.
#[test]
fn oracle_stayed_buggy_hole_blindness() {
    let src = "f\"{ d['}'] }\"";
    let h = hand(src.as_bytes(), 0, src.len(), Target::Python3);
    let lit = first_literal(&h).expect("the f-string is a literal");
    let hole = lit
        .parts
        .iter()
        .find_map(|lp| match lp {
            LiteralPart::Hole(x) => Some(x),
            _ => None,
        })
        .expect("one hole");
    assert_eq!(
        (hole.span.start, hole.span.end),
        (3, 7),
        "the hand oracle was fixed (no longer string-blind) — the Δ1 fix teeth are now vacuous"
    );
    assert!(
        hole.parts.iter().all(|p| matches!(p, NativePart::Text(_))),
        "the oracle's truncated hole interior must stay all-water (string-blind)"
    );
}

/// B-7 (T-N8 — FLIPPED at Δ2): the `{{` escape is now consumed WHOLE (both braces), so the
/// second `{` no longer opens a phantom hole. Escaped braces are content, not interpolation.
/// This is a FIXED row — the hand `Lexer::hole_at` still phantom-opens (oracle_stayed_buggy).
#[test]
fn b7_double_brace_no_phantom_hole() {
    // f"{{x}}" — NO phantom hole now; the escaped `x` is content.
    let src = "f\"{{x}}\"";
    let parts = machine(src.as_bytes(), 0, src.len(), Target::Python3);
    let lit = first_literal(&parts).expect("literal");
    let holes: Vec<(usize, usize)> = lit
        .parts
        .iter()
        .filter_map(|lp| match lp {
            LiteralPart::Hole(h) => Some((h.span.start, h.span.end)),
            _ => None,
        })
        .collect();
    assert_eq!(holes, Vec::<(usize, usize)>::new(), "no phantom hole (Δ2)");
    assert_ne!(
        format!("{parts:?}"),
        format!("{:?}", hand(src.as_bytes(), 0, src.len(), Target::Python3)),
        "Δ2 fix VACUOUS: the hand oracle already agrees (it must still phantom-open)"
    );
    check_partition(&parts, 0, src.len(), src);

    // f"{{$.n}}" — the escaped `$.n` is now content, NOT a live Ref.
    let src2 = "f\"{{$.n}}\"";
    let parts2 = machine(src2.as_bytes(), 0, src2.len(), Target::Python3);
    assert!(
        ref_texts(&parts2, src2).is_empty(),
        "escaped `$.n` is content, not code (Δ2)"
    );
    assert_ne!(
        format!("{parts2:?}"),
        format!("{:?}", hand(src2.as_bytes(), 0, src2.len(), Target::Python3)),
        "Δ2 fix VACUOUS on the ref case"
    );
    check_partition(&parts2, 0, src2.len(), src2);
}

/// `oracle_stayed_buggy` (Δ2 anti-vacuity): the hand `Lexer::hole_at` still opens a PHANTOM
/// hole on the second brace of `{{` — `native_parts_hand` records a hole `(4,5)` for `f"{{x}}"`.
/// Any repair makes the Δ2 fix teeth vacuous.
#[test]
fn oracle_stayed_buggy_double_brace_phantom() {
    let src = "f\"{{x}}\"";
    let h = hand(src.as_bytes(), 0, src.len(), Target::Python3);
    let lit = first_literal(&h).expect("literal");
    let holes: Vec<(usize, usize)> = lit
        .parts
        .iter()
        .filter_map(|lp| match lp {
            LiteralPart::Hole(x) => Some((x.span.start, x.span.end)),
            _ => None,
        })
        .collect();
    assert_eq!(
        holes,
        vec![(4, 5)],
        "the hand oracle was fixed (no longer phantom-opens `{{`) — the Δ2 fix teeth are vacuous"
    );
}

/// B-8 (T-N9, carried): dispatch priority — opaque → inst → embed → ref (inst/embed before
/// ref is load-bearing), tail flush, empty-run guard, adjacency.
#[test]
fn b8_dispatch_priority_and_adjacency() {
    for t in TARGETS {
        let b = "@@Sub(1)";
        let parts = machine(b.as_bytes(), 0, b.len(), t);
        assert_eq!(parts.len(), 1);
        assert!(
            matches!(&parts[0], NativePart::Instantiate(i) if i.name == "Sub"),
            "an instantiation, not a ref + water ({t:?})"
        );

        let b = "@@:self.a.b(1)";
        let parts = machine(b.as_bytes(), 0, b.len(), t);
        assert_eq!(parts.len(), 1);
        assert!(
            matches!(&parts[0], NativePart::EmbedCall(e) if e.field == "a" && e.method == "b"),
            "an embed call, not a ref ({t:?})"
        );

        let b = "@@:self.x";
        let parts = machine(b.as_bytes(), 0, b.len(), t);
        assert_eq!(parts.len(), 1);
        assert!(
            matches!(&parts[0], NativePart::Ref(r) if r.kind == RefKind::ContextSelf && r.name == "x"),
            "a plain ref, not an embed ({t:?})"
        );

        // Adjacency pack: no text between islands; empty-run guard (no zero-length Text).
        for src in ["$.a$.b@@:self.c.d()", "$.a@@Sub()$.b", "@@Sub()@@:params.k"] {
            let parts = machine(src.as_bytes(), 0, src.len(), t);
            check_partition(&parts, 0, src.len(), src);
            assert!(
                parts
                    .iter()
                    .all(|p| (p as &dyn Node).span().start < (p as &dyn Node).span().end),
                "no empty parts ({t:?})"
            );
            agree_sweep(src, t);
        }
    }
}

/// T-R3 (carried — an EARNED merge): `$.` / `@@:` with no name is not a ref; it stays
/// water by grammar design.
#[test]
fn tr3_bare_sigils_are_water() {
    for t in TARGETS {
        for src in ["$. alone", "@@: bare", "a $$. b @@ c"] {
            let parts = machine(src.as_bytes(), 0, src.len(), t);
            assert!(
                ref_texts(&parts, src).is_empty(),
                "no ref from a bare sigil in {src:?} ({t:?})"
            );
            agree_sweep(src, t);
        }
    }
}

/// B-11 (T-N6, leave-latent): depth-8 nested holes — correctness only. The recursion is a
/// pushdown whose stack is the host call stack; per-level walks are independent (the
/// decomposition of `[from,to)` is a pure function of `(bytes, from, to, target)`), so no
/// depth register exists and NO depth bound is asserted here — that absence is part of the
/// recorded leave-latent plea (void condition: suspension / streaming / depth limits /
/// "where was I" reporting).
#[test]
fn b11_deep_nested_holes_are_correct() {
    let mut src = String::from("$.deep");
    for _ in 0..8 {
        src = format!("f\"{{ {src} }}\"");
    }
    let parts = machine(src.as_bytes(), 0, src.len(), Target::Python3);
    assert_eq!(
        ref_texts(&parts, &src),
        vec!["$.deep"],
        "the ref survives 8 levels of hole nesting"
    );
    check_partition(&parts, 0, src.len(), &src);
    agree_window(src.as_bytes(), 0, src.len(), Target::Python3);
}

/// B-9 (T-R1, carried — flips at Δ5, gated on H-1): an UNKNOWN `@@:word` context defaults
/// to `ContextSelf` instead of the documented refusal — on the ROUTED production path (the
/// statement scanner's assign-LHS now runs `ref_scan::scan` — Item 4 Commit C) AND the
/// oracle, identically (the recorded proof that differentials carry glosses).
#[test]
fn b9_unknown_context_word_defaults_to_contextself() {
    use frame_compiler::scan::segment;
    use frame_compiler::text::scan::parts::frame_ref_at_hand;
    use frame_compiler::text::scan::ref_scan;
    use frame_compiler::tree::body::Stmt;
    use frame_compiler::tree::{Item, MachineMember, Section, StateMember};
    use frame_compiler::Source;

    // The real statement path: a handler body whose only statement is `@@:wat.x = 1`.
    let text = "@@system S {\n    interface:\n        go()\n    machine:\n        $A {\n            go() {\n                @@:wat.x = 1\n            }\n        }\n}\n";
    let src = Source::new("b9.frm", text.as_bytes().to_vec()).unwrap();
    let ast = segment(&src, Target::Rust).unwrap();
    let sys = ast
        .items
        .iter()
        .find_map(|i| match i {
            Item::System(s) => Some(s),
            _ => None,
        })
        .expect("a system");
    let machine_sec = sys
        .sections
        .iter()
        .find_map(|sec| match sec {
            Section::Machine(m) => Some(m),
            _ => None,
        })
        .expect("a machine section");
    let assign = machine_sec
        .members
        .iter()
        .find_map(|m| match m {
            MachineMember::State(s) => s.members.iter().find_map(|sm| match sm {
                StateMember::Handler(h) => h.body.stmts.iter().find_map(|st| match st {
                    Stmt::Assign(a) => Some(a),
                    _ => None,
                }),
                _ => None,
            }),
            _ => None,
        })
        .expect("the @@:wat.x assignment IS an Assign statement (via the gloss)");
    assert_eq!(
        assign.lhs.kind,
        RefKind::ContextSelf,
        "unknown context word silently defaults to ContextSelf today (Δ5 makes it a refusal)"
    );
    assert_eq!(assign.lhs.name, "x");

    // Both recognizers carry the gloss identically.
    let b = b"@@:wat.x = 1";
    let (k, n, _) = ref_scan::scan(b, 0).expect("system recognizes");
    assert_eq!((k, n.as_str()), (RefKind::ContextSelf, "x"));
    let r = frame_ref_at_hand(b, 0, b.len()).expect("oracle recognizes");
    assert_eq!((r.kind, r.name.as_str()), (RefKind::ContextSelf, "x"));
}

/// B-10 (T-R2, carried — flips at Δ5): prefix-overmatch — the kind ladder uses
/// `starts_with`, not segment-match, so `@@:database.k` reads as ContextData and
/// `@@:selfish.y` as ContextSelf. Same ladder in system and oracle.
#[test]
fn b10_prefix_overmatch_pinned() {
    use frame_compiler::text::scan::parts::frame_ref_at_hand;
    use frame_compiler::text::scan::ref_scan;

    for (src, kind, name) in [
        ("@@:database.k", RefKind::ContextData, "database.k"),
        ("@@:selfish.y", RefKind::ContextSelf, "selfish.y"),
        ("@@:paramsX.z", RefKind::ContextParams, "paramsX.z"),
    ] {
        let b = src.as_bytes();
        let (k, n, _) = ref_scan::scan(b, 0).expect("system recognizes");
        // NOTE the NAME is everything after the first `.` — `k`, `y`, `z` — while the KIND
        // came from a PREFIX match on the word. Both sides, identically.
        let expect_name = name.split_once('.').map(|(_, rest)| rest).unwrap_or(name);
        assert_eq!((k, n.as_str()), (kind, expect_name), "system on {src:?}");
        let r = frame_ref_at_hand(b, 0, b.len()).expect("oracle recognizes");
        assert_eq!(
            (r.kind, r.name.as_str()),
            (kind, expect_name),
            "oracle on {src:?}"
        );
    }
}

// ------------------------------------------------------------------ the differential gate

/// B-13a: the curated corpus — every ledger row's input class — swept structurally.
#[test]
fn structural_differential_curated_corpus() {
    let corpus: &[&str] = &[
        // Unterminated string/comment/raw/triple, in water:
        "x /* never closes $.n",
        "a = \"unterminated $.x and @@Sub() ",
        "let s = r#\"open $.x",
        "s = \"\"\"open $.x and @@Sub(",
        "s = '''tri $.y",
        // ... and in holes:
        "f\"{ 'x } $.y \"",
        "f\"{ /* } $.z \"",
        // The R6 / phantom-brace family:
        "f\"{ d['}'] }\"",
        "f\"{{x}}\"",
        "f\"{{}}\"",
        "f\"{{$.n}}\"",
        // Escapes:
        "s = \"a\\\"b $.c\"",
        "s = \"\\{ $.d }\"",
        // Adjacency + priority:
        "$.a$.b@@:self.c.d()",
        "@@Sub(1) @@:self.a.b(1) @@:self.x",
        "f($.a,@@:params.k)+$.c",
        // Char forms:
        "c = 'x'; d = $.n",
        // Nested holes (see also b11):
        "s = f\"a { f'b {$.deep} c' } d\"",
        // Comments, terminated:
        "a = 1 // $.x\nb = $.real",
        "a = 1 # $.x\nb = $.real",
        "/* c1 */ x /* c2 $.h */ y",
        // Nested block comment (Rust/C++ `nests: true`): the composed
        // walk/driver path over a self-nesting comment form (GATE-A C-1).
        "a /* /* nested $.h */ still */ c",
        // Plain and empty:
        "plain text, no islands",
        "",
    ];
    let mut fixed_rows = 0usize;
    for t in TARGETS {
        for src in corpus {
            fixed_rows += agree_or_fixed_sweep(src, t);
            // I1 on the machine output, full window.
            let parts = machine(src.as_bytes(), 0, src.len(), t);
            check_partition(&parts, 0, src.len(), src);
        }
    }
    // Non-vacuity: the corpus must actually exercise the Δ1 fixed class (`f"{ d['}'] }"`,
    // whose hole hides a `}` inside a string), or the partition-aware differential proves
    // nothing beyond the carried rows.
    assert!(
        fixed_rows > 0,
        "the curated corpus never reached a FIXED (string-aware-hole) row — Δ1 differential vacuous"
    );
}

/// B-13b: the short adversarial corpus under the FULL `(from, to)` cross-product —
/// literal-at-`to`-boundary, `to = hole.end`, empty windows, all of it, by exhaustion.
#[test]
fn structural_differential_all_windows() {
    let corpus: &[&str] = &[
        "a = \"s $.x\" b",
        "f\"{$.a}\" $.b",
        "x /* c */ $.r",
        "f\"{{x}}\" 'q'",
        "r#\"raw $.n\"# $.m",
        "# c\n$.k",
    ];
    let mut fixed_rows = 0usize;
    for t in TARGETS {
        for src in corpus {
            fixed_rows += agree_all_windows(src, t);
        }
    }
    // Non-vacuity: `f"{{x}}" 'q'` must reach the Δ2 no-phantom-brace FIXED class (machine has no
    // hole; the string-blind hand still phantom-opens the second `{`).
    assert!(
        fixed_rows > 0,
        "the all-windows corpus never reached a FIXED (no-phantom-brace) row — Δ2 differential vacuous"
    );
}

// ------------------------------------------------------------------ fuzz with teeth

struct Rng(u64);
impl Rng {
    fn new(seed: u64) -> Self {
        Rng(seed.max(1))
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

/// The Item-4 fragment pool: sigils, openers/closers, braces, escapes — biased so the
/// generator actually FORMS refs, instantiations, embed calls, literals, holes, comments,
/// unterminated bodies, and boundary straddles.
const FRAGS: &[&[u8]] = &[
    b"$.", b"$.x", b"$.ab", b"@@:", b"@@:self.f", b"@@:self.a.b(1)", b"@@:params.k", b"@@X(",
    b"@@Sub()", b"@@Sub(1,2)", b"{", b"}", b"{{", b"}}", b"{$.n}", b"\"", b"'", b"\"\"\"",
    b"'''", b"r\"", b"r#\"", b"\"#", b"f\"", b"//", b"/*", b"*/", b"#", b"\\", b"\\\"", b"\n",
    b" ", b"=", b";", b"(", b")", b"[", b"]", b"x", b"1", b"abc",
    // Closed holed literals, whole — so the `holes` register actually fires under fuzz
    // (holes exist only on the Python quoted forms among the 4 gated targets).
    b"f\"a{$.n}b\"", b"'{$.k}'", b"\"w{x}\"",
    // Hole-hides-delim literals, whole — the Δ1 FIXED class: the hole's real `}` sits after a
    // `}` hidden inside a nested string, so the string-blind oracle mis-delimits and the
    // string-aware machine does not (guarantees the partition-aware differential has teeth).
    b"f\"{'}'}\"", b"\"{d['}']}\"", b"f\"{ '{' }\"",
];

fn gen_fuzz(rng: &mut Rng, max_frags: usize) -> String {
    let n = rng.below(max_frags + 1);
    let mut v: Vec<u8> = Vec::new();
    for _ in 0..n {
        v.extend_from_slice(FRAGS[rng.below(FRAGS.len())]);
    }
    String::from_utf8(v).expect("fuzz fragments are ASCII")
}

/// B-13c: deterministic fuzz — every `from` position, windows, 4 targets — with TEETH
/// COUNTERS: the corpus must actually produce each phenomenon the ledger names, or the
/// differential is vacuous over that class.
#[test]
fn structural_differential_fuzz_with_teeth() {
    let mut literals = 0usize;
    let mut holes = 0usize;
    let mut unterminated = 0usize;
    let mut interior_islands_suppressed = 0usize;
    let mut clamped_comments = 0usize;
    let mut fixed_rows = 0usize;

    for seed in 0u64..800 {
        let mut rng = Rng::new(seed ^ 0xD00F_1E14);
        let src = gen_fuzz(&mut rng, 10);
        let b = src.as_bytes();
        for &t in &TARGETS {
            // Full-window PARTITION-AWARE differential at every from; a shifted-to window per
            // seed. A divergence is a Δ1 FIXED row (string-aware hole vs the string-blind
            // oracle) — the machine must still be well-formed there (checked inside).
            for from in 0..=b.len() {
                fixed_rows += agree_or_fixed_window(b, from, b.len(), t) as usize;
            }
            let to = rng.below(b.len() + 1);
            fixed_rows += agree_or_fixed_window(b, 0, to, t) as usize;

            // I1 on the full window.
            let parts = machine(b, 0, b.len(), t);
            check_partition(&parts, 0, b.len(), &src);

            // Teeth.
            let mut first_untermed: Option<usize> = None;
            for i in 0..b.len() {
                match opaque_at(b, i, t) {
                    OpaqueAt::Unterminated => {
                        unterminated += 1;
                        first_untermed.get_or_insert(i);
                    }
                    OpaqueAt::Comment(e) if e > b.len() => clamped_comments += 1,
                    _ => {}
                }
            }
            for p in &parts {
                if let NativePart::Literal(l) = p {
                    // A STRING literal has a quote delim (`"`/`'`); a comment has `#`/`/` (Δ4).
                    if matches!(l.delim, b'"' | b'\'') {
                        literals += 1;
                        if l.parts.iter().any(|lp| matches!(lp, LiteralPart::Hole(_))) {
                            holes += 1;
                        }
                    }
                }
            }
            // Δ3 teeth (fix ACTIVELY firing, not the old bug): count windows where the hand
            // oracle splices an island (ref/inst) into an unterminated interior that the machine
            // correctly SUPPRESSES (text-runs). A false-positive `first_untermed` inside a
            // terminated literal shows the SAME islands on both sides → not counted; only a genuine
            // unterminated interior shows hand > machine.
            if let Some(u) = first_untermed {
                let after = |ps: &[NativePart]| {
                    ps.iter()
                        .filter(|p| {
                            matches!(p, NativePart::Ref(r) if r.span.start > u)
                                || matches!(p, NativePart::Instantiate(i) if i.span.start > u)
                        })
                        .count()
                };
                if after(&hand(b, 0, b.len(), t)) > after(&parts) {
                    interior_islands_suppressed += 1;
                }
            }
            // Clamped comments need a `to` inside a comment — probe one directly.
            for i in 0..b.len() {
                if let OpaqueAt::Comment(e) = opaque_at(b, i, t) {
                    if e > i + 2 {
                        let to = e - 1;
                        if i < to {
                            fixed_rows += agree_or_fixed_window(b, i, to, t) as usize;
                            let w = machine(b, i, to, t);
                            // A comment node carries the real opener delim now (`#`/`/`, Δ4).
                            if w.iter().any(
                                |p| matches!(p, NativePart::Literal(l) if matches!(l.delim, b'#' | b'/') && l.span.end == to),
                            ) {
                                clamped_comments += 1;
                            }
                        }
                    }
                    break;
                }
            }
        }
    }

    for (n, name, min) in [
        (literals, "literals", 50),
        (holes, "holed literals", 10),
        (unterminated, "unterminated positions", 50),
        // Δ3 fix-with-teeth: the machine must actually SUPPRESS islands the hand splices into
        // unterminated interiors (else the fix is never exercised by the fuzz).
        (interior_islands_suppressed, "interior islands suppressed (Δ3)", 10),
        (clamped_comments, "clamped comments", 10),
        // Δ1 fix-with-teeth: the fuzz must actually reach the string-aware-hole FIXED class
        // (machine != string-blind hand), or the partition-aware differential is vacuous.
        (fixed_rows, "fixed (string-aware-hole) rows", 5),
    ] {
        assert!(
            n >= min,
            "fuzz lacks teeth: only {n} {name} (need >= {min}) — the differential would be \
             vacuous over that ledger class"
        );
    }
}

// ------------------------------------------------------------------ the R3 fixture move

/// The TS/JS fixtures that tests/holes.rs carried beyond the R3 4-target gate, verbatim,
/// now against the HAND oracle (production refuses those targets before `segment()`, so
/// this is not a production behavior — it documents what the hand path did, until C-final
/// deletes the oracle).
#[test]
fn ts_js_originals_against_the_hand_oracle() {
    fn hand_refs(code: &str, t: Target) -> Vec<String> {
        let b = code.as_bytes();
        let parts = hand(b, 0, b.len(), t);
        ref_texts(&parts, code)
    }
    let found = hand_refs("const s = `outer ${ `inner ${$.deep}` }`;", Target::TypeScript);
    assert_eq!(found, vec!["$.deep"], "a ref in a NESTED hole is still a ref");

    for (code, target) in [
        ("const s = `x ${ `y ${$.z}` }`;", Target::TypeScript),
        (r#"s = "a } brace"; t = $.n;"#, Target::JavaScript),
    ] {
        let b = code.as_bytes();
        let parts = hand(b, 0, b.len(), target);
        check_partition(&parts, 0, code.len(), code);
    }
}
