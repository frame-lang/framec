//! Per-line native-code rewriting for Erlang body emission.
//!
//! RFC-0035 round 3: the classifier moved to a Frame-implemented
//! module at `crate::frame_c::compiler::erlang_classifier`. This
//! file is now a thin re-export so existing call sites (notably
//! `body_processor.rs`) continue to compile unchanged.
//!
//! The classifier walks each line of spliced handler text and
//! tags it as one of seven `ErlangRewrite` variants
//! (ActionCall, ActionCallWithBind, InterfaceCall,
//! InterfaceCallWithBind, RecordUpdate, Plain, Reply). The
//! body processor uses the tag to decide how to thread the
//! Data record through the resulting Erlang statement.
//!
//! See `erlang_classifier/mod.rs` for the public API and
//! `erlang_classifier/erlang_line_classifier.frs` for the
//! Frame source.

pub(super) use crate::frame_c::compiler::erlang_classifier::{
    erlang_rewrite_native_classified_full, ErlangRewrite,
};

#[allow(unused_imports)]
pub(super) use crate::frame_c::compiler::erlang_classifier::erlang_rewrite_native_classified;
