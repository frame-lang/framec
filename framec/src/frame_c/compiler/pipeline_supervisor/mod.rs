//! Pipeline supervisor — the framec compile pipeline expressed as a Frame
//! state machine that **drives** the orchestrator (RFC-0035 Round 8).
//!
//! `pipeline_supervisor.frs` defines `PipelineFsm`: one state per compile
//! phase, each state's `$>` enter handler running a `do_*` phase over the
//! shared [`PipelineCtx`] and transitioning to the next phase (or stashing an
//! early-exit [`CompileResult`] in `early` and jumping to `$Done`). The
//! machine self-drives from `$Idle` to a terminal state inside a single
//! `run()` call — the transition graph *is* the pipeline control flow.
//!
//! Earlier rounds (R7) had this FSM merely *observe* phase boundaries; R8
//! makes it the controller. The phase bodies stay native in
//! `pipeline/compiler.rs::do_*` (opaque pass-through); Frame owns the
//! sequencing. `framec compile -l graphviz pipeline_supervisor.frs` renders
//! the real pipeline diagram.
//!
//! To regenerate after editing the `.frs` (then rename to `.gen.rs`):
//!   framec compile -l rust -o \
//!     framec/src/frame_c/compiler/pipeline_supervisor/ \
//!     framec/src/frame_c/compiler/pipeline_supervisor/pipeline_supervisor.frs

mod pipeline_fsm {
    #![allow(unreachable_patterns)]
    #![allow(unused_mut)]
    #![allow(dead_code)]
    #![allow(non_snake_case)]
    #![allow(unused_variables)]
    #![allow(unused_parens)]

    // The phase functions + the threaded context/result types the generated
    // handlers reference unqualified. Phase bodies live in `pipeline/compiler.rs`.
    use crate::frame_c::compiler::pipeline::compiler::{
        do_assemble, do_graphviz, do_module_gates, do_parse, do_segment, do_validate_codegen,
        CompileResult, PipelineCtx,
    };

    include!("pipeline_supervisor.gen.rs");
}

use crate::frame_c::compiler::pipeline::compiler::{CompileResult, PipelineCtx};
use crate::frame_c::compiler::pipeline::config::PipelineConfig;

/// Run the full compile pipeline, driven by the `PipelineFsm` state machine.
///
/// Builds the FSM, seeds it with the real [`PipelineCtx`], and runs it. The
/// enter-handler chain executes every phase in order and lands in `$Done`
/// with the finished `CompileResult` in `early`.
pub fn run_pipeline(source: &[u8], config: &PipelineConfig) -> CompileResult {
    let mut fsm = pipeline_fsm::PipelineFsm::__create();
    fsm.ctx = PipelineCtx::new(source, config);
    fsm.run();
    fsm.early
        .expect("PipelineFsm always reaches $Done with `early` set")
}
