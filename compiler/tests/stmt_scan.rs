//! **The statement classifier, as a system, yields the known `(kind, end)` — standalone.**
//!
//! `stmt_scan::classify` is generated from `stmt_scan.frs`, a `@@[scan(u8)]` Frame system.
//! This proves — by running — that its `(kind, end)` classification matches the KNOWN-CORRECT
//! values (captured from the running system; no hand oracle) at EVERY position, across all the
//! constructs and the native cases — including the load-bearing `(exit)->` guard that keeps
//! `(*p)->field` and `(a) -> b` from being transitions when read as a STATEMENT START (position
//! 0), which each `check` below pins by asserting position 0 is native `(0, 0)`.
//!
//! Kinds (from `stmt_scan.frs`): 0=Native 1=Transition 2=StackPush 3=StackPop 4=Forward
//! 5=(bare `pop$`). A native (non-Frame) position always classifies as `(0, i)`.
//!
//! SCAFFOLDING (white-box on the internal `stmt_scan::classify`).

use frame_compiler::text::scan::stmt_scan;

/// Assert `stmt_scan::classify` yields the pinned `(kind, end)` at each listed non-native position
/// `nz`, and the native default `(0, i)` at EVERY other position of `src`. Because a statement
/// start that is native classifies as `(0, i)`, listing NO entry for position 0 pins it native —
/// exactly the `(exit)->` guard rows.
fn check(src: &str, nz: &[(usize, i32, usize)]) {
    let b = src.as_bytes();
    for i in 0..b.len() {
        let got = stmt_scan::classify(b, i, b.len());
        match nz.iter().find(|r| r.0 == i) {
            Some(&(_, k, e)) => assert_eq!(got, (k, e), "at byte {i} of {src:?}"),
            None => assert_eq!(got, (0, i), "expected native (0,{i}) at byte {i} of {src:?}"),
        }
    }
}

#[test]
fn every_construct() {
    check("-> $Next", &[(0, 1, 8)]); // Transition
    check("-> (arg) $Next(state)", &[(0, 1, 21)]); // Transition with enter args
    check("push$ -> $Work", &[(0, 2, 14), (6, 1, 14)]); // StackPush (and the inner `-> $Work`)
    check("-> pop$", &[(0, 3, 7), (3, 5, 7)]); // StackPop (and the inner bare `pop$`)
    check("(reason) -> pop$", &[(0, 3, 16), (9, 3, 16), (12, 5, 16)]); // StackPop with exit args
    check("(exit) -> (enter) $Back", &[(0, 1, 23), (7, 1, 23)]); // Transition, exit+enter args
    check("=> $^", &[(0, 4, 5)]); // Forward
}

#[test]
fn the_native_guard_cases() {
    // `(a) -> b` and `(*p)->field` have no $Target -> NOT a transition when read at position 0
    // (the statement start): `check` asserts position 0 is native `(0, 0)`. The `->b` / `->field`
    // read mid-string (position 4) IS classified kind 1 by the shared arrow leaf, and that is
    // pinned too — but it is NOT where the statement begins.
    check("(a) -> b", &[(4, 1, 8)]);
    check("(*p)->field", &[(4, 1, 11)]);
    check("(x) -> 3 + 4", &[(4, 1, 12)]);
    check("let y = 1;", &[]); // entirely native
    check("self.cursor = self.cursor + 1;", &[]); // entirely native
    check("", &[]);
}

#[test]
fn packed_and_edge_positions() {
    check("-> $A -> $B", &[(0, 1, 11), (6, 1, 11)]);
    check("push$->$X", &[(0, 2, 9), (5, 1, 9)]);
    check("( ) -> pop$", &[(0, 3, 11), (4, 3, 11), (7, 5, 11)]);
    check("=>$^;", &[(0, 4, 5)]);
}
