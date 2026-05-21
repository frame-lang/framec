
// Frame type string → C++ type spelling. Used for std::any_cast<T>
// targets and similar contexts. User-defined types (e.g.,
// std::vector<int>, custom classes) pass through verbatim.

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
mod _cpp_map_type_framec {
    use super::*;
    extern crate alloc;
    use alloc::{vec, format};
    #[derive(Clone, Debug)]
    #[allow(dead_code, non_camel_case_types)]
    enum CppMapTypeFrameEvent {
        Map { t: String },
        FrameEnter {},
        FrameExit {},
    }

    #[derive(Clone)]
    #[allow(dead_code, non_camel_case_types)]
    enum CppMapTypeFrameReturn {
        Map(String),
        _Lifecycle(alloc::rc::Rc<dyn core::any::Any>),
    }

    #[allow(dead_code)]
    impl CppMapTypeFrameEvent {
        fn name(&self) -> &'static str {
            match self {
                CppMapTypeFrameEvent::Map { .. } => "map",
                CppMapTypeFrameEvent::FrameEnter { .. } => "$>",
                CppMapTypeFrameEvent::FrameExit { .. } => "<$",
            }
        }
    }

    #[derive(Clone, Debug)]
    #[allow(dead_code, non_camel_case_types)]
    enum CppMapTypeFrameValue {
        Int(i64),
        Float(f64),
        Bool(bool),
        Str(String),
        List(Vec<Self>),
        Dict(alloc::collections::BTreeMap<String, Self>),
    }

    #[allow(dead_code, non_camel_case_types)]
    struct CppMapTypeFrameContext {
        event: alloc::rc::Rc<CppMapTypeFrameEvent>,
        _return: Option<CppMapTypeFrameReturn>,
        _data: alloc::collections::BTreeMap<String, CppMapTypeFrameValue>,
        _transitioned: bool,
    }

    impl CppMapTypeFrameContext {
        fn new(event: alloc::rc::Rc<CppMapTypeFrameEvent>, default_return: Option<CppMapTypeFrameReturn>) -> Self {
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
    enum CppMapTypeStateContext {
        Active,
        Empty,
    }

    impl Default for CppMapTypeStateContext {
        fn default() -> Self {
            CppMapTypeStateContext::Active
        }
    }

    #[allow(dead_code, non_camel_case_types)]
    #[derive(Clone)]
    struct CppMapTypeCompartment {
        state: String,
        state_context: CppMapTypeStateContext,
        forward_event: Option<CppMapTypeFrameEvent>,
        parent_compartment: Option<Box<CppMapTypeCompartment>>,
    }

    impl CppMapTypeCompartment {
        fn new(state: &str) -> Self {
            let state_context = match state {
                "Active" => CppMapTypeStateContext::Active,
                _ => CppMapTypeStateContext::Empty,
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
    pub struct CppMapType {
        _state_stack: Vec<CppMapTypeCompartment>,
        __compartment: CppMapTypeCompartment,
        __next_compartment: Option<CppMapTypeCompartment>,
        _context_stack: Vec<CppMapTypeFrameContext>,
    }

    #[allow(non_snake_case)]
    impl CppMapType {
        pub fn new() -> Self {
            Self {
                _state_stack: Vec::new(),
                _context_stack: Vec::new(),
                __compartment: CppMapTypeCompartment::new("Active"),
                __next_compartment: None,
            }
        }

        pub fn __create() -> Self {
            let mut c = Self::new();
            c.__compartment = c.__prepareEnter("Active");
            let __e = alloc::rc::Rc::new(CppMapTypeFrameEvent::FrameEnter {});
            let __ctx = CppMapTypeFrameContext::new(alloc::rc::Rc::clone(&__e), None);
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

        fn __prepareEnter(&mut self, leaf: &str) -> CppMapTypeCompartment {
            let chain = self.__hsm_chain(leaf);
            let mut comp: Option<CppMapTypeCompartment> = None;
            for name in chain.iter() {
                let mut new_comp = CppMapTypeCompartment::new(name);
                if let Some(parent) = comp.take() {
                    new_comp.parent_compartment = Some(Box::new(parent));
                }
                comp = Some(new_comp);
            }
            comp.expect("chain must contain at least the leaf state")
        }

        fn __kernel(&mut self, __e: &alloc::rc::Rc<CppMapTypeFrameEvent>) {
            // Route event to current state.
            self.__router(__e);
            // Drain any transitions queued by the handler.
            while self.__next_compartment.is_some() {
                let next_compartment = self.__next_compartment.take().expect("invariant: while-loop guard checked is_some()");
                // Exit the current (leaf) state. RFC-0025.1: exit args live in the
                // source state's typed ctx (written at the transition site), so the
                // synthesized `<$` event carries no payload.
                let exit_event = alloc::rc::Rc::new(CppMapTypeFrameEvent::FrameExit {});
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
                        let enter_event = alloc::rc::Rc::new(CppMapTypeFrameEvent::FrameEnter {});
                        self.__router(&enter_event);
                    }
                    Some(fwd) if matches!(fwd, CppMapTypeFrameEvent::FrameEnter { .. }) => {
                        // Forwarded event IS $> — dispatch directly so the
                        // destination's $> handler receives the caller's payload.
                        let fwd_rc = alloc::rc::Rc::new(fwd);
                        self.__router(&fwd_rc);
                    }
                    Some(fwd) => {
                        // Forwarded event is not $> — initialize the destination
                        // with a fresh $>, then dispatch the forward.
                        let enter_event = alloc::rc::Rc::new(CppMapTypeFrameEvent::FrameEnter {});
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

        fn __router(&mut self, __e: &alloc::rc::Rc<CppMapTypeFrameEvent>) {
            let __ev: &CppMapTypeFrameEvent = __e;
            match self.__compartment.state.as_str() {
                "Active" => self._state_Active(__ev),
                _ => {}
            }
        }

        fn __transition(&mut self, next_compartment: CppMapTypeCompartment) {
            self.__next_compartment = Some(next_compartment);
        }

        pub fn map(&mut self, t: String) -> String {
            let __e = alloc::rc::Rc::new(CppMapTypeFrameEvent::Map { t: t.clone() });
            let mut __ctx = CppMapTypeFrameContext::new(alloc::rc::Rc::clone(&__e), None);
            self._context_stack.push(__ctx);
            self.__kernel(&__e);
            let __ctx = self._context_stack.pop().expect("invariant: handler must have pushed a context before reading return");
            match __ctx._return {
                Some(CppMapTypeFrameReturn::Map(v)) => v,
                Some(CppMapTypeFrameReturn::_Lifecycle(v)) => v.downcast_ref::<String>().cloned().unwrap_or_default(),
                _ => Default::default(),
            }
        }

        fn _state_Active(&mut self, __e: &CppMapTypeFrameEvent) {
            match __e {
                CppMapTypeFrameEvent::Map { t, .. } => {
                    self._s_Active_hdl_user_map(__e, t.clone());
                }
                _ => {}
            }
        }

        fn _s_Active_hdl_user_map(&mut self, __e: &CppMapTypeFrameEvent, t: String) {
                            let result = match t.as_str() {
                                "Any" => "std::any".to_string(),
                                "str" | "string" | "String" => "std::string".to_string(),
                                "int" | "i32" | "i64" | "number" => "int".to_string(),
                                "float" | "f64" | "f32" => "double".to_string(),
                                "bool" | "boolean" => "bool".to_string(),
                                "void" => "void".to_string(),
                                _ => t.clone(),
                            };
            let __return_val = CppMapTypeFrameReturn::Map(result.clone());
                            if let Some(ctx) = self._context_stack.last_mut() { ctx._return = Some(__return_val); }
        }
    }
}
pub use _cpp_map_type_framec::*;

