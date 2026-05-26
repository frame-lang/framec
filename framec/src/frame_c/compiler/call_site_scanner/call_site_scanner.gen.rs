
// RFC-0035 Round 10 — the assembler's `@@SystemName(args)` call-site scanner
// as a Frame FSM.
//
// Replaces the hand-rolled byte-walk in `assembler/mod.rs`
// (`expand_system_instantiations`). A native code region is a flat stream of
// {comment, string, `@@[!]Name(args)` call-site, plain text}; this scanner
// lexes it into a `CallToken` stream (`Literal` runs verbatim, `Call` for each
// instantiation). The FSM OWNS the scan (RFC-0039 B1: `bytes`/`pos`/`end`/
// `cur_literal`/`tokens` are domain fields) and delegates comment/string/
// balanced-paren detection to the language's `SyntaxSkipper` (no duplicated
// lexing). The actual *expansion* of a `Call` token — constructor rendering,
// arg resolution, the cross-file vs defined-system decision, and error
// reporting — stays native in the assembler wrapper, which has the (borrowed)
// system-params maps. So the FSM is a pure lexer; the maps never enter it.
//
// Byte-output identical to the recursive form — the assembler unit tests, the
// snapshot suites, and the matrix are the parity gate.

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
mod _call_site_scanner_fsm_framec {
    use super::*;
    extern crate alloc;
    use alloc::{vec, format};
    #[derive(Clone, Debug)]
    #[allow(dead_code, non_camel_case_types)]
    enum CallSiteScannerFsmFrameEvent {
        Scan {  },
        FrameEnter {},
        FrameExit {},
    }

    #[derive(Clone)]
    #[allow(dead_code, non_camel_case_types)]
    enum CallSiteScannerFsmFrameReturn {
        _Lifecycle(alloc::rc::Rc<dyn core::any::Any>),
    }

    #[allow(dead_code)]
    impl CallSiteScannerFsmFrameEvent {
        fn name(&self) -> &'static str {
            match self {
                CallSiteScannerFsmFrameEvent::Scan { .. } => "scan",
                CallSiteScannerFsmFrameEvent::FrameEnter { .. } => "$>",
                CallSiteScannerFsmFrameEvent::FrameExit { .. } => "<$",
            }
        }
    }

    #[derive(Clone, Debug)]
    #[allow(dead_code, non_camel_case_types)]
    enum CallSiteScannerFsmFrameValue {
        Int(i64),
        Float(f64),
        Bool(bool),
        Str(String),
        List(Vec<Self>),
        Dict(alloc::collections::BTreeMap<String, Self>),
    }

    #[allow(dead_code, non_camel_case_types)]
    struct CallSiteScannerFsmFrameContext {
        event: alloc::rc::Rc<CallSiteScannerFsmFrameEvent>,
        _return: Option<CallSiteScannerFsmFrameReturn>,
        _data: alloc::collections::BTreeMap<String, CallSiteScannerFsmFrameValue>,
        _transitioned: bool,
    }

    impl CallSiteScannerFsmFrameContext {
        fn new(event: alloc::rc::Rc<CallSiteScannerFsmFrameEvent>, default_return: Option<CallSiteScannerFsmFrameReturn>) -> Self {
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
    enum CallSiteScannerFsmStateContext {
        Start,
        Scan,
        Done,
        __NoContext,
    }

    impl Default for CallSiteScannerFsmStateContext {
        fn default() -> Self {
            CallSiteScannerFsmStateContext::Start
        }
    }

    #[allow(dead_code, non_camel_case_types)]
    #[derive(Clone)]
    struct CallSiteScannerFsmCompartment {
        state: String,
        state_context: CallSiteScannerFsmStateContext,
        forward_event: Option<CallSiteScannerFsmFrameEvent>,
        parent_compartment: Option<Box<CallSiteScannerFsmCompartment>>,
    }

    impl CallSiteScannerFsmCompartment {
        fn new(state: &str) -> Self {
            let state_context = match state {
                "Start" => CallSiteScannerFsmStateContext::Start,
                "Scan" => CallSiteScannerFsmStateContext::Scan,
                "Done" => CallSiteScannerFsmStateContext::Done,
                _ => CallSiteScannerFsmStateContext::__NoContext,
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
    pub struct CallSiteScannerFsm {
        _state_stack: Vec<CallSiteScannerFsmCompartment>,
        __compartment: CallSiteScannerFsmCompartment,
        __next_compartment: Option<CallSiteScannerFsmCompartment>,
        _context_stack: Vec<CallSiteScannerFsmFrameContext>,
        pub bytes: Vec<u8>,
        pub pos: usize,
        pub end: usize,
        pub skipper: Box<dyn SyntaxSkipper>,
        pub cur_literal: String,
        pub tokens: Vec<CallToken>,
    }

    #[allow(non_snake_case)]
    impl CallSiteScannerFsm {
        pub fn new() -> Self {
            Self {
                _state_stack: Vec::new(),
                _context_stack: Vec::new(),
                bytes: Vec::new(),
                pos: 0,
                end: 0,
                skipper: create_skipper(TargetLanguage::Python3),
                cur_literal: String::new(),
                tokens: Vec::new(),
                __compartment: CallSiteScannerFsmCompartment::new("Start"),
                __next_compartment: None,
            }
        }

        pub fn __create() -> Self {
            let mut c = Self::new();
            c.__compartment = c.__prepareEnter("Start");
            let __e = alloc::rc::Rc::new(CallSiteScannerFsmFrameEvent::FrameEnter {});
            let __ctx = CallSiteScannerFsmFrameContext::new(alloc::rc::Rc::clone(&__e), None);
            c._context_stack.push(__ctx);
            c.__kernel(&__e);
            c._context_stack.pop();
            c
        }

        fn __hsm_chain(&mut self, leaf: &str) -> &'static [&'static str] {
            match leaf {
                "Start" => &["Start"],
                "Scan" => &["Scan"],
                "Done" => &["Done"],
                _ => &[],
            }
        }

        fn __prepareEnter(&mut self, leaf: &str) -> CallSiteScannerFsmCompartment {
            let chain = self.__hsm_chain(leaf);
            let mut comp: Option<CallSiteScannerFsmCompartment> = None;
            for name in chain.iter() {
                let mut new_comp = CallSiteScannerFsmCompartment::new(name);
                if let Some(parent) = comp.take() {
                    new_comp.parent_compartment = Some(Box::new(parent));
                }
                comp = Some(new_comp);
            }
            comp.expect("chain must contain at least the leaf state")
        }

        fn __kernel(&mut self, __e: &alloc::rc::Rc<CallSiteScannerFsmFrameEvent>) {
            // Route event to current state.
            self.__router(__e);
            // Drain any transitions queued by the handler.
            while self.__next_compartment.is_some() {
                let next_compartment = self.__next_compartment.take().expect("invariant: while-loop guard checked is_some()");
                // Exit the current (leaf) state. RFC-0025.1: exit args live in the
                // source state's typed ctx (written at the transition site), so the
                // synthesized `<$` event carries no payload.
                let exit_event = alloc::rc::Rc::new(CallSiteScannerFsmFrameEvent::FrameExit {});
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
                        let enter_event = alloc::rc::Rc::new(CallSiteScannerFsmFrameEvent::FrameEnter {});
                        self.__router(&enter_event);
                    }
                    Some(fwd) if matches!(fwd, CallSiteScannerFsmFrameEvent::FrameEnter { .. }) => {
                        // Forwarded event IS $> — dispatch directly so the
                        // destination's $> handler receives the caller's payload.
                        let fwd_rc = alloc::rc::Rc::new(fwd);
                        self.__router(&fwd_rc);
                    }
                    Some(fwd) => {
                        // Forwarded event is not $> — initialize the destination
                        // with a fresh $>, then dispatch the forward.
                        let enter_event = alloc::rc::Rc::new(CallSiteScannerFsmFrameEvent::FrameEnter {});
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

        fn __router(&mut self, __e: &alloc::rc::Rc<CallSiteScannerFsmFrameEvent>) {
            let __ev: &CallSiteScannerFsmFrameEvent = __e;
            match self.__compartment.state.as_str() {
                "Start" => self._state_Start(__ev),
                "Scan" => self._state_Scan(__ev),
                "Done" => self._state_Done(__ev),
                _ => {}
            }
        }

        fn __transition(&mut self, next_compartment: CallSiteScannerFsmCompartment) {
            self.__next_compartment = Some(next_compartment);
        }

        pub fn scan(&mut self) {
            let __e = alloc::rc::Rc::new(CallSiteScannerFsmFrameEvent::Scan {});
            let mut __ctx = CallSiteScannerFsmFrameContext::new(alloc::rc::Rc::clone(&__e), None);
            self._context_stack.push(__ctx);
            self.__kernel(&__e);
            self._context_stack.pop();
        }

        fn _state_Start(&mut self, __e: &CallSiteScannerFsmFrameEvent) {
            match __e {
                CallSiteScannerFsmFrameEvent::Scan { .. } => { self._s_Start_hdl_user_scan(__e); }
                _ => {}
            }
        }

        // One unit per entry (a comment span, a string span, a call-site, an
        // unmatched `@@` run, or one plain byte). Self-loops to $Done at EOF.
        fn _state_Scan(&mut self, __e: &CallSiteScannerFsmFrameEvent) {
            match __e {
                CallSiteScannerFsmFrameEvent::FrameEnter { .. } => { self._s_Scan_hdl_frame_enter(__e); }
                _ => {}
            }
        }

        fn _state_Done(&mut self, __e: &CallSiteScannerFsmFrameEvent) {
            match __e {
                _ => {}
            }
        }

        fn _s_Start_hdl_user_scan(&mut self, __e: &CallSiteScannerFsmFrameEvent) {
            let mut __compartment = self.__prepareEnter("Scan");
            self.__transition(__compartment);
            return;
        }

        fn _s_Scan_hdl_frame_enter(&mut self, __e: &CallSiteScannerFsmFrameEvent) {
            let n = self.end;
            
            // EOF — flush the trailing literal run and finish.
            if self.pos >= n {
                if !self.cur_literal.is_empty() {
                    self.tokens
                        .push(CallToken::Literal(std::mem::take(&mut self.cur_literal)));
                }
                let mut __compartment = self.__prepareEnter("Done");
                self.__transition(__compartment);
                return;
            }
            
            // Comments and strings pass through verbatim (delegated to the
            // language's SyntaxSkipper — no duplicated comment/string logic).
            if let Some(after) = self.skipper.skip_comment(&self.bytes, self.pos, n) {
                self.cur_literal
                    .push_str(&String::from_utf8_lossy(&self.bytes[self.pos..after]));
                self.pos = after;
                let mut __compartment = self.__prepareEnter("Scan");
                self.__transition(__compartment);
                return;
            }
            if let Some(after) = self.skipper.skip_string(&self.bytes, self.pos, n) {
                self.cur_literal
                    .push_str(&String::from_utf8_lossy(&self.bytes[self.pos..after]));
                self.pos = after;
                let mut __compartment = self.__prepareEnter("Scan");
                self.__transition(__compartment);
                return;
            }
            
            // `@@Name(args)` / `@@!Name(args)` call-site. Needs at least
            // three bytes (`@@X`); a trailing `@@` is treated as plain text.
            if self.pos + 2 < n && self.bytes[self.pos] == b'@' && self.bytes[self.pos + 1] == b'@' {
                let start = self.pos;
                let mut j = self.pos + 2;
                let no_init = j < n && self.bytes[j] == b'!';
                if no_init {
                    j += 1;
                }
                if j < n && self.bytes[j].is_ascii_uppercase() {
                    let name_start = j;
                    while j < n && (self.bytes[j].is_ascii_alphanumeric() || self.bytes[j] == b'_') {
                        j += 1;
                    }
                    let name = std::str::from_utf8(&self.bytes[name_start..j])
                        .unwrap_or("")
                        .to_string();
                    if j < n && self.bytes[j] == b'(' {
                        if let Some(close) = self.skipper.balanced_paren_end(&self.bytes, j, n) {
                            let args = std::str::from_utf8(&self.bytes[j + 1..close - 1])
                                .unwrap_or("")
                                .to_string();
                            if !self.cur_literal.is_empty() {
                                self.tokens.push(CallToken::Literal(std::mem::take(
                                    &mut self.cur_literal,
                                )));
                            }
                            self.tokens.push(CallToken::Call { name, args, no_init });
                            self.pos = close;
                            let mut __compartment = self.__prepareEnter("Scan");
                            self.__transition(__compartment);
                            return;
                        }
                    }
                }
                // Not a valid instantiation: emit the consumed `@@`/`@@!`/
                // `@@Name` bytes verbatim and resume just past them.
                let stop = if j > start + 2 { j } else { start + 2 };
                self.cur_literal
                    .push_str(&String::from_utf8_lossy(&self.bytes[start..stop]));
                self.pos = stop;
                let mut __compartment = self.__prepareEnter("Scan");
                self.__transition(__compartment);
                return;
            }
            
            // Plain byte.
            self.cur_literal.push(self.bytes[self.pos] as char);
            self.pos += 1;
            let mut __compartment = self.__prepareEnter("Scan");
            self.__transition(__compartment);
            return;
        }
    }
}
pub use _call_site_scanner_fsm_framec::*;
