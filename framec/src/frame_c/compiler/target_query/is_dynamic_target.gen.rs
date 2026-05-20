
// Target-language predicate: is this a dynamic/loosely-typed
// target? Used by interface_gen and friends to decide whether
// to emit casts, type annotations, or untyped passthroughs.
//
// Frame's interface is stringly-typed for the moment — the
// TargetLanguage enum doesn't round-trip cleanly through a
// Frame event param. So the input here is the
// `TargetLanguage::as_str()` form (the lowercased canonical
// name), and the answer comes back as "true" / "false" which
// the glue mod.rs converts back to bool.
//
// This is part of RFC-0035 round 2 — a deliberately awkward fit
// (boolean returns expressed as string round-trip) is worth
// recording as an ergonomics observation.

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
mod _is_dynamic_target_framec {
    use super::*;
    extern crate alloc;
    #[derive(Clone, Debug)]
    #[allow(dead_code, non_camel_case_types)]
    enum IsDynamicTargetFrameEvent {
        Check { lang: String },
        FrameEnter { args: Vec<String> },
        FrameExit { args: Vec<String> },
    }

    #[derive(Clone)]
    #[allow(dead_code, non_camel_case_types)]
    enum IsDynamicTargetFrameReturn {
        Check(String),
        _Lifecycle(alloc::rc::Rc<dyn core::any::Any>),
    }

    #[allow(dead_code)]
    impl IsDynamicTargetFrameEvent {
        fn name(&self) -> &'static str {
            match self {
                IsDynamicTargetFrameEvent::Check { .. } => "check",
                IsDynamicTargetFrameEvent::FrameEnter { .. } => "$>",
                IsDynamicTargetFrameEvent::FrameExit { .. } => "<$",
            }
        }
    }

    #[derive(Clone, Debug)]
    #[allow(dead_code, non_camel_case_types)]
    enum IsDynamicTargetFrameValue {
        Int(i64),
        Float(f64),
        Bool(bool),
        Str(String),
        List(Vec<Self>),
        Dict(alloc::collections::BTreeMap<String, Self>),
    }

    #[allow(dead_code, non_camel_case_types)]
    struct IsDynamicTargetFrameContext {
        event: alloc::rc::Rc<IsDynamicTargetFrameEvent>,
        _return: Option<IsDynamicTargetFrameReturn>,
        _data: alloc::collections::BTreeMap<String, IsDynamicTargetFrameValue>,
        _transitioned: bool,
    }

    impl IsDynamicTargetFrameContext {
        fn new(event: alloc::rc::Rc<IsDynamicTargetFrameEvent>, default_return: Option<IsDynamicTargetFrameReturn>) -> Self {
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
    enum IsDynamicTargetStateContext {
        Active,
        Empty,
    }

    impl Default for IsDynamicTargetStateContext {
        fn default() -> Self {
            IsDynamicTargetStateContext::Active
        }
    }

    #[allow(dead_code, non_camel_case_types)]
    #[derive(Clone)]
    struct IsDynamicTargetCompartment {
        state: String,
        state_context: IsDynamicTargetStateContext,
        enter_args: Vec<String>,
        exit_args: Vec<String>,
        forward_event: Option<IsDynamicTargetFrameEvent>,
        parent_compartment: Option<Box<IsDynamicTargetCompartment>>,
    }

    impl IsDynamicTargetCompartment {
        fn new(state: &str) -> Self {
            let state_context = match state {
                "Active" => IsDynamicTargetStateContext::Active,
                _ => IsDynamicTargetStateContext::Empty,
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
    pub struct IsDynamicTarget {
        _state_stack: Vec<IsDynamicTargetCompartment>,
        __compartment: IsDynamicTargetCompartment,
        __next_compartment: Option<IsDynamicTargetCompartment>,
        _context_stack: Vec<IsDynamicTargetFrameContext>,
    }

    #[allow(non_snake_case)]
    impl IsDynamicTarget {
        pub fn new() -> Self {
            Self {
                _state_stack: Vec::new(),
                _context_stack: Vec::new(),
                __compartment: IsDynamicTargetCompartment::new("Active"),
                __next_compartment: None,
            }
        }

        pub fn __create() -> Self {
            let mut c = Self::new();
            c.__compartment = c.__prepareEnter("Active", vec![]);
            let __e = alloc::rc::Rc::new(IsDynamicTargetFrameEvent::FrameEnter { args: c.__compartment.enter_args.clone() });
            let __ctx = IsDynamicTargetFrameContext::new(alloc::rc::Rc::clone(&__e), None);
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

        fn __prepareEnter(&mut self, leaf: &str, enter_args: Vec<String>) -> IsDynamicTargetCompartment {
            let chain = self.__hsm_chain(leaf);
            let mut comp: Option<IsDynamicTargetCompartment> = None;
            for name in chain.iter() {
                let mut new_comp = IsDynamicTargetCompartment::new(name);
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

        fn __kernel(&mut self, __e: &alloc::rc::Rc<IsDynamicTargetFrameEvent>) {
            // Route event to current state.
            self.__router(__e);
            // Drain any transitions queued by the handler.
            while self.__next_compartment.is_some() {
                let next_compartment = self.__next_compartment.take().expect("invariant: while-loop guard checked is_some()");
                // Exit the current (leaf) state.
                let exit_args = self.__compartment.exit_args.clone();
                let exit_event = alloc::rc::Rc::new(IsDynamicTargetFrameEvent::FrameExit { args: exit_args });
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
                        let enter_event = alloc::rc::Rc::new(IsDynamicTargetFrameEvent::FrameEnter { args: enter_args });
                        self.__router(&enter_event);
                    }
                    Some(fwd) if matches!(fwd, IsDynamicTargetFrameEvent::FrameEnter { .. }) => {
                        // Forwarded event IS $> — dispatch directly so the
                        // destination's $> handler receives the caller's payload.
                        let fwd_rc = alloc::rc::Rc::new(fwd);
                        self.__router(&fwd_rc);
                    }
                    Some(fwd) => {
                        // Forwarded event is not $> — initialize the destination
                        // with a fresh $>, then dispatch the forward.
                        let enter_args = self.__compartment.enter_args.clone();
                        let enter_event = alloc::rc::Rc::new(IsDynamicTargetFrameEvent::FrameEnter { args: enter_args });
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

        fn __router(&mut self, __e: &alloc::rc::Rc<IsDynamicTargetFrameEvent>) {
            let __ev: &IsDynamicTargetFrameEvent = __e;
            match self.__compartment.state.as_str() {
                "Active" => self._state_Active(__ev),
                _ => {}
            }
        }

        fn __transition(&mut self, next_compartment: IsDynamicTargetCompartment) {
            self.__next_compartment = Some(next_compartment);
        }

        pub fn check(&mut self, lang: String) -> String {
            let __e = alloc::rc::Rc::new(IsDynamicTargetFrameEvent::Check { lang: lang.clone() });
            let mut __ctx = IsDynamicTargetFrameContext::new(alloc::rc::Rc::clone(&__e), None);
            self._context_stack.push(__ctx);
            self.__kernel(&__e);
            let __ctx = self._context_stack.pop().expect("invariant: handler must have pushed a context before reading return");
            match __ctx._return {
                Some(IsDynamicTargetFrameReturn::Check(v)) => v,
                Some(IsDynamicTargetFrameReturn::_Lifecycle(v)) => v.downcast_ref::<String>().cloned().unwrap_or_default(),
                _ => Default::default(),
            }
        }

        fn _state_Active(&mut self, __e: &IsDynamicTargetFrameEvent) {
            match __e {
                IsDynamicTargetFrameEvent::Check { lang, .. } => {
                    self._s_Active_hdl_user_check(__e, lang.clone());
                }
                _ => {}
            }
        }

        fn _s_Active_hdl_user_check(&mut self, __e: &IsDynamicTargetFrameEvent, lang: String) {
                            let result = matches!(
                                lang.as_str(),
                                "python_3" | "javascript" | "ruby" | "lua" | "php" | "gdscript" | "erlang"
                            );
            let __return_val = IsDynamicTargetFrameReturn::Check(if result { "true".to_string() } else { "false".to_string() });
                            if let Some(ctx) = self._context_stack.last_mut() { ctx._return = Some(__return_val); }
        }
    }
}
pub use _is_dynamic_target_framec::*;

