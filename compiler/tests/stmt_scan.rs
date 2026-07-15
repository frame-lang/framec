//! **The statement classifier, as a system, agrees with the hand `frame_stmt`.**
//!
//! `stmt_scan::classify` is generated from `stmt_scan.frs`, a `@@[scan(u8)]` Frame system.
//! This proves — by running — that its (kind, end) classification matches the production
//! `frame_stmt_classify` at EVERY position, across all the constructs and the native cases
//! (including the `(exit)->` guard that keeps `(*p)->field` and `(a) -> b` from being
//! transitions).

use frame_compiler::text::scan::machine::frame_stmt_classify;
use frame_compiler::text::scan::stmt_scan;

fn agree(src: &str) {
    let bytes = src.as_bytes();
    for i in 0..bytes.len() {
        assert_eq!(
            stmt_scan::classify(bytes, i, bytes.len()),
            frame_stmt_classify(bytes, i, bytes.len()),
            "disagreement at byte {i} of {src:?}"
        );
    }
}

#[test]
fn every_construct_agrees() {
    agree("-> $Next");                 // Transition
    agree("-> (arg) $Next(state)");    // Transition with enter args
    agree("push$ -> $Work");           // StackPush
    agree("-> pop$");                  // StackPop
    agree("(reason) -> pop$");         // StackPop with exit args
    agree("(exit) -> (enter) $Back");  // Transition with exit+enter args
    agree("=> $^");                    // Forward
}

#[test]
fn the_native_guard_cases_agree() {
    // `(a) -> b` and `(*p)->field` have no $Target -> NOT a transition, native.
    agree("(a) -> b");
    agree("(*p)->field");
    agree("(x) -> 3 + 4");
    agree("let y = 1;");
    agree("self.cursor = self.cursor + 1;");
    agree("");
}

#[test]
fn packed_and_edge_positions_agree() {
    agree("-> $A -> $B");
    agree("push$->$X");
    agree("( ) -> pop$");
    agree("=>$^;");
}
