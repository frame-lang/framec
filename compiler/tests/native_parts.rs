//! **Item 4 — the native-water decomposition, the system's standalone ledger pins.**
//!
//! Production `native_parts` is a construction driver over the `NativePartsScan`
//! `@@[scan(u8)]` system (walk = island boundaries + kinds + text runs; nodes = fold +
//! `opaque_probe` registers + hole recursion). This battery pins the standalone facts (no hand
//! oracle):
//!
//! - **One machine pin per Phase-1/2 ledger row** (b1..b11, tr3, b9, b10) — the CURRENT
//!   behavior asserted directly: unterminated interiors become one Text run, the to-boundary
//!   clamp/demote policy (b3/b4 are the ONLY standalone spec of it), string-aware holes, no
//!   phantom `{{` hole, the real comment opener delim, dispatch priority, deep hole nesting,
//!   the refused-unknown-context ref through the routed production path.
//! - **I1, recursively** (`check_partition`): parts partition `[from, to)` exactly, and each
//!   Hole's parts partition its span.
//!
//! The position-exhaustive structural differential against the retired hand walk lived here; it
//! is removed at test-severance (the system's own pins carry the spec). SCAFFOLDING (white-box on
//! the internal `native_parts` + the routed `segment()`/validator path).

use frame_compiler::text::scan::literals::Target;
use frame_compiler::text::scan::opaque_scan::{opaque_at, OpaqueAt};
use frame_compiler::text::scan::parts::native_parts;
use frame_compiler::tree::body::{LiteralNode, LiteralPart, NativePart, RefKind};
use frame_compiler::tree::{check_total, Node};

const TARGETS: [Target; 4] = [Target::C, Target::Java, Target::Rust, Target::Python3];

fn machine(b: &[u8], from: usize, to: usize, t: Target) -> Vec<NativePart> {
    native_parts(b, from, to, t)
}

/// I1, recursive: the parts partition `[from, to)` exactly, each node passes `check_total`, and
/// each Hole's parts partition the hole's span (explicit recursion).
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

/// B-1 (T-N1, DP-1): an UNTERMINATED block comment's interior is NOT scanned for islands — the
/// rescued interior becomes ONE plain Text run to `to`. The lexer's refusal is honored (content,
/// not code); `native_parts` grows no diagnostics channel.
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
        // No comment node, no `$.n` ref — the interior is ONE Text run to `to`.
        assert!(
            !parts
                .iter()
                .any(|p| matches!(p, NativePart::Literal(l) if l.delim == b'/')),
            "no comment node may be built from an unterminated comment ({t:?})"
        );
        assert!(
            ref_texts(&parts, src).is_empty(),
            "the unterminated interior is content, not code — no island ({t:?})"
        );
        assert_eq!(parts.len(), 1, "the whole range is ONE plain Text run ({t:?})");
        assert!(matches!(&parts[0], NativePart::Text(x) if x.span.start == 0 && x.span.end == src.len()));
        check_partition(&parts, 0, src.len(), src);
    }
}

/// B-2 (T-N2, DP-1): an unterminated LITERAL's interior is NOT island-scanned — the `$.x` /
/// `@@Sub()` inside the user's mis-terminated string (the #224/#215 corruption class) is content,
/// not code. The rescued interior becomes ONE plain Text run.
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
        assert_eq!(parts.len(), 1, "the interior is ONE plain Text run ({t:?})");
        assert!(matches!(&parts[0], NativePart::Text(x) if x.span.start == 0 && x.span.end == src.len()));
        check_partition(&parts, 0, src.len(), src);
    }

    // The hole-interior variant (python): the unterminated `'` INSIDE a hole is likewise text-run
    // by the recursion (the interior has no islands either way).
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
        "the hole's unterminated `'x ` interior is one Text run"
    );
    // `$.y` sits in the literal's CONTENT (after the hole) — not a ref (content is not code).
    assert!(ref_texts(&parts, py).is_empty());
    check_partition(&parts, 0, py.len(), py);
}

/// B-3 (T-N3): a literal whose extent crosses `to` is DEMOTED to water and its interior
/// island-scanned. **The only standalone spec of the demote half of the to-boundary policy.**
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
            "the demoted interior is island-scanned ({t:?})"
        );
        check_partition(&cut, 0, to, &src[..to]);
    }
}

/// B-4 (T-N4): the boundary asymmetry, both directions on the SAME input frame — a comment
/// crossing `to` is CLAMPED and still a comment node; a literal crossing `to` demotes to water
/// (B-3's policy). **The only standalone spec of the clamp half of the to-boundary policy.**
#[test]
fn b4_comment_clamps_where_literal_demotes() {
    for (src, comment_pos, t) in [
        ("a /* long comment */ b = \"sss\" c", 2, Target::C),
        ("a /* long comment */ b = \"sss\" c", 2, Target::Rust),
        ("a // tail comment\nb = \"sss\" c", 2, Target::Java),
        ("a # tail comment\nb = 'sss' c", 2, Target::Python3),
    ] {
        let b = src.as_bytes();
        // A `to` INSIDE the comment: clamped comment node, span ends exactly at `to`.
        let to_in_comment = comment_pos + 6;
        let parts = machine(b, 0, to_in_comment, t);
        let lit = first_literal(&parts).expect("clamped comment node exists");
        // Δ4: the comment delim is the real opener byte — `#` for Python, `/` for a `/`-comment.
        let expect_delim = if matches!(t, Target::Python3) { b'#' } else { b'/' };
        assert_eq!(lit.delim, expect_delim, "it is the comment kind ({t:?})");
        assert_eq!(
            lit.span.end, to_in_comment,
            "the comment is CLAMPED to `to`, not demoted ({t:?})"
        );
        // A `to` INSIDE the string: demoted (no STRING-LITERAL node at all).
        let str_pos = src.find('=').unwrap() + 2;
        let to_in_string = str_pos + 2;
        let cut = machine(b, 0, to_in_string, t);
        assert!(
            !cut.iter()
                .any(|p| matches!(p, NativePart::Literal(l) if matches!(l.delim, b'"' | b'\''))),
            "the literal crossing `to` must DEMOTE ({t:?})"
        );
        check_partition(&parts, 0, to_in_comment, src);
        check_partition(&cut, 0, to_in_string, src);
    }
}

/// B-5 (T-N5, Δ4): a comment node's `delim` is the ACTUAL opener byte, sourced from the probe —
/// `#` for a Python `#` comment, `/` for a C/Java/Rust `//`|`/*`.
#[test]
fn b5_python_hash_comment_delim_is_the_real_opener() {
    let src = "x = 1 # a note";
    let parts = machine(src.as_bytes(), 0, src.len(), Target::Python3);
    let lit = first_literal(&parts).expect("the comment is a node");
    assert_eq!(lit.delim, b'#', "the Python comment delim is the real opener `#` (Δ4)");

    for (csrc, t) in [("x = 1 // note", Target::Rust), ("a /* b */ c", Target::C)] {
        let p = machine(csrc.as_bytes(), 0, csrc.len(), t);
        let l = first_literal(&p).expect("the comment is a node");
        assert_eq!(l.delim, b'/', "a `/`-comment's real opener is `/` ({t:?})");
    }
}

/// B-6 (T-N7 / R6, Δ1): string-AWARE hole delimitation. In `f"{ d['}'] }"` the hole closes at the
/// REAL `}` (position 11), not the one hidden inside the `'}'` string — and the `'}'` inside the
/// hole is recognized as a nested literal (content, not a delimiter).
#[test]
fn b6_string_aware_hole_extent_pinned() {
    let src = "f\"{ d['}'] }\"";
    //         f=0 "=1 {=2 ␠=3 d=4 [=5 '=6 }=7 '=8 ]=9 ␠=10 }=11 "=12
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
    assert!(
        hole.parts
            .iter()
            .any(|p| matches!(p, NativePart::Literal(l) if l.delim == b'\'')),
        "the `'}}'` inside the hole is a nested char literal now (string-aware)"
    );
    check_partition(&parts, 0, src.len(), src);
}

/// B-7 (T-N8, Δ2): the `{{` escape is consumed WHOLE (both braces), so the second `{` no longer
/// opens a phantom hole. Escaped braces are content, not interpolation.
#[test]
fn b7_double_brace_no_phantom_hole() {
    // f"{{x}}" — NO phantom hole; the escaped `x` is content.
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
    check_partition(&parts, 0, src.len(), src);

    // f"{{$.n}}" — the escaped `$.n` is content, NOT a live Ref.
    let src2 = "f\"{{$.n}}\"";
    let parts2 = machine(src2.as_bytes(), 0, src2.len(), Target::Python3);
    assert!(
        ref_texts(&parts2, src2).is_empty(),
        "escaped `$.n` is content, not code (Δ2)"
    );
    check_partition(&parts2, 0, src2.len(), src2);
}

/// B-8 (T-N9): dispatch priority — opaque → inst → embed → ref (inst/embed before ref is
/// load-bearing), tail flush, empty-run guard, adjacency.
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
        }
    }
}

/// T-R3 (an EARNED merge): `$.` / `@@:` with no name is not a ref; it stays water by grammar
/// design.
#[test]
fn tr3_bare_sigils_are_water() {
    for t in TARGETS {
        for src in ["$. alone", "@@: bare", "a $$. b @@ c"] {
            let parts = machine(src.as_bytes(), 0, src.len(), t);
            assert!(
                ref_texts(&parts, src).is_empty(),
                "no ref from a bare sigil in {src:?} ({t:?})"
            );
            check_partition(&parts, 0, src.len(), src);
        }
    }
}

/// B-11 (T-N6, leave-latent): depth-8 nested holes — correctness only. The recursion is a
/// pushdown whose stack is the host call stack; per-level walks are independent, so no depth
/// register exists and NO depth bound is asserted here (part of the recorded leave-latent plea).
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
}

/// B-9 (T-R1, Δ5, H-1): an UNKNOWN `@@:word` context is REFUSED (`RefKind::Unknown`, refusal as
/// data), not silently defaulted to `ContextSelf` — on the ROUTED production path (the statement
/// scanner's assign-LHS runs `ref_scan::scan`) AND directly. The validator OWNS membership: it
/// diagnoses the Unknown ref with E408.
#[test]
fn b9_unknown_context_word_is_refused_and_diagnosed() {
    use frame_compiler::resolve::resolve;
    use frame_compiler::scan::segment;
    use frame_compiler::text::scan::ref_scan;
    use frame_compiler::tree::body::Stmt;
    use frame_compiler::tree::{Item, MachineMember, Section, StateMember};
    use frame_compiler::validate::validate;
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
        RefKind::Unknown,
        "an unknown context word is REFUSED (Unknown), not defaulted to ContextSelf (Δ5)"
    );
    assert_eq!(assign.lhs.name, "wat.x", "the whole word is carried for the diagnostic");

    // The validator OWNS membership (H-1): it diagnoses the Unknown ref with E408.
    let (syms, _) = resolve(&ast);
    let vdiags = validate(&ast, &syms);
    let e408: Vec<_> = vdiags.iter().filter(|d| d.code == "E408").collect();
    assert_eq!(e408.len(), 1, "expected exactly one E408 for `@@:wat.x`: {vdiags:#?}");
    assert_eq!(e408[0].span, assign.lhs.span, "the diagnostic carries the ref's span");

    // The SYSTEM refuses directly too: refusal as data.
    let b = b"@@:wat.x = 1";
    let (k, n, _) = ref_scan::scan(b, 0).expect("system recognizes the shape");
    assert_eq!((k, n.as_str()), (RefKind::Unknown, "wat.x"), "system: refusal as data");
}

/// B-10 (T-R2, Δ5): prefix-overmatch is FIXED — the kind ladder uses a proper segment/word-
/// boundary match, so `@@:database` (segment `database` ≠ `data`) and `@@:selfish` (`selfish` ≠
/// `self`) are Unknown, not ContextData/ContextSelf.
#[test]
fn b10_prefix_overmatch_refused_by_segment_match() {
    use frame_compiler::text::scan::ref_scan;

    for (src, word) in [
        ("@@:database.k", "database.k"),
        ("@@:selfish.y", "selfish.y"),
        ("@@:paramsX.z", "paramsX.z"),
    ] {
        let b = src.as_bytes();
        // The SYSTEM segment-matches: the first segment is not a known context → Unknown, and the
        // whole word is the name.
        let (k, n, _) = ref_scan::scan(b, 0).expect("system recognizes the shape");
        assert_eq!((k, n.as_str()), (RefKind::Unknown, word), "system segment-match on {src:?}");
    }
}
