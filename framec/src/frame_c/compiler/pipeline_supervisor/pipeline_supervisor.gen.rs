
// Pipeline supervisor — the framec compile pipeline expressed as
// a Frame state machine.
//
// RFC-0035 round 7. Earlier rounds dogfooded Frame on framec's
// utilities, classifiers, validators, and graph algorithms.
// Round 7 puts Frame at the META level: it describes the
// compiler's own pipeline as a state machine.
//
// This FSM is OBSERVATIONAL — it does not drive the
// orchestrator's control flow. The orchestrator in
// `pipeline/compiler.rs` stays in native Rust and calls into
// this FSM at each phase boundary. Wrong supervisor logic
// produces wrong `--debug` output but cannot change compilation
// correctness. That bounded-risk profile is what makes Round 7
// tractable: the pipeline orchestrator is 1500 lines of
// intertwined linear logic with many bookkeeping side effects;
// rewriting it as an FSM-driven controller would be a multi-day
// arc with high regression risk. The supervisor-as-observer
// shape gives us the dogfood demo (Frame describing the meta-
// compiler) without rewriting the pipeline.
//
// Bonus deliverable: `framec compile -l graphviz
// pipeline_supervisor.frs > docs/pipeline.svg` renders the
// compiler's own pipeline diagram. The .frs source IS the
// pipeline spec; the SVG is the documentation. Self-describing
// meta-compiler.
//
// States:
//
//   $Idle    — initial; no phase has begun
//   $Running — actively executing a phase; non-fatal errors
//              may be collected and the pipeline continues
//   $Aborted — fatal error (e.g. segmentation failure); the
//              pipeline cannot continue. Terminal.
//   $Failed  — pipeline ran to completion but non-fatal errors
//              were collected (per-system parse failures,
//              validator E-codes that don't prevent codegen).
//              Terminal.
//   $Done    — clean exit, zero errors. Terminal.
//
// Error severity distinction:
//
//   abort(code, msg)        — fatal; transitions $Running → $Aborted
//   record_nonfatal(code, msg) — collected; stays in $Running
//   finish()                — completes; → $Done if no errors,
//                             → $Failed if record_nonfatal was called
//
// Phase tracking: each begin_phase(name) records the phase name
// in the `phase_log` domain field (CSV-encoded). The summary()
// event returns the log + error/warning counts as a tagged
// string the caller can parse for debug output.

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
mod _pipeline_supervisor_framec {
    use super::*;
    #[derive(Clone, Debug)]
    #[allow(dead_code, non_camel_case_types)]
    enum PipelineSupervisorFrameEvent {
        BeginPhase { name: String },
        CompletePhase {  },
        RecordNonfatal { code: String, msg: String },
        Abort { code: String, msg: String },
        Finish {  },
        Summary {  },
        FrameEnter { args: Vec<String> },
        FrameExit { args: Vec<String> },
    }

    #[derive(Clone)]
    #[allow(dead_code, non_camel_case_types)]
    enum PipelineSupervisorFrameReturn {
        Abort(String),
        BeginPhase(String),
        CompletePhase(String),
        Finish(String),
        RecordNonfatal(String),
        Summary(String),
        _Lifecycle(std::rc::Rc<dyn std::any::Any>),
    }

    #[allow(dead_code)]
    impl PipelineSupervisorFrameEvent {
        fn name(&self) -> &'static str {
            match self {
                PipelineSupervisorFrameEvent::BeginPhase { .. } => "begin_phase",
                PipelineSupervisorFrameEvent::CompletePhase { .. } => "complete_phase",
                PipelineSupervisorFrameEvent::RecordNonfatal { .. } => "record_nonfatal",
                PipelineSupervisorFrameEvent::Abort { .. } => "abort",
                PipelineSupervisorFrameEvent::Finish { .. } => "finish",
                PipelineSupervisorFrameEvent::Summary { .. } => "summary",
                PipelineSupervisorFrameEvent::FrameEnter { .. } => "$>",
                PipelineSupervisorFrameEvent::FrameExit { .. } => "<$",
            }
        }
    }

    #[derive(Clone, Debug)]
    #[allow(dead_code, non_camel_case_types)]
    enum PipelineSupervisorFrameValue {
        Int(i64),
        Float(f64),
        Bool(bool),
        Str(String),
        List(Vec<Self>),
        Dict(std::collections::HashMap<String, Self>),
    }

    #[allow(dead_code, non_camel_case_types)]
    struct PipelineSupervisorFrameContext {
        event: std::rc::Rc<PipelineSupervisorFrameEvent>,
        _return: Option<PipelineSupervisorFrameReturn>,
        _data: std::collections::HashMap<String, PipelineSupervisorFrameValue>,
        _transitioned: bool,
    }

    impl PipelineSupervisorFrameContext {
        fn new(event: std::rc::Rc<PipelineSupervisorFrameEvent>, default_return: Option<PipelineSupervisorFrameReturn>) -> Self {
            Self {
                event,
                _return: default_return,
                _data: std::collections::HashMap::new(),
                _transitioned: false,
            }
        }
    }

    #[allow(dead_code, non_camel_case_types)]
    #[derive(Clone)]
    enum PipelineSupervisorStateContext {
        Idle,
        Running,
        Aborted,
        Failed,
        Done,
        Empty,
    }

    impl Default for PipelineSupervisorStateContext {
        fn default() -> Self {
            PipelineSupervisorStateContext::Idle
        }
    }

    #[allow(dead_code, non_camel_case_types)]
    #[derive(Clone)]
    struct PipelineSupervisorCompartment {
        state: String,
        state_context: PipelineSupervisorStateContext,
        enter_args: Vec<String>,
        exit_args: Vec<String>,
        forward_event: Option<PipelineSupervisorFrameEvent>,
        parent_compartment: Option<Box<PipelineSupervisorCompartment>>,
    }

    impl PipelineSupervisorCompartment {
        fn new(state: &str) -> Self {
            let state_context = match state {
                "Idle" => PipelineSupervisorStateContext::Idle,
                "Running" => PipelineSupervisorStateContext::Running,
                "Aborted" => PipelineSupervisorStateContext::Aborted,
                "Failed" => PipelineSupervisorStateContext::Failed,
                "Done" => PipelineSupervisorStateContext::Done,
                _ => PipelineSupervisorStateContext::Empty,
            };
            Self {
                state: state.to_string(),
                state_context,
                enter_args: Vec::new(),
                exit_args: Vec::new(),
                forward_event: None,
                parent_compartment: None,
            }
        }
    }

    #[allow(dead_code)]
    pub struct PipelineSupervisor {
        _state_stack: Vec<PipelineSupervisorCompartment>,
        __compartment: PipelineSupervisorCompartment,
        __next_compartment: Option<PipelineSupervisorCompartment>,
        _context_stack: Vec<PipelineSupervisorFrameContext>,
        pub current_phase: String,
        pub phase_log: String,
        pub error_count: i32,
        pub abort_code: String,
        pub abort_msg: String,
    }

    #[allow(non_snake_case)]
    impl PipelineSupervisor {
        pub fn new() -> Self {
            Self {
                _state_stack: Vec::new(),
                _context_stack: Vec::new(),
                current_phase: String::new(),
                phase_log: String::new(),
                error_count: 0,
                abort_code: String::new(),
                abort_msg: String::new(),
                __compartment: PipelineSupervisorCompartment::new("Idle"),
                __next_compartment: None,
            }
        }

        pub fn __create() -> Self {
            let mut c = Self::new();
            c.__compartment = c.__prepareEnter("Idle", vec![]);
            let __e = std::rc::Rc::new(PipelineSupervisorFrameEvent::FrameEnter { args: c.__compartment.enter_args.clone() });
            let __ctx = PipelineSupervisorFrameContext::new(std::rc::Rc::clone(&__e), None);
            c._context_stack.push(__ctx);
            c.__kernel(&__e);
            c._context_stack.pop();
            c
        }

        fn __hsm_chain(&mut self, leaf: &str) -> &'static [&'static str] {
            match leaf {
                "Idle" => &["Idle"],
                "Running" => &["Running"],
                "Aborted" => &["Aborted"],
                "Failed" => &["Failed"],
                "Done" => &["Done"],
                _ => &[],
            }
        }

        fn __prepareEnter(&mut self, leaf: &str, enter_args: Vec<String>) -> PipelineSupervisorCompartment {
            let chain = self.__hsm_chain(leaf);
            let mut comp: Option<PipelineSupervisorCompartment> = None;
            for name in chain.iter() {
                let mut new_comp = PipelineSupervisorCompartment::new(name);
                new_comp.enter_args = enter_args.clone();
                if let Some(parent) = comp.take() {
                    new_comp.parent_compartment = Some(Box::new(parent));
                }
                comp = Some(new_comp);
            }
            comp.expect("chain must contain at least the leaf state")
        }

        fn __prepareExit(&mut self, exit_args: Vec<String>) {
            self.__compartment.exit_args = exit_args.clone();
            let mut cursor = self.__compartment.parent_compartment.as_deref_mut();
            while let Some(c) = cursor {
                c.exit_args = exit_args.clone();
                cursor = c.parent_compartment.as_deref_mut();
            }
        }

        fn __kernel(&mut self, __e: &std::rc::Rc<PipelineSupervisorFrameEvent>) {
            // Route event to current state.
            self.__router(__e);
            // Drain any transitions queued by the handler.
            while self.__next_compartment.is_some() {
                let next_compartment = self.__next_compartment.take().expect("invariant: while-loop guard checked is_some()");
                // Exit the current (leaf) state.
                let exit_args = self.__compartment.exit_args.clone();
                let exit_event = std::rc::Rc::new(PipelineSupervisorFrameEvent::FrameExit { args: exit_args });
                self.__router(&exit_event);
                // Switch to the new compartment.
                self.__compartment = next_compartment;
                // Three-branch forward-event handling (RFC-0025 Track B.1: forward
                // event is matched on enum variant; $> recognition is now a
                // structural match, not a string compare).
                match self.__compartment.forward_event.take() {
                    None => {
                        // No forwarded event — synthesize a fresh $>.
                        let enter_args = self.__compartment.enter_args.clone();
                        let enter_event = std::rc::Rc::new(PipelineSupervisorFrameEvent::FrameEnter { args: enter_args });
                        self.__router(&enter_event);
                    }
                    Some(fwd) if matches!(fwd, PipelineSupervisorFrameEvent::FrameEnter { .. }) => {
                        // Forwarded event IS $> — dispatch directly so the
                        // destination's $> handler receives the caller's payload.
                        let fwd_rc = std::rc::Rc::new(fwd);
                        self.__router(&fwd_rc);
                    }
                    Some(fwd) => {
                        // Forwarded event is not $> — initialize the destination
                        // with a fresh $>, then dispatch the forward.
                        let enter_args = self.__compartment.enter_args.clone();
                        let enter_event = std::rc::Rc::new(PipelineSupervisorFrameEvent::FrameEnter { args: enter_args });
                        self.__router(&enter_event);
                        let fwd_rc = std::rc::Rc::new(fwd);
                        self.__router(&fwd_rc);
                    }
                }
                for ctx in self._context_stack.iter_mut() {
                    ctx._transitioned = true;
                }
            }
        }

        fn __router(&mut self, __e: &std::rc::Rc<PipelineSupervisorFrameEvent>) {
            let __ev: &PipelineSupervisorFrameEvent = __e;
            match self.__compartment.state.as_str() {
                "Idle" => self._state_Idle(__ev),
                "Running" => self._state_Running(__ev),
                "Aborted" => self._state_Aborted(__ev),
                "Failed" => self._state_Failed(__ev),
                "Done" => self._state_Done(__ev),
                _ => {}
            }
        }

        fn __transition(&mut self, next_compartment: PipelineSupervisorCompartment) {
            self.__next_compartment = Some(next_compartment);
        }

        pub fn begin_phase(&mut self, name: String) -> String {
            let __e = std::rc::Rc::new(PipelineSupervisorFrameEvent::BeginPhase { name: name.clone() });
            let mut __ctx = PipelineSupervisorFrameContext::new(std::rc::Rc::clone(&__e), None);
            self._context_stack.push(__ctx);
            self.__kernel(&__e);
            let __ctx = self._context_stack.pop().expect("invariant: handler must have pushed a context before reading return");
            match __ctx._return {
                Some(PipelineSupervisorFrameReturn::BeginPhase(v)) => v,
                Some(PipelineSupervisorFrameReturn::_Lifecycle(v)) => v.downcast_ref::<String>().cloned().unwrap_or_default(),
                _ => Default::default(),
            }
        }

        pub fn complete_phase(&mut self) -> String {
            let __e = std::rc::Rc::new(PipelineSupervisorFrameEvent::CompletePhase {});
            let mut __ctx = PipelineSupervisorFrameContext::new(std::rc::Rc::clone(&__e), None);
            self._context_stack.push(__ctx);
            self.__kernel(&__e);
            let __ctx = self._context_stack.pop().expect("invariant: handler must have pushed a context before reading return");
            match __ctx._return {
                Some(PipelineSupervisorFrameReturn::CompletePhase(v)) => v,
                Some(PipelineSupervisorFrameReturn::_Lifecycle(v)) => v.downcast_ref::<String>().cloned().unwrap_or_default(),
                _ => Default::default(),
            }
        }

        pub fn record_nonfatal(&mut self, code: String, msg: String) -> String {
            let __e = std::rc::Rc::new(PipelineSupervisorFrameEvent::RecordNonfatal { code: code.clone(), msg: msg.clone() });
            let mut __ctx = PipelineSupervisorFrameContext::new(std::rc::Rc::clone(&__e), None);
            self._context_stack.push(__ctx);
            self.__kernel(&__e);
            let __ctx = self._context_stack.pop().expect("invariant: handler must have pushed a context before reading return");
            match __ctx._return {
                Some(PipelineSupervisorFrameReturn::RecordNonfatal(v)) => v,
                Some(PipelineSupervisorFrameReturn::_Lifecycle(v)) => v.downcast_ref::<String>().cloned().unwrap_or_default(),
                _ => Default::default(),
            }
        }

        pub fn abort(&mut self, code: String, msg: String) -> String {
            let __e = std::rc::Rc::new(PipelineSupervisorFrameEvent::Abort { code: code.clone(), msg: msg.clone() });
            let mut __ctx = PipelineSupervisorFrameContext::new(std::rc::Rc::clone(&__e), None);
            self._context_stack.push(__ctx);
            self.__kernel(&__e);
            let __ctx = self._context_stack.pop().expect("invariant: handler must have pushed a context before reading return");
            match __ctx._return {
                Some(PipelineSupervisorFrameReturn::Abort(v)) => v,
                Some(PipelineSupervisorFrameReturn::_Lifecycle(v)) => v.downcast_ref::<String>().cloned().unwrap_or_default(),
                _ => Default::default(),
            }
        }

        pub fn finish(&mut self) -> String {
            let __e = std::rc::Rc::new(PipelineSupervisorFrameEvent::Finish {});
            let mut __ctx = PipelineSupervisorFrameContext::new(std::rc::Rc::clone(&__e), None);
            self._context_stack.push(__ctx);
            self.__kernel(&__e);
            let __ctx = self._context_stack.pop().expect("invariant: handler must have pushed a context before reading return");
            match __ctx._return {
                Some(PipelineSupervisorFrameReturn::Finish(v)) => v,
                Some(PipelineSupervisorFrameReturn::_Lifecycle(v)) => v.downcast_ref::<String>().cloned().unwrap_or_default(),
                _ => Default::default(),
            }
        }

        pub fn summary(&mut self) -> String {
            let __e = std::rc::Rc::new(PipelineSupervisorFrameEvent::Summary {});
            let mut __ctx = PipelineSupervisorFrameContext::new(std::rc::Rc::clone(&__e), None);
            self._context_stack.push(__ctx);
            self.__kernel(&__e);
            let __ctx = self._context_stack.pop().expect("invariant: handler must have pushed a context before reading return");
            match __ctx._return {
                Some(PipelineSupervisorFrameReturn::Summary(v)) => v,
                Some(PipelineSupervisorFrameReturn::_Lifecycle(v)) => v.downcast_ref::<String>().cloned().unwrap_or_default(),
                _ => Default::default(),
            }
        }

        fn _state_Idle(&mut self, __e: &PipelineSupervisorFrameEvent) {
            match __e {
                PipelineSupervisorFrameEvent::BeginPhase { name, .. } => {
                    self._s_Idle_hdl_user_begin_phase(__e, name.clone());
                }
                PipelineSupervisorFrameEvent::Summary { .. } => { self._s_Idle_hdl_user_summary(__e); }
                _ => {}
            }
        }

        fn _state_Running(&mut self, __e: &PipelineSupervisorFrameEvent) {
            match __e {
                PipelineSupervisorFrameEvent::Abort { code, msg, .. } => {
                    self._s_Running_hdl_user_abort(__e, code.clone(), msg.clone());
                }
                PipelineSupervisorFrameEvent::BeginPhase { name, .. } => {
                    self._s_Running_hdl_user_begin_phase(__e, name.clone());
                }
                PipelineSupervisorFrameEvent::CompletePhase { .. } => { self._s_Running_hdl_user_complete_phase(__e); }
                PipelineSupervisorFrameEvent::Finish { .. } => { self._s_Running_hdl_user_finish(__e); }
                PipelineSupervisorFrameEvent::RecordNonfatal { code, msg, .. } => {
                    self._s_Running_hdl_user_record_nonfatal(__e, code.clone(), msg.clone());
                }
                PipelineSupervisorFrameEvent::Summary { .. } => { self._s_Running_hdl_user_summary(__e); }
                _ => {}
            }
        }

        fn _state_Aborted(&mut self, __e: &PipelineSupervisorFrameEvent) {
            match __e {
                PipelineSupervisorFrameEvent::Abort { code, msg, .. } => {
                    self._s_Aborted_hdl_user_abort(__e, code.clone(), msg.clone());
                }
                PipelineSupervisorFrameEvent::BeginPhase { name, .. } => {
                    self._s_Aborted_hdl_user_begin_phase(__e, name.clone());
                }
                PipelineSupervisorFrameEvent::CompletePhase { .. } => { self._s_Aborted_hdl_user_complete_phase(__e); }
                PipelineSupervisorFrameEvent::Finish { .. } => { self._s_Aborted_hdl_user_finish(__e); }
                PipelineSupervisorFrameEvent::RecordNonfatal { code, msg, .. } => {
                    self._s_Aborted_hdl_user_record_nonfatal(__e, code.clone(), msg.clone());
                }
                PipelineSupervisorFrameEvent::Summary { .. } => { self._s_Aborted_hdl_user_summary(__e); }
                _ => {}
            }
        }

        fn _state_Failed(&mut self, __e: &PipelineSupervisorFrameEvent) {
            match __e {
                PipelineSupervisorFrameEvent::Abort { code, msg, .. } => {
                    self._s_Failed_hdl_user_abort(__e, code.clone(), msg.clone());
                }
                PipelineSupervisorFrameEvent::BeginPhase { name, .. } => {
                    self._s_Failed_hdl_user_begin_phase(__e, name.clone());
                }
                PipelineSupervisorFrameEvent::CompletePhase { .. } => { self._s_Failed_hdl_user_complete_phase(__e); }
                PipelineSupervisorFrameEvent::Finish { .. } => { self._s_Failed_hdl_user_finish(__e); }
                PipelineSupervisorFrameEvent::RecordNonfatal { code, msg, .. } => {
                    self._s_Failed_hdl_user_record_nonfatal(__e, code.clone(), msg.clone());
                }
                PipelineSupervisorFrameEvent::Summary { .. } => { self._s_Failed_hdl_user_summary(__e); }
                _ => {}
            }
        }

        fn _state_Done(&mut self, __e: &PipelineSupervisorFrameEvent) {
            match __e {
                PipelineSupervisorFrameEvent::Abort { code, msg, .. } => {
                    self._s_Done_hdl_user_abort(__e, code.clone(), msg.clone());
                }
                PipelineSupervisorFrameEvent::BeginPhase { name, .. } => {
                    self._s_Done_hdl_user_begin_phase(__e, name.clone());
                }
                PipelineSupervisorFrameEvent::CompletePhase { .. } => { self._s_Done_hdl_user_complete_phase(__e); }
                PipelineSupervisorFrameEvent::Finish { .. } => { self._s_Done_hdl_user_finish(__e); }
                PipelineSupervisorFrameEvent::RecordNonfatal { code, msg, .. } => {
                    self._s_Done_hdl_user_record_nonfatal(__e, code.clone(), msg.clone());
                }
                PipelineSupervisorFrameEvent::Summary { .. } => { self._s_Done_hdl_user_summary(__e); }
                _ => {}
            }
        }

        fn _s_Idle_hdl_user_begin_phase(&mut self, __e: &PipelineSupervisorFrameEvent, name: String) {
                            self.current_phase = name.clone();
                            let mut __compartment = self.__prepareEnter("Running", vec![]);
                            self.__transition(__compartment);
            let __return_val = PipelineSupervisorFrameReturn::BeginPhase(format!("BEGIN|{}", name));
                            if let Some(ctx) = self._context_stack.last_mut() { ctx._return = Some(__return_val); }
                            return;
        }

        fn _s_Idle_hdl_user_summary(&mut self, __e: &PipelineSupervisorFrameEvent) {
            let __return_val = PipelineSupervisorFrameReturn::Summary("IDLE|phases=|errors=0|warnings=0".to_string());
                            if let Some(ctx) = self._context_stack.last_mut() { ctx._return = Some(__return_val); }
        }

        fn _s_Running_hdl_user_abort(&mut self, __e: &PipelineSupervisorFrameEvent, code: String, msg: String) {
                            self.abort_code = code.clone();
                            self.abort_msg = msg.clone();
                            let mut __compartment = self.__prepareEnter("Aborted", vec![]);
                            self.__transition(__compartment);
            let __return_val = PipelineSupervisorFrameReturn::Abort(format!("ABORT|{}|{}", code, msg));
                            if let Some(ctx) = self._context_stack.last_mut() { ctx._return = Some(__return_val); }
                            return;
        }

        fn _s_Running_hdl_user_begin_phase(&mut self, __e: &PipelineSupervisorFrameEvent, name: String) {
                            // Closing the prior phase implicitly when caller
                            // begins a new one without calling complete_phase()
                            // first — defensive shape, matches how the existing
                            // orchestrator flows from segment → parse → ...
                            // without explicit "done" markers. If complete_phase()
                            // already ran, current_phase is empty and we skip the
                            // implicit close.
                            if !self.current_phase.is_empty() {
                                if !self.phase_log.is_empty() {
                                    self.phase_log.push(',');
                                }
                                self.phase_log.push_str(&self.current_phase);
                            }
                            self.current_phase = name.clone();
            let __return_val = PipelineSupervisorFrameReturn::BeginPhase(format!("BEGIN|{}", name));
                            if let Some(ctx) = self._context_stack.last_mut() { ctx._return = Some(__return_val); }
        }

        fn _s_Running_hdl_user_complete_phase(&mut self, __e: &PipelineSupervisorFrameEvent) {
                            if !self.phase_log.is_empty() {
                                self.phase_log.push(',');
                            }
                            self.phase_log.push_str(&self.current_phase);
                            let done_phase = self.current_phase.clone();
                            self.current_phase = String::new();
            let __return_val = PipelineSupervisorFrameReturn::CompletePhase(format!("COMPLETE|{}", done_phase));
                            if let Some(ctx) = self._context_stack.last_mut() { ctx._return = Some(__return_val); }
        }

        fn _s_Running_hdl_user_finish(&mut self, __e: &PipelineSupervisorFrameEvent) {
                            if !self.current_phase.is_empty() {
                                if !self.phase_log.is_empty() {
                                    self.phase_log.push(',');
                                }
                                self.phase_log.push_str(&self.current_phase);
                                self.current_phase = String::new();
                            }
                            if self.error_count > 0 {
                                let mut __compartment = self.__prepareEnter("Failed", vec![]);
                                self.__transition(__compartment);
            let __return_val = PipelineSupervisorFrameReturn::Finish(format!("FAILED|errors={}", self.error_count));
                                if let Some(ctx) = self._context_stack.last_mut() { ctx._return = Some(__return_val); }
                                return;
            
                            }
                            let mut __compartment = self.__prepareEnter("Done", vec![]);
                            self.__transition(__compartment);
            let __return_val = PipelineSupervisorFrameReturn::Finish("DONE".to_string());
                            if let Some(ctx) = self._context_stack.last_mut() { ctx._return = Some(__return_val); }
                            return;
        }

        fn _s_Running_hdl_user_record_nonfatal(&mut self, __e: &PipelineSupervisorFrameEvent, code: String, msg: String) {
                            self.error_count += 1;
            let __return_val = PipelineSupervisorFrameReturn::RecordNonfatal(format!("NONFATAL|{}|{}", code, msg));
                            if let Some(ctx) = self._context_stack.last_mut() { ctx._return = Some(__return_val); }
        }

        fn _s_Running_hdl_user_summary(&mut self, __e: &PipelineSupervisorFrameEvent) {
            let __return_val = PipelineSupervisorFrameReturn::Summary((format!(
                                "RUNNING|phases={}|current={}|errors={}|warnings=0",
                                self.phase_log, self.current_phase, self.error_count
                            )));
                            if let Some(ctx) = self._context_stack.last_mut() { ctx._return = Some(__return_val); }
        }

        fn _s_Aborted_hdl_user_abort(&mut self, __e: &PipelineSupervisorFrameEvent, code: String, msg: String) {
            let __return_val = PipelineSupervisorFrameReturn::Abort("ABSORBED".to_string());
                            if let Some(ctx) = self._context_stack.last_mut() { ctx._return = Some(__return_val); }
        }

        fn _s_Aborted_hdl_user_begin_phase(&mut self, __e: &PipelineSupervisorFrameEvent, name: String) {
            let __return_val = PipelineSupervisorFrameReturn::BeginPhase("ABSORBED".to_string());
                            if let Some(ctx) = self._context_stack.last_mut() { ctx._return = Some(__return_val); }
        }

        fn _s_Aborted_hdl_user_complete_phase(&mut self, __e: &PipelineSupervisorFrameEvent) {
            let __return_val = PipelineSupervisorFrameReturn::CompletePhase("ABSORBED".to_string());
                            if let Some(ctx) = self._context_stack.last_mut() { ctx._return = Some(__return_val); }
        }

        fn _s_Aborted_hdl_user_finish(&mut self, __e: &PipelineSupervisorFrameEvent) {
            let __return_val = PipelineSupervisorFrameReturn::Finish(format!("ABORTED|{}|{}", self.abort_code, self.abort_msg));
                            if let Some(ctx) = self._context_stack.last_mut() { ctx._return = Some(__return_val); }
        }

        fn _s_Aborted_hdl_user_record_nonfatal(&mut self, __e: &PipelineSupervisorFrameEvent, code: String, msg: String) {
            let __return_val = PipelineSupervisorFrameReturn::RecordNonfatal("ABSORBED".to_string());
                            if let Some(ctx) = self._context_stack.last_mut() { ctx._return = Some(__return_val); }
        }

        fn _s_Aborted_hdl_user_summary(&mut self, __e: &PipelineSupervisorFrameEvent) {
            let __return_val = PipelineSupervisorFrameReturn::Summary((format!(
                                "ABORTED|phases={}|code={}|msg={}|errors={}",
                                self.phase_log, self.abort_code, self.abort_msg, self.error_count
                            )));
                            if let Some(ctx) = self._context_stack.last_mut() { ctx._return = Some(__return_val); }
        }

        fn _s_Failed_hdl_user_abort(&mut self, __e: &PipelineSupervisorFrameEvent, code: String, msg: String) {
            let __return_val = PipelineSupervisorFrameReturn::Abort("ABSORBED".to_string());
                            if let Some(ctx) = self._context_stack.last_mut() { ctx._return = Some(__return_val); }
        }

        fn _s_Failed_hdl_user_begin_phase(&mut self, __e: &PipelineSupervisorFrameEvent, name: String) {
            let __return_val = PipelineSupervisorFrameReturn::BeginPhase("ABSORBED".to_string());
                            if let Some(ctx) = self._context_stack.last_mut() { ctx._return = Some(__return_val); }
        }

        fn _s_Failed_hdl_user_complete_phase(&mut self, __e: &PipelineSupervisorFrameEvent) {
            let __return_val = PipelineSupervisorFrameReturn::CompletePhase("ABSORBED".to_string());
                            if let Some(ctx) = self._context_stack.last_mut() { ctx._return = Some(__return_val); }
        }

        fn _s_Failed_hdl_user_finish(&mut self, __e: &PipelineSupervisorFrameEvent) {
            let __return_val = PipelineSupervisorFrameReturn::Finish(format!("FAILED|errors={}", self.error_count));
                            if let Some(ctx) = self._context_stack.last_mut() { ctx._return = Some(__return_val); }
        }

        fn _s_Failed_hdl_user_record_nonfatal(&mut self, __e: &PipelineSupervisorFrameEvent, code: String, msg: String) {
            let __return_val = PipelineSupervisorFrameReturn::RecordNonfatal("ABSORBED".to_string());
                            if let Some(ctx) = self._context_stack.last_mut() { ctx._return = Some(__return_val); }
        }

        fn _s_Failed_hdl_user_summary(&mut self, __e: &PipelineSupervisorFrameEvent) {
            let __return_val = PipelineSupervisorFrameReturn::Summary((format!(
                                "FAILED|phases={}|errors={}|warnings=0",
                                self.phase_log, self.error_count
                            )));
                            if let Some(ctx) = self._context_stack.last_mut() { ctx._return = Some(__return_val); }
        }

        fn _s_Done_hdl_user_abort(&mut self, __e: &PipelineSupervisorFrameEvent, code: String, msg: String) {
            let __return_val = PipelineSupervisorFrameReturn::Abort("ABSORBED".to_string());
                            if let Some(ctx) = self._context_stack.last_mut() { ctx._return = Some(__return_val); }
        }

        fn _s_Done_hdl_user_begin_phase(&mut self, __e: &PipelineSupervisorFrameEvent, name: String) {
            let __return_val = PipelineSupervisorFrameReturn::BeginPhase("ABSORBED".to_string());
                            if let Some(ctx) = self._context_stack.last_mut() { ctx._return = Some(__return_val); }
        }

        fn _s_Done_hdl_user_complete_phase(&mut self, __e: &PipelineSupervisorFrameEvent) {
            let __return_val = PipelineSupervisorFrameReturn::CompletePhase("ABSORBED".to_string());
                            if let Some(ctx) = self._context_stack.last_mut() { ctx._return = Some(__return_val); }
        }

        fn _s_Done_hdl_user_finish(&mut self, __e: &PipelineSupervisorFrameEvent) {
            let __return_val = PipelineSupervisorFrameReturn::Finish("DONE".to_string());
                            if let Some(ctx) = self._context_stack.last_mut() { ctx._return = Some(__return_val); }
        }

        fn _s_Done_hdl_user_record_nonfatal(&mut self, __e: &PipelineSupervisorFrameEvent, code: String, msg: String) {
            let __return_val = PipelineSupervisorFrameReturn::RecordNonfatal("ABSORBED".to_string());
                            if let Some(ctx) = self._context_stack.last_mut() { ctx._return = Some(__return_val); }
        }

        fn _s_Done_hdl_user_summary(&mut self, __e: &PipelineSupervisorFrameEvent) {
            let __return_val = PipelineSupervisorFrameReturn::Summary(format!("DONE|phases={}|errors=0|warnings=0", self.phase_log));
                            if let Some(ctx) = self._context_stack.last_mut() { ctx._return = Some(__return_val); }
        }
    }
}
pub use _pipeline_supervisor_framec::*;

