
// RFC-0042 — the @@fsm lexer as a Frame @@system byte-scanner.
//
// Tokenizes the raw bytes of an @@fsm declaration into a Vec<FsmToken>.
// Built as a state machine because the load-bearing lexing decision —
// is `/` a regex-literal delimiter or a division operator? — IS a state
// machine (the classic regex-vs-division problem). The lexer's mode is
// its state:
//
//   $Header       — `@@fsm Name(params) : Type = default {` ... up to and
//                   including the body-opening `{`. Header punctuation +
//                   the default expression.
//   $ElementLevel — the state list / match-element level. A `/` here
//                   starts a regex literal (scanned whole, emitted as one
//                   RegexLiteral token).
//   $ExprLevel    — inside action blocks / when-conditions / bare
//                   expressions. A `/` here is division. (TODO: full
//                   expansion — v1 covers only what the smoke fixture
//                   needs; see the task tracker.)
//
// The byte-level work (whitespace/comment skip, token emission) is native
// Rust inside the `$>` handlers, exactly as expr_scanner.frs and
// domain_scanner.frs do it. Frame owns the mode control flow.
//
// v1 SCOPE: tokenizes the smoke fixture
//   @@fsm M(text: bytes) : bool = false { /a/ true }
// into the documented token stream. Embedding-action operators, the full
// operator set, `when`/`if`/`else`, `@@:` probes, and real $ExprLevel
// division handling are TODO — added incrementally as later fixtures need
// them. Each addition keeps the smoke fixture green.
//
// The wrapper (fsm_parser/mod.rs::lex_fsm_block) builds this system, sets
// `bytes`, calls `tokenize()`, and lifts `tokens` / `error` out.

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
mod _fsm_lexer_framec {
    use super::*;
    extern crate alloc;
    use alloc::{vec, format};
    #[derive(Clone, Debug)]
    #[allow(dead_code, non_camel_case_types)]
    enum FsmLexerFrameEvent {
        Tokenize {  },
        FrameEnter {},
        FrameExit {},
    }

    #[derive(Clone)]
    #[allow(dead_code, non_camel_case_types)]
    enum FsmLexerFrameReturn {
        _Lifecycle(alloc::rc::Rc<dyn core::any::Any>),
    }

    #[allow(dead_code)]
    impl FsmLexerFrameEvent {
        fn name(&self) -> &'static str {
            match self {
                FsmLexerFrameEvent::Tokenize { .. } => "tokenize",
                FsmLexerFrameEvent::FrameEnter { .. } => "$>",
                FsmLexerFrameEvent::FrameExit { .. } => "<$",
            }
        }
    }

    #[derive(Clone, Debug)]
    #[allow(dead_code, non_camel_case_types)]
    enum FsmLexerFrameValue {
        Int(i64),
        Float(f64),
        Bool(bool),
        Str(String),
        List(Vec<Self>),
        Dict(alloc::collections::BTreeMap<String, Self>),
    }

    #[allow(dead_code, non_camel_case_types)]
    struct FsmLexerFrameContext {
        event: alloc::rc::Rc<FsmLexerFrameEvent>,
        _return: Option<FsmLexerFrameReturn>,
        _data: alloc::collections::BTreeMap<String, FsmLexerFrameValue>,
        _transitioned: bool,
    }

    impl FsmLexerFrameContext {
        fn new(event: alloc::rc::Rc<FsmLexerFrameEvent>, default_return: Option<FsmLexerFrameReturn>) -> Self {
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
    enum FsmLexerStateContext {
        Start,
        Header,
        ElementLevel,
        Done,
        __NoContext,
    }

    impl Default for FsmLexerStateContext {
        fn default() -> Self {
            FsmLexerStateContext::Start
        }
    }

    #[allow(dead_code, non_camel_case_types)]
    #[derive(Clone)]
    struct FsmLexerCompartment {
        state: String,
        state_context: FsmLexerStateContext,
        forward_event: Option<FsmLexerFrameEvent>,
        parent_compartment: Option<Box<FsmLexerCompartment>>,
    }

    impl FsmLexerCompartment {
        fn new(state: &str) -> Self {
            let state_context = match state {
                "Start" => FsmLexerStateContext::Start,
                "Header" => FsmLexerStateContext::Header,
                "ElementLevel" => FsmLexerStateContext::ElementLevel,
                "Done" => FsmLexerStateContext::Done,
                _ => FsmLexerStateContext::__NoContext,
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
    pub struct FsmLexer {
        _state_stack: Vec<FsmLexerCompartment>,
        __compartment: FsmLexerCompartment,
        __next_compartment: Option<FsmLexerCompartment>,
        _context_stack: Vec<FsmLexerFrameContext>,
        pub bytes: Vec<u8>,
        pub pos: usize,
        pub tokens: Vec<FsmToken>,
        pub error: Option<ParseError>,
    }

    #[allow(non_snake_case)]
    impl FsmLexer {
        pub fn new() -> Self {
            Self {
                _state_stack: Vec::new(),
                _context_stack: Vec::new(),
                bytes: Vec::new(),
                pos: 0,
                tokens: Vec::new(),
                error: None,
                __compartment: FsmLexerCompartment::new("Start"),
                __next_compartment: None,
            }
        }

        pub fn __create() -> Self {
            let mut c = Self::new();
            c.__compartment = c.__prepareEnter("Start");
            let __e = alloc::rc::Rc::new(FsmLexerFrameEvent::FrameEnter {});
            let __ctx = FsmLexerFrameContext::new(alloc::rc::Rc::clone(&__e), None);
            c._context_stack.push(__ctx);
            c.__kernel(&__e);
            c._context_stack.pop();
            c
        }

        fn __hsm_chain(&mut self, leaf: &str) -> &'static [&'static str] {
            match leaf {
                "Start" => &["Start"],
                "Header" => &["Header"],
                "ElementLevel" => &["ElementLevel"],
                "Done" => &["Done"],
                _ => &[],
            }
        }

        fn __prepareEnter(&mut self, leaf: &str) -> FsmLexerCompartment {
            let chain = self.__hsm_chain(leaf);
            let mut comp: Option<FsmLexerCompartment> = None;
            for name in chain.iter() {
                let mut new_comp = FsmLexerCompartment::new(name);
                if let Some(parent) = comp.take() {
                    new_comp.parent_compartment = Some(Box::new(parent));
                }
                comp = Some(new_comp);
            }
            comp.expect("chain must contain at least the leaf state")
        }

        fn __kernel(&mut self, __e: &alloc::rc::Rc<FsmLexerFrameEvent>) {
            // Route event to current state.
            self.__router(__e);
            // Drain any transitions queued by the handler.
            while self.__next_compartment.is_some() {
                let next_compartment = self.__next_compartment.take().expect("invariant: while-loop guard checked is_some()");
                // Exit the current (leaf) state. RFC-0025.1: exit args live in the
                // source state's typed ctx (written at the transition site), so the
                // synthesized `<$` event carries no payload.
                let exit_event = alloc::rc::Rc::new(FsmLexerFrameEvent::FrameExit {});
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
                        let enter_event = alloc::rc::Rc::new(FsmLexerFrameEvent::FrameEnter {});
                        self.__router(&enter_event);
                    }
                    Some(fwd) if matches!(fwd, FsmLexerFrameEvent::FrameEnter { .. }) => {
                        // Forwarded event IS $> — dispatch directly so the
                        // destination's $> handler receives the caller's payload.
                        let fwd_rc = alloc::rc::Rc::new(fwd);
                        self.__router(&fwd_rc);
                    }
                    Some(fwd) => {
                        // Forwarded event is not $> — initialize the destination
                        // with a fresh $>, then dispatch the forward.
                        let enter_event = alloc::rc::Rc::new(FsmLexerFrameEvent::FrameEnter {});
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

        fn __router(&mut self, __e: &alloc::rc::Rc<FsmLexerFrameEvent>) {
            let __ev: &FsmLexerFrameEvent = __e;
            match self.__compartment.state.as_str() {
                "Start" => self._state_Start(__ev),
                "Header" => self._state_Header(__ev),
                "ElementLevel" => self._state_ElementLevel(__ev),
                "Done" => self._state_Done(__ev),
                _ => {}
            }
        }

        fn __transition(&mut self, next_compartment: FsmLexerCompartment) {
            self.__next_compartment = Some(next_compartment);
        }

        pub fn tokenize(&mut self) {
            let __e = alloc::rc::Rc::new(FsmLexerFrameEvent::Tokenize {});
            let mut __ctx = FsmLexerFrameContext::new(alloc::rc::Rc::clone(&__e), None);
            self._context_stack.push(__ctx);
            self.__kernel(&__e);
            self._context_stack.pop();
        }

        fn _state_Start(&mut self, __e: &FsmLexerFrameEvent) {
            match __e {
                FsmLexerFrameEvent::Tokenize { .. } => { self._s_Start_hdl_user_tokenize(__e); }
                _ => {}
            }
        }

        // Lex the @@fsm header through the body-opening `{`.
        fn _state_Header(&mut self, __e: &FsmLexerFrameEvent) {
            match __e {
                FsmLexerFrameEvent::FrameEnter { .. } => { self._s_Header_hdl_frame_enter(__e); }
                _ => {}
            }
        }

        // Match-element level. `/` starts a regex literal here.
        fn _state_ElementLevel(&mut self, __e: &FsmLexerFrameEvent) {
            match __e {
                FsmLexerFrameEvent::FrameEnter { .. } => { self._s_ElementLevel_hdl_frame_enter(__e); }
                _ => {}
            }
        }

        fn _state_Done(&mut self, __e: &FsmLexerFrameEvent) {
            match __e {
                _ => {}
            }
        }

        fn _s_Start_hdl_user_tokenize(&mut self, __e: &FsmLexerFrameEvent) {
            let mut __compartment = self.__prepareEnter("Header");
            self.__transition(__compartment);
            return;
        }

        fn _s_Header_hdl_frame_enter(&mut self, __e: &FsmLexerFrameEvent) {
            let src = &self.bytes;
            let n = src.len();
            let mut pos = self.pos;
            
            // The header is a fixed shape: `@@fsm` IDENT `(` ... `)` `:`
            // TYPE `=` DEFAULT `{`. We scan token-by-token, skipping
            // whitespace, until we emit the body-opening `{`.
            loop {
                // Skip whitespace + comments.
                pos = skip_ws_comments(src, pos);
                if pos >= n {
                    self.error = Some(ParseError {
                        message: "unexpected end of input in @@fsm header".to_string(),
                        span: Span::new(pos, pos),
                    });
                    self.pos = pos;
                    let mut __compartment = self.__prepareEnter("Done");
                    self.__transition(__compartment);
                    return;
                }
            
                let b = src[pos];
            
                // `@@fsm` keyword.
                if b == b'@' && pos + 4 < n && &src[pos..pos + 5] == b"@@fsm" {
                    self.tokens.push(FsmToken {
                        kind: FsmTokenKind::KwFsm,
                        span: Span::new(pos, pos + 5),
                    });
                    pos += 5;
                    continue;
                }
            
                // Punctuation that can appear in a header.
                match b {
                    b'(' => { push1(&mut self.tokens, FsmTokenKind::LParen, pos); pos += 1; continue; }
                    b')' => { push1(&mut self.tokens, FsmTokenKind::RParen, pos); pos += 1; continue; }
                    b':' => { push1(&mut self.tokens, FsmTokenKind::Colon, pos); pos += 1; continue; }
                    b',' => { push1(&mut self.tokens, FsmTokenKind::Comma, pos); pos += 1; continue; }
                    b'=' => { push1(&mut self.tokens, FsmTokenKind::Eq, pos); pos += 1; continue; }
                    b'{' => {
                        // Body opens. Emit LBrace and switch to element level.
                        push1(&mut self.tokens, FsmTokenKind::LBrace, pos);
                        pos += 1;
                        self.pos = pos;
                        let mut __compartment = self.__prepareEnter("ElementLevel");
                        self.__transition(__compartment);
                        return;
                    }
                    _ => {}
                }
            
                // Identifier / keyword (true/false). Header identifiers
                // are the name, parameter names, type names, and the
                // boolean default literal.
                if b.is_ascii_alphabetic() || b == b'_' {
                    let start = pos;
                    while pos < n && (src[pos].is_ascii_alphanumeric() || src[pos] == b'_') {
                        pos += 1;
                    }
                    let word = std::str::from_utf8(&src[start..pos]).unwrap_or("");
                    let kind = match word {
                        "true" => FsmTokenKind::KwTrue,
                        "false" => FsmTokenKind::KwFalse,
                        other => FsmTokenKind::Ident(other.to_string()),
                    };
                    self.tokens.push(FsmToken { kind, span: Span::new(start, pos) });
                    continue;
                }
            
                // Integer literal (header default exprs like `= 0`).
                if b.is_ascii_digit() {
                    let start = pos;
                    while pos < n && src[pos].is_ascii_digit() {
                        pos += 1;
                    }
                    let text = std::str::from_utf8(&src[start..pos]).unwrap_or("0");
                    let val: i64 = text.parse().unwrap_or(0);
                    self.tokens.push(FsmToken {
                        kind: FsmTokenKind::IntLit(val),
                        span: Span::new(start, pos),
                    });
                    continue;
                }
            
                // Unexpected byte in header.
                self.error = Some(ParseError {
                    message: format!("unexpected byte '{}' in @@fsm header", b as char),
                    span: Span::new(pos, pos + 1),
                });
                self.pos = pos;
                let mut __compartment = self.__prepareEnter("Done");
                self.__transition(__compartment);
                return;
            }
        }

        fn _s_ElementLevel_hdl_frame_enter(&mut self, __e: &FsmLexerFrameEvent) {
            let src = &self.bytes;
            let n = src.len();
            let mut pos = self.pos;
            
            loop {
                pos = skip_ws_comments(src, pos);
                if pos >= n {
                    self.tokens.push(FsmToken {
                        kind: FsmTokenKind::Eof,
                        span: Span::new(pos, pos),
                    });
                    self.pos = pos;
                    let mut __compartment = self.__prepareEnter("Done");
                    self.__transition(__compartment);
                    return;
                }
            
                let b = src[pos];
            
                // Regex literal: scan to the matching unescaped `/`.
                if b == b'/' {
                    let start = pos;
                    pos += 1; // opening /
                    let body_start = pos;
                    while pos < n && src[pos] != b'/' {
                        if src[pos] == b'\\' && pos + 1 < n {
                            pos += 2; // escaped char (incl. \/)
                        } else {
                            pos += 1;
                        }
                    }
                    if pos >= n {
                        self.error = Some(ParseError {
                            message: "unterminated regex literal".to_string(),
                            span: Span::new(start, pos),
                        });
                        self.pos = pos;
                        let mut __compartment = self.__prepareEnter("Done");
                        self.__transition(__compartment);
                        return;
                    }
                    let body = std::str::from_utf8(&src[body_start..pos])
                        .unwrap_or("")
                        .to_string();
                    pos += 1; // closing /
                    self.tokens.push(FsmToken {
                        kind: FsmTokenKind::RegexLiteral(body),
                        span: Span::new(start, pos),
                    });
                    continue;
                }
            
                // Body close.
                if b == b'}' {
                    push1(&mut self.tokens, FsmTokenKind::RBrace, pos);
                    pos += 1;
                    continue;
                }
            
                // Identifier / keyword — a bare expression at element
                // level. v1: emit the keyword/ident directly. (TODO:
                // transition into $ExprLevel for multi-token bare
                // expressions with operators.)
                if b.is_ascii_alphabetic() || b == b'_' {
                    let start = pos;
                    while pos < n && (src[pos].is_ascii_alphanumeric() || src[pos] == b'_') {
                        pos += 1;
                    }
                    let word = std::str::from_utf8(&src[start..pos]).unwrap_or("");
                    let kind = match word {
                        "true" => FsmTokenKind::KwTrue,
                        "false" => FsmTokenKind::KwFalse,
                        other => FsmTokenKind::Ident(other.to_string()),
                    };
                    self.tokens.push(FsmToken { kind, span: Span::new(start, pos) });
                    continue;
                }
            
                // Unexpected byte at element level (v1 — broader handling TODO).
                self.error = Some(ParseError {
                    message: format!("unexpected byte '{}' at match-element level", b as char),
                    span: Span::new(pos, pos + 1),
                });
                self.pos = pos;
                let mut __compartment = self.__prepareEnter("Done");
                self.__transition(__compartment);
                return;
            }
        }
    }
}
pub use _fsm_lexer_framec::*;
