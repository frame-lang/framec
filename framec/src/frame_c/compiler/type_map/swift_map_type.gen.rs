
// Frame type string → Swift type spelling. This is the most
// interesting Frame fit in Round 2: the mapper recurses (for
// `T | nil` and `T[]` suffixes), and Frame's `@@:(value)` only
// SETS the return value — it does not return early. So the body
// has to compute one final answer through a chain of
// if-let / else and call `@@:(...)` exactly once at the end.
// Worth recording as a Frame ergonomics observation in
// RFC-0035: the natural Rust idiom (early-return on prefix
// match) doesn't translate verbatim. We work around it cleanly
// here, but the native Rust function is shorter and clearer.

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
mod _swift_map_type_framec {
    use super::*;
    extern crate alloc;
    use alloc::{vec, format};
    #[derive(Clone, Debug)]
    #[allow(dead_code, non_camel_case_types)]
    enum SwiftMapTypeFrameEvent {
        Map { t: String },
        FrameEnter { args: Vec<alloc::rc::Rc<dyn core::any::Any>> },
        FrameExit { args: Vec<alloc::rc::Rc<dyn core::any::Any>> },
    }

    #[derive(Clone)]
    #[allow(dead_code, non_camel_case_types)]
    enum SwiftMapTypeFrameReturn {
        Map(String),
        _Lifecycle(alloc::rc::Rc<dyn core::any::Any>),
    }

    #[allow(dead_code)]
    impl SwiftMapTypeFrameEvent {
        fn name(&self) -> &'static str {
            match self {
                SwiftMapTypeFrameEvent::Map { .. } => "map",
                SwiftMapTypeFrameEvent::FrameEnter { .. } => "$>",
                SwiftMapTypeFrameEvent::FrameExit { .. } => "<$",
            }
        }
    }

    #[derive(Clone, Debug)]
    #[allow(dead_code, non_camel_case_types)]
    enum SwiftMapTypeFrameValue {
        Int(i64),
        Float(f64),
        Bool(bool),
        Str(String),
        List(Vec<Self>),
        Dict(alloc::collections::BTreeMap<String, Self>),
    }

    #[allow(dead_code, non_camel_case_types)]
    struct SwiftMapTypeFrameContext {
        event: alloc::rc::Rc<SwiftMapTypeFrameEvent>,
        _return: Option<SwiftMapTypeFrameReturn>,
        _data: alloc::collections::BTreeMap<String, SwiftMapTypeFrameValue>,
        _transitioned: bool,
    }

    impl SwiftMapTypeFrameContext {
        fn new(event: alloc::rc::Rc<SwiftMapTypeFrameEvent>, default_return: Option<SwiftMapTypeFrameReturn>) -> Self {
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
    enum SwiftMapTypeStateContext {
        Active,
        Empty,
    }

    impl Default for SwiftMapTypeStateContext {
        fn default() -> Self {
            SwiftMapTypeStateContext::Active
        }
    }

    #[allow(dead_code, non_camel_case_types)]
    #[derive(Clone)]
    struct SwiftMapTypeCompartment {
        state: String,
        state_context: SwiftMapTypeStateContext,
        enter_args: Vec<alloc::rc::Rc<dyn core::any::Any>>,
        exit_args: Vec<alloc::rc::Rc<dyn core::any::Any>>,
        forward_event: Option<SwiftMapTypeFrameEvent>,
        parent_compartment: Option<Box<SwiftMapTypeCompartment>>,
    }

    impl SwiftMapTypeCompartment {
        fn new(state: &str) -> Self {
            let state_context = match state {
                "Active" => SwiftMapTypeStateContext::Active,
                _ => SwiftMapTypeStateContext::Empty,
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
    pub struct SwiftMapType {
        _state_stack: Vec<SwiftMapTypeCompartment>,
        __compartment: SwiftMapTypeCompartment,
        __next_compartment: Option<SwiftMapTypeCompartment>,
        _context_stack: Vec<SwiftMapTypeFrameContext>,
    }

    #[allow(non_snake_case)]
    impl SwiftMapType {
        pub fn new() -> Self {
            Self {
                _state_stack: Vec::new(),
                _context_stack: Vec::new(),
                __compartment: SwiftMapTypeCompartment::new("Active"),
                __next_compartment: None,
            }
        }

        pub fn __create() -> Self {
            let mut c = Self::new();
            c.__compartment = c.__prepareEnter("Active", vec![]);
            let __e = alloc::rc::Rc::new(SwiftMapTypeFrameEvent::FrameEnter { args: c.__compartment.enter_args.clone() });
            let __ctx = SwiftMapTypeFrameContext::new(alloc::rc::Rc::clone(&__e), None);
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

        fn __prepareEnter(&mut self, leaf: &str, enter_args: Vec<alloc::rc::Rc<dyn core::any::Any>>) -> SwiftMapTypeCompartment {
            let chain = self.__hsm_chain(leaf);
            let mut comp: Option<SwiftMapTypeCompartment> = None;
            for name in chain.iter() {
                let mut new_comp = SwiftMapTypeCompartment::new(name);
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

        fn __kernel(&mut self, __e: &alloc::rc::Rc<SwiftMapTypeFrameEvent>) {
            // Route event to current state.
            self.__router(__e);
            // Drain any transitions queued by the handler.
            while self.__next_compartment.is_some() {
                let next_compartment = self.__next_compartment.take().expect("invariant: while-loop guard checked is_some()");
                // Exit the current (leaf) state.
                let exit_args = self.__compartment.exit_args.clone();
                let exit_event = alloc::rc::Rc::new(SwiftMapTypeFrameEvent::FrameExit { args: exit_args });
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
                        let enter_event = alloc::rc::Rc::new(SwiftMapTypeFrameEvent::FrameEnter { args: enter_args });
                        self.__router(&enter_event);
                    }
                    Some(fwd) if matches!(fwd, SwiftMapTypeFrameEvent::FrameEnter { .. }) => {
                        // Forwarded event IS $> — dispatch directly so the
                        // destination's $> handler receives the caller's payload.
                        let fwd_rc = alloc::rc::Rc::new(fwd);
                        self.__router(&fwd_rc);
                    }
                    Some(fwd) => {
                        // Forwarded event is not $> — initialize the destination
                        // with a fresh $>, then dispatch the forward.
                        let enter_args = self.__compartment.enter_args.clone();
                        let enter_event = alloc::rc::Rc::new(SwiftMapTypeFrameEvent::FrameEnter { args: enter_args });
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

        fn __router(&mut self, __e: &alloc::rc::Rc<SwiftMapTypeFrameEvent>) {
            let __ev: &SwiftMapTypeFrameEvent = __e;
            match self.__compartment.state.as_str() {
                "Active" => self._state_Active(__ev),
                _ => {}
            }
        }

        fn __transition(&mut self, next_compartment: SwiftMapTypeCompartment) {
            self.__next_compartment = Some(next_compartment);
        }

        pub fn map(&mut self, t: String) -> String {
            let __e = alloc::rc::Rc::new(SwiftMapTypeFrameEvent::Map { t: t.clone() });
            let mut __ctx = SwiftMapTypeFrameContext::new(alloc::rc::Rc::clone(&__e), None);
            self._context_stack.push(__ctx);
            self.__kernel(&__e);
            let __ctx = self._context_stack.pop().expect("invariant: handler must have pushed a context before reading return");
            match __ctx._return {
                Some(SwiftMapTypeFrameReturn::Map(v)) => v,
                Some(SwiftMapTypeFrameReturn::_Lifecycle(v)) => v.downcast_ref::<String>().cloned().unwrap_or_default(),
                _ => Default::default(),
            }
        }

        fn _state_Active(&mut self, __e: &SwiftMapTypeFrameEvent) {
            match __e {
                SwiftMapTypeFrameEvent::Map { t, .. } => {
                    self._s_Active_hdl_user_map(__e, t.clone());
                }
                _ => {}
            }
        }

        fn _s_Active_hdl_user_map(&mut self, __e: &SwiftMapTypeFrameEvent, t: String) {
                            let trimmed = t.trim();
                            let result = if let Some(pipe_pos) = trimmed.find('|') {
                                let base = trimmed[..pipe_pos].trim();
                                let suffix = trimmed[pipe_pos + 1..].trim();
                                if suffix == "nil" || suffix == "null" || suffix == "None" {
                                    let inner = crate::frame_c::compiler::type_map::swift_map_type(base);
                                    format!("{}?", inner)
                                } else {
                                    // pipe present but not a known null sentinel — fall through
                                    match trimmed {
                                        "Any" | "Object" | "object" => "Any".to_string(),
                                        "str" | "string" | "String" => "String".to_string(),
                                        "int" | "i32" | "i64" | "number" => "Int".to_string(),
                                        "float" | "f64" | "f32" | "double" => "Double".to_string(),
                                        "bool" | "boolean" | "Boolean" => "Bool".to_string(),
                                        "void" => "Void".to_string(),
                                        other => other.to_string(),
                                    }
                                }
                            } else if let Some(base) = trimmed.strip_suffix("[]") {
                                let inner = crate::frame_c::compiler::type_map::swift_map_type(base);
                                format!("[{}]", inner)
                            } else {
                                match trimmed {
                                    "Any" | "Object" | "object" => "Any".to_string(),
                                    "str" | "string" | "String" => "String".to_string(),
                                    "int" | "i32" | "i64" | "number" => "Int".to_string(),
                                    "float" | "f64" | "f32" | "double" => "Double".to_string(),
                                    "bool" | "boolean" | "Boolean" => "Bool".to_string(),
                                    "void" => "Void".to_string(),
                                    other => other.to_string(),
                                }
                            };
            let __return_val = SwiftMapTypeFrameReturn::Map(result.clone());
                            if let Some(ctx) = self._context_stack.last_mut() { ctx._return = Some(__return_val); }
        }
    }
}
pub use _swift_map_type_framec::*;

