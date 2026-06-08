//! Conformance-coverage guard for RFC-0042 `@@fsm`.
//!
//! The RFC's "Validation Tests" section defines a conformance suite where each
//! entry has a stable id (`**FSM-TEST-NNN — ...**`). This test asserts that
//! every such id has at least one backing test in the framepiler source,
//! tagged with a `FSM-TEST-NNN` reference (conventionally a `/// FSM-TEST-NNN`
//! doc-comment on the test, but the id appearing anywhere in the test source
//! counts). It fails — listing the gaps — if a spec id has no backing test, so
//! the RFC↔test mapping cannot silently drift as the suite or the
//! implementation evolves.
//!
//! Scope: this checks *traceability* (every spec id is claimed by a test), not
//! that each test's assertions are correct — that is the job of the tests
//! themselves.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

/// Spec ids the RFC defines but that are intentionally not yet individually
/// backed by a test, each with a justification. Keep this EMPTY if at all
/// possible — an entry here is a *documented coverage debt*, deliberately
/// visible, not a silent gap. Adding one is a conscious decision.
const ALLOWLIST: &[(&str, &str)] = &[
    // Both require E706 (type mismatch). The @@fsm validator deliberately has
    // no type system (RFC-0042 §4.1; see fsm_validator/mod.rs module docs:
    // "E706 ... is out of scope: Frame has no type system"), so these two
    // conformance points cannot be exercised until a type checker exists.
    // Deferred for v0.1 — tracked here rather than silently uncovered.
    (
        "FSM-TEST-102",
        "E706 return-type mismatch — no @@fsm type system in v0.1",
    ),
    (
        "FSM-TEST-702",
        "E706 Mode C type mismatch — no @@fsm type system in v0.1",
    ),
    // Impl gap needing a dedicated decision: the validator only checks
    // `self.<field>` references, so a *bare* name (`count` vs `self.count`) is
    // not flagged E703 (RFC §4.2). A fix needs context-aware bare-name
    // validation (exclude initializer-scope param refs — FSM-TEST-103 — plus
    // call targets, built-ins, and action names) with false-positive risk;
    // deferred pending that design decision rather than a rushed broad change.
    (
        "FSM-TEST-033",
        "bare-name (non-`self.`) E703 validation not implemented — needs context-aware design",
    ),
    // The `%{}` (leave-final) embedding action is emitted, but it fires only on
    // a DFA *step* from an accepting into a non-accepting state; a plain
    // `/[0-9]+/` simply halts at a non-match without such a step, so a clean
    // firing scenario needs dedicated semantics work. Deferred. (FSM-TEST-600
    // `>{}`/`${}`, 601 `@{}`, and 602 `@eof{}` are covered.)
    (
        "FSM-TEST-603",
        "`%{}` leave-final firing needs a dedicated DFA-step scenario — deferred",
    ),
    // Mode C error validation (§8.3) is not implemented: the framepiler does
    // not reject an inner/outer alphabet mismatch (E731) or a dynamic
    // `/@which/` reference (E732). Enforcing these needs cross-fsm resolution
    // (compare inner vs outer alphabet) and static-resolvability analysis of
    // the `/@name/` target — a dedicated effort, deferred for v0.1.
    (
        "FSM-TEST-703",
        "Mode C alphabet-mismatch E731 not enforced — needs cross-fsm validation",
    ),
    (
        "FSM-TEST-704",
        "Mode C dynamic-dispatch E732 not enforced — needs static-resolvability check",
    ),
];

fn manifest() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// Every `FSM-TEST-<digits>` id defined by a bold header in the RFC's
/// Validation Tests section (`**FSM-TEST-NNN — ...`). Casual cross-references
/// in prose (e.g. "Companion to FSM-TEST-002") are not definitions and are
/// ignored — only lines that *begin* a test entry count.
fn spec_ids() -> BTreeSet<String> {
    let rfc = manifest().join("../docs/rfcs/rfc-0042.md");
    let text =
        fs::read_to_string(&rfc).unwrap_or_else(|e| panic!("cannot read {}: {e}", rfc.display()));
    let mut ids = BTreeSet::new();
    for line in text.lines() {
        if let Some(rest) = line.trim_start().strip_prefix("**FSM-TEST-") {
            let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
            if !digits.is_empty() {
                ids.insert(format!("FSM-TEST-{digits}"));
            }
        }
    }
    ids
}

/// Every `FSM-TEST-<digits>` reference appearing anywhere in `framec/src`.
fn source_ids() -> BTreeSet<String> {
    let mut ids = BTreeSet::new();
    collect_rs(&manifest().join("src"), &mut ids);
    ids
}

fn collect_rs(dir: &Path, ids: &mut BTreeSet<String>) {
    let entries = fs::read_dir(dir).unwrap_or_else(|e| panic!("read_dir {}: {e}", dir.display()));
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_rs(&path, ids);
        } else if path.extension().is_some_and(|e| e == "rs") {
            if let Ok(text) = fs::read_to_string(&path) {
                ids_in(&text, ids);
            }
        }
    }
}

/// Collect `FSM-TEST-<digits>` tokens from arbitrary text.
fn ids_in(text: &str, out: &mut BTreeSet<String>) {
    const NEEDLE: &str = "FSM-TEST-";
    let mut rest = text;
    while let Some(pos) = rest.find(NEEDLE) {
        let after = &rest[pos + NEEDLE.len()..];
        let digits: String = after.chars().take_while(|c| c.is_ascii_digit()).collect();
        if !digits.is_empty() {
            out.insert(format!("FSM-TEST-{digits}"));
        }
        rest = after;
    }
}

#[test]
fn every_fsm_test_id_has_a_backing_test() {
    let spec = spec_ids();
    assert!(
        spec.len() >= 80,
        "expected ~88 FSM-TEST ids in the RFC's Validation Tests section, \
         found only {} — has the section moved or the header format changed?",
        spec.len()
    );

    let src = source_ids();
    let allow: BTreeSet<&str> = ALLOWLIST.iter().map(|(id, _)| *id).collect();

    let missing: Vec<&str> = spec
        .iter()
        .map(String::as_str)
        .filter(|id| !src.contains(*id) && !allow.contains(id))
        .collect();

    assert!(
        missing.is_empty(),
        "{} RFC-0042 conformance id(s) have no backing test in framec/src:\n  {}\n\n\
         Fix: add a `/// {{id}} — <title>` doc-comment to the test that covers each \
         (or, if genuinely deferred, add it to ALLOWLIST with a justification).",
        missing.len(),
        missing.join("\n  ")
    );

    // Guard the allowlist against rot: an entry that names a non-spec id, or
    // an id that now *is* backed, is itself a defect to clean up.
    for (id, why) in ALLOWLIST {
        assert!(
            spec.contains(*id),
            "ALLOWLIST entry {id} ({why}) is not a defined RFC FSM-TEST id"
        );
        assert!(
            !src.contains(*id),
            "ALLOWLIST entry {id} ({why}) now has a backing test — remove it from ALLOWLIST"
        );
    }
}
