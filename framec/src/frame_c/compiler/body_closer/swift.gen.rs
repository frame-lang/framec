
// Dogfooded body closer — Swift language brace matcher.
// Swift has: //, /* */ (nestable!), "...", """...""" multi-line strings, char literals in strings.
// String interpolation \(expr) can contain braces — handled by tracking depth within strings.
//
// State machine flow:
//   $Init.scan() → $Scanning.$>() ↔ $InString/$InRawString/$InLineComment/$InBlockComment

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
mod _swift_body_closer_fsm_framec {
    use super::*;
    extern crate alloc;
    use alloc::{vec, format};
    #[derive(Clone, Debug)]
    #[allow(dead_code, non_camel_case_types)]
    enum SwiftBodyCloserFsmFrameEvent {
        Scan {  },
        FrameEnter {},
        FrameExit {},
    }

    #[derive(Clone)]
    #[allow(dead_code, non_camel_case_types)]
    enum SwiftBodyCloserFsmFrameReturn {
        _Lifecycle(alloc::rc::Rc<dyn core::any::Any>),
    }

    #[allow(dead_code)]
    impl SwiftBodyCloserFsmFrameEvent {
        fn name(&self) -> &'static str {
            match self {
                SwiftBodyCloserFsmFrameEvent::Scan { .. } => "scan",
                SwiftBodyCloserFsmFrameEvent::FrameEnter { .. } => "$>",
                SwiftBodyCloserFsmFrameEvent::FrameExit { .. } => "<$",
            }
        }
    }

    #[derive(Clone, Debug)]
    #[allow(dead_code, non_camel_case_types)]
    enum SwiftBodyCloserFsmFrameValue {
        Int(i64),
        Float(f64),
        Bool(bool),
        Str(String),
        List(Vec<Self>),
        Dict(alloc::collections::BTreeMap<String, Self>),
    }

    #[allow(dead_code, non_camel_case_types)]
    struct SwiftBodyCloserFsmFrameContext {
        event: alloc::rc::Rc<SwiftBodyCloserFsmFrameEvent>,
        _return: Option<SwiftBodyCloserFsmFrameReturn>,
        _data: alloc::collections::BTreeMap<String, SwiftBodyCloserFsmFrameValue>,
        _transitioned: bool,
    }

    impl SwiftBodyCloserFsmFrameContext {
        fn new(event: alloc::rc::Rc<SwiftBodyCloserFsmFrameEvent>, default_return: Option<SwiftBodyCloserFsmFrameReturn>) -> Self {
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
    enum SwiftBodyCloserFsmStateContext {
        Init,
        Scanning,
        InString,
        InRawString,
        InLineComment,
        InBlockComment,
        Empty,
    }

    impl Default for SwiftBodyCloserFsmStateContext {
        fn default() -> Self {
            SwiftBodyCloserFsmStateContext::Init
        }
    }

    #[allow(dead_code, non_camel_case_types)]
    #[derive(Clone)]
    struct SwiftBodyCloserFsmCompartment {
        state: String,
        state_context: SwiftBodyCloserFsmStateContext,
        forward_event: Option<SwiftBodyCloserFsmFrameEvent>,
        parent_compartment: Option<Box<SwiftBodyCloserFsmCompartment>>,
    }

    impl SwiftBodyCloserFsmCompartment {
        fn new(state: &str) -> Self {
            let state_context = match state {
                "Init" => SwiftBodyCloserFsmStateContext::Init,
                "Scanning" => SwiftBodyCloserFsmStateContext::Scanning,
                "InString" => SwiftBodyCloserFsmStateContext::InString,
                "InRawString" => SwiftBodyCloserFsmStateContext::InRawString,
                "InLineComment" => SwiftBodyCloserFsmStateContext::InLineComment,
                "InBlockComment" => SwiftBodyCloserFsmStateContext::InBlockComment,
                _ => SwiftBodyCloserFsmStateContext::Empty,
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
    pub struct SwiftBodyCloserFsm {
        _state_stack: Vec<SwiftBodyCloserFsmCompartment>,
        __compartment: SwiftBodyCloserFsmCompartment,
        __next_compartment: Option<SwiftBodyCloserFsmCompartment>,
        _context_stack: Vec<SwiftBodyCloserFsmFrameContext>,
        pub bytes: Vec<u8>,
        pub pos: usize,
        pub depth: i32,
        pub comment_depth: i32,
        pub result_pos: usize,
        pub error_kind: usize,
        pub error_msg: String,
    }

    #[allow(non_snake_case)]
    impl SwiftBodyCloserFsm {
        pub fn new() -> Self {
            Self {
                _state_stack: Vec::new(),
                _context_stack: Vec::new(),
                bytes: Vec::new(),
                pos: 0,
                depth: 1,
                comment_depth: 0,
                result_pos: 0,
                error_kind: 0,
                error_msg: String::new(),
                __compartment: SwiftBodyCloserFsmCompartment::new("Init"),
                __next_compartment: None,
            }
        }

        pub fn __create() -> Self {
            let mut c = Self::new();
            c.__compartment = c.__prepareEnter("Init");
            let __e = alloc::rc::Rc::new(SwiftBodyCloserFsmFrameEvent::FrameEnter {});
            let __ctx = SwiftBodyCloserFsmFrameContext::new(alloc::rc::Rc::clone(&__e), None);
            c._context_stack.push(__ctx);
            c.__kernel(&__e);
            c._context_stack.pop();
            c
        }

        fn __hsm_chain(&mut self, leaf: &str) -> &'static [&'static str] {
            match leaf {
                "Init" => &["Init"],
                "Scanning" => &["Scanning"],
                "InString" => &["InString"],
                "InRawString" => &["InRawString"],
                "InLineComment" => &["InLineComment"],
                "InBlockComment" => &["InBlockComment"],
                _ => &[],
            }
        }

        fn __prepareEnter(&mut self, leaf: &str) -> SwiftBodyCloserFsmCompartment {
            let chain = self.__hsm_chain(leaf);
            let mut comp: Option<SwiftBodyCloserFsmCompartment> = None;
            for name in chain.iter() {
                let mut new_comp = SwiftBodyCloserFsmCompartment::new(name);
                if let Some(parent) = comp.take() {
                    new_comp.parent_compartment = Some(Box::new(parent));
                }
                comp = Some(new_comp);
            }
            comp.expect("chain must contain at least the leaf state")
        }

        fn __kernel(&mut self, __e: &alloc::rc::Rc<SwiftBodyCloserFsmFrameEvent>) {
            // Route event to current state.
            self.__router(__e);
            // Drain any transitions queued by the handler.
            while self.__next_compartment.is_some() {
                let next_compartment = self.__next_compartment.take().expect("invariant: while-loop guard checked is_some()");
                // Exit the current (leaf) state. RFC-0025.1: exit args live in the
                // source state's typed ctx (written at the transition site), so the
                // synthesized `<$` event carries no payload.
                let exit_event = alloc::rc::Rc::new(SwiftBodyCloserFsmFrameEvent::FrameExit {});
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
                        let enter_event = alloc::rc::Rc::new(SwiftBodyCloserFsmFrameEvent::FrameEnter {});
                        self.__router(&enter_event);
                    }
                    Some(fwd) if matches!(fwd, SwiftBodyCloserFsmFrameEvent::FrameEnter { .. }) => {
                        // Forwarded event IS $> — dispatch directly so the
                        // destination's $> handler receives the caller's payload.
                        let fwd_rc = alloc::rc::Rc::new(fwd);
                        self.__router(&fwd_rc);
                    }
                    Some(fwd) => {
                        // Forwarded event is not $> — initialize the destination
                        // with a fresh $>, then dispatch the forward.
                        let enter_event = alloc::rc::Rc::new(SwiftBodyCloserFsmFrameEvent::FrameEnter {});
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

        fn __router(&mut self, __e: &alloc::rc::Rc<SwiftBodyCloserFsmFrameEvent>) {
            let __ev: &SwiftBodyCloserFsmFrameEvent = __e;
            match self.__compartment.state.as_str() {
                "Init" => self._state_Init(__ev),
                "Scanning" => self._state_Scanning(__ev),
                "InString" => self._state_InString(__ev),
                "InRawString" => self._state_InRawString(__ev),
                "InLineComment" => self._state_InLineComment(__ev),
                "InBlockComment" => self._state_InBlockComment(__ev),
                _ => {}
            }
        }

        fn __transition(&mut self, next_compartment: SwiftBodyCloserFsmCompartment) {
            self.__next_compartment = Some(next_compartment);
        }

        pub fn scan(&mut self) {
            let __e = alloc::rc::Rc::new(SwiftBodyCloserFsmFrameEvent::Scan {});
            let mut __ctx = SwiftBodyCloserFsmFrameContext::new(alloc::rc::Rc::clone(&__e), None);
            self._context_stack.push(__ctx);
            self.__kernel(&__e);
            self._context_stack.pop();
        }

        fn _state_Init(&mut self, __e: &SwiftBodyCloserFsmFrameEvent) {
            match __e {
                SwiftBodyCloserFsmFrameEvent::Scan { .. } => { self._s_Init_hdl_user_scan(__e); }
                _ => {}
            }
        }

        fn _state_Scanning(&mut self, __e: &SwiftBodyCloserFsmFrameEvent) {
            match __e {
                SwiftBodyCloserFsmFrameEvent::FrameEnter { .. } => { self._s_Scanning_hdl_frame_enter(__e); }
                _ => {}
            }
        }

        fn _state_InString(&mut self, __e: &SwiftBodyCloserFsmFrameEvent) {
            match __e {
                SwiftBodyCloserFsmFrameEvent::FrameEnter { .. } => { self._s_InString_hdl_frame_enter(__e); }
                _ => {}
            }
        }

        fn _state_InRawString(&mut self, __e: &SwiftBodyCloserFsmFrameEvent) {
            match __e {
                SwiftBodyCloserFsmFrameEvent::FrameEnter { .. } => { self._s_InRawString_hdl_frame_enter(__e); }
                _ => {}
            }
        }

        fn _state_InLineComment(&mut self, __e: &SwiftBodyCloserFsmFrameEvent) {
            match __e {
                SwiftBodyCloserFsmFrameEvent::FrameEnter { .. } => { self._s_InLineComment_hdl_frame_enter(__e); }
                _ => {}
            }
        }

        fn _state_InBlockComment(&mut self, __e: &SwiftBodyCloserFsmFrameEvent) {
            match __e {
                SwiftBodyCloserFsmFrameEvent::FrameEnter { .. } => { self._s_InBlockComment_hdl_frame_enter(__e); }
                _ => {}
            }
        }

        fn _s_Init_hdl_user_scan(&mut self, __e: &SwiftBodyCloserFsmFrameEvent) {
            let mut __compartment = self.__prepareEnter("Scanning");
            self.__transition(__compartment);
            return;
        }

        fn _s_Scanning_hdl_frame_enter(&mut self, __e: &SwiftBodyCloserFsmFrameEvent) {
            let n = self.bytes.len();
            while self.pos < n {
                let b = self.bytes[self.pos];
                if b == b'\n' {
                    self.pos += 1;
                } else if b == b'/' && self.pos + 1 < n && self.bytes[self.pos + 1] == b'/' {
                    self.pos += 2;
                    let mut __compartment = self.__prepareEnter("InLineComment");
                    self.__transition(__compartment);
                    return;
                } else if b == b'/' && self.pos + 1 < n && self.bytes[self.pos + 1] == b'*' {
                    self.pos += 2;
                    self.comment_depth = 1;
                    let mut __compartment = self.__prepareEnter("InBlockComment");
                    self.__transition(__compartment);
                    return;
                } else if b == b'"' && self.pos + 2 < n && self.bytes[self.pos + 1] == b'"' && self.bytes[self.pos + 2] == b'"' {
                    self.pos += 3;
                    let mut __compartment = self.__prepareEnter("InRawString");
                    self.__transition(__compartment);
                    return;
                } else if b == b'"' {
                    self.pos += 1;
                    let mut __compartment = self.__prepareEnter("InString");
                    self.__transition(__compartment);
                    return;
                } else if b == b'{' {
                    self.depth += 1;
                    self.pos += 1;
                } else if b == b'}' {
                    self.depth -= 1;
                    self.pos += 1;
                    if self.depth == 0 {
                        self.result_pos = self.pos - 1;
                        self.error_kind = 0;
                        return
                    }
                } else {
                    self.pos += 1;
                }
            }
            self.error_kind = 3;
            self.error_msg = "body not closed".to_string();
        }

        fn _s_InString_hdl_frame_enter(&mut self, __e: &SwiftBodyCloserFsmFrameEvent) {
            let n = self.bytes.len();
            while self.pos < n {
                if self.bytes[self.pos] == b'\\' {
                    self.pos += 2;
                    continue;
                }
                if self.bytes[self.pos] == b'"' {
                    self.pos += 1;
                    let mut __compartment = self.__prepareEnter("Scanning");
                    self.__transition(__compartment);
                    return;
                }
                self.pos += 1;
            }
            self.error_kind = 1;
            self.error_msg = "unterminated string".to_string();
        }

        fn _s_InRawString_hdl_frame_enter(&mut self, __e: &SwiftBodyCloserFsmFrameEvent) {
            let n = self.bytes.len();
            while self.pos + 2 < n {
                if self.bytes[self.pos] == b'"' && self.bytes[self.pos + 1] == b'"' && self.bytes[self.pos + 2] == b'"' {
                    self.pos += 3;
                    let mut __compartment = self.__prepareEnter("Scanning");
                    self.__transition(__compartment);
                    return;
                }
                self.pos += 1;
            }
            self.pos = n;
            self.error_kind = 1;
            self.error_msg = "unterminated raw string".to_string();
        }

        fn _s_InLineComment_hdl_frame_enter(&mut self, __e: &SwiftBodyCloserFsmFrameEvent) {
            let n = self.bytes.len();
            while self.pos < n && self.bytes[self.pos] != b'\n' {
                self.pos += 1;
            }
            let mut __compartment = self.__prepareEnter("Scanning");
            self.__transition(__compartment);
            return;
        }

        fn _s_InBlockComment_hdl_frame_enter(&mut self, __e: &SwiftBodyCloserFsmFrameEvent) {
            let n = self.bytes.len();
            while self.pos + 1 < n {
                if self.bytes[self.pos] == b'/' && self.bytes[self.pos + 1] == b'*' {
                    self.comment_depth += 1;
                    self.pos += 2;
                    continue;
                }
                if self.bytes[self.pos] == b'*' && self.bytes[self.pos + 1] == b'/' {
                    self.comment_depth -= 1;
                    self.pos += 2;
                    if self.comment_depth == 0 {
                        let mut __compartment = self.__prepareEnter("Scanning");
                        self.__transition(__compartment);
                        return;
                    }
                    continue;
                }
                self.pos += 1;
            }
            self.error_kind = 2;
            self.error_msg = "unterminated comment".to_string();
        }
    }
}
pub use _swift_body_closer_fsm_framec::*;

