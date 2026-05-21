
// Dogfooded body closer — Frame-structural brace matcher.
//
// Used by the GraphViz pipeline, which can be invoked on a `.frs`
// written for any production target (Rust, Python, Ruby, Java,
// etc.). Combines the Rust body-closer's lexical rules (`//`,
// `/* */`, `'X'` char literals, `"..."` strings) with `#` line-
// comment recognition (Python / Ruby / Erlang flavor), so source
// written for any of those targets renders through `framec -l
// graphviz` without false-positive parse failures.
//
// Differences from `rust_lang.frs`:
//   - Adds `#` line-comment handling.
//   - Drops raw-string (`r#"..."#`) — uncommon at the structural
//     level, and adding it back requires no special parsing
//     (`r` becomes an ordinary identifier byte; the next `"` opens
//     a regular string).
//
// State machine flow:
//   $Init.scan() → $Scanning.$>() ↔
//       $InString / $InCharLiteral / $InLineComment / $InBlockComment

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
mod _frame_structural_body_closer_fsm_framec {
    use super::*;
    extern crate alloc;
    use alloc::{vec, format};
    #[derive(Clone, Debug)]
    #[allow(dead_code, non_camel_case_types)]
    enum FrameStructuralBodyCloserFsmFrameEvent {
        Scan {  },
        FrameEnter {},
        FrameExit {},
    }

    #[derive(Clone)]
    #[allow(dead_code, non_camel_case_types)]
    enum FrameStructuralBodyCloserFsmFrameReturn {
        _Lifecycle(alloc::rc::Rc<dyn core::any::Any>),
    }

    #[allow(dead_code)]
    impl FrameStructuralBodyCloserFsmFrameEvent {
        fn name(&self) -> &'static str {
            match self {
                FrameStructuralBodyCloserFsmFrameEvent::Scan { .. } => "scan",
                FrameStructuralBodyCloserFsmFrameEvent::FrameEnter { .. } => "$>",
                FrameStructuralBodyCloserFsmFrameEvent::FrameExit { .. } => "<$",
            }
        }
    }

    #[derive(Clone, Debug)]
    #[allow(dead_code, non_camel_case_types)]
    enum FrameStructuralBodyCloserFsmFrameValue {
        Int(i64),
        Float(f64),
        Bool(bool),
        Str(String),
        List(Vec<Self>),
        Dict(alloc::collections::BTreeMap<String, Self>),
    }

    #[allow(dead_code, non_camel_case_types)]
    struct FrameStructuralBodyCloserFsmFrameContext {
        event: alloc::rc::Rc<FrameStructuralBodyCloserFsmFrameEvent>,
        _return: Option<FrameStructuralBodyCloserFsmFrameReturn>,
        _data: alloc::collections::BTreeMap<String, FrameStructuralBodyCloserFsmFrameValue>,
        _transitioned: bool,
    }

    impl FrameStructuralBodyCloserFsmFrameContext {
        fn new(event: alloc::rc::Rc<FrameStructuralBodyCloserFsmFrameEvent>, default_return: Option<FrameStructuralBodyCloserFsmFrameReturn>) -> Self {
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
    enum FrameStructuralBodyCloserFsmStateContext {
        Init,
        Scanning,
        InString,
        InLineComment,
        InBlockComment,
        Empty,
    }

    impl Default for FrameStructuralBodyCloserFsmStateContext {
        fn default() -> Self {
            FrameStructuralBodyCloserFsmStateContext::Init
        }
    }

    #[allow(dead_code, non_camel_case_types)]
    #[derive(Clone)]
    struct FrameStructuralBodyCloserFsmCompartment {
        state: String,
        state_context: FrameStructuralBodyCloserFsmStateContext,
        forward_event: Option<FrameStructuralBodyCloserFsmFrameEvent>,
        parent_compartment: Option<Box<FrameStructuralBodyCloserFsmCompartment>>,
    }

    impl FrameStructuralBodyCloserFsmCompartment {
        fn new(state: &str) -> Self {
            let state_context = match state {
                "Init" => FrameStructuralBodyCloserFsmStateContext::Init,
                "Scanning" => FrameStructuralBodyCloserFsmStateContext::Scanning,
                "InString" => FrameStructuralBodyCloserFsmStateContext::InString,
                "InLineComment" => FrameStructuralBodyCloserFsmStateContext::InLineComment,
                "InBlockComment" => FrameStructuralBodyCloserFsmStateContext::InBlockComment,
                _ => FrameStructuralBodyCloserFsmStateContext::Empty,
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
    pub struct FrameStructuralBodyCloserFsm {
        _state_stack: Vec<FrameStructuralBodyCloserFsmCompartment>,
        __compartment: FrameStructuralBodyCloserFsmCompartment,
        __next_compartment: Option<FrameStructuralBodyCloserFsmCompartment>,
        _context_stack: Vec<FrameStructuralBodyCloserFsmFrameContext>,
        pub bytes: Vec<u8>,
        pub pos: usize,
        pub depth: i32,
        pub result_pos: usize,
        pub error_kind: usize,
        pub error_msg: String,
        pub block_comment_nest: i32,
    }

    #[allow(non_snake_case)]
    impl FrameStructuralBodyCloserFsm {
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
                block_comment_nest: 0,
                __compartment: FrameStructuralBodyCloserFsmCompartment::new("Init"),
                __next_compartment: None,
            }
        }

        pub fn __create() -> Self {
            let mut c = Self::new();
            c.__compartment = c.__prepareEnter("Init");
            let __e = alloc::rc::Rc::new(FrameStructuralBodyCloserFsmFrameEvent::FrameEnter {});
            let __ctx = FrameStructuralBodyCloserFsmFrameContext::new(alloc::rc::Rc::clone(&__e), None);
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
                "InLineComment" => &["InLineComment"],
                "InBlockComment" => &["InBlockComment"],
                _ => &[],
            }
        }

        fn __prepareEnter(&mut self, leaf: &str) -> FrameStructuralBodyCloserFsmCompartment {
            let chain = self.__hsm_chain(leaf);
            let mut comp: Option<FrameStructuralBodyCloserFsmCompartment> = None;
            for name in chain.iter() {
                let mut new_comp = FrameStructuralBodyCloserFsmCompartment::new(name);
                if let Some(parent) = comp.take() {
                    new_comp.parent_compartment = Some(Box::new(parent));
                }
                comp = Some(new_comp);
            }
            comp.expect("chain must contain at least the leaf state")
        }

        fn __kernel(&mut self, __e: &alloc::rc::Rc<FrameStructuralBodyCloserFsmFrameEvent>) {
            // Route event to current state.
            self.__router(__e);
            // Drain any transitions queued by the handler.
            while self.__next_compartment.is_some() {
                let next_compartment = self.__next_compartment.take().expect("invariant: while-loop guard checked is_some()");
                // Exit the current (leaf) state. RFC-0025.1: exit args live in the
                // source state's typed ctx (written at the transition site), so the
                // synthesized `<$` event carries no payload.
                let exit_event = alloc::rc::Rc::new(FrameStructuralBodyCloserFsmFrameEvent::FrameExit {});
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
                        let enter_event = alloc::rc::Rc::new(FrameStructuralBodyCloserFsmFrameEvent::FrameEnter {});
                        self.__router(&enter_event);
                    }
                    Some(fwd) if matches!(fwd, FrameStructuralBodyCloserFsmFrameEvent::FrameEnter { .. }) => {
                        // Forwarded event IS $> — dispatch directly so the
                        // destination's $> handler receives the caller's payload.
                        let fwd_rc = alloc::rc::Rc::new(fwd);
                        self.__router(&fwd_rc);
                    }
                    Some(fwd) => {
                        // Forwarded event is not $> — initialize the destination
                        // with a fresh $>, then dispatch the forward.
                        let enter_event = alloc::rc::Rc::new(FrameStructuralBodyCloserFsmFrameEvent::FrameEnter {});
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

        fn __router(&mut self, __e: &alloc::rc::Rc<FrameStructuralBodyCloserFsmFrameEvent>) {
            let __ev: &FrameStructuralBodyCloserFsmFrameEvent = __e;
            match self.__compartment.state.as_str() {
                "Init" => self._state_Init(__ev),
                "Scanning" => self._state_Scanning(__ev),
                "InString" => self._state_InString(__ev),
                "InLineComment" => self._state_InLineComment(__ev),
                "InBlockComment" => self._state_InBlockComment(__ev),
                _ => {}
            }
        }

        fn __transition(&mut self, next_compartment: FrameStructuralBodyCloserFsmCompartment) {
            self.__next_compartment = Some(next_compartment);
        }

        pub fn scan(&mut self) {
            let __e = alloc::rc::Rc::new(FrameStructuralBodyCloserFsmFrameEvent::Scan {});
            let mut __ctx = FrameStructuralBodyCloserFsmFrameContext::new(alloc::rc::Rc::clone(&__e), None);
            self._context_stack.push(__ctx);
            self.__kernel(&__e);
            self._context_stack.pop();
        }

        fn _state_Init(&mut self, __e: &FrameStructuralBodyCloserFsmFrameEvent) {
            match __e {
                FrameStructuralBodyCloserFsmFrameEvent::Scan { .. } => { self._s_Init_hdl_user_scan(__e); }
                _ => {}
            }
        }

        fn _state_Scanning(&mut self, __e: &FrameStructuralBodyCloserFsmFrameEvent) {
            match __e {
                FrameStructuralBodyCloserFsmFrameEvent::FrameEnter { .. } => { self._s_Scanning_hdl_frame_enter(__e); }
                _ => {}
            }
        }

        fn _state_InString(&mut self, __e: &FrameStructuralBodyCloserFsmFrameEvent) {
            match __e {
                FrameStructuralBodyCloserFsmFrameEvent::FrameEnter { .. } => { self._s_InString_hdl_frame_enter(__e); }
                _ => {}
            }
        }

        fn _state_InLineComment(&mut self, __e: &FrameStructuralBodyCloserFsmFrameEvent) {
            match __e {
                FrameStructuralBodyCloserFsmFrameEvent::FrameEnter { .. } => { self._s_InLineComment_hdl_frame_enter(__e); }
                _ => {}
            }
        }

        fn _state_InBlockComment(&mut self, __e: &FrameStructuralBodyCloserFsmFrameEvent) {
            match __e {
                FrameStructuralBodyCloserFsmFrameEvent::FrameEnter { .. } => { self._s_InBlockComment_hdl_frame_enter(__e); }
                _ => {}
            }
        }

        fn _s_Init_hdl_user_scan(&mut self, __e: &FrameStructuralBodyCloserFsmFrameEvent) {
            let mut __compartment = self.__prepareEnter("Scanning");
            self.__transition(__compartment);
            return;
        }

        fn _s_Scanning_hdl_frame_enter(&mut self, __e: &FrameStructuralBodyCloserFsmFrameEvent) {
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
                    self.block_comment_nest = 1;
                    self.pos += 2;
                    let mut __compartment = self.__prepareEnter("InBlockComment");
                    self.__transition(__compartment);
                    return;
                } else if b == b'#' {
                    // Python / Ruby / Erlang line comment.
                    self.pos += 1;
                    let mut __compartment = self.__prepareEnter("InLineComment");
                    self.__transition(__compartment);
                    return;
                } else if b == b'\'' {
                    // Distinguish char literal from lifetime
                    // (`'static`, `'a`). The pattern check is
                    // the same as the Rust string skipper:
                    // `'X'`, `'\X...'`, `'\u{...}'` are literals;
                    // `'` followed by alphanumeric without a
                    // closing `'` within 12 bytes is treated as
                    // an ordinary byte (lifetime / English
                    // apostrophe in non-comment context).
                    let j = self.pos + 1;
                    if j < n && self.bytes[j] == b'\\' {
                        // Escape form — scan up to next `'`
                        // bounded at 12 bytes.
                        let mut k = j + 1;
                        while k < n && k < j + 12 && self.bytes[k] != b'\'' {
                            k += 1;
                        }
                        if k < n && self.bytes[k] == b'\'' {
                            self.pos = k + 1;
                            continue;
                        }
                        // Unterminated — treat `'` as ordinary.
                        self.pos += 1;
                        continue;
                    } else if j + 1 < n && self.bytes[j + 1] == b'\'' {
                        // `'X'` simple char literal.
                        self.pos = j + 2;
                        continue;
                    } else {
                        // Lifetime / apostrophe — pass through.
                        self.pos += 1;
                        continue;
                    }
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

        fn _s_InString_hdl_frame_enter(&mut self, __e: &FrameStructuralBodyCloserFsmFrameEvent) {
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

        fn _s_InLineComment_hdl_frame_enter(&mut self, __e: &FrameStructuralBodyCloserFsmFrameEvent) {
            let n = self.bytes.len();
            while self.pos < n && self.bytes[self.pos] != b'\n' {
                self.pos += 1;
            }
            let mut __compartment = self.__prepareEnter("Scanning");
            self.__transition(__compartment);
            return;
        }

        fn _s_InBlockComment_hdl_frame_enter(&mut self, __e: &FrameStructuralBodyCloserFsmFrameEvent) {
            // Nested block comments — same as Rust.
            let n = self.bytes.len();
            while self.pos + 1 < n {
                if self.bytes[self.pos] == b'/' && self.bytes[self.pos + 1] == b'*' {
                    self.block_comment_nest += 1;
                    self.pos += 2;
                    continue;
                }
                if self.bytes[self.pos] == b'*' && self.bytes[self.pos + 1] == b'/' {
                    self.block_comment_nest -= 1;
                    self.pos += 2;
                    if self.block_comment_nest == 0 {
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
pub use _frame_structural_body_closer_fsm_framec::*;

