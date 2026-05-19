//! Body closer for the Frame-structural pseudo-target — Frame-generated state machine.
//!
//! Source: frame_structural.frs (Frame specification)
//! Generated: frame_structural.gen.rs (via `framec compile -l rust`)
//! This file: glue module wiring generated FSM to the `BodyCloser` trait.
//!
//! Used by the GraphViz pipeline. See `frame_structural.frs` for the
//! state-machine spec; see FRAMEC_BUGS.md #24, #25, #26 for the bug
//! history that motivated the dogfooded approach.
//!
//! To regenerate:
//!   ./target/release/framec compile -l rust \
//!     -o framec/src/frame_c/compiler/body_closer/ \
//!     framec/src/frame_c/compiler/body_closer/frame_structural.frs
//!   mv framec/src/frame_c/compiler/body_closer/frame_structural.rs \
//!     framec/src/frame_c/compiler/body_closer/frame_structural.gen.rs

#![allow(unreachable_patterns)]
#![allow(unused_mut)]
#![allow(dead_code)]
#![allow(non_snake_case)]
#![allow(unused_variables)]

include!("frame_structural.gen.rs");

use super::{BodyCloser, CloseError, CloseErrorKind};

pub struct BodyCloserFrameStructural;

impl BodyCloser for BodyCloserFrameStructural {
    fn close_byte(&mut self, bytes: &[u8], open_brace_index: usize) -> Result<usize, CloseError> {
        let mut fsm = FrameStructuralBodyCloserFsm::new();
        fsm.bytes = bytes.to_vec();
        fsm.pos = open_brace_index + 1;
        fsm.depth = 1;
        fsm.scan();
        match fsm.error_kind {
            0 => Ok(fsm.result_pos),
            1 => Err(CloseError {
                kind: CloseErrorKind::UnterminatedString,
                message: fsm.error_msg,
            }),
            2 => Err(CloseError {
                kind: CloseErrorKind::UnterminatedComment,
                message: fsm.error_msg,
            }),
            _ => Err(CloseError {
                kind: CloseErrorKind::UnmatchedBraces,
                message: fsm.error_msg,
            }),
        }
    }
}
