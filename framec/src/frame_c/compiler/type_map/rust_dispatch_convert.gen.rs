
// RFC-0033 helper: how to convert a system-method's
// borrowed-or-owned param value into the owned form the event
// variant holds. Returns one of the three suffix tokens:
//
//   ".to_string()"  — for &str  (variant holds String)
//   ".to_vec()"     — for &[T]  (variant holds Vec<T>)
//   ".clone()"      — for everything else (no-op on Copy types,
//                     real clone on owned ones)
//
// Single result binding so the body's one `@@:(...)` call wins
// (Frame's @@:(value) sets the return — it does not return
// early).

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
mod _rust_dispatch_convert_framec {
    use super::*;
    extern crate alloc;
    use alloc::{vec, format};
    #[derive(Clone, Debug)]
    #[allow(dead_code, non_camel_case_types)]
    enum RustDispatchConvertFrameEvent {
        Suffix { t: String },
        FrameEnter { args: Vec<String> },
        FrameExit { args: Vec<String> },
    }

    #[derive(Clone)]
    #[allow(dead_code, non_camel_case_types)]
    enum RustDispatchConvertFrameReturn {
        Suffix(String),
        _Lifecycle(alloc::rc::Rc<dyn core::any::Any>),
    }

    #[allow(dead_code)]
    impl RustDispatchConvertFrameEvent {
        fn name(&self) -> &'static str {
            match self {
                RustDispatchConvertFrameEvent::Suffix { .. } => "suffix",
                RustDispatchConvertFrameEvent::FrameEnter { .. } => "$>",
                RustDispatchConvertFrameEvent::FrameExit { .. } => "<$",
            }
        }
    }

    #[derive(Clone, Debug)]
    #[allow(dead_code, non_camel_case_types)]
    enum RustDispatchConvertFrameValue {
        Int(i64),
        Float(f64),
        Bool(bool),
        Str(String),
        List(Vec<Self>),
        Dict(alloc::collections::BTreeMap<String, Self>),
    }

    #[allow(dead_code, non_camel_case_types)]
    struct RustDispatchConvertFrameContext {
        event: alloc::rc::Rc<RustDispatchConvertFrameEvent>,
        _return: Option<RustDispatchConvertFrameReturn>,
        _data: alloc::collections::BTreeMap<String, RustDispatchConvertFrameValue>,
        _transitioned: bool,
    }

    impl RustDispatchConvertFrameContext {
        fn new(event: alloc::rc::Rc<RustDispatchConvertFrameEvent>, default_return: Option<RustDispatchConvertFrameReturn>) -> Self {
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
    enum RustDispatchConvertStateContext {
        Active,
        Empty,
    }

    impl Default for RustDispatchConvertStateContext {
        fn default() -> Self {
            RustDispatchConvertStateContext::Active
        }
    }

    #[allow(dead_code, non_camel_case_types)]
    #[derive(Clone)]
    struct RustDispatchConvertCompartment {
        state: String,
        state_context: RustDispatchConvertStateContext,
        enter_args: Vec<String>,
        exit_args: Vec<String>,
        forward_event: Option<RustDispatchConvertFrameEvent>,
        parent_compartment: Option<Box<RustDispatchConvertCompartment>>,
    }

    impl RustDispatchConvertCompartment {
        fn new(state: &str) -> Self {
            let state_context = match state {
                "Active" => RustDispatchConvertStateContext::Active,
                _ => RustDispatchConvertStateContext::Empty,
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
    pub struct RustDispatchConvert {
        _state_stack: Vec<RustDispatchConvertCompartment>,
        __compartment: RustDispatchConvertCompartment,
        __next_compartment: Option<RustDispatchConvertCompartment>,
        _context_stack: Vec<RustDispatchConvertFrameContext>,
    }

    #[allow(non_snake_case)]
    impl RustDispatchConvert {
        pub fn new() -> Self {
            Self {
                _state_stack: Vec::new(),
                _context_stack: Vec::new(),
                __compartment: RustDispatchConvertCompartment::new("Active"),
                __next_compartment: None,
            }
        }

        pub fn __create() -> Self {
            let mut c = Self::new();
            c.__compartment = c.__prepareEnter("Active", vec![]);
            let __e = alloc::rc::Rc::new(RustDispatchConvertFrameEvent::FrameEnter { args: c.__compartment.enter_args.clone() });
            let __ctx = RustDispatchConvertFrameContext::new(alloc::rc::Rc::clone(&__e), None);
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

        fn __prepareEnter(&mut self, leaf: &str, enter_args: Vec<String>) -> RustDispatchConvertCompartment {
            let chain = self.__hsm_chain(leaf);
            let mut comp: Option<RustDispatchConvertCompartment> = None;
            for name in chain.iter() {
                let mut new_comp = RustDispatchConvertCompartment::new(name);
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

        fn __kernel(&mut self, __e: &alloc::rc::Rc<RustDispatchConvertFrameEvent>) {
            // Route event to current state.
            self.__router(__e);
            // Drain any transitions queued by the handler.
            while self.__next_compartment.is_some() {
                let next_compartment = self.__next_compartment.take().expect("invariant: while-loop guard checked is_some()");
                // Exit the current (leaf) state.
                let exit_args = self.__compartment.exit_args.clone();
                let exit_event = alloc::rc::Rc::new(RustDispatchConvertFrameEvent::FrameExit { args: exit_args });
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
                        let enter_event = alloc::rc::Rc::new(RustDispatchConvertFrameEvent::FrameEnter { args: enter_args });
                        self.__router(&enter_event);
                    }
                    Some(fwd) if matches!(fwd, RustDispatchConvertFrameEvent::FrameEnter { .. }) => {
                        // Forwarded event IS $> — dispatch directly so the
                        // destination's $> handler receives the caller's payload.
                        let fwd_rc = alloc::rc::Rc::new(fwd);
                        self.__router(&fwd_rc);
                    }
                    Some(fwd) => {
                        // Forwarded event is not $> — initialize the destination
                        // with a fresh $>, then dispatch the forward.
                        let enter_args = self.__compartment.enter_args.clone();
                        let enter_event = alloc::rc::Rc::new(RustDispatchConvertFrameEvent::FrameEnter { args: enter_args });
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

        fn __router(&mut self, __e: &alloc::rc::Rc<RustDispatchConvertFrameEvent>) {
            let __ev: &RustDispatchConvertFrameEvent = __e;
            match self.__compartment.state.as_str() {
                "Active" => self._state_Active(__ev),
                _ => {}
            }
        }

        fn __transition(&mut self, next_compartment: RustDispatchConvertCompartment) {
            self.__next_compartment = Some(next_compartment);
        }

        pub fn suffix(&mut self, t: String) -> String {
            let __e = alloc::rc::Rc::new(RustDispatchConvertFrameEvent::Suffix { t: t.clone() });
            let mut __ctx = RustDispatchConvertFrameContext::new(alloc::rc::Rc::clone(&__e), None);
            self._context_stack.push(__ctx);
            self.__kernel(&__e);
            let __ctx = self._context_stack.pop().expect("invariant: handler must have pushed a context before reading return");
            match __ctx._return {
                Some(RustDispatchConvertFrameReturn::Suffix(v)) => v,
                Some(RustDispatchConvertFrameReturn::_Lifecycle(v)) => v.downcast_ref::<String>().cloned().unwrap_or_default(),
                _ => Default::default(),
            }
        }

        fn _state_Active(&mut self, __e: &RustDispatchConvertFrameEvent) {
            match __e {
                RustDispatchConvertFrameEvent::Suffix { t, .. } => {
                    self._s_Active_hdl_user_suffix(__e, t.clone());
                }
                _ => {}
            }
        }

        fn _s_Active_hdl_user_suffix(&mut self, __e: &RustDispatchConvertFrameEvent, t: String) {
                            let result = if t == "&str" {
                                ".to_string()".to_string()
                            } else if t.starts_with("&[") && t.ends_with(']') {
                                ".to_vec()".to_string()
                            } else {
                                ".clone()".to_string()
                            };
            let __return_val = RustDispatchConvertFrameReturn::Suffix(result.clone());
                            if let Some(ctx) = self._context_stack.last_mut() { ctx._return = Some(__return_val); }
        }
    }
}
pub use _rust_dispatch_convert_framec::*;

