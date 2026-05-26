
// RFC-0035 Round 8 — the framec compile pipeline AS a Frame state machine
// that DRIVES the orchestrator (it no longer merely observes it).
//
// Each compile phase is a STATE. The state's `$>` enter handler runs that
// phase (a `do_*` function over the shared `PipelineCtx`) and then transitions
// to the next phase — or, on an early exit (validation error, GraphViz DOT
// output, segmentation failure), stashes the finished `CompileResult` in
// `early` and jumps straight to `$Done`. The machine self-drives: `run()`
// kicks it from `$Idle`, and the enter-handler chain runs synchronously to a
// terminal state. The transition graph IS the pipeline control flow — delete
// `$ValidateCodegen`'s `-> $Assemble` and codegen genuinely stops feeding
// assembly. `framec compile -l graphviz` on this file renders the real
// pipeline, not a hand-maintained cartoon of it.
//
// The phase bodies stay native (in `pipeline/compiler.rs::do_*`) — Frame owns
// the *control structure*, the native fns are opaque pass-through, exactly as
// the dogfooding thesis intends. `PipelineCtx` (owned: source, config, and
// every intermediate) is the single domain field threaded through the phases;
// `early` carries the result out. The caller (`compile_ast_based`) builds the
// system, assigns the real `ctx`, calls `run()`, and returns `early`.

#[allow(dead_code)]
#[allow(non_camel_case_types)]
#[allow(non_snake_case)]
#[allow(unused_variables)]
#[allow(unused_mut)]
#[allow(unused_imports)]
#[allow(clippy::assign_op_pattern)]
#[allow(clippy::clone_on_copy)]
#[allow(clippy::derivable_impls)]
#[allow(clippy::match_single_binding)]
#[allow(clippy::needless_return)]
#[allow(clippy::new_without_default)]
#[allow(clippy::single_match)]
mod _pipeline_fsm_framec {
    use super::*;
    extern crate alloc;
    use alloc::{vec, format};
    #[derive(Clone, Debug)]
    #[allow(dead_code, non_camel_case_types)]
    enum PipelineFsmFrameEvent {
        Run {  },
        FrameEnter {},
        FrameExit {},
    }

    #[derive(Clone)]
    #[allow(dead_code, non_camel_case_types)]
    enum PipelineFsmFrameReturn {
        _Lifecycle(alloc::rc::Rc<dyn core::any::Any>),
    }

    #[allow(dead_code)]
    impl PipelineFsmFrameEvent {
        fn name(&self) -> &'static str {
            match self {
                PipelineFsmFrameEvent::Run { .. } => "run",
                PipelineFsmFrameEvent::FrameEnter { .. } => "$>",
                PipelineFsmFrameEvent::FrameExit { .. } => "<$",
            }
        }
    }

    #[derive(Clone, Debug)]
    #[allow(dead_code, non_camel_case_types)]
    enum PipelineFsmFrameValue {
        Int(i64),
        Float(f64),
        Bool(bool),
        Str(String),
        List(Vec<Self>),
        Dict(alloc::collections::BTreeMap<String, Self>),
    }

    #[allow(dead_code, non_camel_case_types)]
    struct PipelineFsmFrameContext {
        event: alloc::rc::Rc<PipelineFsmFrameEvent>,
        _return: Option<PipelineFsmFrameReturn>,
        _data: alloc::collections::BTreeMap<String, PipelineFsmFrameValue>,
        _transitioned: bool,
    }

    impl PipelineFsmFrameContext {
        fn new(event: alloc::rc::Rc<PipelineFsmFrameEvent>, default_return: Option<PipelineFsmFrameReturn>) -> Self {
            Self {
                event,
                _return: default_return,
                _data: alloc::collections::BTreeMap::new(),
                _transitioned: false,
            }
        }
    }

    #[allow(dead_code, non_camel_case_types)]
    #[derive(Clone)]
    enum PipelineFsmStateContext {
        Idle,
        Segment,
        Parse,
        ModuleGates,
        Graphviz,
        ValidateCodegen,
        Assemble,
        Done,
        __NoContext,
    }

    impl Default for PipelineFsmStateContext {
        fn default() -> Self {
            PipelineFsmStateContext::Idle
        }
    }

    #[allow(dead_code, non_camel_case_types)]
    #[derive(Clone)]
    struct PipelineFsmCompartment {
        state: String,
        state_context: PipelineFsmStateContext,
        forward_event: Option<PipelineFsmFrameEvent>,
        parent_compartment: Option<Box<PipelineFsmCompartment>>,
    }

    impl PipelineFsmCompartment {
        fn new(state: &str) -> Self {
            let state_context = match state {
                "Idle" => PipelineFsmStateContext::Idle,
                "Segment" => PipelineFsmStateContext::Segment,
                "Parse" => PipelineFsmStateContext::Parse,
                "ModuleGates" => PipelineFsmStateContext::ModuleGates,
                "Graphviz" => PipelineFsmStateContext::Graphviz,
                "ValidateCodegen" => PipelineFsmStateContext::ValidateCodegen,
                "Assemble" => PipelineFsmStateContext::Assemble,
                "Done" => PipelineFsmStateContext::Done,
                _ => PipelineFsmStateContext::__NoContext,
            };
            Self {
                state: state.to_string(),
                state_context,
                forward_event: None,
                parent_compartment: None,
            }
        }
    }

    #[allow(dead_code)]
    pub struct PipelineFsm {
        _state_stack: Vec<PipelineFsmCompartment>,
        __compartment: PipelineFsmCompartment,
        __next_compartment: Option<PipelineFsmCompartment>,
        _context_stack: Vec<PipelineFsmFrameContext>,
        pub ctx: PipelineCtx,
        pub early: Option<CompileResult>,
    }

    #[allow(non_snake_case)]
    impl PipelineFsm {
        pub fn new() -> Self {
            Self {
                _state_stack: Vec::new(),
                _context_stack: Vec::new(),
                ctx: PipelineCtx::empty(),
                early: None,
                __compartment: PipelineFsmCompartment::new("Idle"),
                __next_compartment: None,
            }
        }

        pub fn __create() -> Self {
            let mut c = Self::new();
            c.__compartment = c.__prepareEnter("Idle");
            let __e = alloc::rc::Rc::new(PipelineFsmFrameEvent::FrameEnter {});
            let __ctx = PipelineFsmFrameContext::new(alloc::rc::Rc::clone(&__e), None);
            c._context_stack.push(__ctx);
            c.__kernel(&__e);
            c._context_stack.pop();
            c
        }

        fn __hsm_chain(&mut self, leaf: &str) -> &'static [&'static str] {
            match leaf {
                "Idle" => &["Idle"],
                "Segment" => &["Segment"],
                "Parse" => &["Parse"],
                "ModuleGates" => &["ModuleGates"],
                "Graphviz" => &["Graphviz"],
                "ValidateCodegen" => &["ValidateCodegen"],
                "Assemble" => &["Assemble"],
                "Done" => &["Done"],
                _ => &[],
            }
        }

        fn __prepareEnter(&mut self, leaf: &str) -> PipelineFsmCompartment {
            let chain = self.__hsm_chain(leaf);
            let mut comp: Option<PipelineFsmCompartment> = None;
            for name in chain.iter() {
                let mut new_comp = PipelineFsmCompartment::new(name);
                if let Some(parent) = comp.take() {
                    new_comp.parent_compartment = Some(Box::new(parent));
                }
                comp = Some(new_comp);
            }
            comp.expect("chain must contain at least the leaf state")
        }

        fn __kernel(&mut self, __e: &alloc::rc::Rc<PipelineFsmFrameEvent>) {
            // Route event to current state.
            self.__router(__e);
            // Drain any transitions queued by the handler.
            while self.__next_compartment.is_some() {
                let next_compartment = self.__next_compartment.take().expect("invariant: while-loop guard checked is_some()");
                // Exit the current (leaf) state. RFC-0025.1: exit args live in the
                // source state's typed ctx (written at the transition site), so the
                // synthesized `<$` event carries no payload.
                let exit_event = alloc::rc::Rc::new(PipelineFsmFrameEvent::FrameExit {});
                self.__router(&exit_event);
                // Switch to the new compartment.
                self.__compartment = next_compartment;
                // Three-branch forward-event handling (RFC-0025 Track B.1: forward
                // event is matched on enum variant; $> recognition is now a
                // structural match, not a string compare).
                match self.__compartment.forward_event.take() {
                    None => {
                        // No forwarded event — synthesize a fresh $>. RFC-0025.1:
                        // enter args live in the destination's typed ctx.
                        let enter_event = alloc::rc::Rc::new(PipelineFsmFrameEvent::FrameEnter {});
                        self.__router(&enter_event);
                    }
                    Some(fwd) if matches!(fwd, PipelineFsmFrameEvent::FrameEnter { .. }) => {
                        // Forwarded event IS $> — dispatch directly so the
                        // destination's $> handler receives the caller's payload.
                        let fwd_rc = alloc::rc::Rc::new(fwd);
                        self.__router(&fwd_rc);
                    }
                    Some(fwd) => {
                        // Forwarded event is not $> — initialize the destination
                        // with a fresh $>, then dispatch the forward.
                        let enter_event = alloc::rc::Rc::new(PipelineFsmFrameEvent::FrameEnter {});
                        self.__router(&enter_event);
                        let fwd_rc = alloc::rc::Rc::new(fwd);
                        self.__router(&fwd_rc);
                    }
                }
                for ctx in self._context_stack.iter_mut() {
                    ctx._transitioned = true;
                }
            }
        }

        fn __router(&mut self, __e: &alloc::rc::Rc<PipelineFsmFrameEvent>) {
            let __ev: &PipelineFsmFrameEvent = __e;
            match self.__compartment.state.as_str() {
                "Idle" => self._state_Idle(__ev),
                "Segment" => self._state_Segment(__ev),
                "Parse" => self._state_Parse(__ev),
                "ModuleGates" => self._state_ModuleGates(__ev),
                "Graphviz" => self._state_Graphviz(__ev),
                "ValidateCodegen" => self._state_ValidateCodegen(__ev),
                "Assemble" => self._state_Assemble(__ev),
                "Done" => self._state_Done(__ev),
                _ => {}
            }
        }

        fn __transition(&mut self, next_compartment: PipelineFsmCompartment) {
            self.__next_compartment = Some(next_compartment);
        }

        pub fn run(&mut self) {
            let __e = alloc::rc::Rc::new(PipelineFsmFrameEvent::Run {});
            let mut __ctx = PipelineFsmFrameContext::new(alloc::rc::Rc::clone(&__e), None);
            self._context_stack.push(__ctx);
            self.__kernel(&__e);
            self._context_stack.pop();
        }

        fn _state_Idle(&mut self, __e: &PipelineFsmFrameEvent) {
            match __e {
                PipelineFsmFrameEvent::Run { .. } => { self._s_Idle_hdl_user_run(__e); }
                _ => {}
            }
        }

        fn _state_Segment(&mut self, __e: &PipelineFsmFrameEvent) {
            match __e {
                PipelineFsmFrameEvent::FrameEnter { .. } => { self._s_Segment_hdl_frame_enter(__e); }
                _ => {}
            }
        }

        fn _state_Parse(&mut self, __e: &PipelineFsmFrameEvent) {
            match __e {
                PipelineFsmFrameEvent::FrameEnter { .. } => { self._s_Parse_hdl_frame_enter(__e); }
                _ => {}
            }
        }

        fn _state_ModuleGates(&mut self, __e: &PipelineFsmFrameEvent) {
            match __e {
                PipelineFsmFrameEvent::FrameEnter { .. } => { self._s_ModuleGates_hdl_frame_enter(__e); }
                _ => {}
            }
        }

        fn _state_Graphviz(&mut self, __e: &PipelineFsmFrameEvent) {
            match __e {
                PipelineFsmFrameEvent::FrameEnter { .. } => { self._s_Graphviz_hdl_frame_enter(__e); }
                _ => {}
            }
        }

        fn _state_ValidateCodegen(&mut self, __e: &PipelineFsmFrameEvent) {
            match __e {
                PipelineFsmFrameEvent::FrameEnter { .. } => { self._s_ValidateCodegen_hdl_frame_enter(__e); }
                _ => {}
            }
        }

        fn _state_Assemble(&mut self, __e: &PipelineFsmFrameEvent) {
            match __e {
                PipelineFsmFrameEvent::FrameEnter { .. } => { self._s_Assemble_hdl_frame_enter(__e); }
                _ => {}
            }
        }

        fn _state_Done(&mut self, __e: &PipelineFsmFrameEvent) {
            match __e {
                _ => {}
            }
        }

        fn _s_Idle_hdl_user_run(&mut self, __e: &PipelineFsmFrameEvent) {
            let mut __compartment = self.__prepareEnter("Segment");
            self.__transition(__compartment);
            return;
        }

        fn _s_Segment_hdl_frame_enter(&mut self, __e: &PipelineFsmFrameEvent) {
            if let Some(r) = do_segment(&mut self.ctx) {
                self.early = Some(r);
                let mut __compartment = self.__prepareEnter("Done");
                self.__transition(__compartment);
                return;
            }
            let mut __compartment = self.__prepareEnter("Parse");
            self.__transition(__compartment);
            return;
        }

        fn _s_Parse_hdl_frame_enter(&mut self, __e: &PipelineFsmFrameEvent) {
            if let Some(r) = do_parse(&mut self.ctx) {
                self.early = Some(r);
                let mut __compartment = self.__prepareEnter("Done");
                self.__transition(__compartment);
                return;
            }
            let mut __compartment = self.__prepareEnter("ModuleGates");
            self.__transition(__compartment);
            return;
        }

        fn _s_ModuleGates_hdl_frame_enter(&mut self, __e: &PipelineFsmFrameEvent) {
            if let Some(r) = do_module_gates(&mut self.ctx) {
                self.early = Some(r);
                let mut __compartment = self.__prepareEnter("Done");
                self.__transition(__compartment);
                return;
            }
            let mut __compartment = self.__prepareEnter("Graphviz");
            self.__transition(__compartment);
            return;
        }

        fn _s_Graphviz_hdl_frame_enter(&mut self, __e: &PipelineFsmFrameEvent) {
            if let Some(r) = do_graphviz(&mut self.ctx) {
                self.early = Some(r);
                let mut __compartment = self.__prepareEnter("Done");
                self.__transition(__compartment);
                return;
            }
            let mut __compartment = self.__prepareEnter("ValidateCodegen");
            self.__transition(__compartment);
            return;
        }

        fn _s_ValidateCodegen_hdl_frame_enter(&mut self, __e: &PipelineFsmFrameEvent) {
            if let Some(r) = do_validate_codegen(&mut self.ctx) {
                self.early = Some(r);
                let mut __compartment = self.__prepareEnter("Done");
                self.__transition(__compartment);
                return;
            }
            let mut __compartment = self.__prepareEnter("Assemble");
            self.__transition(__compartment);
            return;
        }

        fn _s_Assemble_hdl_frame_enter(&mut self, __e: &PipelineFsmFrameEvent) {
            self.early = Some(do_assemble(&mut self.ctx));
            let mut __compartment = self.__prepareEnter("Done");
            self.__transition(__compartment);
            return;
        }
    }
}
pub use _pipeline_fsm_framec::*;
