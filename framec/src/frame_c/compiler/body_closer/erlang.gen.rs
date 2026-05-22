
// Erlang body closer — Frame-generated FSM for brace matching.
// Erlang has % line comments, "..." strings, '...' quoted atoms.
// No block comments.

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
mod _erlang_body_closer_fsm_framec {
    use super::*;
    extern crate alloc;
    use alloc::{vec, format};
    #[derive(Clone, Debug)]
    #[allow(dead_code, non_camel_case_types)]
    enum ErlangBodyCloserFsmFrameEvent {
        Scan {  },
        FrameEnter {},
        FrameExit {},
    }

    #[derive(Clone)]
    #[allow(dead_code, non_camel_case_types)]
    enum ErlangBodyCloserFsmFrameReturn {
        _Lifecycle(alloc::rc::Rc<dyn core::any::Any>),
    }

    #[allow(dead_code)]
    impl ErlangBodyCloserFsmFrameEvent {
        fn name(&self) -> &'static str {
            match self {
                ErlangBodyCloserFsmFrameEvent::Scan { .. } => "scan",
                ErlangBodyCloserFsmFrameEvent::FrameEnter { .. } => "$>",
                ErlangBodyCloserFsmFrameEvent::FrameExit { .. } => "<$",
            }
        }
    }

    #[derive(Clone, Debug)]
    #[allow(dead_code, non_camel_case_types)]
    enum ErlangBodyCloserFsmFrameValue {
        Int(i64),
        Float(f64),
        Bool(bool),
        Str(String),
        List(Vec<Self>),
        Dict(alloc::collections::BTreeMap<String, Self>),
    }

    #[allow(dead_code, non_camel_case_types)]
    struct ErlangBodyCloserFsmFrameContext {
        event: alloc::rc::Rc<ErlangBodyCloserFsmFrameEvent>,
        _return: Option<ErlangBodyCloserFsmFrameReturn>,
        _data: alloc::collections::BTreeMap<String, ErlangBodyCloserFsmFrameValue>,
        _transitioned: bool,
    }

    impl ErlangBodyCloserFsmFrameContext {
        fn new(event: alloc::rc::Rc<ErlangBodyCloserFsmFrameEvent>, default_return: Option<ErlangBodyCloserFsmFrameReturn>) -> Self {
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
    enum ErlangBodyCloserFsmStateContext {
        Init,
        Scanning,
        __NoContext,
    }

    impl Default for ErlangBodyCloserFsmStateContext {
        fn default() -> Self {
            ErlangBodyCloserFsmStateContext::Init
        }
    }

    #[allow(dead_code, non_camel_case_types)]
    #[derive(Clone)]
    struct ErlangBodyCloserFsmCompartment {
        state: String,
        state_context: ErlangBodyCloserFsmStateContext,
        forward_event: Option<ErlangBodyCloserFsmFrameEvent>,
        parent_compartment: Option<Box<ErlangBodyCloserFsmCompartment>>,
    }

    impl ErlangBodyCloserFsmCompartment {
        fn new(state: &str) -> Self {
            let state_context = match state {
                "Init" => ErlangBodyCloserFsmStateContext::Init,
                "Scanning" => ErlangBodyCloserFsmStateContext::Scanning,
                _ => ErlangBodyCloserFsmStateContext::__NoContext,
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
    pub struct ErlangBodyCloserFsm {
        _state_stack: Vec<ErlangBodyCloserFsmCompartment>,
        __compartment: ErlangBodyCloserFsmCompartment,
        __next_compartment: Option<ErlangBodyCloserFsmCompartment>,
        _context_stack: Vec<ErlangBodyCloserFsmFrameContext>,
        pub bytes: Vec<u8>,
        pub pos: usize,
        pub end: usize,
        pub depth: i32,
        pub success: usize,
        pub error_kind: usize,
    }

    #[allow(non_snake_case)]
    impl ErlangBodyCloserFsm {
        pub fn new() -> Self {
            Self {
                _state_stack: Vec::new(),
                _context_stack: Vec::new(),
                bytes: Vec::new(),
                pos: 0,
                end: 0,
                depth: 1,
                success: 1,
                error_kind: 0,
                __compartment: ErlangBodyCloserFsmCompartment::new("Init"),
                __next_compartment: None,
            }
        }

        pub fn __create() -> Self {
            let mut c = Self::new();
            c.__compartment = c.__prepareEnter("Init");
            let __e = alloc::rc::Rc::new(ErlangBodyCloserFsmFrameEvent::FrameEnter {});
            let __ctx = ErlangBodyCloserFsmFrameContext::new(alloc::rc::Rc::clone(&__e), None);
            c._context_stack.push(__ctx);
            c.__kernel(&__e);
            c._context_stack.pop();
            c
        }

        fn __hsm_chain(&mut self, leaf: &str) -> &'static [&'static str] {
            match leaf {
                "Init" => &["Init"],
                "Scanning" => &["Scanning"],
                _ => &[],
            }
        }

        fn __prepareEnter(&mut self, leaf: &str) -> ErlangBodyCloserFsmCompartment {
            let chain = self.__hsm_chain(leaf);
            let mut comp: Option<ErlangBodyCloserFsmCompartment> = None;
            for name in chain.iter() {
                let mut new_comp = ErlangBodyCloserFsmCompartment::new(name);
                if let Some(parent) = comp.take() {
                    new_comp.parent_compartment = Some(Box::new(parent));
                }
                comp = Some(new_comp);
            }
            comp.expect("chain must contain at least the leaf state")
        }

        fn __kernel(&mut self, __e: &alloc::rc::Rc<ErlangBodyCloserFsmFrameEvent>) {
            // Route event to current state.
            self.__router(__e);
            // Drain any transitions queued by the handler.
            while self.__next_compartment.is_some() {
                let next_compartment = self.__next_compartment.take().expect("invariant: while-loop guard checked is_some()");
                // Exit the current (leaf) state. RFC-0025.1: exit args live in the
                // source state's typed ctx (written at the transition site), so the
                // synthesized `<$` event carries no payload.
                let exit_event = alloc::rc::Rc::new(ErlangBodyCloserFsmFrameEvent::FrameExit {});
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
                        let enter_event = alloc::rc::Rc::new(ErlangBodyCloserFsmFrameEvent::FrameEnter {});
                        self.__router(&enter_event);
                    }
                    Some(fwd) if matches!(fwd, ErlangBodyCloserFsmFrameEvent::FrameEnter { .. }) => {
                        // Forwarded event IS $> — dispatch directly so the
                        // destination's $> handler receives the caller's payload.
                        let fwd_rc = alloc::rc::Rc::new(fwd);
                        self.__router(&fwd_rc);
                    }
                    Some(fwd) => {
                        // Forwarded event is not $> — initialize the destination
                        // with a fresh $>, then dispatch the forward.
                        let enter_event = alloc::rc::Rc::new(ErlangBodyCloserFsmFrameEvent::FrameEnter {});
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

        fn __router(&mut self, __e: &alloc::rc::Rc<ErlangBodyCloserFsmFrameEvent>) {
            let __ev: &ErlangBodyCloserFsmFrameEvent = __e;
            match self.__compartment.state.as_str() {
                "Init" => self._state_Init(__ev),
                "Scanning" => self._state_Scanning(__ev),
                _ => {}
            }
        }

        fn __transition(&mut self, next_compartment: ErlangBodyCloserFsmCompartment) {
            self.__next_compartment = Some(next_compartment);
        }

        pub fn scan(&mut self) {
            let __e = alloc::rc::Rc::new(ErlangBodyCloserFsmFrameEvent::Scan {});
            let mut __ctx = ErlangBodyCloserFsmFrameContext::new(alloc::rc::Rc::clone(&__e), None);
            self._context_stack.push(__ctx);
            self.__kernel(&__e);
            self._context_stack.pop();
        }

        fn _state_Init(&mut self, __e: &ErlangBodyCloserFsmFrameEvent) {
            match __e {
                ErlangBodyCloserFsmFrameEvent::Scan { .. } => { self._s_Init_hdl_user_scan(__e); }
                _ => {}
            }
        }

        fn _state_Scanning(&mut self, __e: &ErlangBodyCloserFsmFrameEvent) {
            match __e {
                ErlangBodyCloserFsmFrameEvent::FrameEnter { .. } => { self._s_Scanning_hdl_frame_enter(__e); }
                _ => {}
            }
        }

        fn _s_Init_hdl_user_scan(&mut self, __e: &ErlangBodyCloserFsmFrameEvent) {
            let mut __compartment = self.__prepareEnter("Scanning");
            self.__transition(__compartment);
            return;
        }

        fn _s_Scanning_hdl_frame_enter(&mut self, __e: &ErlangBodyCloserFsmFrameEvent) {
            let bytes = &self.bytes;
            let end = self.end;
            let mut i = self.pos;
            let mut depth = self.depth;
            
            while i < end {
                let b = bytes[i];
                match b {
                    b'{' => { depth += 1; i += 1; }
                    b'}' => {
                        depth -= 1;
                        if depth == 0 {
                            self.pos = i;
                            self.depth = 0;
                            self.success = 1;
                            return
                        }
                        i += 1;
                    }
                    b'%' => {
                        // Line comment — skip to newline
                        i += 1;
                        while i < end && bytes[i] != b'\n' {
                            i += 1;
                        }
                    }
                    b'"' => {
                        // String — skip to closing quote
                        i += 1;
                        while i < end {
                            if bytes[i] == b'\\' { i += 2; continue; }
                            if bytes[i] == b'"' { i += 1; break; }
                            i += 1;
                        }
                    }
                    b'\'' => {
                        // Quoted atom — skip to closing quote
                        i += 1;
                        while i < end {
                            if bytes[i] == b'\\' { i += 2; continue; }
                            if bytes[i] == b'\'' { i += 1; break; }
                            i += 1;
                        }
                    }
                    _ => { i += 1; }
                }
            }
            // Reached end without matching — unmatched brace
            self.pos = i;
            self.depth = depth;
            self.error_kind = 3;
            self.success = 0;
        }
    }
}
pub use _erlang_body_closer_fsm_framec::*;

