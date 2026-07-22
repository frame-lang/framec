//! **The angle fork is adjudicated by declared arity — exactly-one admissible proceeds,
//! neither/both is diagnosed (E407), never guessed — proven by running.**
//!
//! `validate::adjudicate` is the ONE shared adjudication seam (design record §11.3):
//! consumed by validate's `Instantiate` arm (diagnostics) and by
//! `lower_instantiation` (candidate choice), so the two consumers can never disagree.
//! Battery: the §11.7 roster (7) + the gate amendment's `adjudicate_named_coverage`.
//! Direct-construction unit style (SystemParams + Instantiation built through the REAL
//! production scan seat, `inst_scan::scan_node`) plus end-to-end through `validate()` /
//! `driver::emit` on real water — the parsed tree now carries the fork natively (the
//! wired `native_parts` -> `scan_node` -> ArgScan path; the pre-wire injection bridge is
//! gone).

use frame_compiler::resolve::{resolve, Severity};
use frame_compiler::scan::{literals::Target, segment};
use frame_compiler::text::emit::{driver, rust::Rust};
use frame_compiler::text::scan::inst_scan;
use frame_compiler::tree::body::{ArgAngles, Instantiation, NativePart, Stmt};
use frame_compiler::tree::{Item, MachineMember, Param, Section, StateMember, SystemParams};
use frame_compiler::validate::{adjudicate, validate, Adjudication};
use frame_compiler::Source;

/// An `Instantiation` node built through the REAL production seat (InstScan shape +
/// ArgScan args): `@@M(<interior>)` scanned by `inst_scan::scan_node`. Target-blind
/// mechanism — Java here.
fn node_of(interior: &str) -> Instantiation {
    let water = format!("@@M({interior})");
    inst_scan::scan_node(water.as_bytes(), 0, Target::Java)
        .unwrap_or_else(|| panic!("no instantiation in {water:?}"))
}

/// Declared params: domain group only (the groups are checked independently; domain is
/// where the fork batteries' candidates live).
fn domain_params(decls: &[(&str, Option<&str>)]) -> SystemParams {
    SystemParams {
        state: Vec::new(),
        enter: Vec::new(),
        domain: decls
            .iter()
            .map(|(n, d)| Param {
                name: n.to_string(),
                ty: None,
                default: d.map(str::to_string),
            })
            .collect(),
        ..SystemParams::default()
    }
}

// ============================================================================

#[test]
fn adjudicate_picks_g() {
    // Fork g=2/o=3 vs 2 declared params (no defaults): only G fits -> Primary.
    let inst = node_of("new HashMap<Integer, String>(), z");
    assert!(matches!(inst.angles, ArgAngles::Forked { .. }));
    let p = domain_params(&[("p1", None), ("p2", None)]);
    assert_eq!(adjudicate(&p, &inst), Adjudication::Primary);
}

#[test]
fn adjudicate_picks_o() {
    // The same fork vs 3 declared params (no defaults): only O fits -> Alt.
    let inst = node_of("new HashMap<Integer, String>(), z");
    let p = domain_params(&[("p1", None), ("p2", None), ("p3", None)]);
    assert_eq!(adjudicate(&p, &inst), Adjudication::Alt);
}

#[test]
fn adjudicate_named_strengthens() {
    // Gate amendment (Lemma 3(i) run-initial): `a<b, c=d>e` is exactly the
    // O-names-what-G-cannot divergence — O reads a named `c=d>e` after a positional
    // `a<b` (MIXED -> inadmissible, spec §1108), G reads one positional arg. The named
    // form STRENGTHENS G's uniqueness: with 1 declared param only G is admissible.
    let inst = node_of("a<b, c=d>e");
    match &inst.angles {
        ArgAngles::Forked {
            alt_args,
            alt_named,
        } => {
            assert_eq!(alt_args.len(), 2);
            assert!(alt_named, "O names `c`");
            assert_eq!(alt_args[1].name.as_deref(), Some("c"));
        }
        other => panic!("expected Forked, got {other:?}"),
    }
    assert_eq!(inst.args.len(), 1, "G is the single-arg reading");
    assert!(!inst.named, "G cannot name (the run-initial record is positional)");
    let p = domain_params(&[("p1", None)]);
    assert_eq!(adjudicate(&p, &inst), Adjudication::Primary);
}

#[test]
fn adjudicate_neither_e407() {
    // END-TO-END on real water: fork g=2/o=3 vs 1 declared param -> neither reading
    // matches -> E407 (Error, blocks emission), both interpretations rendered G-first
    // with the parenthesization help.
    let text = r#"@@system M(p1: int) {
    interface:
        go()
    machine:
        $S {
            go() {
                x = @@M(new HashMap<Integer, String>(), z);
            }
        }
}
"#;
    let src = Source::new("t.frm", text.as_bytes().to_vec()).unwrap();
    let ast = segment(&src, Target::Java).unwrap();
    let (syms, diags) = resolve(&ast);
    assert!(
        !diags.iter().any(|d| d.severity == Severity::Error),
        "unexpected resolve errors: {diags:#?}"
    );
    let vdiags = validate(&ast, &syms);
    let e407: Vec<_> = vdiags.iter().filter(|d| d.code == "E407").collect();
    assert_eq!(e407.len(), 1, "expected exactly one E407: {vdiags:#?}");
    let d = e407[0];
    assert_eq!(d.severity, Severity::Error);
    assert!(d.message.contains("reads two ways"), "{}", d.message);
    assert!(
        d.message.contains("new HashMap<Integer, String>()"),
        "G rendering missing: {}",
        d.message
    );
    assert!(
        d.message.contains("`new HashMap<Integer`"),
        "O rendering missing: {}",
        d.message
    );
    assert!(d.message.contains("neither reading matches"), "{}", d.message);
    assert!(d.message.contains("parenthesize"), "help line missing: {}", d.message);
}

#[test]
fn adjudicate_both_e407_defaults() {
    // BothAdmissible is reachable ONLY through defaults (the refined tie-claim): g=2/o=3
    // vs 3 declared params whose third has a default — both counts legal. Ruling:
    // diagnose, never guess.
    let inst = node_of("new HashMap<Integer, String>(), z");
    let p = domain_params(&[("p1", None), ("p2", None), ("p3", Some("0"))]);
    assert_eq!(adjudicate(&p, &inst), Adjudication::BothAdmissible);
}

#[test]
fn pin_mixed_list_e407() {
    // §11.4's pinned break input, END-TO-END: a comparison AND a generic in ONE list.
    // The comparison's `<` never closes -> G nonviable -> the sole reading is O (the
    // generic's inner comma splits, wrongly: 4 args) -> vs 3 declared params -> E407
    // with the Operators message. The documented escape is parenthesization.
    let text = r#"@@system M(p1: int, p2: int, p3: int) {
    interface:
        go()
    machine:
        $S {
            go() {
                x = @@M(a < b, new HashMap<K,V>(), z);
            }
        }
}
"#;
    let src = Source::new("t.frm", text.as_bytes().to_vec()).unwrap();
    let ast = segment(&src, Target::Java).unwrap();
    // The PRODUCTION-parsed node must carry the Operators reading (the mixed list
    // killed G) — the fork rides the tree straight from the wired scan path.
    let mut saw_operators = false;
    for item in &ast.items {
        let Item::System(sys) = item else { continue };
        for sec in &sys.sections {
            let Section::Machine(m) = sec else { continue };
            for mm in &m.members {
                let MachineMember::State(st) = mm else { continue };
                for member in &st.members {
                    let StateMember::Handler(h) = member else { continue };
                    for stmt in &h.body.stmts {
                        let Stmt::Native(n) = stmt else { continue };
                        for part in &n.parts {
                            if let NativePart::Instantiate(inst) = part {
                                assert_eq!(inst.angles, ArgAngles::Operators);
                                assert_eq!(inst.args.len(), 4);
                                saw_operators = true;
                            }
                        }
                    }
                }
            }
        }
    }
    assert!(saw_operators);
    let (syms, _) = resolve(&ast);
    let vdiags = validate(&ast, &syms);
    let e407: Vec<_> = vdiags.iter().filter(|d| d.code == "E407").collect();
    assert_eq!(e407.len(), 1, "expected exactly one E407: {vdiags:#?}");
    assert!(
        e407[0].message.contains("read as operators"),
        "Operators variant expected: {}",
        e407[0].message
    );
    assert!(e407[0].message.contains("parenthesize"), "{}", e407[0].message);
}

#[test]
fn pin_unresolved_name_primary_g() {
    // L37 / owner item 6: when arity is unavailable (unresolved system name), the G
    // reading is the PRIMARY rendering — G keeps generics whole. END-TO-END through
    // emission: the raw G span (with its tell-tale interior spacing `a<b , c>d`) reaches
    // the emitted call; the O re-join (`a<b, c>d`) does not.
    let text = r#"@@system Host {
    interface:
        go()
    machine:
        $S {
            go() {
                let q = @@Unknown(a<b , c>d);
            }
        }
}
"#;
    let src = Source::new("t.frm", text.as_bytes().to_vec()).unwrap();
    let ast = segment(&src, Target::Rust).unwrap();
    let (syms, diags) = resolve(&ast);
    assert!(
        !diags.iter().any(|d| d.severity == Severity::Error),
        "unexpected resolve errors: {diags:#?}"
    );
    let out = driver::emit(&src, &ast, &syms, &Rust);
    assert!(
        out.contains("a<b , c>d"),
        "the G candidate (raw span) did not reach emission:\n{out}"
    );
    assert!(
        !out.contains("a<b, c>d"),
        "the O re-join was rendered instead of the G raw span:\n{out}"
    );
}

#[test]
fn adjudicate_named_coverage() {
    // Gate amendment: named-form admissibility requires every UNPROVIDED declared param
    // to have a default. `x=a<b, y=c>d` vs declared {x, y} with no defaults: G's single
    // named arg (`x=` over the raw span) leaves `y` unprovided-without-default ->
    // inadmissible -> O picked (both names cover). With a default on `y`, G becomes
    // admissible too -> BothAdmissible (the tie is defaults-only) -> E407, never a guess.
    let inst = node_of("x=a<b, y=c>d");
    match &inst.angles {
        ArgAngles::Forked { alt_args, .. } => {
            assert_eq!(alt_args.len(), 2);
            assert_eq!(alt_args[0].name.as_deref(), Some("x"));
            assert_eq!(alt_args[1].name.as_deref(), Some("y"));
        }
        other => panic!("expected Forked, got {other:?}"),
    }
    assert_eq!(inst.args.len(), 1);
    assert_eq!(inst.args[0].name.as_deref(), Some("x"), "G names the run-initial `x`");
    let no_defaults = domain_params(&[("x", None), ("y", None)]);
    assert_eq!(adjudicate(&no_defaults, &inst), Adjudication::Alt);
    let y_defaulted = domain_params(&[("x", None), ("y", Some("0"))]);
    assert_eq!(adjudicate(&y_defaulted, &inst), Adjudication::BothAdmissible);
}
