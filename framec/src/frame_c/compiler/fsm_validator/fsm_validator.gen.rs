
// RFC-0042 §4, §9 — the @@fsm validator as a Frame @@system.
//
// One validation PASS per state (mirroring pipeline_supervisor.frs's
// phase-per-state shape). Each pass's `$>` runs a native check over the
// owned FsmDeclAst (`decl`) and accumulates FsmDiagnostics; the machine
// self-drives from $Start to $Done in one `validate()` call. The check
// bodies are native helpers (in the wrapper module) — Frame owns the
// pass sequencing, exactly as the dogfooding thesis intends.
//
//   $CheckHeader      — E713 (input-param alphabet type).
//   $CheckTransitions — E731 (undeclared transition-target state),
//                       E732 (undeclared stage in a stage-ref target).
//   $CheckRegex       — per-stage regex compilation (E720-E723, W704,
//                       anchor deferral) + E701 match exhaustiveness.
//   $Done             — terminal; `diagnostics` holds the findings.
//
// SCOPE (v1): the two checks above. More passes ($CheckStages for E730
// duplicate stage labels, $CheckTypes for E703/E706, $CheckExhaustive
// for E701, unused-var warnings) are added as states alongside these.
//
// The wrapper (fsm_validator::validate_fsm) builds the system, assigns
// the cloned `decl`, calls `validate()`, and lifts `diagnostics` out.

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
mod _fsm_validator_framec {
    use super::*;
    extern crate alloc;
    use alloc::{vec, format};
    #[derive(Clone, Debug)]
    #[allow(dead_code, non_camel_case_types)]
    enum FsmValidatorFrameEvent {
        Validate {  },
        FrameEnter {},
        FrameExit {},
    }

    #[derive(Clone)]
    #[allow(dead_code, non_camel_case_types)]
    enum FsmValidatorFrameReturn {
        _Lifecycle(alloc::rc::Rc<dyn core::any::Any>),
    }

    #[allow(dead_code)]
    impl FsmValidatorFrameEvent {
        fn name(&self) -> &'static str {
            match self {
                FsmValidatorFrameEvent::Validate { .. } => "validate",
                FsmValidatorFrameEvent::FrameEnter { .. } => "$>",
                FsmValidatorFrameEvent::FrameExit { .. } => "<$",
            }
        }
    }

    #[derive(Clone, Debug)]
    #[allow(dead_code, non_camel_case_types)]
    enum FsmValidatorFrameValue {
        Int(i64),
        Float(f64),
        Bool(bool),
        Str(String),
        List(Vec<Self>),
        Dict(alloc::collections::BTreeMap<String, Self>),
    }

    #[allow(dead_code, non_camel_case_types)]
    struct FsmValidatorFrameContext {
        event: alloc::rc::Rc<FsmValidatorFrameEvent>,
        _return: Option<FsmValidatorFrameReturn>,
        _data: alloc::collections::BTreeMap<String, FsmValidatorFrameValue>,
        _transitioned: bool,
    }

    impl FsmValidatorFrameContext {
        fn new(event: alloc::rc::Rc<FsmValidatorFrameEvent>, default_return: Option<FsmValidatorFrameReturn>) -> Self {
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
    enum FsmValidatorStateContext {
        Start,
        CheckHeader,
        CheckStructure,
        CheckTransitions,
        CheckNames,
        CheckWarnings,
        CheckRegex,
        Done,
        __NoContext,
    }

    impl Default for FsmValidatorStateContext {
        fn default() -> Self {
            FsmValidatorStateContext::Start
        }
    }

    #[allow(dead_code, non_camel_case_types)]
    #[derive(Clone)]
    struct FsmValidatorCompartment {
        state: String,
        state_context: FsmValidatorStateContext,
        forward_event: Option<FsmValidatorFrameEvent>,
        parent_compartment: Option<Box<FsmValidatorCompartment>>,
    }

    impl FsmValidatorCompartment {
        fn new(state: &str) -> Self {
            let state_context = match state {
                "Start" => FsmValidatorStateContext::Start,
                "CheckHeader" => FsmValidatorStateContext::CheckHeader,
                "CheckStructure" => FsmValidatorStateContext::CheckStructure,
                "CheckTransitions" => FsmValidatorStateContext::CheckTransitions,
                "CheckNames" => FsmValidatorStateContext::CheckNames,
                "CheckWarnings" => FsmValidatorStateContext::CheckWarnings,
                "CheckRegex" => FsmValidatorStateContext::CheckRegex,
                "Done" => FsmValidatorStateContext::Done,
                _ => FsmValidatorStateContext::__NoContext,
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
    pub struct FsmValidator {
        _state_stack: Vec<FsmValidatorCompartment>,
        __compartment: FsmValidatorCompartment,
        __next_compartment: Option<FsmValidatorCompartment>,
        _context_stack: Vec<FsmValidatorFrameContext>,
        pub decl: FsmDeclAst,
        pub diagnostics: Vec<FsmDiagnostic>,
    }

    #[allow(non_snake_case)]
    impl FsmValidator {
        pub fn new() -> Self {
            Self {
                _state_stack: Vec::new(),
                _context_stack: Vec::new(),
                decl: FsmDeclAst::empty(),
                diagnostics: Vec::new(),
                __compartment: FsmValidatorCompartment::new("Start"),
                __next_compartment: None,
            }
        }

        pub fn __create() -> Self {
            let mut c = Self::new();
            c.__compartment = c.__prepareEnter("Start");
            let __e = alloc::rc::Rc::new(FsmValidatorFrameEvent::FrameEnter {});
            let __ctx = FsmValidatorFrameContext::new(alloc::rc::Rc::clone(&__e), None);
            c._context_stack.push(__ctx);
            c.__kernel(&__e);
            c._context_stack.pop();
            c
        }

        fn __hsm_chain(&mut self, leaf: &str) -> &'static [&'static str] {
            match leaf {
                "Start" => &["Start"],
                "CheckHeader" => &["CheckHeader"],
                "CheckStructure" => &["CheckStructure"],
                "CheckTransitions" => &["CheckTransitions"],
                "CheckNames" => &["CheckNames"],
                "CheckWarnings" => &["CheckWarnings"],
                "CheckRegex" => &["CheckRegex"],
                "Done" => &["Done"],
                _ => &[],
            }
        }

        fn __prepareEnter(&mut self, leaf: &str) -> FsmValidatorCompartment {
            let chain = self.__hsm_chain(leaf);
            let mut comp: Option<FsmValidatorCompartment> = None;
            for name in chain.iter() {
                let mut new_comp = FsmValidatorCompartment::new(name);
                if let Some(parent) = comp.take() {
                    new_comp.parent_compartment = Some(Box::new(parent));
                }
                comp = Some(new_comp);
            }
            comp.expect("chain must contain at least the leaf state")
        }

        fn __kernel(&mut self, __e: &alloc::rc::Rc<FsmValidatorFrameEvent>) {
            // Route event to current state.
            self.__router(__e);
            // Drain any transitions queued by the handler.
            while self.__next_compartment.is_some() {
                let next_compartment = self.__next_compartment.take().expect("invariant: while-loop guard checked is_some()");
                // Exit the current (leaf) state. RFC-0025.1: exit args live in the
                // source state's typed ctx (written at the transition site), so the
                // synthesized `<$` event carries no payload.
                let exit_event = alloc::rc::Rc::new(FsmValidatorFrameEvent::FrameExit {});
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
                        let enter_event = alloc::rc::Rc::new(FsmValidatorFrameEvent::FrameEnter {});
                        self.__router(&enter_event);
                    }
                    Some(fwd) if matches!(fwd, FsmValidatorFrameEvent::FrameEnter { .. }) => {
                        // Forwarded event IS $> — dispatch directly so the
                        // destination's $> handler receives the caller's payload.
                        let fwd_rc = alloc::rc::Rc::new(fwd);
                        self.__router(&fwd_rc);
                    }
                    Some(fwd) => {
                        // Forwarded event is not $> — initialize the destination
                        // with a fresh $>, then dispatch the forward.
                        let enter_event = alloc::rc::Rc::new(FsmValidatorFrameEvent::FrameEnter {});
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

        fn __router(&mut self, __e: &alloc::rc::Rc<FsmValidatorFrameEvent>) {
            let __ev: &FsmValidatorFrameEvent = __e;
            match self.__compartment.state.as_str() {
                "Start" => self._state_Start(__ev),
                "CheckHeader" => self._state_CheckHeader(__ev),
                "CheckStructure" => self._state_CheckStructure(__ev),
                "CheckTransitions" => self._state_CheckTransitions(__ev),
                "CheckNames" => self._state_CheckNames(__ev),
                "CheckWarnings" => self._state_CheckWarnings(__ev),
                "CheckRegex" => self._state_CheckRegex(__ev),
                "Done" => self._state_Done(__ev),
                _ => {}
            }
        }

        fn __transition(&mut self, next_compartment: FsmValidatorCompartment) {
            self.__next_compartment = Some(next_compartment);
        }

        pub fn validate(&mut self) {
            let __e = alloc::rc::Rc::new(FsmValidatorFrameEvent::Validate {});
            let mut __ctx = FsmValidatorFrameContext::new(alloc::rc::Rc::clone(&__e), None);
            self._context_stack.push(__ctx);
            self.__kernel(&__e);
            self._context_stack.pop();
        }

        fn _state_Start(&mut self, __e: &FsmValidatorFrameEvent) {
            match __e {
                FsmValidatorFrameEvent::Validate { .. } => { self._s_Start_hdl_user_validate(__e); }
                _ => {}
            }
        }

        fn _state_CheckHeader(&mut self, __e: &FsmValidatorFrameEvent) {
            match __e {
                FsmValidatorFrameEvent::FrameEnter { .. } => { self._s_CheckHeader_hdl_frame_enter(__e); }
                _ => {}
            }
        }

        fn _state_CheckStructure(&mut self, __e: &FsmValidatorFrameEvent) {
            match __e {
                FsmValidatorFrameEvent::FrameEnter { .. } => { self._s_CheckStructure_hdl_frame_enter(__e); }
                _ => {}
            }
        }

        fn _state_CheckTransitions(&mut self, __e: &FsmValidatorFrameEvent) {
            match __e {
                FsmValidatorFrameEvent::FrameEnter { .. } => { self._s_CheckTransitions_hdl_frame_enter(__e); }
                _ => {}
            }
        }

        fn _state_CheckNames(&mut self, __e: &FsmValidatorFrameEvent) {
            match __e {
                FsmValidatorFrameEvent::FrameEnter { .. } => { self._s_CheckNames_hdl_frame_enter(__e); }
                _ => {}
            }
        }

        fn _state_CheckWarnings(&mut self, __e: &FsmValidatorFrameEvent) {
            match __e {
                FsmValidatorFrameEvent::FrameEnter { .. } => { self._s_CheckWarnings_hdl_frame_enter(__e); }
                _ => {}
            }
        }

        fn _state_CheckRegex(&mut self, __e: &FsmValidatorFrameEvent) {
            match __e {
                FsmValidatorFrameEvent::FrameEnter { .. } => { self._s_CheckRegex_hdl_frame_enter(__e); }
                _ => {}
            }
        }

        fn _state_Done(&mut self, __e: &FsmValidatorFrameEvent) {
            match __e {
                _ => {}
            }
        }

        fn _s_Start_hdl_user_validate(&mut self, __e: &FsmValidatorFrameEvent) {
            let mut __compartment = self.__prepareEnter("CheckHeader");
            self.__transition(__compartment);
            return;
        }

        fn _s_CheckHeader_hdl_frame_enter(&mut self, __e: &FsmValidatorFrameEvent) {
            if let Some(d) = check_input_param_type(&self.decl) {
                self.diagnostics.push(d);
            }
            let mut __compartment = self.__prepareEnter("CheckStructure");
            self.__transition(__compartment);
            return;
        }

        fn _s_CheckStructure_hdl_frame_enter(&mut self, __e: &FsmValidatorFrameEvent) {
            let mut found = check_structure(&self.decl);
            self.diagnostics.append(&mut found);
            let mut __compartment = self.__prepareEnter("CheckTransitions");
            self.__transition(__compartment);
            return;
        }

        fn _s_CheckTransitions_hdl_frame_enter(&mut self, __e: &FsmValidatorFrameEvent) {
            let mut found = check_transition_targets(&self.decl);
            self.diagnostics.append(&mut found);
            let mut __compartment = self.__prepareEnter("CheckNames");
            self.__transition(__compartment);
            return;
        }

        fn _s_CheckNames_hdl_frame_enter(&mut self, __e: &FsmValidatorFrameEvent) {
            let mut found = check_undeclared_reads(&self.decl);
            self.diagnostics.append(&mut found);
            let mut __compartment = self.__prepareEnter("CheckWarnings");
            self.__transition(__compartment);
            return;
        }

        fn _s_CheckWarnings_hdl_frame_enter(&mut self, __e: &FsmValidatorFrameEvent) {
            let mut found = check_warnings(&self.decl);
            self.diagnostics.append(&mut found);
            let mut __compartment = self.__prepareEnter("CheckRegex");
            self.__transition(__compartment);
            return;
        }

        fn _s_CheckRegex_hdl_frame_enter(&mut self, __e: &FsmValidatorFrameEvent) {
            let mut found = check_regexes(&self.decl);
            self.diagnostics.append(&mut found);
            let mut __compartment = self.__prepareEnter("Done");
            self.__transition(__compartment);
            return;
        }
    }
}
pub use _fsm_validator_framec::*;
