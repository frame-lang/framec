
// RFC-0033 helper: returns the promoted owned-type spelling for
// a Rust interface-parameter type string, OR the empty string if
// the type is already owned (no promotion needed).
//
//   "&str"     → "String"
//   "&[T]"     → "Vec<T>"
//   otherwise  → ""
//
// Returning the empty string for "no promotion" keeps the
// interface contract simple (all answers are String). Callers
// branch on `.is_empty()` and the glue mod translates back to
// `Option<String>`.
//
// Single result binding so the body's one `@@:(...)` call wins.

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
mod _rust_owned_promotion_framec {
    use super::*;
    extern crate alloc;
    #[derive(Clone, Debug)]
    #[allow(dead_code, non_camel_case_types)]
    enum RustOwnedPromotionFrameEvent {
        Promote { t: String },
        FrameEnter { args: Vec<String> },
        FrameExit { args: Vec<String> },
    }

    #[derive(Clone)]
    #[allow(dead_code, non_camel_case_types)]
    enum RustOwnedPromotionFrameReturn {
        Promote(String),
        _Lifecycle(alloc::rc::Rc<dyn core::any::Any>),
    }

    #[allow(dead_code)]
    impl RustOwnedPromotionFrameEvent {
        fn name(&self) -> &'static str {
            match self {
                RustOwnedPromotionFrameEvent::Promote { .. } => "promote",
                RustOwnedPromotionFrameEvent::FrameEnter { .. } => "$>",
                RustOwnedPromotionFrameEvent::FrameExit { .. } => "<$",
            }
        }
    }

    #[derive(Clone, Debug)]
    #[allow(dead_code, non_camel_case_types)]
    enum RustOwnedPromotionFrameValue {
        Int(i64),
        Float(f64),
        Bool(bool),
        Str(String),
        List(Vec<Self>),
        Dict(alloc::collections::BTreeMap<String, Self>),
    }

    #[allow(dead_code, non_camel_case_types)]
    struct RustOwnedPromotionFrameContext {
        event: alloc::rc::Rc<RustOwnedPromotionFrameEvent>,
        _return: Option<RustOwnedPromotionFrameReturn>,
        _data: alloc::collections::BTreeMap<String, RustOwnedPromotionFrameValue>,
        _transitioned: bool,
    }

    impl RustOwnedPromotionFrameContext {
        fn new(event: alloc::rc::Rc<RustOwnedPromotionFrameEvent>, default_return: Option<RustOwnedPromotionFrameReturn>) -> Self {
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
    enum RustOwnedPromotionStateContext {
        Active,
        Empty,
    }

    impl Default for RustOwnedPromotionStateContext {
        fn default() -> Self {
            RustOwnedPromotionStateContext::Active
        }
    }

    #[allow(dead_code, non_camel_case_types)]
    #[derive(Clone)]
    struct RustOwnedPromotionCompartment {
        state: String,
        state_context: RustOwnedPromotionStateContext,
        enter_args: Vec<String>,
        exit_args: Vec<String>,
        forward_event: Option<RustOwnedPromotionFrameEvent>,
        parent_compartment: Option<Box<RustOwnedPromotionCompartment>>,
    }

    impl RustOwnedPromotionCompartment {
        fn new(state: &str) -> Self {
            let state_context = match state {
                "Active" => RustOwnedPromotionStateContext::Active,
                _ => RustOwnedPromotionStateContext::Empty,
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
    pub struct RustOwnedPromotion {
        _state_stack: Vec<RustOwnedPromotionCompartment>,
        __compartment: RustOwnedPromotionCompartment,
        __next_compartment: Option<RustOwnedPromotionCompartment>,
        _context_stack: Vec<RustOwnedPromotionFrameContext>,
    }

    #[allow(non_snake_case)]
    impl RustOwnedPromotion {
        pub fn new() -> Self {
            Self {
                _state_stack: Vec::new(),
                _context_stack: Vec::new(),
                __compartment: RustOwnedPromotionCompartment::new("Active"),
                __next_compartment: None,
            }
        }

        pub fn __create() -> Self {
            let mut c = Self::new();
            c.__compartment = c.__prepareEnter("Active", vec![]);
            let __e = alloc::rc::Rc::new(RustOwnedPromotionFrameEvent::FrameEnter { args: c.__compartment.enter_args.clone() });
            let __ctx = RustOwnedPromotionFrameContext::new(alloc::rc::Rc::clone(&__e), None);
            c._context_stack.push(__ctx);
            c.__kernel(&__e);
            c._context_stack.pop();
            c
        }

        fn __hsm_chain(&mut self, leaf: &str) -> &'static [&'static str] {
            match leaf {
                "Active" => &["Active"],
                _ => &[],
            }
        }

        fn __prepareEnter(&mut self, leaf: &str, enter_args: Vec<String>) -> RustOwnedPromotionCompartment {
            let chain = self.__hsm_chain(leaf);
            let mut comp: Option<RustOwnedPromotionCompartment> = None;
            for name in chain.iter() {
                let mut new_comp = RustOwnedPromotionCompartment::new(name);
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

        fn __kernel(&mut self, __e: &alloc::rc::Rc<RustOwnedPromotionFrameEvent>) {
            // Route event to current state.
            self.__router(__e);
            // Drain any transitions queued by the handler.
            while self.__next_compartment.is_some() {
                let next_compartment = self.__next_compartment.take().expect("invariant: while-loop guard checked is_some()");
                // Exit the current (leaf) state.
                let exit_args = self.__compartment.exit_args.clone();
                let exit_event = alloc::rc::Rc::new(RustOwnedPromotionFrameEvent::FrameExit { args: exit_args });
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
                        let enter_event = alloc::rc::Rc::new(RustOwnedPromotionFrameEvent::FrameEnter { args: enter_args });
                        self.__router(&enter_event);
                    }
                    Some(fwd) if matches!(fwd, RustOwnedPromotionFrameEvent::FrameEnter { .. }) => {
                        // Forwarded event IS $> — dispatch directly so the
                        // destination's $> handler receives the caller's payload.
                        let fwd_rc = alloc::rc::Rc::new(fwd);
                        self.__router(&fwd_rc);
                    }
                    Some(fwd) => {
                        // Forwarded event is not $> — initialize the destination
                        // with a fresh $>, then dispatch the forward.
                        let enter_args = self.__compartment.enter_args.clone();
                        let enter_event = alloc::rc::Rc::new(RustOwnedPromotionFrameEvent::FrameEnter { args: enter_args });
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

        fn __router(&mut self, __e: &alloc::rc::Rc<RustOwnedPromotionFrameEvent>) {
            let __ev: &RustOwnedPromotionFrameEvent = __e;
            match self.__compartment.state.as_str() {
                "Active" => self._state_Active(__ev),
                _ => {}
            }
        }

        fn __transition(&mut self, next_compartment: RustOwnedPromotionCompartment) {
            self.__next_compartment = Some(next_compartment);
        }

        pub fn promote(&mut self, t: String) -> String {
            let __e = alloc::rc::Rc::new(RustOwnedPromotionFrameEvent::Promote { t: t.clone() });
            let mut __ctx = RustOwnedPromotionFrameContext::new(alloc::rc::Rc::clone(&__e), None);
            self._context_stack.push(__ctx);
            self.__kernel(&__e);
            let __ctx = self._context_stack.pop().expect("invariant: handler must have pushed a context before reading return");
            match __ctx._return {
                Some(RustOwnedPromotionFrameReturn::Promote(v)) => v,
                Some(RustOwnedPromotionFrameReturn::_Lifecycle(v)) => v.downcast_ref::<String>().cloned().unwrap_or_default(),
                _ => Default::default(),
            }
        }

        fn _state_Active(&mut self, __e: &RustOwnedPromotionFrameEvent) {
            match __e {
                RustOwnedPromotionFrameEvent::Promote { t, .. } => {
                    self._s_Active_hdl_user_promote(__e, t.clone());
                }
                _ => {}
            }
        }

        fn _s_Active_hdl_user_promote(&mut self, __e: &RustOwnedPromotionFrameEvent, t: String) {
                            let trimmed = t.trim();
                            let result = if trimmed == "&str" {
                                "String".to_string()
                            } else if trimmed.starts_with("&[") && trimmed.ends_with(']') {
                                let inner = &trimmed[2..trimmed.len() - 1];
                                format!("Vec<{}>", inner)
                            } else {
                                String::new()
                            };
            let __return_val = RustOwnedPromotionFrameReturn::Promote(result.clone());
                            if let Some(ctx) = self._context_stack.last_mut() { ctx._return = Some(__return_val); }
        }
    }
}
pub use _rust_owned_promotion_framec::*;

