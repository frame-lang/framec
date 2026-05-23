
// Frame type string → Swift type spelling.
// Frame has NO type system: type names pass through VERBATIM
// (docs/frame_language.md). The name-alias table (str→String,
// int→Int, Any→Any, …) was exterminated — it contradicted the
// passthrough contract. Write Swift's own type names.
//
// What REMAINS is structural SYNTAX, not name-aliasing: Frame's
// portable nullable/array forms map to Swift's spelling — `T | nil`
// → `T?` and `T[]` → `[T]` (recursing so the inner native type is
// preserved), and the framework's `void` token → `Void` (Swift's
// no-return spelling). Everything else passes through unchanged.

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
        FrameEnter {},
        FrameExit {},
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
        __NoContext,
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
        forward_event: Option<SwiftMapTypeFrameEvent>,
        parent_compartment: Option<Box<SwiftMapTypeCompartment>>,
    }

    impl SwiftMapTypeCompartment {
        fn new(state: &str) -> Self {
            let state_context = match state {
                "Active" => SwiftMapTypeStateContext::Active,
                _ => SwiftMapTypeStateContext::__NoContext,
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
            c.__compartment = c.__prepareEnter("Active");
            let __e = alloc::rc::Rc::new(SwiftMapTypeFrameEvent::FrameEnter {});
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

        fn __prepareEnter(&mut self, leaf: &str) -> SwiftMapTypeCompartment {
            let chain = self.__hsm_chain(leaf);
            let mut comp: Option<SwiftMapTypeCompartment> = None;
            for name in chain.iter() {
                let mut new_comp = SwiftMapTypeCompartment::new(name);
                if let Some(parent) = comp.take() {
                    new_comp.parent_compartment = Some(Box::new(parent));
                }
                comp = Some(new_comp);
            }
            comp.expect("chain must contain at least the leaf state")
        }

        fn __kernel(&mut self, __e: &alloc::rc::Rc<SwiftMapTypeFrameEvent>) {
            // Route event to current state.
            self.__router(__e);
            // Drain any transitions queued by the handler.
            while self.__next_compartment.is_some() {
                let next_compartment = self.__next_compartment.take().expect("invariant: while-loop guard checked is_some()");
                // Exit the current (leaf) state. RFC-0025.1: exit args live in the
                // source state's typed ctx (written at the transition site), so the
                // synthesized `<$` event carries no payload.
                let exit_event = alloc::rc::Rc::new(SwiftMapTypeFrameEvent::FrameExit {});
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
                        let enter_event = alloc::rc::Rc::new(SwiftMapTypeFrameEvent::FrameEnter {});
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
                        let enter_event = alloc::rc::Rc::new(SwiftMapTypeFrameEvent::FrameEnter {});
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
                                } else if trimmed == "void" {
                                    "Void".to_string()
                                } else {
                                    // pipe present but not a known null sentinel — passthrough
                                    trimmed.to_string()
                                }
                            } else if let Some(base) = trimmed.strip_suffix("[]") {
                                let inner = crate::frame_c::compiler::type_map::swift_map_type(base);
                                format!("[{}]", inner)
                            } else if trimmed == "void" {
                                "Void".to_string()
                            } else {
                                trimmed.to_string()
                            };
            let __return_val = SwiftMapTypeFrameReturn::Map(result.clone());
                            if let Some(ctx) = self._context_stack.last_mut() { ctx._return = Some(__return_val); }
        }
    }
}
pub use _swift_map_type_framec::*;
