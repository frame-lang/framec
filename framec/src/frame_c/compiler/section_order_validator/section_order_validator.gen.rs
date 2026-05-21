
// E113 validator — `@@system` section blocks must appear in the
// canonical order operations: → interface: → machine: → actions:
// → domain:. Out-of-order blocks raise E113.
//
// RFC-0035 round 4. This is the round where Frame's multi-state
// shape actually fits the problem. The validator is naturally
// expressed as:
//
//   $Walking     — accept sections, update last_idx
//   $OutOfOrder  — terminal; any subsequent section is ignored
//                  (E113 is reported once per system, per the
//                  existing validator contract)
//
// The transition $Walking → $OutOfOrder is fired on the first
// out-of-order section seen. Subsequent check() calls land in
// $OutOfOrder's handler, which simply absorbs the call. This is
// the canonical "error-absorbing terminal state" pattern that
// Frame state machines were designed for, and it maps cleanly
// onto E113's "report once per system" semantics.
//
// The Frame system carries `last_idx` as a domain field; it
// evolves across calls. Each check(kind) decodes the kind to its
// canonical index (Operations=0..Domain=4) and compares to
// last_idx. If the new idx is strictly greater than last_idx,
// it's an in-order section and we keep walking. Otherwise we
// transition to $OutOfOrder and report E113.
//
// The interface returns "" (no error) or "E113|<message>". The
// glue Rust function parses the response and either pushes a
// ValidationError or moves on.

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
mod _section_order_validator_framec {
    use super::*;
    extern crate alloc;
    use alloc::{vec, format};
    #[derive(Clone, Debug)]
    #[allow(dead_code, non_camel_case_types)]
    enum SectionOrderValidatorFrameEvent {
        Check { kind: String },
        FrameEnter { args: Vec<alloc::rc::Rc<dyn core::any::Any>> },
        FrameExit { args: Vec<alloc::rc::Rc<dyn core::any::Any>> },
    }

    #[derive(Clone)]
    #[allow(dead_code, non_camel_case_types)]
    enum SectionOrderValidatorFrameReturn {
        Check(String),
        _Lifecycle(alloc::rc::Rc<dyn core::any::Any>),
    }

    #[allow(dead_code)]
    impl SectionOrderValidatorFrameEvent {
        fn name(&self) -> &'static str {
            match self {
                SectionOrderValidatorFrameEvent::Check { .. } => "check",
                SectionOrderValidatorFrameEvent::FrameEnter { .. } => "$>",
                SectionOrderValidatorFrameEvent::FrameExit { .. } => "<$",
            }
        }
    }

    #[derive(Clone, Debug)]
    #[allow(dead_code, non_camel_case_types)]
    enum SectionOrderValidatorFrameValue {
        Int(i64),
        Float(f64),
        Bool(bool),
        Str(String),
        List(Vec<Self>),
        Dict(alloc::collections::BTreeMap<String, Self>),
    }

    #[allow(dead_code, non_camel_case_types)]
    struct SectionOrderValidatorFrameContext {
        event: alloc::rc::Rc<SectionOrderValidatorFrameEvent>,
        _return: Option<SectionOrderValidatorFrameReturn>,
        _data: alloc::collections::BTreeMap<String, SectionOrderValidatorFrameValue>,
        _transitioned: bool,
    }

    impl SectionOrderValidatorFrameContext {
        fn new(event: alloc::rc::Rc<SectionOrderValidatorFrameEvent>, default_return: Option<SectionOrderValidatorFrameReturn>) -> Self {
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
    enum SectionOrderValidatorStateContext {
        Walking,
        OutOfOrder,
        Empty,
    }

    impl Default for SectionOrderValidatorStateContext {
        fn default() -> Self {
            SectionOrderValidatorStateContext::Walking
        }
    }

    #[allow(dead_code, non_camel_case_types)]
    #[derive(Clone)]
    struct SectionOrderValidatorCompartment {
        state: String,
        state_context: SectionOrderValidatorStateContext,
        enter_args: Vec<alloc::rc::Rc<dyn core::any::Any>>,
        exit_args: Vec<alloc::rc::Rc<dyn core::any::Any>>,
        forward_event: Option<SectionOrderValidatorFrameEvent>,
        parent_compartment: Option<Box<SectionOrderValidatorCompartment>>,
    }

    impl SectionOrderValidatorCompartment {
        fn new(state: &str) -> Self {
            let state_context = match state {
                "Walking" => SectionOrderValidatorStateContext::Walking,
                "OutOfOrder" => SectionOrderValidatorStateContext::OutOfOrder,
                _ => SectionOrderValidatorStateContext::Empty,
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
    pub struct SectionOrderValidator {
        _state_stack: Vec<SectionOrderValidatorCompartment>,
        __compartment: SectionOrderValidatorCompartment,
        __next_compartment: Option<SectionOrderValidatorCompartment>,
        _context_stack: Vec<SectionOrderValidatorFrameContext>,
        pub last_idx: i32,
    }

    #[allow(non_snake_case)]
    impl SectionOrderValidator {
        pub fn new() -> Self {
            Self {
                _state_stack: Vec::new(),
                _context_stack: Vec::new(),
                last_idx: -1,
                __compartment: SectionOrderValidatorCompartment::new("Walking"),
                __next_compartment: None,
            }
        }

        pub fn __create() -> Self {
            let mut c = Self::new();
            c.__compartment = c.__prepareEnter("Walking", vec![]);
            let __e = alloc::rc::Rc::new(SectionOrderValidatorFrameEvent::FrameEnter { args: c.__compartment.enter_args.clone() });
            let __ctx = SectionOrderValidatorFrameContext::new(alloc::rc::Rc::clone(&__e), None);
            c._context_stack.push(__ctx);
            c.__kernel(&__e);
            c._context_stack.pop();
            c
        }

        fn __hsm_chain(&mut self, leaf: &str) -> &'static [&'static str] {
            match leaf {
                "Walking" => &["Walking"],
                "OutOfOrder" => &["OutOfOrder"],
                _ => &[],
            }
        }

        fn __prepareEnter(&mut self, leaf: &str, enter_args: Vec<alloc::rc::Rc<dyn core::any::Any>>) -> SectionOrderValidatorCompartment {
            let chain = self.__hsm_chain(leaf);
            let mut comp: Option<SectionOrderValidatorCompartment> = None;
            for name in chain.iter() {
                let mut new_comp = SectionOrderValidatorCompartment::new(name);
                new_comp.enter_args = enter_args.clone();
                if let Some(parent) = comp.take() {
                    new_comp.parent_compartment = Some(Box::new(parent));
                }
                comp = Some(new_comp);
            }
            comp.expect("chain must contain at least the leaf state")
        }

        fn __prepareExit(&mut self, exit_args: Vec<alloc::rc::Rc<dyn core::any::Any>>) {
            self.__compartment.exit_args = exit_args.clone();
            let mut cursor = self.__compartment.parent_compartment.as_deref_mut();
            while let Some(c) = cursor {
                c.exit_args = exit_args.clone();
                cursor = c.parent_compartment.as_deref_mut();
            }
        }

        fn __kernel(&mut self, __e: &alloc::rc::Rc<SectionOrderValidatorFrameEvent>) {
            // Route event to current state.
            self.__router(__e);
            // Drain any transitions queued by the handler.
            while self.__next_compartment.is_some() {
                let next_compartment = self.__next_compartment.take().expect("invariant: while-loop guard checked is_some()");
                // Exit the current (leaf) state.
                let exit_args = self.__compartment.exit_args.clone();
                let exit_event = alloc::rc::Rc::new(SectionOrderValidatorFrameEvent::FrameExit { args: exit_args });
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
                        let enter_event = alloc::rc::Rc::new(SectionOrderValidatorFrameEvent::FrameEnter { args: enter_args });
                        self.__router(&enter_event);
                    }
                    Some(fwd) if matches!(fwd, SectionOrderValidatorFrameEvent::FrameEnter { .. }) => {
                        // Forwarded event IS $> — dispatch directly so the
                        // destination's $> handler receives the caller's payload.
                        let fwd_rc = alloc::rc::Rc::new(fwd);
                        self.__router(&fwd_rc);
                    }
                    Some(fwd) => {
                        // Forwarded event is not $> — initialize the destination
                        // with a fresh $>, then dispatch the forward.
                        let enter_args = self.__compartment.enter_args.clone();
                        let enter_event = alloc::rc::Rc::new(SectionOrderValidatorFrameEvent::FrameEnter { args: enter_args });
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

        fn __router(&mut self, __e: &alloc::rc::Rc<SectionOrderValidatorFrameEvent>) {
            let __ev: &SectionOrderValidatorFrameEvent = __e;
            match self.__compartment.state.as_str() {
                "Walking" => self._state_Walking(__ev),
                "OutOfOrder" => self._state_OutOfOrder(__ev),
                _ => {}
            }
        }

        fn __transition(&mut self, next_compartment: SectionOrderValidatorCompartment) {
            self.__next_compartment = Some(next_compartment);
        }

        pub fn check(&mut self, kind: String) -> String {
            let __e = alloc::rc::Rc::new(SectionOrderValidatorFrameEvent::Check { kind: kind.clone() });
            let mut __ctx = SectionOrderValidatorFrameContext::new(alloc::rc::Rc::clone(&__e), None);
            self._context_stack.push(__ctx);
            self.__kernel(&__e);
            let __ctx = self._context_stack.pop().expect("invariant: handler must have pushed a context before reading return");
            match __ctx._return {
                Some(SectionOrderValidatorFrameReturn::Check(v)) => v,
                Some(SectionOrderValidatorFrameReturn::_Lifecycle(v)) => v.downcast_ref::<String>().cloned().unwrap_or_default(),
                _ => Default::default(),
            }
        }

        fn _state_Walking(&mut self, __e: &SectionOrderValidatorFrameEvent) {
            match __e {
                SectionOrderValidatorFrameEvent::Check { kind, .. } => {
                    self._s_Walking_hdl_user_check(__e, kind.clone());
                }
                _ => {}
            }
        }

        fn _state_OutOfOrder(&mut self, __e: &SectionOrderValidatorFrameEvent) {
            match __e {
                SectionOrderValidatorFrameEvent::Check { kind, .. } => {
                    self._s_OutOfOrder_hdl_user_check(__e, kind.clone());
                }
                _ => {}
            }
        }

        fn _s_Walking_hdl_user_check(&mut self, __e: &SectionOrderValidatorFrameEvent, kind: String) {
                            let idx: i32 = match kind.as_str() {
                                "Operations" => 0,
                                "Interface" => 1,
                                "Machine" => 2,
                                "Actions" => 3,
                                "Domain" => 4,
                                _ => -1,
                            };
                            if idx < 0 {
            let __return_val = SectionOrderValidatorFrameReturn::Check(String::new());
                                if let Some(ctx) = self._context_stack.last_mut() { ctx._return = Some(__return_val); }
                            }
                            if idx < self.last_idx {
                                let msg = format!(
                                    "blocks out of order. Expected: operations:, interface:, machine:, actions:, domain:"
                                );
                                let mut __compartment = self.__prepareEnter("OutOfOrder", vec![]);
                                self.__transition(__compartment);
            let __return_val = SectionOrderValidatorFrameReturn::Check(format!("E113|{}", msg));
                                if let Some(ctx) = self._context_stack.last_mut() { ctx._return = Some(__return_val); }
                                return;
            
                            }
                            self.last_idx = idx;
            let __return_val = SectionOrderValidatorFrameReturn::Check(String::new());
                            if let Some(ctx) = self._context_stack.last_mut() { ctx._return = Some(__return_val); }
        }

        // Terminal error state: E113 has already been
        // reported. Subsequent check() calls are absorbed.
        fn _s_OutOfOrder_hdl_user_check(&mut self, __e: &SectionOrderValidatorFrameEvent, kind: String) {
            let __return_val = SectionOrderValidatorFrameReturn::Check(String::new());
                            if let Some(ctx) = self._context_stack.last_mut() { ctx._return = Some(__return_val); }
        }
    }
}
pub use _section_order_validator_framec::*;

