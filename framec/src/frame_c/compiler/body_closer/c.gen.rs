
// First dogfooded body closer — C language brace matcher.
// Generated Rust code replaces the hand-written c.rs implementation.
// Architecture: Option B — internal loops in Frame state handlers.
//
// State machine flow:
//   $Init.scan() → $Scanning.$>() ↔ $InString/$InCharLiteral/$InLineComment/$InBlockComment
//
// All scanning work happens in enter handlers ($>). The kernel's transition
// loop chains them within a single scan() call. When scanning completes
// (depth==0 or error), no transition is made and the kernel loop ends.

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
mod _c_body_closer_fsm_framec {
    use super::*;
    extern crate alloc;
    use alloc::{vec, format};
    #[derive(Clone, Debug)]
    #[allow(dead_code, non_camel_case_types)]
    enum CBodyCloserFsmFrameEvent {
        Scan {  },
        FrameEnter {},
        FrameExit {},
    }

    #[derive(Clone)]
    #[allow(dead_code, non_camel_case_types)]
    enum CBodyCloserFsmFrameReturn {
        _Lifecycle(alloc::rc::Rc<dyn core::any::Any>),
    }

    #[allow(dead_code)]
    impl CBodyCloserFsmFrameEvent {
        fn name(&self) -> &'static str {
            match self {
                CBodyCloserFsmFrameEvent::Scan { .. } => "scan",
                CBodyCloserFsmFrameEvent::FrameEnter { .. } => "$>",
                CBodyCloserFsmFrameEvent::FrameExit { .. } => "<$",
            }
        }
    }

    #[derive(Clone, Debug)]
    #[allow(dead_code, non_camel_case_types)]
    enum CBodyCloserFsmFrameValue {
        Int(i64),
        Float(f64),
        Bool(bool),
        Str(String),
        List(Vec<Self>),
        Dict(alloc::collections::BTreeMap<String, Self>),
    }

    #[allow(dead_code, non_camel_case_types)]
    struct CBodyCloserFsmFrameContext {
        event: alloc::rc::Rc<CBodyCloserFsmFrameEvent>,
        _return: Option<CBodyCloserFsmFrameReturn>,
        _data: alloc::collections::BTreeMap<String, CBodyCloserFsmFrameValue>,
        _transitioned: bool,
    }

    impl CBodyCloserFsmFrameContext {
        fn new(event: alloc::rc::Rc<CBodyCloserFsmFrameEvent>, default_return: Option<CBodyCloserFsmFrameReturn>) -> Self {
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
    enum CBodyCloserFsmStateContext {
        Init,
        Scanning,
        InString,
        InCharLiteral,
        InLineComment,
        InBlockComment,
        __NoContext,
    }

    impl Default for CBodyCloserFsmStateContext {
        fn default() -> Self {
            CBodyCloserFsmStateContext::Init
        }
    }

    #[allow(dead_code, non_camel_case_types)]
    #[derive(Clone)]
    struct CBodyCloserFsmCompartment {
        state: String,
        state_context: CBodyCloserFsmStateContext,
        forward_event: Option<CBodyCloserFsmFrameEvent>,
        parent_compartment: Option<Box<CBodyCloserFsmCompartment>>,
    }

    impl CBodyCloserFsmCompartment {
        fn new(state: &str) -> Self {
            let state_context = match state {
                "Init" => CBodyCloserFsmStateContext::Init,
                "Scanning" => CBodyCloserFsmStateContext::Scanning,
                "InString" => CBodyCloserFsmStateContext::InString,
                "InCharLiteral" => CBodyCloserFsmStateContext::InCharLiteral,
                "InLineComment" => CBodyCloserFsmStateContext::InLineComment,
                "InBlockComment" => CBodyCloserFsmStateContext::InBlockComment,
                _ => CBodyCloserFsmStateContext::__NoContext,
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
    pub struct CBodyCloserFsm {
        _state_stack: Vec<CBodyCloserFsmCompartment>,
        __compartment: CBodyCloserFsmCompartment,
        __next_compartment: Option<CBodyCloserFsmCompartment>,
        _context_stack: Vec<CBodyCloserFsmFrameContext>,
        pub bytes: Vec<u8>,
        pub pos: usize,
        pub depth: i32,
        pub result_pos: usize,
        pub error_kind: usize,
        pub error_msg: String,
    }

    #[allow(non_snake_case)]
    impl CBodyCloserFsm {
        pub fn new() -> Self {
            Self {
                _state_stack: Vec::new(),
                _context_stack: Vec::new(),
                bytes: Vec::new(),
                pos: 0,
                depth: 1,
                result_pos: 0,
                error_kind: 0,
                error_msg: String::new(),
                __compartment: CBodyCloserFsmCompartment::new("Init"),
                __next_compartment: None,
            }
        }

        pub fn __create() -> Self {
            let mut c = Self::new();
            c.__compartment = c.__prepareEnter("Init");
            let __e = alloc::rc::Rc::new(CBodyCloserFsmFrameEvent::FrameEnter {});
            let __ctx = CBodyCloserFsmFrameContext::new(alloc::rc::Rc::clone(&__e), None);
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
                "InCharLiteral" => &["InCharLiteral"],
                "InLineComment" => &["InLineComment"],
                "InBlockComment" => &["InBlockComment"],
                _ => &[],
            }
        }

        fn __prepareEnter(&mut self, leaf: &str) -> CBodyCloserFsmCompartment {
            let chain = self.__hsm_chain(leaf);
            let mut comp: Option<CBodyCloserFsmCompartment> = None;
            for name in chain.iter() {
                let mut new_comp = CBodyCloserFsmCompartment::new(name);
                if let Some(parent) = comp.take() {
                    new_comp.parent_compartment = Some(Box::new(parent));
                }
                comp = Some(new_comp);
            }
            comp.expect("chain must contain at least the leaf state")
        }

        fn __kernel(&mut self, __e: &alloc::rc::Rc<CBodyCloserFsmFrameEvent>) {
            // Route event to current state.
            self.__router(__e);
            // Drain any transitions queued by the handler.
            while self.__next_compartment.is_some() {
                let next_compartment = self.__next_compartment.take().expect("invariant: while-loop guard checked is_some()");
                // Exit the current (leaf) state. RFC-0025.1: exit args live in the
                // source state's typed ctx (written at the transition site), so the
                // synthesized `<$` event carries no payload.
                let exit_event = alloc::rc::Rc::new(CBodyCloserFsmFrameEvent::FrameExit {});
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
                        let enter_event = alloc::rc::Rc::new(CBodyCloserFsmFrameEvent::FrameEnter {});
                        self.__router(&enter_event);
                    }
                    Some(fwd) if matches!(fwd, CBodyCloserFsmFrameEvent::FrameEnter { .. }) => {
                        // Forwarded event IS $> — dispatch directly so the
                        // destination's $> handler receives the caller's payload.
                        let fwd_rc = alloc::rc::Rc::new(fwd);
                        self.__router(&fwd_rc);
                    }
                    Some(fwd) => {
                        // Forwarded event is not $> — initialize the destination
                        // with a fresh $>, then dispatch the forward.
                        let enter_event = alloc::rc::Rc::new(CBodyCloserFsmFrameEvent::FrameEnter {});
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

        fn __router(&mut self, __e: &alloc::rc::Rc<CBodyCloserFsmFrameEvent>) {
            let __ev: &CBodyCloserFsmFrameEvent = __e;
            match self.__compartment.state.as_str() {
                "Init" => self._state_Init(__ev),
                "Scanning" => self._state_Scanning(__ev),
                "InString" => self._state_InString(__ev),
                "InCharLiteral" => self._state_InCharLiteral(__ev),
                "InLineComment" => self._state_InLineComment(__ev),
                "InBlockComment" => self._state_InBlockComment(__ev),
                _ => {}
            }
        }

        fn __transition(&mut self, next_compartment: CBodyCloserFsmCompartment) {
            self.__next_compartment = Some(next_compartment);
        }

        pub fn scan(&mut self) {
            let __e = alloc::rc::Rc::new(CBodyCloserFsmFrameEvent::Scan {});
            let mut __ctx = CBodyCloserFsmFrameContext::new(alloc::rc::Rc::clone(&__e), None);
            self._context_stack.push(__ctx);
            self.__kernel(&__e);
            self._context_stack.pop();
        }

        fn _state_Init(&mut self, __e: &CBodyCloserFsmFrameEvent) {
            match __e {
                CBodyCloserFsmFrameEvent::Scan { .. } => { self._s_Init_hdl_user_scan(__e); }
                _ => {}
            }
        }

        fn _state_Scanning(&mut self, __e: &CBodyCloserFsmFrameEvent) {
            match __e {
                CBodyCloserFsmFrameEvent::FrameEnter { .. } => { self._s_Scanning_hdl_frame_enter(__e); }
                _ => {}
            }
        }

        fn _state_InString(&mut self, __e: &CBodyCloserFsmFrameEvent) {
            match __e {
                CBodyCloserFsmFrameEvent::FrameEnter { .. } => { self._s_InString_hdl_frame_enter(__e); }
                _ => {}
            }
        }

        fn _state_InCharLiteral(&mut self, __e: &CBodyCloserFsmFrameEvent) {
            match __e {
                CBodyCloserFsmFrameEvent::FrameEnter { .. } => { self._s_InCharLiteral_hdl_frame_enter(__e); }
                _ => {}
            }
        }

        fn _state_InLineComment(&mut self, __e: &CBodyCloserFsmFrameEvent) {
            match __e {
                CBodyCloserFsmFrameEvent::FrameEnter { .. } => { self._s_InLineComment_hdl_frame_enter(__e); }
                _ => {}
            }
        }

        fn _state_InBlockComment(&mut self, __e: &CBodyCloserFsmFrameEvent) {
            match __e {
                CBodyCloserFsmFrameEvent::FrameEnter { .. } => { self._s_InBlockComment_hdl_frame_enter(__e); }
                _ => {}
            }
        }

        fn _s_Init_hdl_user_scan(&mut self, __e: &CBodyCloserFsmFrameEvent) {
            let mut __compartment = self.__prepareEnter("Scanning");
            self.__transition(__compartment);
            return;
        }

        fn _s_Scanning_hdl_frame_enter(&mut self, __e: &CBodyCloserFsmFrameEvent) {
            // Main scanning loop — re-enters after each sub-state returns
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
                    let mut __compartment = self.__prepareEnter("InBlockComment");
                    self.__transition(__compartment);
                    return;
                } else if b == b'\'' {
                    self.pos += 1;
                    let mut __compartment = self.__prepareEnter("InCharLiteral");
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
            // Fell off the end — unmatched braces
            self.error_kind = 3;
            self.error_msg = "body not closed".to_string();
        }

        fn _s_InString_hdl_frame_enter(&mut self, __e: &CBodyCloserFsmFrameEvent) {
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
            // EOF in string
            self.error_kind = 1;
            self.error_msg = "unterminated string".to_string();
        }

        fn _s_InCharLiteral_hdl_frame_enter(&mut self, __e: &CBodyCloserFsmFrameEvent) {
            let n = self.bytes.len();
            while self.pos < n {
                if self.bytes[self.pos] == b'\\' {
                    self.pos += 2;
                    continue;
                }
                if self.bytes[self.pos] == b'\'' {
                    self.pos += 1;
                    let mut __compartment = self.__prepareEnter("Scanning");
                    self.__transition(__compartment);
                    return;
                }
                self.pos += 1;
            }
            // EOF in char literal
            self.error_kind = 1;
            self.error_msg = "unterminated char".to_string();
        }

        fn _s_InLineComment_hdl_frame_enter(&mut self, __e: &CBodyCloserFsmFrameEvent) {
            let n = self.bytes.len();
            while self.pos < n && self.bytes[self.pos] != b'\n' {
                self.pos += 1;
            }
            let mut __compartment = self.__prepareEnter("Scanning");
            self.__transition(__compartment);
            return;
        }

        fn _s_InBlockComment_hdl_frame_enter(&mut self, __e: &CBodyCloserFsmFrameEvent) {
            let n = self.bytes.len();
            while self.pos + 1 < n {
                if self.bytes[self.pos] == b'*' && self.bytes[self.pos + 1] == b'/' {
                    self.pos += 2;
                    let mut __compartment = self.__prepareEnter("Scanning");
                    self.__transition(__compartment);
                    return;
                }
                self.pos += 1;
            }
            // EOF in block comment
            self.error_kind = 2;
            self.error_msg = "unterminated comment".to_string();
        }
    }
}
pub use _c_body_closer_fsm_framec::*;

