
// Frame type string → Rust owned-type spelling.
// Used inside generated structs (XContext, event variants,
// return-enum variants). RFC-0033 borrowed → owned promotion
// fires here: `&str` widens to `String`, `&[T]` widens to
// `Vec<T>`. The call-boundary keeps the borrowed signature
// (see rust_owned_promotion + the dispatch site in
// rust_system.rs); this helper only fires where the value
// has to be stored owned.
//
// Note: input here is the *type-name string* (`&str` / `&[i32]`
// / `MyType`). Frame_ast::Type carries the `Type::Unknown`
// fallback, which the glue mod collapses to "" before invoking
// this mapper — i.e. unknown types arrive as an empty input
// string and we return "String" to match the prior behavior.
//
// Single match expression at the end so the body's one
// `@@:(...)` call wins. (See swift_map_type.frs for the
// observation that Frame's @@:(value) sets the return value
// rather than returning early.)

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
mod _rust_map_type_framec {
    use super::*;
    extern crate alloc;
    use alloc::{vec, format};
    #[derive(Clone, Debug)]
    #[allow(dead_code, non_camel_case_types)]
    enum RustMapTypeFrameEvent {
        Map { t: String },
        FrameEnter {},
        FrameExit {},
    }

    #[derive(Clone)]
    #[allow(dead_code, non_camel_case_types)]
    enum RustMapTypeFrameReturn {
        Map(String),
        _Lifecycle(alloc::rc::Rc<dyn core::any::Any>),
    }

    #[allow(dead_code)]
    impl RustMapTypeFrameEvent {
        fn name(&self) -> &'static str {
            match self {
                RustMapTypeFrameEvent::Map { .. } => "map",
                RustMapTypeFrameEvent::FrameEnter { .. } => "$>",
                RustMapTypeFrameEvent::FrameExit { .. } => "<$",
            }
        }
    }

    #[derive(Clone, Debug)]
    #[allow(dead_code, non_camel_case_types)]
    enum RustMapTypeFrameValue {
        Int(i64),
        Float(f64),
        Bool(bool),
        Str(String),
        List(Vec<Self>),
        Dict(alloc::collections::BTreeMap<String, Self>),
    }

    #[allow(dead_code, non_camel_case_types)]
    struct RustMapTypeFrameContext {
        event: alloc::rc::Rc<RustMapTypeFrameEvent>,
        _return: Option<RustMapTypeFrameReturn>,
        _data: alloc::collections::BTreeMap<String, RustMapTypeFrameValue>,
        _transitioned: bool,
    }

    impl RustMapTypeFrameContext {
        fn new(event: alloc::rc::Rc<RustMapTypeFrameEvent>, default_return: Option<RustMapTypeFrameReturn>) -> Self {
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
    enum RustMapTypeStateContext {
        Active,
        __NoContext,
    }

    impl Default for RustMapTypeStateContext {
        fn default() -> Self {
            RustMapTypeStateContext::Active
        }
    }

    #[allow(dead_code, non_camel_case_types)]
    #[derive(Clone)]
    struct RustMapTypeCompartment {
        state: String,
        state_context: RustMapTypeStateContext,
        forward_event: Option<RustMapTypeFrameEvent>,
        parent_compartment: Option<Box<RustMapTypeCompartment>>,
    }

    impl RustMapTypeCompartment {
        fn new(state: &str) -> Self {
            let state_context = match state {
                "Active" => RustMapTypeStateContext::Active,
                _ => RustMapTypeStateContext::__NoContext,
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
    pub struct RustMapType {
        _state_stack: Vec<RustMapTypeCompartment>,
        __compartment: RustMapTypeCompartment,
        __next_compartment: Option<RustMapTypeCompartment>,
        _context_stack: Vec<RustMapTypeFrameContext>,
    }

    #[allow(non_snake_case)]
    impl RustMapType {
        pub fn new() -> Self {
            Self {
                _state_stack: Vec::new(),
                _context_stack: Vec::new(),
                __compartment: RustMapTypeCompartment::new("Active"),
                __next_compartment: None,
            }
        }

        pub fn __create() -> Self {
            let mut c = Self::new();
            c.__compartment = c.__prepareEnter("Active");
            let __e = alloc::rc::Rc::new(RustMapTypeFrameEvent::FrameEnter {});
            let __ctx = RustMapTypeFrameContext::new(alloc::rc::Rc::clone(&__e), None);
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

        fn __prepareEnter(&mut self, leaf: &str) -> RustMapTypeCompartment {
            let chain = self.__hsm_chain(leaf);
            let mut comp: Option<RustMapTypeCompartment> = None;
            for name in chain.iter() {
                let mut new_comp = RustMapTypeCompartment::new(name);
                if let Some(parent) = comp.take() {
                    new_comp.parent_compartment = Some(Box::new(parent));
                }
                comp = Some(new_comp);
            }
            comp.expect("chain must contain at least the leaf state")
        }

        fn __kernel(&mut self, __e: &alloc::rc::Rc<RustMapTypeFrameEvent>) {
            // Route event to current state.
            self.__router(__e);
            // Drain any transitions queued by the handler.
            while self.__next_compartment.is_some() {
                let next_compartment = self.__next_compartment.take().expect("invariant: while-loop guard checked is_some()");
                // Exit the current (leaf) state. RFC-0025.1: exit args live in the
                // source state's typed ctx (written at the transition site), so the
                // synthesized `<$` event carries no payload.
                let exit_event = alloc::rc::Rc::new(RustMapTypeFrameEvent::FrameExit {});
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
                        let enter_event = alloc::rc::Rc::new(RustMapTypeFrameEvent::FrameEnter {});
                        self.__router(&enter_event);
                    }
                    Some(fwd) if matches!(fwd, RustMapTypeFrameEvent::FrameEnter { .. }) => {
                        // Forwarded event IS $> — dispatch directly so the
                        // destination's $> handler receives the caller's payload.
                        let fwd_rc = alloc::rc::Rc::new(fwd);
                        self.__router(&fwd_rc);
                    }
                    Some(fwd) => {
                        // Forwarded event is not $> — initialize the destination
                        // with a fresh $>, then dispatch the forward.
                        let enter_event = alloc::rc::Rc::new(RustMapTypeFrameEvent::FrameEnter {});
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

        fn __router(&mut self, __e: &alloc::rc::Rc<RustMapTypeFrameEvent>) {
            let __ev: &RustMapTypeFrameEvent = __e;
            match self.__compartment.state.as_str() {
                "Active" => self._state_Active(__ev),
                _ => {}
            }
        }

        fn __transition(&mut self, next_compartment: RustMapTypeCompartment) {
            self.__next_compartment = Some(next_compartment);
        }

        pub fn map(&mut self, t: String) -> String {
            let __e = alloc::rc::Rc::new(RustMapTypeFrameEvent::Map { t: t.clone() });
            let mut __ctx = RustMapTypeFrameContext::new(alloc::rc::Rc::clone(&__e), None);
            self._context_stack.push(__ctx);
            self.__kernel(&__e);
            let __ctx = self._context_stack.pop().expect("invariant: handler must have pushed a context before reading return");
            match __ctx._return {
                Some(RustMapTypeFrameReturn::Map(v)) => v,
                Some(RustMapTypeFrameReturn::_Lifecycle(v)) => v.downcast_ref::<String>().cloned().unwrap_or_default(),
                _ => Default::default(),
            }
        }

        fn _state_Active(&mut self, __e: &RustMapTypeFrameEvent) {
            match __e {
                RustMapTypeFrameEvent::Map { t, .. } => {
                    self._s_Active_hdl_user_map(__e, t.clone());
                }
                _ => {}
            }
        }

        fn _s_Active_hdl_user_map(&mut self, __e: &RustMapTypeFrameEvent, t: String) {
                            let result = if t.is_empty() {
                                "String".to_string()
                            } else if t == "&str" {
                                "String".to_string()
                            } else if t.starts_with("&[") && t.ends_with(']') {
                                let inner = &t[2..t.len() - 1];
                                format!("Vec<{}>", inner)
                            } else {
                                match t.as_str() {
                                    "int" => "i64".to_string(),
                                    "float" => "f64".to_string(),
                                    "str" | "string" | "String" => "String".to_string(),
                                    "bool" => "bool".to_string(),
                                    "Any" => "String".to_string(),
                                    other => other.to_string(),
                                }
                            };
            let __return_val = RustMapTypeFrameReturn::Map(result.clone());
                            if let Some(ctx) = self._context_stack.last_mut() { ctx._return = Some(__return_val); }
        }
    }
}
pub use _rust_map_type_framec::*;

