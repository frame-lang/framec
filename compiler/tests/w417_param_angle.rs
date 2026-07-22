//! **W417 — the declaration-site angle fork is SURFACED, never guessed silently (RFC-0060),
//! proven end-to-end through `segment()` -> `validate()`.**
//!
//! The declaration site (`@@system Name(...)`) has no downstream arity oracle, so it applies
//! `favor-the-template` — unchanged from before RFC-0060. `W417` is *additive visibility*
//! over that decision: it fires iff the angle hypotheses genuinely fork AND **both** readings
//! are well-formed parameter lists (every segment's name a bare identifier). Ordinary generics
//! stay silent because their operator reading yields a non-identifier segment (`Map<K, V>` ->
//! `V>`); the real comparison-operator straddle fires. Emission is invariant: the emitted param
//! count is the favored G reading in every case, W417 or not.
//!
//! Scaffolding (internal `segment()`/`validate()` tree API + cleanroom W417) — never promotes
//! to the cross-language corpus.

use frame_compiler::resolve::{resolve, Severity};
use frame_compiler::scan::{literals::Target, segment};
use frame_compiler::tree::Item;
use frame_compiler::validate::validate;
use frame_compiler::Source;

/// Wrap a header param list in a minimal well-formed `@@system`, scan + resolve + validate,
/// and return `(validation diagnostics, emitted param count)`. The emitted count is the
/// FAVORED (template) reading straight off the tree — the invariant W417 must not perturb.
fn run(params: &str) -> (Vec<frame_compiler::resolve::Diagnostic>, usize) {
    let text = format!(
        "@@system Foo({params}) {{\n    \
         interface:\n        step()\n    \
         machine:\n        $Idle {{ step() {{}} }}\n}}\n"
    );
    let src = Source::new("t.frm", text.into_bytes()).unwrap();
    let ast = segment(&src, Target::Rust).unwrap();
    let (syms, _) = resolve(&ast);
    let vdiags = validate(&ast, &syms);
    let n = ast
        .items
        .iter()
        .find_map(|it| match it {
            Item::System(sys) => {
                Some(sys.params.state.len() + sys.params.enter.len() + sys.params.domain.len())
            }
            _ => None,
        })
        .expect("a system in the tree");
    (vdiags, n)
}

fn w417s(diags: &[frame_compiler::resolve::Diagnostic]) -> Vec<&frame_compiler::resolve::Diagnostic> {
    diags.iter().filter(|d| d.code == "W417").collect()
}

/// The flagship straddle: `<` of `a`'s default and `>` of `b`'s default balance across the
/// comma. Both readings are well-formed param lists -> W417 FIRES. Favor-the-template merges
/// to ONE param (`b` dropped) — the emitted count is unchanged by the warning.
#[test]
fn straddle_fires_w417_and_favors_template() {
    let (diags, n) = run("a: int = x < y, b: int = z > w");
    let w = w417s(&diags);
    assert_eq!(w.len(), 1, "expected exactly one W417: {diags:#?}");
    assert_eq!(n, 1, "favor-the-template must still emit ONE param (b merged away)");

    let d = w[0];
    assert_eq!(d.severity, Severity::Warning);
    assert!(d.message.contains("favors the template"), "{}", d.message);
    assert!(d.message.contains("as generic brackets (1 param)"), "{}", d.message);
    assert!(
        d.message.contains("as comparison operators (2 params)"),
        "{}",
        d.message
    );
    // Both readings are named in the message.
    assert!(
        d.message.contains("a: int = x < y, b: int = z > w"),
        "generic (1-param) rendering missing: {}",
        d.message
    );
    assert!(
        d.message.contains("`b: int = z > w`"),
        "operator (2-param) rendering missing: {}",
        d.message
    );
    assert!(d.message.contains("parenthesize"), "help line missing: {}", d.message);
}

/// The benign generic: the operator reading splits into `store: Map<K` / `V>`, and `V>` is not
/// a well-formed parameter (its name is not an identifier). Exactly one reading is well-formed
/// -> taken in SILENCE, one param. This is today's behavior, now *justified*.
#[test]
fn generic_map_is_silent() {
    let (diags, n) = run("store: Map<K, V>");
    assert!(w417s(&diags).is_empty(), "generic must not warn: {diags:#?}");
    assert_eq!(n, 1, "the template reading is one param");
}

/// The zero-syntax fix: a `<`/`>` inside `(...)` sits at bracket-depth >= 1 and is never
/// angle-counted, so the fork collapses (`z > w`'s `>` has no open `<` -> G nonviable ->
/// Operators, not Forked) and the comma splits cleanly into TWO params. No W417.
#[test]
fn parenthesized_splits_two_and_is_silent() {
    let (diags, n) = run("a: int = (x < y), b: int = z > w");
    assert!(w417s(&diags).is_empty(), "parenthesized form must not warn: {diags:#?}");
    assert_eq!(n, 2, "the comma splits cleanly into two params");
}

/// The accepted BENIGN RESIDUE (RFC-0060 honest scope): an associated-type-shaped generic
/// whose operator reading ALSO parses as identifier-named segments (`x: HashMap<K` / `Item =
/// V>`) fires W417 even though favor-the-template is correct. Bounded noise, not a wrong
/// program: it is a warning, and the FAVORED reading (one param) is right. Eliminating it needs
/// a finer, still type-ignorant signal (operator spacing, #248) — deferred resolution, not
/// detection.
#[test]
fn associated_type_binding_is_benign_residue() {
    let (diags, n) = run("x: HashMap<K, Item = V>");
    let w = w417s(&diags);
    assert_eq!(w.len(), 1, "the residue case warns (documented benign): {diags:#?}");
    assert_eq!(n, 1, "the favored (correct) reading is one param");
}
