//! Transition-string metadata parser — wrapper for the Frame FSM in
//! `transition_meta_scanner.gen.rs` (RFC-0035 Round 11).
//!
//! `parse_transition_meta` decomposes a (trimmed) transition string against
//! the grammar `(exit)? -> (=>)? (enter)? ($State(args)? | pop$) "label"?`
//! into a `SegmentMetadata::Transition`. The FSM walks the grammar one state
//! per element (`$Target → $ExitArgs → $EnterArgs → $StateArgs →
//! $LabelForward → $Done`); this wrapper assembles the result. It replaces the
//! hand-rolled `Transition` arm of `unified/metadata.rs` — the rest of that
//! file stays a flat `match kind` dispatch (not an FSM candidate).
//!
//! To regenerate after editing the `.frs` (then rename to `.gen.rs`):
//!   framec compile -l rust -o \
//!     framec/src/frame_c/compiler/transition_meta_scanner/ \
//!     framec/src/frame_c/compiler/transition_meta_scanner/transition_meta_scanner.frs

use crate::frame_c::compiler::native_region_scanner::SegmentMetadata;

mod scanner {
    #![allow(unreachable_patterns)]
    #![allow(unused_mut)]
    #![allow(dead_code)]
    #![allow(non_snake_case)]
    #![allow(unused_variables)]
    #![allow(unused_parens)]

    include!("transition_meta_scanner.gen.rs");
}

/// Parse a trimmed transition string into its `SegmentMetadata::Transition`.
pub fn parse_transition_meta(trimmed: &str) -> SegmentMetadata {
    let mut fsm = scanner::TransitionMetaScannerFsm::__create();
    fsm.bytes = trimmed.as_bytes().to_vec();
    fsm.parse();
    SegmentMetadata::Transition {
        // state args are meaningless on pop$ — the popped compartment brings
        // its own from the snapshot; the target is the literal "pop$".
        target_state: if fsm.has_pop {
            "pop$".to_string()
        } else {
            std::mem::take(&mut fsm.target)
        },
        exit_args: fsm.exit_args.take(),
        enter_args: fsm.enter_args.take(),
        state_args: if fsm.has_pop {
            None
        } else {
            fsm.state_args.take()
        },
        label: fsm.label.take(),
        is_pop: fsm.has_pop,
        is_forward: fsm.is_forward,
    }
}
