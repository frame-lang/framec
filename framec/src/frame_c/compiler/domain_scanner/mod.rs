//! Domain-section line scanner — wrapper for the Frame FSM in
//! `domain_scanner.gen.rs` (RFC-0035 Round 9).
//!
//! Replaces the hand-rolled outer byte-walk that used to live in
//! `pipeline_parser/domain.rs::parse_domain`. The `.frs` is the source of
//! truth; regenerate with:
//!
//!   framec compile -l rust -o <dir>/ domain_scanner/domain_scanner.frs
//!   # then rename the output to domain_scanner.gen.rs
//!
//! The FSM owns the byte cursor and accumulates `DomainVar`s; this wrapper
//! drives it and hands the caller the fields + resume cursor.

#![allow(clippy::all)]
#![allow(unused_mut)]
#![allow(unused_variables)]
#![allow(dead_code)]

use crate::frame_c::compiler::frame_ast::{DomainVar, Span, Type};
use crate::frame_c::compiler::pipeline_parser::ParseError;

// `ExprScannerFsm` sub-machine (the dogfooded balanced-expression scanner)
// for a field's `= init` RHS. Same `include!` convention as `domain.rs`'s
// former `_expr_scanner`; the `.frs` handler references it as `_ds_expr`.
mod _ds_expr {
    include!("../native_region_scanner/expr_scanner.gen.rs");
}

include!("domain_scanner.gen.rs");

/// Scan a `domain:` section body. `bytes` is the full source; `start` is the
/// byte offset just past `domain:`. Returns the parsed fields together with
/// the cursor to resume the lexer at, or the first `ParseError`.
pub fn scan_domain(bytes: &[u8], start: usize) -> Result<(Vec<DomainVar>, usize), ParseError> {
    let mut fsm = DomainScannerFsm::new();
    fsm.bytes = bytes.to_vec();
    fsm.pos = start;
    fsm.scan();
    match fsm.error.take() {
        Some(e) => Err(e),
        None => Ok((std::mem::take(&mut fsm.vars), fsm.result_cursor)),
    }
}
