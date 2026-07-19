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

/// Full `(from, to)` cross-product — for the short adversarial corpus.
fn agree_all_windows(src: &str, t: Target) {
    let b = src.as_bytes();
    for from in 0..=b.len() {
        for to in from..=b.len() {
            agree_window(b, from, to, t);
        }
    }
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

/// B-1 (T-N1, carried — flips at Δ3): an UNTERMINATED block comment's `Err` is silently
/// discarded; the bytes become water and the scan continues INSIDE the comment interior.
#[test]
fn b1_unterminated_comment_interior_is_water_today() {
    let src = "x /* never closes $.n";
    for t in [Target::C, Target::Java, Target::Rust] {
        // Teeth: the input really is on the `Err` path (a refusal, not a miss).
        assert_eq!(
            opaque_at(src.as_bytes(), 2, t),
            OpaqueAt::Unterminated,
            "the fixture must exercise the swallow ({t:?})"
        );
        let parts = machine(src.as_bytes(), 0, src.len(), t);
        // Pin (CURRENT behavior): no comment node — and worse, the `$.n` INSIDE the
        // comment interior is recognized as a Ref. The lexer said Err; the walk carried on.
        assert!(
            !parts.iter().any(
                |p| matches!(p, NativePart::Literal(l) if l.delim == b'/')
            ),
            "no comment node may be built from an unterminated comment ({t:?})"
        );
        assert_eq!(
            ref_texts(&parts, src),
            vec!["$.n"],
            "the interior is scanned for islands today ({t:?} — Δ3 flips this)"
        );
        agree_sweep(src, t);
    }
}

/// B-2 (T-N2, carried — flips at Δ3): an unterminated LITERAL's interior is island-scanned —
/// a `$.x` / `@@Sub()` inside the user's (mis-terminated) string is recognized and would be
/// spliced (the #224/#215 corruption class, live at this boundary).
#[test]
fn b2_islands_inside_unterminated_literal_interior() {
    let src = "a = \"unterminated $.x and @@Sub() ";
    for t in TARGETS {
        assert_eq!(
            opaque_at(src.as_bytes(), 4, t),
            OpaqueAt::Unterminated,
            "the fixture must exercise the literal swallow ({t:?})"
        );
        let parts = machine(src.as_bytes(), 0, src.len(), t);
        assert_eq!(
            ref_texts(&parts, src),
            vec!["$.x"],
            "a ref IS found inside the unterminated string's interior today ({t:?})"
        );
        assert!(
            parts
                .iter()
                .any(|p| matches!(p, NativePart::Instantiate(i) if i.name == "Sub")),
            "an instantiation IS found inside the interior today ({t:?})"
        );
        agree_sweep(src, t);
    }

    // The hole-interior variant (python): the unterminated `'` INSIDE a hole is swallowed
    // by the recursion (the T-N1/T-N2 reachability includes hole interiors).
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
        "the hole's unterminated `'x ` interior is all water today (Δ3 flips this)"
    );
    // `$.y` sits in the literal's CONTENT (after the hole) — not a ref, and that part is
    // correct forever: content is not code.
    assert!(ref_texts(&parts, py).is_empty());
    agree_sweep(py, Target::Python3);
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
        assert_eq!(lit.delim, b'/', "it is the comment kind ({t:?})");
        assert_eq!(
            lit.span.end, to_in_comment,
            "the comment is CLAMPED to `to`, not demoted ({t:?})"
        );
        // A `to` INSIDE the string: demoted (no literal node at all).
        let str_pos = src.find('=').unwrap() + 2;
        let to_in_string = str_pos + 2;
        let cut = machine(b, 0, to_in_string, t);
        assert!(
            !cut.iter()
                .any(|p| matches!(p, NativePart::Literal(l) if l.delim != b'/')),
            "the literal crossing `to` must DEMOTE ({t:?})"
        );
        agree_sweep(src, t);
    }
}

/// B-5 (T-N5, carried — flips at Δ4): EVERY comment node fabricates `delim: b'/'`, false
/// for a Python `#` comment. Zero readers of `LiteralNode.delim` today; pinned so the
/// first reader cannot inherit it silently.
#[test]
fn b5_python_hash_comment_carries_the_slash_fabrication() {
    let src = "x = 1 # a note";
    let parts = machine(src.as_bytes(), 0, src.len(), Target::Python3);
    let lit = first_literal(&parts).expect("the comment is a node");
    assert_eq!(
        lit.delim,
        b'/',
        "the fabricated delim is CURRENT behavior (Δ4 makes it the real opener byte)"
    );
    agree_sweep(src, Target::Python3);
}

/// B-6 (T-N7 / R6, carried — flips at Δ1): string-blind hole delimitation. In
/// `f"{ d['}'] }"` the hole closes at the FIRST `}` — inside the `'}'` string — in BOTH
/// the hand `hole_at` and `BraceBalance`, so the differential is green-while-wrong. Item 4
/// makes holes CODE nodes, so the wrong extent now shapes the parts tree; pinned exactly.
#[test]
fn b6_string_blind_hole_extent_pinned() {
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
        (3, 7),
        "the WRONG extent — closes at the first `}}` inside `'}}'` (R6; Δ1 fixes)"
    );
    assert!(
        hole.parts.iter().all(|p| matches!(p, NativePart::Text(_))),
        "the truncated hole interior (` d['`) is water today"
    );
    agree_sweep(src, Target::Python3);
}

/// B-7 (T-N8, carried — flips at Δ2): the `{{` escape guard checks only the NEXT byte and
/// the scanner advances 1, so the SECOND brace of `{{` opens a PHANTOM hole — content the
/// user escaped, now treated as code.
#[test]
fn b7_double_brace_phantom_hole_pinned() {
    // f"{{x}}" — the phantom hole contains exactly `x`.
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
    assert_eq!(holes, vec![(4, 5)], "the phantom hole, pinned (Δ2 removes it)");
    agree_sweep(src, Target::Python3);

    // f"{{$.n}}" — the escaped `$.n` is — WRONGLY — a live Ref today.
    let src2 = "f\"{{$.n}}\"";
    let parts2 = machine(src2.as_bytes(), 0, src2.len(), Target::Python3);
    assert_eq!(
        ref_texts(&parts2, src2),
        vec!["$.n"],
        "escaped content is treated as code today (Δ2 flips this)"
    );
    agree_sweep(src2, Target::Python3);
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
    for t in TARGETS {
        for src in corpus {
            agree_sweep(src, t);
            // I1 on the machine output, full window.
            let parts = machine(src.as_bytes(), 0, src.len(), t);
            check_partition(&parts, 0, src.len(), src);
        }
    }
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
    for t in TARGETS {
        for src in corpus {
            agree_all_windows(src, t);
        }
    }
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
    let mut islands_in_interiors = 0usize;
    let mut clamped_comments = 0usize;

    for seed in 0u64..800 {
        let mut rng = Rng::new(seed ^ 0xD00F_1E14);
        let src = gen_fuzz(&mut rng, 10);
        let b = src.as_bytes();
        for &t in &TARGETS {
            // Full-window differential at every from; a shifted-to window per seed.
            for from in 0..=b.len() {
                agree_window(b, from, b.len(), t);
            }
            let to = rng.below(b.len() + 1);
            agree_window(b, 0, to, t);

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
                match p {
                    NativePart::Literal(l) if l.delim != b'/' => {
                        literals += 1;
                        if l.parts.iter().any(|lp| matches!(lp, LiteralPart::Hole(_))) {
                            holes += 1;
                        }
                    }
                    NativePart::Ref(r) => {
                        if first_untermed.is_some_and(|u| r.span.start > u) {
                            islands_in_interiors += 1;
                        }
                    }
                    NativePart::Instantiate(i) => {
                        if first_untermed.is_some_and(|u| i.span.start > u) {
                            islands_in_interiors += 1;
                        }
                    }
                    _ => {}
                }
            }
            // Clamped comments need a `to` inside a comment — probe one directly.
            for i in 0..b.len() {
                if let OpaqueAt::Comment(e) = opaque_at(b, i, t) {
                    if e > i + 2 {
                        let to = e - 1;
                        if i < to {
                            agree_window(b, i, to, t);
                            let w = machine(b, i, to, t);
                            if w.iter().any(
                                |p| matches!(p, NativePart::Literal(l) if l.delim == b'/' && l.span.end == to),
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
        (islands_in_interiors, "islands inside swallowed interiors", 10),
        (clamped_comments, "clamped comments", 10),
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
