
// Frame type string → Kotlin type spelling.
// Note: `Any` maps to `Any?` because Kotlin's `Any?` is the
// platform-nullable counterpart that round-trips through cast
// sites without an NPE explosion.

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
mod _kotlin_map_type_framec {
    use super::*;
    extern crate alloc;
    use alloc::{vec, format};
    #[derive(Clone, Debug)]
    #[allow(dead_code, non_camel_case_types)]
    enum KotlinMapTypeFrameEvent {
        Map { t: String },
        FrameEnter { args: Vec<alloc::rc::Rc<dyn core::any::Any>> },
        FrameExit { args: Vec<alloc::rc::Rc<dyn core::any::Any>> },
    }

    #[derive(Clone)]
    #[allow(dead_code, non_camel_case_types)]
    enum KotlinMapTypeFrameReturn {
        Map(String),
        _Lifecycle(alloc::rc::Rc<dyn core::any::Any>),
    }

    #[allow(dead_code)]
    impl KotlinMapTypeFrameEvent {
        fn name(&self) -> &'static str {
            match self {
                KotlinMapTypeFrameEvent::Map { .. } => "map",
                KotlinMapTypeFrameEvent::FrameEnter { .. } => "$>",
                KotlinMapTypeFrameEvent::FrameExit { .. } => "<$",
            }
        }
    }

    #[derive(Clone, Debug)]
    #[allow(dead_code, non_camel_case_types)]
    enum KotlinMapTypeFrameValue {
        Int(i64),
        Float(f64),
        Bool(bool),
        Str(String),
        List(Vec<Self>),
        Dict(alloc::collections::BTreeMap<String, Self>),
    }

    #[allow(dead_code, non_camel_case_types)]
    struct KotlinMapTypeFrameContext {
        event: alloc::rc::Rc<KotlinMapTypeFrameEvent>,
        _return: Option<KotlinMapTypeFrameReturn>,
        _data: alloc::collections::BTreeMap<String, KotlinMapTypeFrameValue>,
        _transitioned: bool,
    }

    impl KotlinMapTypeFrameContext {
        fn new(event: alloc::rc::Rc<KotlinMapTypeFrameEvent>, default_return: Option<KotlinMapTypeFrameReturn>) -> Self {
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
    enum KotlinMapTypeStateContext {
        Active,
        Empty,
    }

    impl Default for KotlinMapTypeStateContext {
        fn default() -> Self {
            KotlinMapTypeStateContext::Active
        }
    }

    #[allow(dead_code, non_camel_case_types)]
    #[derive(Clone)]
    struct KotlinMapTypeCompartment {
        state: String,
        state_context: KotlinMapTypeStateContext,
        enter_args: Vec<alloc::rc::Rc<dyn core::any::Any>>,
        exit_args: Vec<alloc::rc::Rc<dyn core::any::Any>>,
        forward_event: Option<KotlinMapTypeFrameEvent>,
        parent_compartment: Option<Box<KotlinMapTypeCompartment>>,
    }

    impl KotlinMapTypeCompartment {
        fn new(state: &str) -> Self {
            let state_context = match state {
                "Active" => KotlinMapTypeStateContext::Active,
                _ => KotlinMapTypeStateContext::Empty,
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
    pub struct KotlinMapType {
        _state_stack: Vec<KotlinMapTypeCompartment>,
        __compartment: KotlinMapTypeCompartment,
        __next_compartment: Option<KotlinMapTypeCompartment>,
        _context_stack: Vec<KotlinMapTypeFrameContext>,
    }

    #[allow(non_snake_case)]
    impl KotlinMapType {
        pub fn new() -> Self {
            Self {
                _state_stack: Vec::new(),
                _context_stack: Vec::new(),
                __compartment: KotlinMapTypeCompartment::new("Active"),
                __next_compartment: None,
            }
        }

        pub fn __create() -> Self {
            let mut c = Self::new();
            c.__compartment = c.__prepareEnter("Active", vec![]);
            let __e = alloc::rc::Rc::new(KotlinMapTypeFrameEvent::FrameEnter { args: c.__compartment.enter_args.clone() });
            let __ctx = KotlinMapTypeFrameContext::new(alloc::rc::Rc::clone(&__e), None);
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

        fn __prepareEnter(&mut self, leaf: &str, enter_args: Vec<alloc::rc::Rc<dyn core::any::Any>>) -> KotlinMapTypeCompartment {
            let chain = self.__hsm_chain(leaf);
            let mut comp: Option<KotlinMapTypeCompartment> = None;
            for name in chain.iter() {
                let mut new_comp = KotlinMapTypeCompartment::new(name);
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

        fn __kernel(&mut self, __e: &alloc::rc::Rc<KotlinMapTypeFrameEvent>) {
            // Route event to current state.
            self.__router(__e);
            // Drain any transitions queued by the handler.
            while self.__next_compartment.is_some() {
                let next_compartment = self.__next_compartment.take().expect("invariant: while-loop guard checked is_some()");
                // Exit the current (leaf) state.
                let exit_args = self.__compartment.exit_args.clone();
                let exit_event = alloc::rc::Rc::new(KotlinMapTypeFrameEvent::FrameExit { args: exit_args });
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
                        let enter_event = alloc::rc::Rc::new(KotlinMapTypeFrameEvent::FrameEnter { args: enter_args });
                        self.__router(&enter_event);
                    }
                    Some(fwd) if matches!(fwd, KotlinMapTypeFrameEvent::FrameEnter { .. }) => {
                        // Forwarded event IS $> — dispatch directly so the
                        // destination's $> handler receives the caller's payload.
                        let fwd_rc = alloc::rc::Rc::new(fwd);
                        self.__router(&fwd_rc);
                    }
                    Some(fwd) => {
                        // Forwarded event is not $> — initialize the destination
                        // with a fresh $>, then dispatch the forward.
                        let enter_args = self.__compartment.enter_args.clone();
                        let enter_event = alloc::rc::Rc::new(KotlinMapTypeFrameEvent::FrameEnter { args: enter_args });
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

        fn __router(&mut self, __e: &alloc::rc::Rc<KotlinMapTypeFrameEvent>) {
            let __ev: &KotlinMapTypeFrameEvent = __e;
            match self.__compartment.state.as_str() {
                "Active" => self._state_Active(__ev),
                _ => {}
            }
        }

        fn __transition(&mut self, next_compartment: KotlinMapTypeCompartment) {
            self.__next_compartment = Some(next_compartment);
        }

        pub fn map(&mut self, t: String) -> String {
            let __e = alloc::rc::Rc::new(KotlinMapTypeFrameEvent::Map { t: t.clone() });
            let mut __ctx = KotlinMapTypeFrameContext::new(alloc::rc::Rc::clone(&__e), None);
            self._context_stack.push(__ctx);
            self.__kernel(&__e);
            let __ctx = self._context_stack.pop().expect("invariant: handler must have pushed a context before reading return");
            match __ctx._return {
                Some(KotlinMapTypeFrameReturn::Map(v)) => v,
                Some(KotlinMapTypeFrameReturn::_Lifecycle(v)) => v.downcast_ref::<String>().cloned().unwrap_or_default(),
                _ => Default::default(),
            }
        }

        fn _state_Active(&mut self, __e: &KotlinMapTypeFrameEvent) {
            match __e {
                KotlinMapTypeFrameEvent::Map { t, .. } => {
                    self._s_Active_hdl_user_map(__e, t.clone());
                }
                _ => {}
            }
        }

        fn _s_Active_hdl_user_map(&mut self, __e: &KotlinMapTypeFrameEvent, t: String) {
                            let result = match t.as_str() {
                                "Any" | "Object" | "object" => "Any?".to_string(),
                                "str" | "string" | "String" => "String".to_string(),
                                "int" | "i32" | "i64" | "number" => "Int".to_string(),
                                "float" | "f64" | "f32" | "double" => "Double".to_string(),
                                "bool" | "boolean" => "Boolean".to_string(),
                                "void" => "Unit".to_string(),
                                _ => t.clone(),
                            };
            let __return_val = KotlinMapTypeFrameReturn::Map(result.clone());
                            if let Some(ctx) = self._context_stack.last_mut() { ctx._return = Some(__return_val); }
        }
    }
}
pub use _kotlin_map_type_framec::*;

