
// snake_case → PascalCase converter, expressed in Frame.
// Single-state system; part of RFC-0035 round 1 dogfooding.
//
// Used by the Rust target to emit enum variant names from Frame
// event names: a Frame event `get_status` becomes the variant
// `GetStatus` of the system's FrameEvent enum.
//
// Behaviour: walk the input, capitalizing the first character
// and every character following an `_`. The `_` itself is
// dropped (not emitted). Non-underscore non-position-zero
// characters pass through verbatim.
//
// Examples:
//   "get_status"    → "GetStatus"
//   "tick"          → "Tick"
//   "_leading"      → "Leading"
//   "snake_to_pascal" → "SnakeToPascal"

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
mod _pascal_case_variant_framec {
    use super::*;
    extern crate alloc;
    use alloc::{vec, format};
    #[derive(Clone, Debug)]
    #[allow(dead_code, non_camel_case_types)]
    enum PascalCaseVariantFrameEvent {
        Convert { s: String },
        FrameEnter {},
        FrameExit {},
    }

    #[derive(Clone)]
    #[allow(dead_code, non_camel_case_types)]
    enum PascalCaseVariantFrameReturn {
        Convert(String),
        _Lifecycle(alloc::rc::Rc<dyn core::any::Any>),
    }

    #[allow(dead_code)]
    impl PascalCaseVariantFrameEvent {
        fn name(&self) -> &'static str {
            match self {
                PascalCaseVariantFrameEvent::Convert { .. } => "convert",
                PascalCaseVariantFrameEvent::FrameEnter { .. } => "$>",
                PascalCaseVariantFrameEvent::FrameExit { .. } => "<$",
            }
        }
    }

    #[derive(Clone, Debug)]
    #[allow(dead_code, non_camel_case_types)]
    enum PascalCaseVariantFrameValue {
        Int(i64),
        Float(f64),
        Bool(bool),
        Str(String),
        List(Vec<Self>),
        Dict(alloc::collections::BTreeMap<String, Self>),
    }

    #[allow(dead_code, non_camel_case_types)]
    struct PascalCaseVariantFrameContext {
        event: alloc::rc::Rc<PascalCaseVariantFrameEvent>,
        _return: Option<PascalCaseVariantFrameReturn>,
        _data: alloc::collections::BTreeMap<String, PascalCaseVariantFrameValue>,
        _transitioned: bool,
    }

    impl PascalCaseVariantFrameContext {
        fn new(event: alloc::rc::Rc<PascalCaseVariantFrameEvent>, default_return: Option<PascalCaseVariantFrameReturn>) -> Self {
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
    enum PascalCaseVariantStateContext {
        Active,
        Empty,
    }

    impl Default for PascalCaseVariantStateContext {
        fn default() -> Self {
            PascalCaseVariantStateContext::Active
        }
    }

    #[allow(dead_code, non_camel_case_types)]
    #[derive(Clone)]
    struct PascalCaseVariantCompartment {
        state: String,
        state_context: PascalCaseVariantStateContext,
        forward_event: Option<PascalCaseVariantFrameEvent>,
        parent_compartment: Option<Box<PascalCaseVariantCompartment>>,
    }

    impl PascalCaseVariantCompartment {
        fn new(state: &str) -> Self {
            let state_context = match state {
                "Active" => PascalCaseVariantStateContext::Active,
                _ => PascalCaseVariantStateContext::Empty,
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
    pub struct PascalCaseVariant {
        _state_stack: Vec<PascalCaseVariantCompartment>,
        __compartment: PascalCaseVariantCompartment,
        __next_compartment: Option<PascalCaseVariantCompartment>,
        _context_stack: Vec<PascalCaseVariantFrameContext>,
    }

    #[allow(non_snake_case)]
    impl PascalCaseVariant {
        pub fn new() -> Self {
            Self {
                _state_stack: Vec::new(),
                _context_stack: Vec::new(),
                __compartment: PascalCaseVariantCompartment::new("Active"),
                __next_compartment: None,
            }
        }

        pub fn __create() -> Self {
            let mut c = Self::new();
            c.__compartment = c.__prepareEnter("Active");
            let __e = alloc::rc::Rc::new(PascalCaseVariantFrameEvent::FrameEnter {});
            let __ctx = PascalCaseVariantFrameContext::new(alloc::rc::Rc::clone(&__e), None);
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

        fn __prepareEnter(&mut self, leaf: &str) -> PascalCaseVariantCompartment {
            let chain = self.__hsm_chain(leaf);
            let mut comp: Option<PascalCaseVariantCompartment> = None;
            for name in chain.iter() {
                let mut new_comp = PascalCaseVariantCompartment::new(name);
                if let Some(parent) = comp.take() {
                    new_comp.parent_compartment = Some(Box::new(parent));
                }
                comp = Some(new_comp);
            }
            comp.expect("chain must contain at least the leaf state")
        }

        fn __kernel(&mut self, __e: &alloc::rc::Rc<PascalCaseVariantFrameEvent>) {
            // Route event to current state.
            self.__router(__e);
            // Drain any transitions queued by the handler.
            while self.__next_compartment.is_some() {
                let next_compartment = self.__next_compartment.take().expect("invariant: while-loop guard checked is_some()");
                // Exit the current (leaf) state. RFC-0025.1: exit args live in the
                // source state's typed ctx (written at the transition site), so the
                // synthesized `<$` event carries no payload.
                let exit_event = alloc::rc::Rc::new(PascalCaseVariantFrameEvent::FrameExit {});
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
                        let enter_event = alloc::rc::Rc::new(PascalCaseVariantFrameEvent::FrameEnter {});
                        self.__router(&enter_event);
                    }
                    Some(fwd) if matches!(fwd, PascalCaseVariantFrameEvent::FrameEnter { .. }) => {
                        // Forwarded event IS $> — dispatch directly so the
                        // destination's $> handler receives the caller's payload.
                        let fwd_rc = alloc::rc::Rc::new(fwd);
                        self.__router(&fwd_rc);
                    }
                    Some(fwd) => {
                        // Forwarded event is not $> — initialize the destination
                        // with a fresh $>, then dispatch the forward.
                        let enter_event = alloc::rc::Rc::new(PascalCaseVariantFrameEvent::FrameEnter {});
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

        fn __router(&mut self, __e: &alloc::rc::Rc<PascalCaseVariantFrameEvent>) {
            let __ev: &PascalCaseVariantFrameEvent = __e;
            match self.__compartment.state.as_str() {
                "Active" => self._state_Active(__ev),
                _ => {}
            }
        }

        fn __transition(&mut self, next_compartment: PascalCaseVariantCompartment) {
            self.__next_compartment = Some(next_compartment);
        }

        pub fn convert(&mut self, s: String) -> String {
            let __e = alloc::rc::Rc::new(PascalCaseVariantFrameEvent::Convert { s: s.clone() });
            let mut __ctx = PascalCaseVariantFrameContext::new(alloc::rc::Rc::clone(&__e), None);
            self._context_stack.push(__ctx);
            self.__kernel(&__e);
            let __ctx = self._context_stack.pop().expect("invariant: handler must have pushed a context before reading return");
            match __ctx._return {
                Some(PascalCaseVariantFrameReturn::Convert(v)) => v,
                Some(PascalCaseVariantFrameReturn::_Lifecycle(v)) => v.downcast_ref::<String>().cloned().unwrap_or_default(),
                _ => Default::default(),
            }
        }

        fn _state_Active(&mut self, __e: &PascalCaseVariantFrameEvent) {
            match __e {
                PascalCaseVariantFrameEvent::Convert { s, .. } => {
                    self._s_Active_hdl_user_convert(__e, s.clone());
                }
                _ => {}
            }
        }

        fn _s_Active_hdl_user_convert(&mut self, __e: &PascalCaseVariantFrameEvent, s: String) {
                            let mut result = String::new();
                            let mut capitalize_next = true;
                            for c in s.chars() {
                                if c == '_' {
                                    capitalize_next = true;
                                } else if capitalize_next {
                                    result.push(c.to_ascii_uppercase());
                                    capitalize_next = false;
                                } else {
                                    result.push(c);
                                }
                            }
            let __return_val = PascalCaseVariantFrameReturn::Convert(result.clone());
                            if let Some(ctx) = self._context_stack.last_mut() { ctx._return = Some(__return_val); }
        }
    }
}
pub use _pascal_case_variant_framec::*;

