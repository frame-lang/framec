
// CamelCase / PascalCase → snake_case converter, expressed in
// Frame. Single-state system; the work happens in one handler
// body. Part of RFC-0035 round 1 — uses framec to dogfood the
// language across as many real-world shapes as possible.
//
// Used throughout framec for Erlang naming (atom case),
// generated method/state identifier conversion, and the
// graphviz pipeline's state-name normalization.
//
// Behaviour: for each character at position i in `s`:
//   - if uppercase AND i > 0, prepend `_` to the result
//   - append the lowercase form of the character
//
// Examples:
//   "HelloWorld"   → "hello_world"
//   "getStatus"    → "get_status"
//   "ABCFlag"      → "a_b_c_flag"
//   "already_snk"  → "already_snk"

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
mod _to_snake_case_framec {
    use super::*;
    extern crate alloc;
    use alloc::{vec, format};
    #[derive(Clone, Debug)]
    #[allow(dead_code, non_camel_case_types)]
    enum ToSnakeCaseFrameEvent {
        Convert { s: String },
        FrameEnter {},
        FrameExit {},
    }

    #[derive(Clone)]
    #[allow(dead_code, non_camel_case_types)]
    enum ToSnakeCaseFrameReturn {
        Convert(String),
        _Lifecycle(alloc::rc::Rc<dyn core::any::Any>),
    }

    #[allow(dead_code)]
    impl ToSnakeCaseFrameEvent {
        fn name(&self) -> &'static str {
            match self {
                ToSnakeCaseFrameEvent::Convert { .. } => "convert",
                ToSnakeCaseFrameEvent::FrameEnter { .. } => "$>",
                ToSnakeCaseFrameEvent::FrameExit { .. } => "<$",
            }
        }
    }

    #[derive(Clone, Debug)]
    #[allow(dead_code, non_camel_case_types)]
    enum ToSnakeCaseFrameValue {
        Int(i64),
        Float(f64),
        Bool(bool),
        Str(String),
        List(Vec<Self>),
        Dict(alloc::collections::BTreeMap<String, Self>),
    }

    #[allow(dead_code, non_camel_case_types)]
    struct ToSnakeCaseFrameContext {
        event: alloc::rc::Rc<ToSnakeCaseFrameEvent>,
        _return: Option<ToSnakeCaseFrameReturn>,
        _data: alloc::collections::BTreeMap<String, ToSnakeCaseFrameValue>,
        _transitioned: bool,
    }

    impl ToSnakeCaseFrameContext {
        fn new(event: alloc::rc::Rc<ToSnakeCaseFrameEvent>, default_return: Option<ToSnakeCaseFrameReturn>) -> Self {
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
    enum ToSnakeCaseStateContext {
        Active,
        __NoContext,
    }

    impl Default for ToSnakeCaseStateContext {
        fn default() -> Self {
            ToSnakeCaseStateContext::Active
        }
    }

    #[allow(dead_code, non_camel_case_types)]
    #[derive(Clone)]
    struct ToSnakeCaseCompartment {
        state: String,
        state_context: ToSnakeCaseStateContext,
        forward_event: Option<ToSnakeCaseFrameEvent>,
        parent_compartment: Option<Box<ToSnakeCaseCompartment>>,
    }

    impl ToSnakeCaseCompartment {
        fn new(state: &str) -> Self {
            let state_context = match state {
                "Active" => ToSnakeCaseStateContext::Active,
                _ => ToSnakeCaseStateContext::__NoContext,
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
    pub struct ToSnakeCase {
        _state_stack: Vec<ToSnakeCaseCompartment>,
        __compartment: ToSnakeCaseCompartment,
        __next_compartment: Option<ToSnakeCaseCompartment>,
        _context_stack: Vec<ToSnakeCaseFrameContext>,
    }

    #[allow(non_snake_case)]
    impl ToSnakeCase {
        pub fn new() -> Self {
            Self {
                _state_stack: Vec::new(),
                _context_stack: Vec::new(),
                __compartment: ToSnakeCaseCompartment::new("Active"),
                __next_compartment: None,
            }
        }

        pub fn __create() -> Self {
            let mut c = Self::new();
            c.__compartment = c.__prepareEnter("Active");
            let __e = alloc::rc::Rc::new(ToSnakeCaseFrameEvent::FrameEnter {});
            let __ctx = ToSnakeCaseFrameContext::new(alloc::rc::Rc::clone(&__e), None);
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

        fn __prepareEnter(&mut self, leaf: &str) -> ToSnakeCaseCompartment {
            let chain = self.__hsm_chain(leaf);
            let mut comp: Option<ToSnakeCaseCompartment> = None;
            for name in chain.iter() {
                let mut new_comp = ToSnakeCaseCompartment::new(name);
                if let Some(parent) = comp.take() {
                    new_comp.parent_compartment = Some(Box::new(parent));
                }
                comp = Some(new_comp);
            }
            comp.expect("chain must contain at least the leaf state")
        }

        fn __kernel(&mut self, __e: &alloc::rc::Rc<ToSnakeCaseFrameEvent>) {
            // Route event to current state.
            self.__router(__e);
            // Drain any transitions queued by the handler.
            while self.__next_compartment.is_some() {
                let next_compartment = self.__next_compartment.take().expect("invariant: while-loop guard checked is_some()");
                // Exit the current (leaf) state. RFC-0025.1: exit args live in the
                // source state's typed ctx (written at the transition site), so the
                // synthesized `<$` event carries no payload.
                let exit_event = alloc::rc::Rc::new(ToSnakeCaseFrameEvent::FrameExit {});
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
                        let enter_event = alloc::rc::Rc::new(ToSnakeCaseFrameEvent::FrameEnter {});
                        self.__router(&enter_event);
                    }
                    Some(fwd) if matches!(fwd, ToSnakeCaseFrameEvent::FrameEnter { .. }) => {
                        // Forwarded event IS $> — dispatch directly so the
                        // destination's $> handler receives the caller's payload.
                        let fwd_rc = alloc::rc::Rc::new(fwd);
                        self.__router(&fwd_rc);
                    }
                    Some(fwd) => {
                        // Forwarded event is not $> — initialize the destination
                        // with a fresh $>, then dispatch the forward.
                        let enter_event = alloc::rc::Rc::new(ToSnakeCaseFrameEvent::FrameEnter {});
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

        fn __router(&mut self, __e: &alloc::rc::Rc<ToSnakeCaseFrameEvent>) {
            let __ev: &ToSnakeCaseFrameEvent = __e;
            match self.__compartment.state.as_str() {
                "Active" => self._state_Active(__ev),
                _ => {}
            }
        }

        fn __transition(&mut self, next_compartment: ToSnakeCaseCompartment) {
            self.__next_compartment = Some(next_compartment);
        }

        pub fn convert(&mut self, s: String) -> String {
            let __e = alloc::rc::Rc::new(ToSnakeCaseFrameEvent::Convert { s: s.clone() });
            let mut __ctx = ToSnakeCaseFrameContext::new(alloc::rc::Rc::clone(&__e), None);
            self._context_stack.push(__ctx);
            self.__kernel(&__e);
            let __ctx = self._context_stack.pop().expect("invariant: handler must have pushed a context before reading return");
            match __ctx._return {
                Some(ToSnakeCaseFrameReturn::Convert(v)) => v,
                Some(ToSnakeCaseFrameReturn::_Lifecycle(v)) => v.downcast_ref::<String>().cloned().unwrap_or_default(),
                _ => Default::default(),
            }
        }

        fn _state_Active(&mut self, __e: &ToSnakeCaseFrameEvent) {
            match __e {
                ToSnakeCaseFrameEvent::Convert { s, .. } => {
                    self._s_Active_hdl_user_convert(__e, s.clone());
                }
                _ => {}
            }
        }

        fn _s_Active_hdl_user_convert(&mut self, __e: &ToSnakeCaseFrameEvent, s: String) {
                            let mut result = String::new();
                            for (i, c) in s.chars().enumerate() {
                                if c.is_uppercase() && i > 0 {
                                    result.push('_');
                                }
                                if let Some(lc) = c.to_lowercase().next() {
                                    result.push(lc);
                                }
                            }
            let __return_val = ToSnakeCaseFrameReturn::Convert(result.clone());
                            if let Some(ctx) = self._context_stack.last_mut() { ctx._return = Some(__return_val); }
        }
    }
}
pub use _to_snake_case_framec::*;

