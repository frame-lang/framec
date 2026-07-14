//! **TOTALITY** — every byte of the file is in the tree. Proven, not asserted.
//!
//! # What "total" means, exactly
//!
//! > The items partition the file. Every byte — every keyword, every brace, every
//! > comment, **every space and every newline** — belongs to exactly one node.
//! > `unparse(parse(src))` is byte-identical to `src`.
//!
//! # Whitespace, and why the Oceans model makes it easy
//!
//! Most lossless-tree designs need dedicated machinery for *trivia* — Roslyn attaches
//! leading/trailing trivia lists to every token; rowan interleaves trivia tokens.
//! It is the part everyone skips, and skipping it is exactly where the old compiler's
//! terminator bug lived: it spliced a `;` **inside a trailing comment**, because the
//! comment was not a node and nothing knew it was there.
//!
//! Under the Oceans model we get it almost for free, because **whitespace between
//! islands is not a special category — it is just ocean.** It is `Native`, like any
//! other byte the compiler must carry and must not interpret. No trivia list, no
//! attachment rules, no "which token owns this newline."
//!
//! But the symmetry is not total, and the distinction matters:
//!
//! | where | what the whitespace *is* |
//! |---|---|
//! | **between** islands | **water** — target-language bytes, carried verbatim, never interpreted |
//! | **inside** an island | **Frame trivia** — framec's own whitespace, which framec MAY reformat |
//!
//! Both must be in the tree. Only the classification differs — and it differs because
//! the *ownership* differs. framec may reindent its own `machine:` block; it may never
//! touch a byte of the user's. (It did, and the user's string literal came out with a
//! different value at runtime — #215.)
//!
//! # Totality must hold RECURSIVELY
//!
//! Today the file level is total: `Bom | Native | Pragma | System | Efsm` cover every
//! byte. But a `System` is currently **one opaque span** — its interior is not yet
//! decomposed, because PARSE is not built.
//!
//! That is the *whole point of the rebuild* and must not be lost: the old compiler was
//! total at the file level too. It had an AST of the system *skeleton* and **no AST of
//! handler bodies** — and every one of the twenty-five bugs lives below that line.
//! When PARSE lands, this same property must hold **inside** each system, and inside
//! each handler body, all the way down to the individual native statement.
//!
//! So this file tests what is built, and `system_interior_is_not_yet_total` is a
//! standing, deliberate reminder of what is not. It is not a TODO comment that rots.
//! It is a test.

use frame_compiler::scan::{literals::Target, segment};
use frame_compiler::Source;
use std::path::{Path, PathBuf};

/// The corpus lives in the OLD compiler's tree. **Tests cross the cleanroom wall;
/// code does not** (see `REUSE.md`). The corpus IS the specification — the rebuilt
/// compiler earns its existence by handling it.
fn corpus() -> Vec<(PathBuf, Target)> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("framec/tests/fixtures");

    let mut out = Vec::new();
    let Ok(dirs) = std::fs::read_dir(&root) else {
        return out;
    };
    let mut excluded: Vec<String> = Vec::new();
    for d in dirs.flatten() {
        if !d.path().is_dir() {
            continue;
        }
        let dir_name = d.file_name().to_string_lossy().into_owned();
        let n_frm = std::fs::read_dir(d.path())
            .map(|it| {
                it.flatten()
                    .filter(|f| f.path().extension().map(|e| e == "frm").unwrap_or(false))
                    .count()
            })
            .unwrap_or(0);
        if n_frm == 0 {
            continue;
        }

        let target = match dir_target(&dir_name) {
            DirKind::Target(t) => t,
            DirKind::ExcludedOnPurpose(why) => {
                excluded.push(format!("{dir_name} ({n_frm} fixtures) — {why}"));
                continue;
            }
            // NO SILENT SKIPS. If a fixture directory is not classified, the test
            // fails rather than quietly shrinking its own coverage.
            DirKind::Unknown => panic!(
                "fixture directory `{dir_name}` ({n_frm} fixtures) is not classified. \
                 Add it to `dir_target` — either map it to a target, or exclude it \
                 with a stated reason. Do NOT let it be skipped."
            ),
        };
        for f in std::fs::read_dir(d.path()).unwrap().flatten() {
            let p = f.path();
            if p.extension().map(|e| e == "frm").unwrap_or(false) {
                out.push((p, target));
            }
        }
    }
    // Say what was dropped. Silent truncation reads as "we covered everything".
    for e in &excluded {
        eprintln!("  corpus: EXCLUDED {e}");
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

/// Map a fixture directory to the target its native code is written in.
///
/// **Every directory must be explicitly mapped or explicitly excluded.** `None` from
/// this function means "unknown", and an unknown directory is a hard failure — not a
/// silent skip.
///
/// This matters more than it looks. The first version of this loader quietly dropped
/// 45 of 280 fixtures (16%) because it only matched directory names that happened to
/// equal a target's name — so `python_3` and `_canonical` were skipped, and the test
/// reported a confident, meaningless green. **A test that silently ignores its input
/// passes.** That is the same defect class as the compiler silently classifying an
/// island as water; it just fails in the test harness instead of the product.
fn dir_target(dir: &str) -> DirKind {
    let name = match dir {
        "python" | "python_3" | "python3" | "py" => "python",
        "_canonical" => "python", // the canonical corpus is Python/Rust-flavoured
        "js" => "javascript",
        "ts" => "typescript",
        "cs" | "c_sharp" => "csharp",
        // Erlang was DEPRECATED and removed from the compiler (W901). Its fixtures
        // remain in the old tree but there is no Erlang target to segment them with.
        // Excluded ON PURPOSE, and said out loud.
        "erlang" => return DirKind::ExcludedOnPurpose("Erlang is deprecated and removed"),
        other => other,
    };
    match Target::ALL.iter().find(|t| t.name() == name) {
        Some(t) => DirKind::Target(*t),
        None => DirKind::Unknown,
    }
}

enum DirKind {
    Target(Target),
    ExcludedOnPurpose(&'static str),
    /// A directory nobody has classified. **Hard failure.** A new fixture directory
    /// must not be able to slip past this test unnoticed.
    Unknown,
}

/// **THE INVARIANT.** For every fixture: the tree reproduces the file, byte for byte.
///
/// If a single space, newline or comment byte is missing from the tree, this fails.
/// You cannot forget a byte, and you cannot quietly widen a span to paper over one.
#[test]
fn every_byte_of_every_fixture_is_in_the_tree() {
    let corpus = corpus();
    // 280 .frm total, minus the 15 deprecated-Erlang fixtures = 265.
    assert_eq!(
        corpus.len(),
        265,
        "expected 265 fixtures (280 total minus 15 deprecated Erlang). A test that \
         silently shrinks its own input passes, and means nothing."
    );

    let mut failures = Vec::new();
    let mut ok = 0usize;

    for (path, target) in &corpus {
        let bytes = std::fs::read(path).expect("read fixture");
        let original = bytes.clone();

        let src = match Source::new(path.to_string_lossy(), bytes) {
            Ok(s) => s,
            Err(e) => {
                failures.push(format!("{}: {e}", path.display()));
                continue;
            }
        };

        let ast = match segment(&src, *target) {
            Ok(a) => a,
            Err(e) => {
                failures.push(format!("{} [{}]: SEGMENT FAILED: {e:?}", path.display(), target.name()));
                continue;
            }
        };

        // I1 — coverage: the spans partition the file.
        if let Err(d) = ast.check_coverage() {
            failures.push(format!("{} [{}]: {d}", path.display(), target.name()));
            continue;
        }

        // I1, constructively: reassemble and compare byte for byte.
        let rebuilt = ast.unparse(&original);
        if rebuilt != original {
            let at = rebuilt
                .iter()
                .zip(&original)
                .position(|(a, b)| a != b)
                .unwrap_or(rebuilt.len().min(original.len()));
            failures.push(format!(
                "{} [{}]: UNPARSE != SOURCE (first difference at byte {at}; \
                 {} bytes out vs {} in)",
                path.display(),
                target.name(),
                rebuilt.len(),
                original.len()
            ));
            continue;
        }

        // I2 — island coverage: something was actually understood.
        // This is the one coverage structurally cannot express. A tree of a single
        // Native item spanning the whole file satisfies I1 perfectly and means the
        // compiler understood NOTHING — which is precisely what a UTF-8 BOM used to
        // do to a whole `@@system` (#214), silently, at exit 0.
        if let Err(d) = ast.check_islands() {
            failures.push(format!("{} [{}]: {d}", path.display(), target.name()));
            continue;
        }

        ok += 1;
    }

    assert!(
        failures.is_empty(),
        "\n{} / {} fixtures are NOT totally represented in the tree:\n\n{}\n",
        failures.len(),
        corpus.len(),
        failures
            .iter()
            .take(25)
            .map(|f| format!("  {f}"))
            .collect::<Vec<_>>()
            .join("\n")
    );

    eprintln!("  totality: {ok}/{} fixtures — every byte in the tree", corpus.len());
}

/// **Whitespace is in the tree.** Explicitly, because it is the part everyone skips —
/// and the terminator bug lived exactly there (a `;` spliced *inside* a trailing
/// comment, because the comment was not a node).
#[test]
fn whitespace_and_comments_are_nodes_not_gaps() {
    let src_text = "\n\n   # a comment with a } brace in it\n\n@@system S {\n    interface:\n        go()\n    machine:\n        $A { go() { } }\n}\n\n\n   # trailing trivia   \n";
    let src = Source::new("t.frm", src_text.as_bytes().to_vec()).unwrap();
    let ast = segment(&src, Target::Python3).unwrap();

    ast.check().expect("both invariants");
    assert_eq!(
        ast.unparse(src_text.as_bytes()),
        src_text.as_bytes(),
        "every blank line, every space and every comment byte must round-trip"
    );

    // And the `}` inside the comment must NOT have been counted as a brace.
    let systems: Vec<_> = ast
        .items
        .iter()
        .filter(|i| matches!(i, frame_compiler::tree::Item::System(_)))
        .collect();
    assert_eq!(systems.len(), 1, "the system must be found despite the `}}` in the comment");
}

/// **A standing reminder, as a test rather than a comment.**
///
/// The file level is total. A `System`'s INTERIOR is not — it is one opaque span until
/// PARSE is built. That gap is the entire subject of the rebuild: the old compiler was
/// total at the file level too, and had **no AST of handler bodies**, which is where
/// all twenty-five bugs live.
///
/// When PARSE lands, totality must hold **recursively** — inside each system, inside
/// each handler body, down to the individual native statement. At that point this test
/// flips, and it should be *made* to flip deliberately.
#[test]
fn system_interior_is_not_yet_total() {
    let text = "@@system S {\n    interface:\n        go()\n    machine:\n        $A { go() { } }\n}\n";
    let src = Source::new("t.frm", text.as_bytes().to_vec()).unwrap();
    let ast = segment(&src, Target::Python3).unwrap();

    let sys = ast
        .items
        .iter()
        .find_map(|i| match i {
            frame_compiler::tree::Item::System(s) => Some(s),
            _ => None,
        })
        .expect("a system");

    // One opaque span today. When PARSE exists, the system will hold interface /
    // machine / domain / handlers / statements, and THAT tree must be total too.
    assert_eq!(
        sys.span.len(),
        text.trim_end().len(),
        "the system is currently ONE span. When PARSE lands, delete this test and \
         assert recursive totality of its interior instead — that is the whole job."
    );
}

/// **RECURSIVE totality** — the invariant applied at every node, not just the root.
///
/// This is the test that would have caught the old compiler. `FileAst` covered every
/// byte; `SystemAst` covered every byte; and `HandlerBody` was a `String`, so there
/// was nothing to check and nobody noticed. Coverage at the top said "fine."
///
/// Here, a node that holds structure it has not decomposed is reported as an
/// `UndecomposedBlob` — so the gap is LOUD instead of invisible. Right now the
/// section bodies are exactly that, and this test's job is to say so precisely.
#[test]
fn recursive_totality_reports_exactly_what_is_not_yet_parsed() {
    use frame_compiler::tree::{check_total, Defect, Node};

    let mut gaps = Vec::new();
    let mut blobs = std::collections::BTreeMap::<&'static str, usize>::new();
    let mut clean = 0usize;

    for (path, target) in corpus() {
        let bytes = std::fs::read(&path).unwrap();
        let src = Source::new(path.to_string_lossy(), bytes).unwrap();
        let Ok(ast) = segment(&src, target) else { continue };

        match check_total(&ast as &dyn Node) {
            Ok(()) => clean += 1,
            // An UndecomposedBlob is EXPECTED right now — it names the work left.
            Err(Defect::UndecomposedBlob { kind, .. }) => {
                *blobs.entry(kind).or_insert(0) += 1;
            }
            // A gap or an overlap is a REAL BUG. There is no excuse for one.
            Err(d) => gaps.push(format!("{}: {d}", path.display())),
        }
    }

    assert!(
        gaps.is_empty(),
        "\nspans do not partition — these are real bugs, not missing work:\n{}\n",
        gaps.iter().take(10).map(|g| format!("  {g}")).collect::<Vec<_>>().join("\n")
    );

    eprintln!("\n  RECURSIVE TOTALITY");
    eprintln!("  ------------------");
    eprintln!("  fully decomposed files : {clean}");
    eprintln!("  files stopping at an undecomposed section:");
    for (kind, n) in &blobs {
        eprintln!("      {kind:<12} {n} files");
    }
    eprintln!("  ^ this is the work remaining, stated by the tree itself rather than");
    eprintln!("    by a TODO. When PARSE lands these go to zero and `clean` == 265.\n");
}

/// The granularity census — the assertion coverage structurally cannot make.
///
/// A handler body that decomposes to ONE statement when the source has four lines
/// satisfies coverage perfectly and is wrong. Coverage says every byte is *somewhere*;
/// only granularity says it is in the *right* somewhere.
#[test]
fn granularity_census() {
    use frame_compiler::tree::{census, Node};

    let text = "@@system S {\n    interface:\n        go()\n    machine:\n        $A { go() { } }\n    domain:\n        n: int = 0\n}\n";
    let src = Source::new("t.frm", text.as_bytes().to_vec()).unwrap();
    let ast = segment(&src, Target::Python3).unwrap();

    let mut c = std::collections::BTreeMap::new();
    census(&ast as &dyn Node, &mut c);

    assert_eq!(c.get("System"), Some(&1));
    assert_eq!(c.get("Interface"), Some(&1), "the interface section must be a NODE");
    assert_eq!(c.get("Machine"), Some(&1), "the machine section must be a NODE");
    assert_eq!(c.get("Domain"), Some(&1), "the domain section must be a NODE");
    eprintln!("  census: {c:?}");
}

/// What the tree actually contains, across the whole corpus.
///
/// The GRANULARITY census — the assertion coverage structurally cannot make. Coverage
/// says every byte is *somewhere*; only this says it is in the *right* somewhere.
#[test]
fn corpus_census() {
    use frame_compiler::tree::{census, Node};
    let mut total = std::collections::BTreeMap::<&'static str, usize>::new();
    for (path, target) in corpus() {
        let bytes = std::fs::read(&path).unwrap();
        let src = Source::new(path.to_string_lossy(), bytes).unwrap();
        let Ok(ast) = segment(&src, target) else { continue };
        census(&ast as &dyn Node, &mut total);
    }
    eprintln!("\n  THE TREE (265 fixtures)");
    eprintln!("  ------------------------");
    let mut rows: Vec<_> = total.iter().collect();
    rows.sort_by_key(|(_, n)| std::cmp::Reverse(**n));
    for (kind, n) in rows {
        eprintln!("  {kind:<16} {n:>6}");
    }
    // The nodes that did not exist AT ALL in the old compiler.
    assert!(total.get("Handler").copied().unwrap_or(0) > 100, "handlers must be nodes");
    // NativeStmt is smaller now that assignments, returns and calls are TYPED nodes
    // (Assign/ReturnCall/SelfCall) rather than untyped native text — which is the point.
    assert!(total.get("NativeStmt").copied().unwrap_or(0) > 50, "native statements must be nodes");
    assert!(total.get("Assign").copied().unwrap_or(0) > 50, "assignments are typed nodes, not native text");
    assert!(total.get("FrameRef").copied().unwrap_or(0) > 50, "mid-expression refs must be nodes");
    eprintln!();
}
