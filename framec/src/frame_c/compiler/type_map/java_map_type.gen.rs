
// Frame type string → Java type spelling.
// Used by framec's Java backend for cast/return-type contexts.
// Part of RFC-0035 round 2 — see csharp_map_type.frs.

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
mod _java_map_type_framec {
    use super::*;
    #[derive(Clone, Debug)]
    #[allow(dead_code, non_camel_case_types)]
    enum JavaMapTypeFrameEvent {
        Map { t: String },
        FrameEnter { args: Vec<String> },
        FrameExit { args: Vec<String> },
    }

    #[derive(Clone)]
    #[allow(dead_code, non_camel_case_types)]
    enum JavaMapTypeFrameReturn {
        Map(String),
        _Lifecycle(std::rc::Rc<dyn std::any::Any>),
    }

    #[allow(dead_code)]
    impl JavaMapTypeFrameEvent {
        fn name(&self) -> &'static str {
            match self {
                JavaMapTypeFrameEvent::Map { .. } => "map",
                JavaMapTypeFrameEvent::FrameEnter { .. } => "$>",
                JavaMapTypeFrameEvent::FrameExit { .. } => "<$",
            }
        }
    }

    #[derive(Clone, Debug)]
    #[allow(dead_code, non_camel_case_types)]
    enum JavaMapTypeFrameValue {
        Int(i64),
        Float(f64),
        Bool(bool),
        Str(String),
        List(Vec<Self>),
        Dict(std::collections::HashMap<String, Self>),
    }

    #[allow(dead_code, non_camel_case_types)]
    struct JavaMapTypeFrameContext {
        event: std::rc::Rc<JavaMapTypeFrameEvent>,
        _return: Option<JavaMapTypeFrameReturn>,
        _data: std::collections::HashMap<String, JavaMapTypeFrameValue>,
        _transitioned: bool,
    }

    impl JavaMapTypeFrameContext {
        fn new(event: std::rc::Rc<JavaMapTypeFrameEvent>, default_return: Option<JavaMapTypeFrameReturn>) -> Self {
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
    enum JavaMapTypeStateContext {
        Active,
        Empty,
    }

    impl Default for JavaMapTypeStateContext {
        fn default() -> Self {
            JavaMapTypeStateContext::Active
        }
    }

    #[allow(dead_code, non_camel_case_types)]
    #[derive(Clone)]
    struct JavaMapTypeCompartment {
        state: String,
        state_context: JavaMapTypeStateContext,
        enter_args: Vec<String>,
        exit_args: Vec<String>,
        forward_event: Option<JavaMapTypeFrameEvent>,
        parent_compartment: Option<Box<JavaMapTypeCompartment>>,
    }

    impl JavaMapTypeCompartment {
        fn new(state: &str) -> Self {
            let state_context = match state {
                "Active" => JavaMapTypeStateContext::Active,
                _ => JavaMapTypeStateContext::Empty,
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
    pub struct JavaMapType {
        _state_stack: Vec<JavaMapTypeCompartment>,
        __compartment: JavaMapTypeCompartment,
        __next_compartment: Option<JavaMapTypeCompartment>,
        _context_stack: Vec<JavaMapTypeFrameContext>,
    }

    #[allow(non_snake_case)]
    impl JavaMapType {
        pub fn new() -> Self {
            Self {
                _state_stack: Vec::new(),
                _context_stack: Vec::new(),
                __compartment: JavaMapTypeCompartment::new("Active"),
                __next_compartment: None,
            }
        }

        pub fn __create() -> Self {
            let mut c = Self::new();
            c.__compartment = c.__prepareEnter("Active", vec![]);
            let __e = std::rc::Rc::new(JavaMapTypeFrameEvent::FrameEnter { args: c.__compartment.enter_args.clone() });
            let __ctx = JavaMapTypeFrameContext::new(std::rc::Rc::clone(&__e), None);
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

        fn __prepareEnter(&mut self, leaf: &str, enter_args: Vec<String>) -> JavaMapTypeCompartment {
            let chain = self.__hsm_chain(leaf);
            let mut comp: Option<JavaMapTypeCompartment> = None;
            for name in chain.iter() {
                let mut new_comp = JavaMapTypeCompartment::new(name);
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

        fn __kernel(&mut self, __e: &std::rc::Rc<JavaMapTypeFrameEvent>) {
            // Route event to current state.
            self.__router(__e);
            // Drain any transitions queued by the handler.
            while self.__next_compartment.is_some() {
                let next_compartment = self.__next_compartment.take().expect("invariant: while-loop guard checked is_some()");
                // Exit the current (leaf) state.
                let exit_args = self.__compartment.exit_args.clone();
                let exit_event = std::rc::Rc::new(JavaMapTypeFrameEvent::FrameExit { args: exit_args });
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
                        let enter_event = std::rc::Rc::new(JavaMapTypeFrameEvent::FrameEnter { args: enter_args });
                        self.__router(&enter_event);
                    }
                    Some(fwd) if matches!(fwd, JavaMapTypeFrameEvent::FrameEnter { .. }) => {
                        // Forwarded event IS $> — dispatch directly so the
                        // destination's $> handler receives the caller's payload.
                        let fwd_rc = std::rc::Rc::new(fwd);
                        self.__router(&fwd_rc);
                    }
                    Some(fwd) => {
                        // Forwarded event is not $> — initialize the destination
                        // with a fresh $>, then dispatch the forward.
                        let enter_args = self.__compartment.enter_args.clone();
                        let enter_event = std::rc::Rc::new(JavaMapTypeFrameEvent::FrameEnter { args: enter_args });
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

        fn __router(&mut self, __e: &std::rc::Rc<JavaMapTypeFrameEvent>) {
            let __ev: &JavaMapTypeFrameEvent = __e;
            match self.__compartment.state.as_str() {
                "Active" => self._state_Active(__ev),
                _ => {}
            }
        }

        fn __transition(&mut self, next_compartment: JavaMapTypeCompartment) {
            self.__next_compartment = Some(next_compartment);
        }

        pub fn map(&mut self, t: String) -> String {
            let __e = std::rc::Rc::new(JavaMapTypeFrameEvent::Map { t: t.clone() });
            let mut __ctx = JavaMapTypeFrameContext::new(std::rc::Rc::clone(&__e), None);
            self._context_stack.push(__ctx);
            self.__kernel(&__e);
            let __ctx = self._context_stack.pop().expect("invariant: handler must have pushed a context before reading return");
            match __ctx._return {
                Some(JavaMapTypeFrameReturn::Map(v)) => v,
                Some(JavaMapTypeFrameReturn::_Lifecycle(v)) => v.downcast_ref::<String>().cloned().unwrap_or_default(),
                _ => Default::default(),
            }
        }

        fn _state_Active(&mut self, __e: &JavaMapTypeFrameEvent) {
            match __e {
                JavaMapTypeFrameEvent::Map { t, .. } => {
                    self._s_Active_hdl_user_map(__e, t.clone());
                }
                _ => {}
            }
        }

        fn _s_Active_hdl_user_map(&mut self, __e: &JavaMapTypeFrameEvent, t: String) {
                            let result = match t.as_str() {
                                "Any" => "Object".to_string(),
                                "str" | "string" | "String" => "String".to_string(),
                                "int" | "i32" | "i64" | "number" => "int".to_string(),
                                "float" | "f64" | "f32" => "double".to_string(),
                                "bool" | "boolean" => "boolean".to_string(),
                                "void" => "void".to_string(),
                                _ => t.clone(),
                            };
            let __return_val = JavaMapTypeFrameReturn::Map(result.clone());
                            if let Some(ctx) = self._context_stack.last_mut() { ctx._return = Some(__return_val); }
        }
    }
}
pub use _java_map_type_framec::*;

