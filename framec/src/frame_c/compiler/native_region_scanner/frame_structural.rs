//! Frame-structural skipper used for the GraphViz pipeline —
//! Frame-generated state machine.
//!
//! Source: frame_structural_skipper.frs (Frame specification)
//! Generated: frame_structural_skipper.gen.rs (via `framec compile -l rust`)
//! This file: glue module wiring generated FSM to the
//! `SyntaxSkipper` and `NativeRegionScanner` traits.
//!
//! See `frame_structural_skipper.frs` for the state-machine spec;
//! see FRAMEC_BUGS.md #24, #25, #26 for the bug history that
//! motivated the dogfooded approach (hand-coded scanners kept
//! producing whack-a-mole bugs against subtle syntactic edges).
//!
//! To regenerate:
//!   ./target/release/framec compile -l rust \
//!     -o framec/src/frame_c/compiler/native_region_scanner/ \
//!     framec/src/frame_c/compiler/native_region_scanner/frame_structural_skipper.frs
//!   mv framec/src/frame_c/compiler/native_region_scanner/frame_structural_skipper.rs \
//!     framec/src/frame_c/compiler/native_region_scanner/frame_structural_skipper.gen.rs

#![allow(unreachable_patterns)]
#![allow(unused_mut)]
#![allow(dead_code)]
#![allow(non_snake_case)]
#![allow(unused_variables)]

include!("frame_structural_skipper.gen.rs");

use super::unified::{
    balanced_paren_end_c_like, skip_hash_line_comment, skip_line_comment, skip_rust_string,
    SyntaxSkipper,
};
use super::{NativeRegionScanner, ScanError, ScanResult};
use crate::frame_c::compiler::body_closer::frame_structural::BodyCloserFrameStructural;
use crate::frame_c::compiler::body_closer::BodyCloser;

pub struct FrameStructuralSkipper;

impl SyntaxSkipper for FrameStructuralSkipper {
    fn body_closer(&self) -> Box<dyn BodyCloser> {
        Box::new(BodyCloserFrameStructural)
    }

    fn skip_comment(&self, bytes: &[u8], i: usize, end: usize) -> Option<usize> {
        let mut fsm = FrameStructuralSyntaxSkipperFsm::new();
        fsm.bytes = bytes[..end].to_vec();
        fsm.pos = i;
        fsm.end = end;
        fsm.do_skip_comment();
        if fsm.success != 0 {
            Some(fsm.result_pos)
        } else {
            None
        }
    }

    fn skip_string(&self, bytes: &[u8], i: usize, end: usize) -> Option<usize> {
        let mut fsm = FrameStructuralSyntaxSkipperFsm::new();
        fsm.bytes = bytes[..end].to_vec();
        fsm.pos = i;
        fsm.end = end;
        fsm.do_skip_string();
        if fsm.success != 0 {
            Some(fsm.result_pos)
        } else {
            None
        }
    }

    fn find_line_end(&self, bytes: &[u8], start: usize, end: usize) -> usize {
        let mut fsm = FrameStructuralSyntaxSkipperFsm::new();
        fsm.bytes = bytes[..end].to_vec();
        fsm.pos = start;
        fsm.end = end;
        fsm.do_find_line_end();
        fsm.result_pos
    }

    fn balanced_paren_end(&self, bytes: &[u8], i: usize, end: usize) -> Option<usize> {
        let mut fsm = FrameStructuralSyntaxSkipperFsm::new();
        fsm.bytes = bytes[..end].to_vec();
        fsm.pos = i;
        fsm.end = end;
        fsm.do_balanced_paren_end();
        if fsm.success != 0 {
            Some(fsm.result_pos)
        } else {
            None
        }
    }
}

/// Frame-structural `NativeRegionScanner` — uses the shared scanner
/// over `FrameStructuralSkipper` so Frame-segment detection in the
/// GraphViz path matches the same lexical rules the body_closer
/// uses.
pub struct NativeRegionScannerFrameStructural;

impl NativeRegionScanner for NativeRegionScannerFrameStructural {
    fn scan(&mut self, bytes: &[u8], open_brace_index: usize) -> Result<ScanResult, ScanError> {
        super::unified::scan_native_regions(&FrameStructuralSkipper, bytes, open_brace_index)
    }
}
