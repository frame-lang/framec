
// E413 HSM parent-chain cycle detector, expressed as a
// genuinely multi-state Frame system.
//
// RFC-0035 round 5. The previous rounds documented where Frame
// fits well (state-tracking validators — round 4) and where it
// doesn't (rich return types, recursion, line classifiers —
// round 3). Round 5 puts Frame on a *graph algorithm* — the
// HSM parent-chain walk.
//
// Each call site creates a fresh FSM instance and walks a
// single state's parent chain:
//
//   $Initial   — no walk started; first step seeds `start_name`
//                and `visited`, then transitions to $Walking.
//   $Walking   — chain walk in progress. Each step receives the
//                next parent; either records it in `visited`
//                and stays in $Walking, or detects a revisit
//                and transitions to $CycleFound, or sees an
//                empty parent and transitions to $ChainRoot.
//   $CycleFound — terminal error state; further steps absorb.
//   $ChainRoot  — terminal success state; further steps absorb.
//
// This is the canonical "graph walk as state machine" pattern:
// each state in the FSM corresponds to a phase of the
// algorithm, and the visited set is a domain field that evolves
// over event calls (not a state-arg — Frame's state-args are
// per-state, but visited threads through every state of the
// walk).
//
// The caller orchestrates: pop the start state, feed its parent
// to step(), check the result, look up the next parent, feed
// that to step(), etc. The FSM tracks accumulated state. The
// glue Rust function pushes a ValidationError when step()
// returns "CYCLE|<at>".

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
mod _hsm_cycle_walker_framec {
    use super::*;
    extern crate alloc;
    use alloc::{vec, format};
    #[derive(Clone, Debug)]
    #[allow(dead_code, non_camel_case_types)]
    enum HsmCycleWalkerFrameEvent {
        Step { parent: String },
        FrameEnter {},
        FrameExit {},
    }

    #[derive(Clone)]
    #[allow(dead_code, non_camel_case_types)]
    enum HsmCycleWalkerFrameReturn {
        Step(String),
        _Lifecycle(alloc::rc::Rc<dyn core::any::Any>),
    }

    #[allow(dead_code)]
    impl HsmCycleWalkerFrameEvent {
        fn name(&self) -> &'static str {
            match self {
                HsmCycleWalkerFrameEvent::Step { .. } => "step",
                HsmCycleWalkerFrameEvent::FrameEnter { .. } => "$>",
                HsmCycleWalkerFrameEvent::FrameExit { .. } => "<$",
            }
        }
    }

    #[derive(Clone, Debug)]
    #[allow(dead_code, non_camel_case_types)]
    enum HsmCycleWalkerFrameValue {
        Int(i64),
        Float(f64),
        Bool(bool),
        Str(String),
        List(Vec<Self>),
        Dict(alloc::collections::BTreeMap<String, Self>),
    }

    #[allow(dead_code, non_camel_case_types)]
    struct HsmCycleWalkerFrameContext {
        event: alloc::rc::Rc<HsmCycleWalkerFrameEvent>,
        _return: Option<HsmCycleWalkerFrameReturn>,
        _data: alloc::collections::BTreeMap<String, HsmCycleWalkerFrameValue>,
        _transitioned: bool,
    }

    impl HsmCycleWalkerFrameContext {
        fn new(event: alloc::rc::Rc<HsmCycleWalkerFrameEvent>, default_return: Option<HsmCycleWalkerFrameReturn>) -> Self {
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
    enum HsmCycleWalkerStateContext {
        Initial,
        Walking,
        CycleFound,
        ChainRoot,
        Empty,
    }

    impl Default for HsmCycleWalkerStateContext {
        fn default() -> Self {
            HsmCycleWalkerStateContext::Initial
        }
    }

    #[allow(dead_code, non_camel_case_types)]
    #[derive(Clone)]
    struct HsmCycleWalkerCompartment {
        state: String,
        state_context: HsmCycleWalkerStateContext,
        forward_event: Option<HsmCycleWalkerFrameEvent>,
        parent_compartment: Option<Box<HsmCycleWalkerCompartment>>,
    }

    impl HsmCycleWalkerCompartment {
        fn new(state: &str) -> Self {
            let state_context = match state {
                "Initial" => HsmCycleWalkerStateContext::Initial,
                "Walking" => HsmCycleWalkerStateContext::Walking,
                "CycleFound" => HsmCycleWalkerStateContext::CycleFound,
                "ChainRoot" => HsmCycleWalkerStateContext::ChainRoot,
                _ => HsmCycleWalkerStateContext::Empty,
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
    pub struct HsmCycleWalker {
        _state_stack: Vec<HsmCycleWalkerCompartment>,
        __compartment: HsmCycleWalkerCompartment,
        __next_compartment: Option<HsmCycleWalkerCompartment>,
        _context_stack: Vec<HsmCycleWalkerFrameContext>,
        pub start_name: String,
        pub visited: String,
        pub cycle_at: String,
    }

    #[allow(non_snake_case)]
    impl HsmCycleWalker {
        pub fn new() -> Self {
            Self {
                _state_stack: Vec::new(),
                _context_stack: Vec::new(),
                start_name: String::new(),
                visited: String::new(),
                cycle_at: String::new(),
                __compartment: HsmCycleWalkerCompartment::new("Initial"),
                __next_compartment: None,
            }
        }

        pub fn __create() -> Self {
            let mut c = Self::new();
            c.__compartment = c.__prepareEnter("Initial");
            let __e = alloc::rc::Rc::new(HsmCycleWalkerFrameEvent::FrameEnter {});
            let __ctx = HsmCycleWalkerFrameContext::new(alloc::rc::Rc::clone(&__e), None);
            c._context_stack.push(__ctx);
            c.__kernel(&__e);
            c._context_stack.pop();
            c
        }

        fn __hsm_chain(&mut self, leaf: &str) -> &'static [&'static str] {
            match leaf {
                "Initial" => &["Initial"],
                "Walking" => &["Walking"],
                "CycleFound" => &["CycleFound"],
                "ChainRoot" => &["ChainRoot"],
                _ => &[],
            }
        }

        fn __prepareEnter(&mut self, leaf: &str) -> HsmCycleWalkerCompartment {
            let chain = self.__hsm_chain(leaf);
            let mut comp: Option<HsmCycleWalkerCompartment> = None;
            for name in chain.iter() {
                let mut new_comp = HsmCycleWalkerCompartment::new(name);
                if let Some(parent) = comp.take() {
                    new_comp.parent_compartment = Some(Box::new(parent));
                }
                comp = Some(new_comp);
            }
            comp.expect("chain must contain at least the leaf state")
        }

        fn __kernel(&mut self, __e: &alloc::rc::Rc<HsmCycleWalkerFrameEvent>) {
            // Route event to current state.
            self.__router(__e);
            // Drain any transitions queued by the handler.
            while self.__next_compartment.is_some() {
                let next_compartment = self.__next_compartment.take().expect("invariant: while-loop guard checked is_some()");
                // Exit the current (leaf) state. RFC-0025.1: exit args live in the
                // source state's typed ctx (written at the transition site), so the
                // synthesized `<$` event carries no payload.
                let exit_event = alloc::rc::Rc::new(HsmCycleWalkerFrameEvent::FrameExit {});
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
                        let enter_event = alloc::rc::Rc::new(HsmCycleWalkerFrameEvent::FrameEnter {});
                        self.__router(&enter_event);
                    }
                    Some(fwd) if matches!(fwd, HsmCycleWalkerFrameEvent::FrameEnter { .. }) => {
                        // Forwarded event IS $> — dispatch directly so the
                        // destination's $> handler receives the caller's payload.
                        let fwd_rc = alloc::rc::Rc::new(fwd);
                        self.__router(&fwd_rc);
                    }
                    Some(fwd) => {
                        // Forwarded event is not $> — initialize the destination
                        // with a fresh $>, then dispatch the forward.
                        let enter_event = alloc::rc::Rc::new(HsmCycleWalkerFrameEvent::FrameEnter {});
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

        fn __router(&mut self, __e: &alloc::rc::Rc<HsmCycleWalkerFrameEvent>) {
            let __ev: &HsmCycleWalkerFrameEvent = __e;
            match self.__compartment.state.as_str() {
                "Initial" => self._state_Initial(__ev),
                "Walking" => self._state_Walking(__ev),
                "CycleFound" => self._state_CycleFound(__ev),
                "ChainRoot" => self._state_ChainRoot(__ev),
                _ => {}
            }
        }

        fn __transition(&mut self, next_compartment: HsmCycleWalkerCompartment) {
            self.__next_compartment = Some(next_compartment);
        }

        pub fn step(&mut self, parent: String) -> String {
            let __e = alloc::rc::Rc::new(HsmCycleWalkerFrameEvent::Step { parent: parent.clone() });
            let mut __ctx = HsmCycleWalkerFrameContext::new(alloc::rc::Rc::clone(&__e), None);
            self._context_stack.push(__ctx);
            self.__kernel(&__e);
            let __ctx = self._context_stack.pop().expect("invariant: handler must have pushed a context before reading return");
            match __ctx._return {
                Some(HsmCycleWalkerFrameReturn::Step(v)) => v,
                Some(HsmCycleWalkerFrameReturn::_Lifecycle(v)) => v.downcast_ref::<String>().cloned().unwrap_or_default(),
                _ => Default::default(),
            }
        }

        fn _state_Initial(&mut self, __e: &HsmCycleWalkerFrameEvent) {
            match __e {
                HsmCycleWalkerFrameEvent::Step { parent, .. } => {
                    self._s_Initial_hdl_user_step(__e, parent.clone());
                }
                _ => {}
            }
        }

        fn _state_Walking(&mut self, __e: &HsmCycleWalkerFrameEvent) {
            match __e {
                HsmCycleWalkerFrameEvent::Step { parent, .. } => {
                    self._s_Walking_hdl_user_step(__e, parent.clone());
                }
                _ => {}
            }
        }

        fn _state_CycleFound(&mut self, __e: &HsmCycleWalkerFrameEvent) {
            match __e {
                HsmCycleWalkerFrameEvent::Step { parent, .. } => {
                    self._s_CycleFound_hdl_user_step(__e, parent.clone());
                }
                _ => {}
            }
        }

        fn _state_ChainRoot(&mut self, __e: &HsmCycleWalkerFrameEvent) {
            match __e {
                HsmCycleWalkerFrameEvent::Step { parent, .. } => {
                    self._s_ChainRoot_hdl_user_step(__e, parent.clone());
                }
                _ => {}
            }
        }

        fn _s_Initial_hdl_user_step(&mut self, __e: &HsmCycleWalkerFrameEvent, parent: String) {
                            // First call: caller MUST send the starting node
                            // as `parent` here (the node whose chain we are
                            // about to walk). Subsequent calls send parent
                            // chain links.
                            self.start_name = parent.clone();
                            self.visited = parent.clone();
                            let mut __compartment = self.__prepareEnter("Walking");
                            self.__transition(__compartment);
            let __return_val = HsmCycleWalkerFrameReturn::Step("WALKING".to_string());
                            if let Some(ctx) = self._context_stack.last_mut() { ctx._return = Some(__return_val); }
                            return;
        }

        fn _s_Walking_hdl_user_step(&mut self, __e: &HsmCycleWalkerFrameEvent, parent: String) {
                            if parent.is_empty() {
                                let mut __compartment = self.__prepareEnter("ChainRoot");
                                self.__transition(__compartment);
            let __return_val = HsmCycleWalkerFrameReturn::Step("ROOT".to_string());
                                if let Some(ctx) = self._context_stack.last_mut() { ctx._return = Some(__return_val); }
                                return;
            
                            }
                            // Check visited: split on `,`, look for parent.
                            let already_seen = self
                                .visited
                                .split(',')
                                .any(|v| v == parent.as_str());
                            if already_seen {
                                self.cycle_at = parent.clone();
                                let mut __compartment = self.__prepareEnter("CycleFound");
                                self.__transition(__compartment);
            let __return_val = HsmCycleWalkerFrameReturn::Step(format!("CYCLE|{}", parent));
                                if let Some(ctx) = self._context_stack.last_mut() { ctx._return = Some(__return_val); }
                                return;
            
                            }
                            if !self.visited.is_empty() {
                                self.visited.push(',');
                            }
                            self.visited.push_str(&parent);
            let __return_val = HsmCycleWalkerFrameReturn::Step("WALKING".to_string());
                            if let Some(ctx) = self._context_stack.last_mut() { ctx._return = Some(__return_val); }
        }

        fn _s_CycleFound_hdl_user_step(&mut self, __e: &HsmCycleWalkerFrameEvent, parent: String) {
            let __return_val = HsmCycleWalkerFrameReturn::Step(format!("CYCLE|{}", self.cycle_at));
                            if let Some(ctx) = self._context_stack.last_mut() { ctx._return = Some(__return_val); }
        }

        fn _s_ChainRoot_hdl_user_step(&mut self, __e: &HsmCycleWalkerFrameEvent, parent: String) {
            let __return_val = HsmCycleWalkerFrameReturn::Step("ROOT".to_string());
                            if let Some(ctx) = self._context_stack.last_mut() { ctx._return = Some(__return_val); }
        }
    }
}
pub use _hsm_cycle_walker_framec::*;

