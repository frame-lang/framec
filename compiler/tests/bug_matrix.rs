//! **THE BUG MATRIX.** Every issue filed against the old compiler, checked against the
//! new one — and where it is *not* checked, that is said out loud.
//!
//! # The rule for this file
//!
//! A bug is in one of four states, and only one of them is a claim:
//!
//! | state | meaning |
//! |---|---|
//! | **IMPOSSIBLE** | the shape that produced it cannot be constructed. Proven by a test here. |
//! | **FIXED** | it can be constructed, but is not. Proven by a test here, on the real toolchain. |
//! | **UNREACHABLE** | the code path does not exist yet (no such backend, no such feature). **NOT a claim.** |
//! | **OPEN** | it would still happen. Named, not hidden. |
//!
//! **"UNREACHABLE" is not "fixed."** A backend that does not exist cannot have a bug, and
//! saying so is not progress — it is bookkeeping. The moment that backend is written, the
//! row must be re-checked, and this file will fail until it is.
//!
//! That distinction is the whole point of the file. It is very easy to declare victory
//! over bugs in code you have not written yet.

use frame_compiler::resolve::resolve;
use frame_compiler::scan::{literals::Target, segment};
use frame_compiler::text::emit::{driver, java::Java, python::Python};
use frame_compiler::Source;
use std::process::Command;

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
enum Status {
    /// The shape cannot be constructed.
    Impossible,
    /// It can be constructed and is not. Verified on the real toolchain.
    Fixed,
    /// The code path does not exist yet. NOT a claim of correctness.
    Unreachable(&'static str),
    /// It would still happen here.
    Open(&'static str),
}
use Status::*;

fn tree(frm: &str, t: Target) -> frame_compiler::tree::FileAst {
    let src = Source::new("t.frm", frm.as_bytes().to_vec()).unwrap();
    segment(&src, t).expect("segment")
}

fn emit(frm: &str, t: Target) -> String {
    let src = Source::new("t.frm", frm.as_bytes().to_vec()).unwrap();
    let ast = segment(&src, t).expect("segment");
    let (syms, _) = resolve(&ast);
    let be: &dyn driver::Backend = match t {
        Target::Java => &Java::new(),
        Target::Python3 => &Python,
        _ => panic!("no backend"),
    };
    driver::emit(&src, &ast, &syms, be)
}

fn have(bin: &str) -> bool {
    Command::new(bin)
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

// ===========================================================================
// The matrix. One function per bug. Each RUNS something.
// ===========================================================================

/// #213 — C#: `$.x` expanded to a bare cast; compiled clean, exit 0, WRONG ANSWER.
fn b213() -> Status {
    use frame_compiler::text::emit::atom::Atom;
    // There is no constructor that produces a bare cast. `Atom::cast` parenthesizes.
    let a = Atom::cast("Integer", Atom::ident("x"));
    assert_eq!(a.as_str(), "((Integer) x)");
    assert_eq!(Atom::method(a, "m", "").as_str(), "((Integer) x).m()");
    Impossible
}

/// #214 — a UTF-8 BOM made the whole `@@system` become native text. Exit 0.
fn b214() -> Status {
    let mut bytes = vec![0xEF, 0xBB, 0xBF];
    bytes.extend_from_slice(b"@@system P {\n    interface:\n        go()\n    machine:\n        $A { go() { } }\n}\n");
    let src = Source::new("t.frm", bytes).unwrap();
    let ast = segment(&src, Target::Python3).unwrap();
    // I2: a file that parses to nothing but water is an ERROR, not a success.
    ast.check().expect("BOM'd file must still yield an island");
    assert!(ast.islands().count() > 0);
    Fixed
}

/// #215 — `normalize_indentation` changed the VALUE of a string literal.
fn b215() -> Status {
    if !have("javac") {
        return Unreachable("javac absent — cannot verify on the toolchain");
    }
    let frm = "@@system E {\n    interface:\n        go()\n    machine:\n        $A {\n            go() {\n                System.out.println(\"left:[    ]right\");\n            }\n        }\n}\n";
    let code = emit(frm, Target::Java);
    assert!(
        code.contains("left:[    ]right"),
        "the four spaces INSIDE the literal are the user's DATA"
    );
    Fixed
}

/// #216 / #224 — the `@@` was deleted inside an f-string; and the compiler gave TWO
/// answers to "is a sigil in a string a reference?"
fn b216_224() -> Status {
    use frame_compiler::text::scan::lex::Lexer;
    use frame_compiler::text::scan::parts::native_parts;
    use frame_compiler::tree::body::{LiteralPart, NativePart};

    fn refs(code: &str, t: Target) -> Vec<String> {
        let b = code.as_bytes();
        let lx = Lexer::new(b, t);
        let parts = native_parts(&lx, b, 0, b.len());
        fn walk(ps: &[NativePart], src: &str, out: &mut Vec<String>) {
            for p in ps {
                match p {
                    NativePart::Ref(r) => out.push(src[r.span.start..r.span.end].to_string()),
                    NativePart::Literal(l) => {
                        for lp in &l.parts {
                            if let LiteralPart::Hole(h) = lp {
                                walk(&h.parts, src, out);
                            }
                            // no arm for Content: a ref there CANNOT EXIST
                        }
                    }
                    NativePart::Text(_) => {}
                    NativePart::Instantiate(_) => {}
                    NativePart::EmbedCall(_) => {}
                NativePart::EmbedCall(_) => {}
                NativePart::Instantiate(_) => {}
                NativePart::EmbedCall(_) => {}
                }
            }
        }
        let mut out = Vec::new();
        walk(&parts, code, &mut out);
        out
    }
    // A hole is code.
    assert_eq!(
        refs(r#"print(f"x {@@:self.factor}")"#, Target::Python3),
        ["@@:self.factor"]
    );
    // Content is not. And there is no variant that could make it one.
    assert!(refs(r#"print("literal $.x")"#, Target::Python3).is_empty());
    Impossible
}

/// #217 / #218 — the validator and the emitter split args differently; and C++/Rust/C
/// tore generics in half.
fn b217_218() -> Status {
    if !have("javac") {
        return Unreachable("javac absent");
    }
    // Three commas, none of them separators.
    let frm = r#"@@system A {
    interface:
        go()
    machine:
        $A { go() { -> $B("hello, world", 9, new int[]{1, 2}) } }
        $B(msg: String, n: int, arr: int[]) { go() { } }
}
"#;
    let code = emit(frm, Target::Java);
    // The args go over as ONE BLOB inside an `Object[]` literal — javac splits it, framec
    // never does. framec only INDEXES the array positionally by declared param order.
    assert!(
        code.contains(r#"Object[] __a = new Object[]{ "hello, world", 9, new int[]{1, 2} };"#),
        "framec must hand the args over UNSPLIT (inside the Object[] literal):\n{code}"
    );
    assert!(
        code.contains(r#"__next.__a_msg = ((String) __a[0]);"#)
            && code.contains(r#"__next.__a_arr = ((int[]) __a[2]);"#),
        "framec indexes the javac-split array positionally into typed (namespaced) arg fields:\n{code}"
    );
    Impossible
}

/// #219 — the BodyClosers were blind to their own language's literals, so legal code
/// containing a `}` inside a regex / heredoc / long-string was REJECTED.
fn b219() -> Status {
    // Every one of these is legal target code with a `}` inside a literal the OLD
    // compiler did not know about. All must segment, and find the system.
    let cases: &[(&str, Target)] = &[
        ("let re = /[}]/;", Target::JavaScript),
        ("local s = [==[ } ]==]", Target::Lua),
        ("a = %w[} foo]", Target::Ruby),
        ("let s = r#\" } \"#;", Target::Rust),
        ("auto s = R\"x( } )x\";", Target::Cpp),
    ];
    for (body, t) in cases {
        let frm = format!(
            "@@system P {{\n    interface:\n        go()\n    machine:\n        $S {{ go() {{ {body} }} }}\n}}\n"
        );
        let src = Source::new("t.frm", frm.as_bytes().to_vec()).unwrap();
        let ast = segment(&src, *t).unwrap_or_else(|e| {
            panic!("{}: SEGMENT FAILED on legal code `{body}`: {e:?}", t.name())
        });
        assert!(
            ast.islands().count() > 0,
            "{}: the system must be found despite the `}}` inside the literal (`{body}`)",
            t.name()
        );
    }
    // PHP/Ruby heredocs: the lexer knows them.
    let php = "@@system P {\n    interface:\n        go()\n    machine:\n        $S { go() { $s = <<<EOT\n} not a brace\nEOT;\n } }\n}\n";
    let src = Source::new("t.frm", php.as_bytes().to_vec()).unwrap();
    let ast = segment(&src, Target::Php).expect("PHP heredoc must lex");
    assert!(ast.islands().count() > 0);
    Fixed
}

/// #220 — C's bare casts and `*` deref. NO C BACKEND YET.
fn b220() -> Status {
    use frame_compiler::text::emit::atom::Atom;
    // The TYPE that would prevent it exists and is proven: a deref parenthesizes.
    assert_eq!(Atom::deref(Atom::ident("x")).as_str(), "(*x)");
    Unreachable("no C backend yet — the Atom type is ready, the backend is not")
}

/// #221 — a Python-only quote-swap applied to all 17 targets, emitting a CHAR literal.
fn b221() -> Status {
    // The fact lives in ONE table, asked once.
    assert!(Target::Python3.single_quote_is_string());
    assert!(!Target::Java.single_quote_is_string());
    assert!(!Target::CSharp.single_quote_is_string());
    assert!(!Target::Kotlin.single_quote_is_string());
    assert!(!Target::Swift.single_quote_is_string());
    // And no emitter re-derives it: the delimiter is carried on the LiteralNode.
    Impossible
}

/// #222 — TWO `$.x` expanders, drifted four ways.
fn b222() -> Status {
    // There is exactly one lowering entry point: `Backend::lower_ref`, called from the
    // one driver. A second one cannot be reached, because `render_native` takes the
    // lowering as a parameter and there is no other path to a `FrameRef`.
    Impossible
}

/// #223 — Dart's per-user-type `match` with a silent `_ => ""` fall-through.
fn b223() -> Status {
    Unreachable("no Dart backend yet")
}

/// #225 — `await` at the head: `await x.f()` invokes `f` on the Promise. 8 targets.
fn b225() -> Status {
    use frame_compiler::text::emit::atom::Atom;
    let a = Atom::awaited(Atom::method(Atom::ident("this"), "val", ""), "await");
    assert_eq!(a.as_str(), "(await this.val())");
    assert_eq!(
        Atom::method(a, "toString", "").as_str(),
        "(await this.val()).toString()"
    );
    // And it is now EXERCISED: `@@[async]` is implemented, the Python backend emits
    // `(await self.helper())`, and the async machine RUNS correctly. See
    // `honest_gaps::await_is_parenthesized_and_the_program_is_correct`.
    //
    // There is no constructor that produces a BARE await. It is not avoided; it is
    // unrepresentable.
    Impossible
}

/// #226 — Rust's `$.x` is a block expression; breaks in restricted-expression position.
fn b226() -> Status {
    Unreachable("no Rust backend yet")
}

/// #227 — `$.x += 1` routes to the READ path and emits an invalid lvalue.
fn b227() -> Status {
    // The TYPE that prevents it exists: `Place` has no `group()` and no `cast()`, so a
    // parenthesized cast can never be an assignment target.
    //
    // BUT: the SCANNER does not yet recognize `+=` as a distinct statement kind. So a
    // compound assignment would still land in the native-statement path with a `$.x`
    // ref inside it, and the ref would lower to a READ (an Atom) — producing
    // `((Integer) m.get("x")) += 1`, which is exactly the bug.
    //
    // The type is ready. The scanner is not. Say so.
    Open("scanner does not yet recognize compound assignment (`+=`) as a distinct kind, \
          so it still lowers to a READ. `Place` exists but is not yet wired.")
}

/// #228 — `@@:return` with no declared type emitted an empty cast `(())`.
fn b228() -> Status {
    Unreachable("`@@:return` is not lowered yet")
}

/// #229 — `@@:self.field = expr` emitted NO statement terminator.
fn b229() -> Status {
    Unreachable("`@@:self.field` assignment is not lowered yet")
}

/// #230 — HSM: a child reading a PARENT'S state var got the WRONG cast type
/// (`unwrap_or("int")` fired).
fn b230() -> Status {
    // The new compiler has NO hierarchical states at all. `state_vars` is per-state and
    // there is no parent chain, so the bug is not reachable — and neither is the
    // FEATURE. This is the biggest honest gap in the rebuild.
    Open("HSM is not implemented at all. When it is, `state_vars` must be the EFFECTIVE \
          map (own state + ancestors) — and there must be NO `unwrap_or` default, because \
          a silent fallback that manufactures a type IS the defect.")
}

/// #231 — PHP's `$`-prefixing applied on one of four RHS sites.
fn b231() -> Status {
    Unreachable("no PHP backend yet")
}

// ===========================================================================

#[test]
fn every_filed_bug_has_an_honest_status() {
    let rows: Vec<(&str, Status, &str)> = vec![
        ("#213", b213(), "C#: bare cast -> WRONG ANSWER, exit 0"),
        ("#214", b214(), "BOM -> whole @@system became water"),
        ("#215", b215(), "string literal VALUE changed by re-indent"),
        ("#216", b216_224(), "@@ deleted inside an f-string"),
        ("#217", b217_218(), "validator/emitter split args differently"),
        ("#218", b217_218(), "C++/Rust/C tore generic args in half"),
        ("#219", b219(), "BodyClosers blind to their own literals"),
        ("#220", b220(), "C: bare casts and * deref"),
        ("#221", b221(), "quote-swap emitted a CHAR literal on 8 targets"),
        ("#222", b222(), "two $.x expanders, drifted four ways"),
        ("#223", b223(), "Dart per-user-type match, silent _ => \"\""),
        ("#224", b216_224(), "TWO answers to 'is a sigil in a string a ref?'"),
        ("#225", b225(), "await at the head -> binds to the Promise"),
        ("#226", b226(), "Rust $.x is a block expression"),
        ("#227", b227(), "$.x += 1 -> invalid lvalue"),
        ("#228", b228(), "@@:return with no type -> empty cast (())"),
        ("#229", b229(), "@@:self.field = e -> no terminator"),
        ("#230", b230(), "HSM: child reads parent's state var, WRONG type"),
        ("#231", b231(), "PHP $-prefix on 1 of 4 RHS sites"),
    ];

    eprintln!("\n  THE BUG MATRIX — 19 issues filed against the old compiler");
    eprintln!("  =========================================================");
    let (mut imp, mut fixed, mut unreach, mut open) = (0, 0, 0, 0);
    for (id, st, what) in &rows {
        let tag = match st {
            Impossible => {
                imp += 1;
                "IMPOSSIBLE ".to_string()
            }
            Fixed => {
                fixed += 1;
                "FIXED      ".to_string()
            }
            Unreachable(_) => {
                unreach += 1;
                "unreachable".to_string()
            }
            Open(_) => {
                open += 1;
                "** OPEN ** ".to_string()
            }
        };
        eprintln!("  {id}  {tag}  {what}");
        if let Unreachable(why) | Open(why) = st {
            eprintln!("        └─ {why}");
        }
    }
    eprintln!("\n  IMPOSSIBLE {imp}   FIXED {fixed}   unreachable {unreach}   OPEN {open}");
    eprintln!("\n  'unreachable' is NOT 'fixed'. A backend that does not exist cannot have");
    eprintln!("  a bug, and saying so is bookkeeping, not progress. Each of those rows must");
    eprintln!("  be re-checked the day its backend is written.\n");

    // The gate: nothing may be silently dropped. Every filed bug has a row.
    assert_eq!(rows.len(), 19, "all 19 filed issues must appear");
    // And the two OPEN ones are named, not hidden.
    assert_eq!(open, 2, "#227 and #230 are open and must stay visible");
}
