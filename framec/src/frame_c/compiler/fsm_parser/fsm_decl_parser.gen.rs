
// RFC-0042 — the @@fsm declaration parser as a Frame @@system.
//
// Root of the fsm parser tree. Consumes the FsmTokenStream produced by
// FsmLexer and builds an FsmDeclAst. Per the cooperating-systems design,
// child concerns (states, matches, stages, action blocks, expressions)
// split into their own @@system FSMs as fixtures grow; v1 handles the
// smoke fixture inline across three states:
//
//   $Header — `@@fsm Name(params) : Type = default {`
//   $Body   — the body: one implicit (unlabeled) state holding one match,
//             whose elements are stages (/regex/) and a trailing bare
//             expression. Consumes through the closing `}`.
//   $Done   — terminal; `result` holds the FsmDeclAst or `error` is set.
//
// v1 SCOPE: parses
//   @@fsm M(text: bytes) : bool = false { /a/ true }
// into a complete FsmDeclAst. TODO (expand as fixtures require, splitting
// into child FSMs): labeled states, multiple matches (`|`), transition
// clauses, embedding actions, action/domain blocks, multi-token
// expressions (delegate to ExpressionParser), stage labels, @@: probes.
//
// Token walking is native Rust against `self.tokens` (Option<FsmTokenStream>),
// mirroring how system_backbone.frs drives `self.parser`. Errors thread
// through `error`; the wrapper lifts `result`/`error` out.

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
mod _fsm_decl_parser_framec {
    use super::*;
    extern crate alloc;
    use alloc::{vec, format};
    #[derive(Clone, Debug)]
    #[allow(dead_code, non_camel_case_types)]
    enum FsmDeclParserFrameEvent {
        Parse {  },
        FrameEnter {},
        FrameExit {},
    }

    #[derive(Clone)]
    #[allow(dead_code, non_camel_case_types)]
    enum FsmDeclParserFrameReturn {
        _Lifecycle(alloc::rc::Rc<dyn core::any::Any>),
    }

    #[allow(dead_code)]
    impl FsmDeclParserFrameEvent {
        fn name(&self) -> &'static str {
            match self {
                FsmDeclParserFrameEvent::Parse { .. } => "parse",
                FsmDeclParserFrameEvent::FrameEnter { .. } => "$>",
                FsmDeclParserFrameEvent::FrameExit { .. } => "<$",
            }
        }
    }

    #[derive(Clone, Debug)]
    #[allow(dead_code, non_camel_case_types)]
    enum FsmDeclParserFrameValue {
        Int(i64),
        Float(f64),
        Bool(bool),
        Str(String),
        List(Vec<Self>),
        Dict(alloc::collections::BTreeMap<String, Self>),
    }

    #[allow(dead_code, non_camel_case_types)]
    struct FsmDeclParserFrameContext {
        event: alloc::rc::Rc<FsmDeclParserFrameEvent>,
        _return: Option<FsmDeclParserFrameReturn>,
        _data: alloc::collections::BTreeMap<String, FsmDeclParserFrameValue>,
        _transitioned: bool,
    }

    impl FsmDeclParserFrameContext {
        fn new(event: alloc::rc::Rc<FsmDeclParserFrameEvent>, default_return: Option<FsmDeclParserFrameReturn>) -> Self {
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
    enum FsmDeclParserStateContext {
        Start,
        Header,
        Body,
        Done,
        __NoContext,
    }

    impl Default for FsmDeclParserStateContext {
        fn default() -> Self {
            FsmDeclParserStateContext::Start
        }
    }

    #[allow(dead_code, non_camel_case_types)]
    #[derive(Clone)]
    struct FsmDeclParserCompartment {
        state: String,
        state_context: FsmDeclParserStateContext,
        forward_event: Option<FsmDeclParserFrameEvent>,
        parent_compartment: Option<Box<FsmDeclParserCompartment>>,
    }

    impl FsmDeclParserCompartment {
        fn new(state: &str) -> Self {
            let state_context = match state {
                "Start" => FsmDeclParserStateContext::Start,
                "Header" => FsmDeclParserStateContext::Header,
                "Body" => FsmDeclParserStateContext::Body,
                "Done" => FsmDeclParserStateContext::Done,
                _ => FsmDeclParserStateContext::__NoContext,
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
    pub struct FsmDeclParser {
        _state_stack: Vec<FsmDeclParserCompartment>,
        __compartment: FsmDeclParserCompartment,
        __next_compartment: Option<FsmDeclParserCompartment>,
        _context_stack: Vec<FsmDeclParserFrameContext>,
        pub tokens: Option<FsmTokenStream>,
        pub name: String,
        pub params: Vec<FsmParameter>,
        pub return_type: Type,
        pub default_expr: String,
        pub states: Vec<FsmStateAst>,
        pub result: Option<FsmDeclAst>,
        pub error: Option<ParseError>,
    }

    #[allow(non_snake_case)]
    impl FsmDeclParser {
        pub fn new() -> Self {
            Self {
                _state_stack: Vec::new(),
                _context_stack: Vec::new(),
                tokens: None,
                name: String::new(),
                params: Vec::new(),
                return_type: Type::Unknown,
                default_expr: String::new(),
                states: Vec::new(),
                result: None,
                error: None,
                __compartment: FsmDeclParserCompartment::new("Start"),
                __next_compartment: None,
            }
        }

        pub fn __create() -> Self {
            let mut c = Self::new();
            c.__compartment = c.__prepareEnter("Start");
            let __e = alloc::rc::Rc::new(FsmDeclParserFrameEvent::FrameEnter {});
            let __ctx = FsmDeclParserFrameContext::new(alloc::rc::Rc::clone(&__e), None);
            c._context_stack.push(__ctx);
            c.__kernel(&__e);
            c._context_stack.pop();
            c
        }

        fn __hsm_chain(&mut self, leaf: &str) -> &'static [&'static str] {
            match leaf {
                "Start" => &["Start"],
                "Header" => &["Header"],
                "Body" => &["Body"],
                "Done" => &["Done"],
                _ => &[],
            }
        }

        fn __prepareEnter(&mut self, leaf: &str) -> FsmDeclParserCompartment {
            let chain = self.__hsm_chain(leaf);
            let mut comp: Option<FsmDeclParserCompartment> = None;
            for name in chain.iter() {
                let mut new_comp = FsmDeclParserCompartment::new(name);
                if let Some(parent) = comp.take() {
                    new_comp.parent_compartment = Some(Box::new(parent));
                }
                comp = Some(new_comp);
            }
            comp.expect("chain must contain at least the leaf state")
        }

        fn __kernel(&mut self, __e: &alloc::rc::Rc<FsmDeclParserFrameEvent>) {
            // Route event to current state.
            self.__router(__e);
            // Drain any transitions queued by the handler.
            while self.__next_compartment.is_some() {
                let next_compartment = self.__next_compartment.take().expect("invariant: while-loop guard checked is_some()");
                // Exit the current (leaf) state. RFC-0025.1: exit args live in the
                // source state's typed ctx (written at the transition site), so the
                // synthesized `<$` event carries no payload.
                let exit_event = alloc::rc::Rc::new(FsmDeclParserFrameEvent::FrameExit {});
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
                        let enter_event = alloc::rc::Rc::new(FsmDeclParserFrameEvent::FrameEnter {});
                        self.__router(&enter_event);
                    }
                    Some(fwd) if matches!(fwd, FsmDeclParserFrameEvent::FrameEnter { .. }) => {
                        // Forwarded event IS $> — dispatch directly so the
                        // destination's $> handler receives the caller's payload.
                        let fwd_rc = alloc::rc::Rc::new(fwd);
                        self.__router(&fwd_rc);
                    }
                    Some(fwd) => {
                        // Forwarded event is not $> — initialize the destination
                        // with a fresh $>, then dispatch the forward.
                        let enter_event = alloc::rc::Rc::new(FsmDeclParserFrameEvent::FrameEnter {});
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

        fn __router(&mut self, __e: &alloc::rc::Rc<FsmDeclParserFrameEvent>) {
            let __ev: &FsmDeclParserFrameEvent = __e;
            match self.__compartment.state.as_str() {
                "Start" => self._state_Start(__ev),
                "Header" => self._state_Header(__ev),
                "Body" => self._state_Body(__ev),
                "Done" => self._state_Done(__ev),
                _ => {}
            }
        }

        fn __transition(&mut self, next_compartment: FsmDeclParserCompartment) {
            self.__next_compartment = Some(next_compartment);
        }

        pub fn parse(&mut self) {
            let __e = alloc::rc::Rc::new(FsmDeclParserFrameEvent::Parse {});
            let mut __ctx = FsmDeclParserFrameContext::new(alloc::rc::Rc::clone(&__e), None);
            self._context_stack.push(__ctx);
            self.__kernel(&__e);
            self._context_stack.pop();
        }

        fn _state_Start(&mut self, __e: &FsmDeclParserFrameEvent) {
            match __e {
                FsmDeclParserFrameEvent::Parse { .. } => { self._s_Start_hdl_user_parse(__e); }
                _ => {}
            }
        }

        fn _state_Header(&mut self, __e: &FsmDeclParserFrameEvent) {
            match __e {
                FsmDeclParserFrameEvent::FrameEnter { .. } => { self._s_Header_hdl_frame_enter(__e); }
                _ => {}
            }
        }

        fn _state_Body(&mut self, __e: &FsmDeclParserFrameEvent) {
            match __e {
                FsmDeclParserFrameEvent::FrameEnter { .. } => { self._s_Body_hdl_frame_enter(__e); }
                _ => {}
            }
        }

        fn _state_Done(&mut self, __e: &FsmDeclParserFrameEvent) {
            match __e {
                _ => {}
            }
        }

        fn _s_Start_hdl_user_parse(&mut self, __e: &FsmDeclParserFrameEvent) {
            let mut __compartment = self.__prepareEnter("Header");
            self.__transition(__compartment);
            return;
        }

        fn _s_Header_hdl_frame_enter(&mut self, __e: &FsmDeclParserFrameEvent) {
            let ts = self.tokens.as_mut().unwrap();
            
            // `@@fsm`
            if !ts.eat(&FsmTokenKind::KwFsm) {
                self.error = Some(ParseError {
                    message: "expected `@@fsm`".to_string(),
                    span: ts.cur_span(),
                });
                let mut __compartment = self.__prepareEnter("Done");
                self.__transition(__compartment);
                return;
            }
            
            // construct name
            match ts.peek_kind() {
                FsmTokenKind::Ident(n) => { self.name = n; ts.advance(); }
                _ => {
                    self.error = Some(ParseError {
                        message: "expected @@fsm name".to_string(),
                        span: ts.cur_span(),
                    });
                    let mut __compartment = self.__prepareEnter("Done");
                    self.__transition(__compartment);
                    return;
                }
            }
            
            // `(`
            if !ts.eat(&FsmTokenKind::LParen) {
                self.error = Some(ParseError {
                    message: "expected `(` after @@fsm name".to_string(),
                    span: ts.cur_span(),
                });
                let mut __compartment = self.__prepareEnter("Done");
                self.__transition(__compartment);
                return;
            }
            
            // parameter list: ( name : type [= default] (, ...)* )?
            if !ts.at(&FsmTokenKind::RParen) {
                loop {
                    let pstart = ts.cur_span();
            
                    let pname = match ts.peek_kind() {
                        FsmTokenKind::Ident(n) => { ts.advance(); n }
                        _ => {
                            self.error = Some(ParseError {
                                message: "expected parameter name".to_string(),
                                span: ts.cur_span(),
                            });
                            let mut __compartment = self.__prepareEnter("Done");
                            self.__transition(__compartment);
                            return;
                        }
                    };
            
                    if !ts.eat(&FsmTokenKind::Colon) {
                        self.error = Some(ParseError {
                            message: format!("expected `:` after parameter `{}`", pname),
                            span: ts.cur_span(),
                        });
                        let mut __compartment = self.__prepareEnter("Done");
                        self.__transition(__compartment);
                        return;
                    }
            
                    let ptype = match ts.peek_kind() {
                        FsmTokenKind::Ident(t) => { ts.advance(); Type::Custom(t) }
                        _ => {
                            self.error = Some(ParseError {
                                message: format!("expected type for parameter `{}`", pname),
                                span: ts.cur_span(),
                            });
                            let mut __compartment = self.__prepareEnter("Done");
                            self.__transition(__compartment);
                            return;
                        }
                    };
            
                    // optional `= default` (single primary token in v1)
                    let pdefault = if ts.eat(&FsmTokenKind::Eq) {
                        let d = token_text(&ts.peek_kind());
                        ts.advance();
                        Some(d)
                    } else {
                        None
                    };
            
                    let pend = ts.cur_span();
                    self.params.push(FsmParameter {
                        name: pname,
                        param_type: ptype,
                        default: pdefault,
                        span: Span::new(pstart.start, pend.start),
                    });
            
                    if ts.eat(&FsmTokenKind::Comma) {
                        continue;
                    }
                    break;
                }
            }
            
            // `)`
            if !ts.eat(&FsmTokenKind::RParen) {
                self.error = Some(ParseError {
                    message: "expected `)` to close parameter list".to_string(),
                    span: ts.cur_span(),
                });
                let mut __compartment = self.__prepareEnter("Done");
                self.__transition(__compartment);
                return;
            }
            
            // `:` return type
            if !ts.eat(&FsmTokenKind::Colon) {
                self.error = Some(ParseError {
                    message: "expected `:` before return type".to_string(),
                    span: ts.cur_span(),
                });
                let mut __compartment = self.__prepareEnter("Done");
                self.__transition(__compartment);
                return;
            }
            match ts.peek_kind() {
                FsmTokenKind::Ident(t) => { self.return_type = Type::Custom(t); ts.advance(); }
                _ => {
                    self.error = Some(ParseError {
                        message: "expected return type".to_string(),
                        span: ts.cur_span(),
                    });
                    let mut __compartment = self.__prepareEnter("Done");
                    self.__transition(__compartment);
                    return;
                }
            }
            
            // `=` default value (single primary token in v1)
            if !ts.eat(&FsmTokenKind::Eq) {
                self.error = Some(ParseError {
                    message: "expected `=` and a mandatory default value".to_string(),
                    span: ts.cur_span(),
                });
                let mut __compartment = self.__prepareEnter("Done");
                self.__transition(__compartment);
                return;
            }
            self.default_expr = token_text(&ts.peek_kind());
            ts.advance();
            
            // `{` body open
            if !ts.eat(&FsmTokenKind::LBrace) {
                self.error = Some(ParseError {
                    message: "expected `{` to open the @@fsm body".to_string(),
                    span: ts.cur_span(),
                });
                let mut __compartment = self.__prepareEnter("Done");
                self.__transition(__compartment);
                return;
            }
            
            let mut __compartment = self.__prepareEnter("Body");
            self.__transition(__compartment);
            return;
        }

        fn _s_Body_hdl_frame_enter(&mut self, __e: &FsmDeclParserFrameEvent) {
            let body_start = self.tokens.as_ref().unwrap().cur_span();
            
            // v1: a single implicit (unlabeled) start state with one
            // match. Collect elements until `}`. The token stream is
            // accessed via short-lived borrows so it can be shuttled
            // into the child ExpressionParser for bare expressions.
            let mut elements: Vec<MatchElement> = Vec::new();
            
            loop {
                let next = self.tokens.as_ref().unwrap().peek_kind();
                match next {
                    FsmTokenKind::RBrace => {
                        self.tokens.as_mut().unwrap().advance();
                        break;
                    }
                    FsmTokenKind::Eof => {
                        self.error = Some(ParseError {
                            message: "unexpected end of input; expected `}`".to_string(),
                            span: self.tokens.as_ref().unwrap().cur_span(),
                        });
                        let mut __compartment = self.__prepareEnter("Done");
                        self.__transition(__compartment);
                        return;
                    }
                    FsmTokenKind::RegexLiteral(body) => {
                        let ts = self.tokens.as_mut().unwrap();
                        let sp = ts.cur_span();
                        ts.advance();
                        elements.push(MatchElement::Stage(StageAst {
                            label: None,
                            regex: body,
                            embedding_actions: Vec::new(),
                            span: sp,
                        }));
                    }
                    _ => {
                        // Bare expression — delegate to the child
                        // ExpressionParser (token-stream shuttle).
                        let sp = self.tokens.as_ref().unwrap().cur_span();
                        let mut child = ExpressionParser::__create();
                        child.tokens = self.tokens.take();
                        child.parse();
                        self.tokens = child.tokens.take();
                        if let Some(e) = child.error.take() {
                            self.error = Some(e);
                            let mut __compartment = self.__prepareEnter("Done");
                            self.__transition(__compartment);
                            return;
                        }
                        let expr = child
                            .result
                            .take()
                            .expect("child ExpressionParser sets result when no error");
                        elements.push(MatchElement::BareExpression { expr, span: sp });
                    }
                }
            }
            
            let body_end = self.tokens.as_ref().unwrap().cur_span();
            let span = Span::new(body_start.start, body_end.start);
            
            self.states.push(FsmStateAst {
                label: None,
                matches: vec![MatchAst {
                    elements,
                    transition: None,
                    span: span.clone(),
                }],
                span: span.clone(),
            });
            
            self.result = Some(FsmDeclAst {
                name: std::mem::take(&mut self.name),
                attributes: Vec::new(),
                params: std::mem::take(&mut self.params),
                return_type: std::mem::replace(&mut self.return_type, Type::Unknown),
                default_expr: std::mem::take(&mut self.default_expr),
                states: std::mem::take(&mut self.states),
                actions: None,
                domain: None,
                span,
            });
            
            let mut __compartment = self.__prepareEnter("Done");
            self.__transition(__compartment);
            return;
        }
    }
}
pub use _fsm_decl_parser_framec::*;
