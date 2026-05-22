
// Frame type string → Go type spelling.
// `void` / `None` map to the empty string (Go has no void return
// type — the absence of a return spec IS the void form).

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
mod _go_map_type_framec {
    use super::*;
    extern crate alloc;
    use alloc::{vec, format};
    #[derive(Clone, Debug)]
    #[allow(dead_code, non_camel_case_types)]
    enum GoMapTypeFrameEvent {
        Map { t: String },
        FrameEnter {},
        FrameExit {},
    }

    #[derive(Clone)]
    #[allow(dead_code, non_camel_case_types)]
    enum GoMapTypeFrameReturn {
        Map(String),
        _Lifecycle(alloc::rc::Rc<dyn core::any::Any>),
    }

    #[allow(dead_code)]
    impl GoMapTypeFrameEvent {
        fn name(&self) -> &'static str {
            match self {
                GoMapTypeFrameEvent::Map { .. } => "map",
                GoMapTypeFrameEvent::FrameEnter { .. } => "$>",
                GoMapTypeFrameEvent::FrameExit { .. } => "<$",
            }
        }
    }

    #[derive(Clone, Debug)]
    #[allow(dead_code, non_camel_case_types)]
    enum GoMapTypeFrameValue {
        Int(i64),
        Float(f64),
        Bool(bool),
        Str(String),
        List(Vec<Self>),
        Dict(alloc::collections::BTreeMap<String, Self>),
    }

    #[allow(dead_code, non_camel_case_types)]
    struct GoMapTypeFrameContext {
        event: alloc::rc::Rc<GoMapTypeFrameEvent>,
        _return: Option<GoMapTypeFrameReturn>,
        _data: alloc::collections::BTreeMap<String, GoMapTypeFrameValue>,
        _transitioned: bool,
    }

    impl GoMapTypeFrameContext {
        fn new(event: alloc::rc::Rc<GoMapTypeFrameEvent>, default_return: Option<GoMapTypeFrameReturn>) -> Self {
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
    enum GoMapTypeStateContext {
        Active,
        __NoContext,
    }

    impl Default for GoMapTypeStateContext {
        fn default() -> Self {
            GoMapTypeStateContext::Active
        }
    }

    #[allow(dead_code, non_camel_case_types)]
    #[derive(Clone)]
    struct GoMapTypeCompartment {
        state: String,
        state_context: GoMapTypeStateContext,
        forward_event: Option<GoMapTypeFrameEvent>,
        parent_compartment: Option<Box<GoMapTypeCompartment>>,
    }

    impl GoMapTypeCompartment {
        fn new(state: &str) -> Self {
            let state_context = match state {
                "Active" => GoMapTypeStateContext::Active,
                _ => GoMapTypeStateContext::__NoContext,
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
    pub struct GoMapType {
        _state_stack: Vec<GoMapTypeCompartment>,
        __compartment: GoMapTypeCompartment,
        __next_compartment: Option<GoMapTypeCompartment>,
        _context_stack: Vec<GoMapTypeFrameContext>,
    }

    #[allow(non_snake_case)]
    impl GoMapType {
        pub fn new() -> Self {
            Self {
                _state_stack: Vec::new(),
                _context_stack: Vec::new(),
                __compartment: GoMapTypeCompartment::new("Active"),
                __next_compartment: None,
            }
        }

        pub fn __create() -> Self {
            let mut c = Self::new();
            c.__compartment = c.__prepareEnter("Active");
            let __e = alloc::rc::Rc::new(GoMapTypeFrameEvent::FrameEnter {});
            let __ctx = GoMapTypeFrameContext::new(alloc::rc::Rc::clone(&__e), None);
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

        fn __prepareEnter(&mut self, leaf: &str) -> GoMapTypeCompartment {
            let chain = self.__hsm_chain(leaf);
            let mut comp: Option<GoMapTypeCompartment> = None;
            for name in chain.iter() {
                let mut new_comp = GoMapTypeCompartment::new(name);
                if let Some(parent) = comp.take() {
                    new_comp.parent_compartment = Some(Box::new(parent));
                }
                comp = Some(new_comp);
            }
            comp.expect("chain must contain at least the leaf state")
        }

        fn __kernel(&mut self, __e: &alloc::rc::Rc<GoMapTypeFrameEvent>) {
            // Route event to current state.
            self.__router(__e);
            // Drain any transitions queued by the handler.
            while self.__next_compartment.is_some() {
                let next_compartment = self.__next_compartment.take().expect("invariant: while-loop guard checked is_some()");
                // Exit the current (leaf) state. RFC-0025.1: exit args live in the
                // source state's typed ctx (written at the transition site), so the
                // synthesized `<$` event carries no payload.
                let exit_event = alloc::rc::Rc::new(GoMapTypeFrameEvent::FrameExit {});
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
                        let enter_event = alloc::rc::Rc::new(GoMapTypeFrameEvent::FrameEnter {});
                        self.__router(&enter_event);
                    }
                    Some(fwd) if matches!(fwd, GoMapTypeFrameEvent::FrameEnter { .. }) => {
                        // Forwarded event IS $> — dispatch directly so the
                        // destination's $> handler receives the caller's payload.
                        let fwd_rc = alloc::rc::Rc::new(fwd);
                        self.__router(&fwd_rc);
                    }
                    Some(fwd) => {
                        // Forwarded event is not $> — initialize the destination
                        // with a fresh $>, then dispatch the forward.
                        let enter_event = alloc::rc::Rc::new(GoMapTypeFrameEvent::FrameEnter {});
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

        fn __router(&mut self, __e: &alloc::rc::Rc<GoMapTypeFrameEvent>) {
            let __ev: &GoMapTypeFrameEvent = __e;
            match self.__compartment.state.as_str() {
                "Active" => self._state_Active(__ev),
                _ => {}
            }
        }

        fn __transition(&mut self, next_compartment: GoMapTypeCompartment) {
            self.__next_compartment = Some(next_compartment);
        }

        pub fn map(&mut self, t: String) -> String {
            let __e = alloc::rc::Rc::new(GoMapTypeFrameEvent::Map { t: t.clone() });
            let mut __ctx = GoMapTypeFrameContext::new(alloc::rc::Rc::clone(&__e), None);
            self._context_stack.push(__ctx);
            self.__kernel(&__e);
            let __ctx = self._context_stack.pop().expect("invariant: handler must have pushed a context before reading return");
            match __ctx._return {
                Some(GoMapTypeFrameReturn::Map(v)) => v,
                Some(GoMapTypeFrameReturn::_Lifecycle(v)) => v.downcast_ref::<String>().cloned().unwrap_or_default(),
                _ => Default::default(),
            }
        }

        fn _state_Active(&mut self, __e: &GoMapTypeFrameEvent) {
            match __e {
                GoMapTypeFrameEvent::Map { t, .. } => {
                    self._s_Active_hdl_user_map(__e, t.clone());
                }
                _ => {}
            }
        }

        fn _s_Active_hdl_user_map(&mut self, __e: &GoMapTypeFrameEvent, t: String) {
                            let result = match t.as_str() {
                                "Any" | "object" | "Object" => "any".to_string(),
                                "str" | "string" | "String" => "string".to_string(),
                                "int" | "i32" | "i64" | "number" => "int".to_string(),
                                "float" | "f64" | "f32" => "float64".to_string(),
                                "bool" | "boolean" => "bool".to_string(),
                                "void" | "None" => String::new(),
                                _ => t.clone(),
                            };
            let __return_val = GoMapTypeFrameReturn::Map(result.clone());
                            if let Some(ctx) = self._context_stack.last_mut() { ctx._return = Some(__return_val); }
        }
    }
}
pub use _go_map_type_framec::*;

