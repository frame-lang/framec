//! **The second dogfooded scanner — a counter automaton — runs correctly, and restarts.**
//!
//! `paren_balance::scan` is generated from `paren_balance.frs`, a `@@[scan(u8)]` Frame
//! system whose `depth` domain counter is reset by `scan_at`. This proves framec-ng can
//! emit a restartable counter scanner (the capability the transition-head and arg-splitter
//! scanners need), by running it.

use frame_compiler::text::scan::paren_balance::scan;

#[test]
fn balanced_groups_find_the_matching_close() {
    assert_eq!(scan(b"(a(b)c)xyz", 0), Some(7), "outer group ends at the 7th byte");
    assert_eq!(scan(b"()", 0), Some(2), "empty group");
    assert_eq!(scan(b"(((())))", 0), Some(8), "deep nesting");
    assert_eq!(scan(b"(a)(b)", 0), Some(3), "stops at the FIRST balanced close");
}

#[test]
fn unbalanced_is_rejected() {
    assert_eq!(scan(b"(a(b)c", 0), None, "one unmatched open");
    assert_eq!(scan(b"(", 0), None, "lone open");
}

#[test]
fn scan_is_restartable_the_counter_resets() {
    let s: &[u8] = b"(a(b)c)";
    let mut runs = Vec::new();
    // Re-run on fresh instances AND check the extent is stable across repeated scans —
    // if `depth` leaked between scans, the second answer would differ.
    for _ in 0..3 {
        runs.push(scan(s, 0));
    }
    assert_eq!(runs, vec![Some(7), Some(7), Some(7)], "depth resets each scan");
}
