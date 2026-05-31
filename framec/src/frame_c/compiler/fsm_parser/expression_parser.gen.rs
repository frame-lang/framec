
// RFC-0043 — the expression parser as a Frame @@system child of the
// fsm parser tree.
//
// Consumes tokens from a shared FsmTokenStream (moved in by the parent
// via Option::take, moved back out when done) and builds one Expression.
// This is the first CHILD parser in the tree — it proves the
// cooperating-systems composition pattern (linear token-stream
// ownership shuttling) that the whole design rests on.
//
// v1 SCOPE: a primary expression and call expressions —
//   primary ::= literal | probe | ident | "(" expr ")"
//   call    ::= ident "(" ( expr ("," expr)* )? ")"
// which is what FSM-TEST-004 ( to_int(@@:matched) ) needs. The eight
// precedence levels (logical-or down to unary) per RFC-0043 §3.3 land
// as additional states when a fixture first exercises operators; the
// $Primary state below is the base of that future climb.
//
// Composition contract (matches the parent-child pattern in
// _scratch/rfc_0043_parser_design.md):
//   - input:  `tokens` (Option<FsmTokenStream>) set by the parent.
//   - output: `result` (Option<Expression>) on success, or `error`.
//   - the parent does: child.tokens = self.tokens.take(); child.parse();
//     self.tokens = child.tokens.take(); read child.result / child.error.

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
mod _expression_parser_framec {
    use super::*;
    extern crate alloc;
    use alloc::{vec, format};
    #[derive(Clone, Debug)]
    #[allow(dead_code, non_camel_case_types)]
    enum ExpressionParserFrameEvent {
        Parse {  },
        FrameEnter {},
        FrameExit {},
    }

    #[derive(Clone)]
    #[allow(dead_code, non_camel_case_types)]
    enum ExpressionParserFrameReturn {
        _Lifecycle(alloc::rc::Rc<dyn core::any::Any>),
    }

    #[allow(dead_code)]
    impl ExpressionParserFrameEvent {
        fn name(&self) -> &'static str {
            match self {
                ExpressionParserFrameEvent::Parse { .. } => "parse",
                ExpressionParserFrameEvent::FrameEnter { .. } => "$>",
                ExpressionParserFrameEvent::FrameExit { .. } => "<$",
            }
        }
    }

    #[derive(Clone, Debug)]
    #[allow(dead_code, non_camel_case_types)]
    enum ExpressionParserFrameValue {
        Int(i64),
        Float(f64),
        Bool(bool),
        Str(String),
        List(Vec<Self>),
        Dict(alloc::collections::BTreeMap<String, Self>),
    }

    #[allow(dead_code, non_camel_case_types)]
    struct ExpressionParserFrameContext {
        event: alloc::rc::Rc<ExpressionParserFrameEvent>,
        _return: Option<ExpressionParserFrameReturn>,
        _data: alloc::collections::BTreeMap<String, ExpressionParserFrameValue>,
        _transitioned: bool,
    }

    impl ExpressionParserFrameContext {
        fn new(event: alloc::rc::Rc<ExpressionParserFrameEvent>, default_return: Option<ExpressionParserFrameReturn>) -> Self {
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
    enum ExpressionParserStateContext {
        Start,
        Primary,
        CallArgs,
        ParenInner,
        Done,
        __NoContext,
    }

    impl Default for ExpressionParserStateContext {
        fn default() -> Self {
            ExpressionParserStateContext::Start
        }
    }

    #[allow(dead_code, non_camel_case_types)]
    #[derive(Clone)]
    struct ExpressionParserCompartment {
        state: String,
        state_context: ExpressionParserStateContext,
        forward_event: Option<ExpressionParserFrameEvent>,
        parent_compartment: Option<Box<ExpressionParserCompartment>>,
    }

    impl ExpressionParserCompartment {
        fn new(state: &str) -> Self {
            let state_context = match state {
                "Start" => ExpressionParserStateContext::Start,
                "Primary" => ExpressionParserStateContext::Primary,
                "CallArgs" => ExpressionParserStateContext::CallArgs,
                "ParenInner" => ExpressionParserStateContext::ParenInner,
                "Done" => ExpressionParserStateContext::Done,
                _ => ExpressionParserStateContext::__NoContext,
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
    pub struct ExpressionParser {
        _state_stack: Vec<ExpressionParserCompartment>,
        __compartment: ExpressionParserCompartment,
        __next_compartment: Option<ExpressionParserCompartment>,
        _context_stack: Vec<ExpressionParserFrameContext>,
        pub tokens: Option<FsmTokenStream>,
        pub pending_callee: String,
        pub call_args: Vec<Expression>,
        pub result: Option<Expression>,
        pub error: Option<ParseError>,
    }

    #[allow(non_snake_case)]
    impl ExpressionParser {
        pub fn new() -> Self {
            Self {
                _state_stack: Vec::new(),
                _context_stack: Vec::new(),
                tokens: None,
                pending_callee: String::new(),
                call_args: Vec::new(),
                result: None,
                error: None,
                __compartment: ExpressionParserCompartment::new("Start"),
                __next_compartment: None,
            }
        }

        pub fn __create() -> Self {
            let mut c = Self::new();
            c.__compartment = c.__prepareEnter("Start");
            let __e = alloc::rc::Rc::new(ExpressionParserFrameEvent::FrameEnter {});
            let __ctx = ExpressionParserFrameContext::new(alloc::rc::Rc::clone(&__e), None);
            c._context_stack.push(__ctx);
            c.__kernel(&__e);
            c._context_stack.pop();
            c
        }

        fn __hsm_chain(&mut self, leaf: &str) -> &'static [&'static str] {
            match leaf {
                "Start" => &["Start"],
                "Primary" => &["Primary"],
                "CallArgs" => &["CallArgs"],
                "ParenInner" => &["ParenInner"],
                "Done" => &["Done"],
                _ => &[],
            }
        }

        fn __prepareEnter(&mut self, leaf: &str) -> ExpressionParserCompartment {
            let chain = self.__hsm_chain(leaf);
            let mut comp: Option<ExpressionParserCompartment> = None;
            for name in chain.iter() {
                let mut new_comp = ExpressionParserCompartment::new(name);
                if let Some(parent) = comp.take() {
                    new_comp.parent_compartment = Some(Box::new(parent));
                }
                comp = Some(new_comp);
            }
            comp.expect("chain must contain at least the leaf state")
        }

        fn __kernel(&mut self, __e: &alloc::rc::Rc<ExpressionParserFrameEvent>) {
            // Route event to current state.
            self.__router(__e);
            // Drain any transitions queued by the handler.
            while self.__next_compartment.is_some() {
                let next_compartment = self.__next_compartment.take().expect("invariant: while-loop guard checked is_some()");
                // Exit the current (leaf) state. RFC-0025.1: exit args live in the
                // source state's typed ctx (written at the transition site), so the
                // synthesized `<$` event carries no payload.
                let exit_event = alloc::rc::Rc::new(ExpressionParserFrameEvent::FrameExit {});
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
                        let enter_event = alloc::rc::Rc::new(ExpressionParserFrameEvent::FrameEnter {});
                        self.__router(&enter_event);
                    }
                    Some(fwd) if matches!(fwd, ExpressionParserFrameEvent::FrameEnter { .. }) => {
                        // Forwarded event IS $> — dispatch directly so the
                        // destination's $> handler receives the caller's payload.
                        let fwd_rc = alloc::rc::Rc::new(fwd);
                        self.__router(&fwd_rc);
                    }
                    Some(fwd) => {
                        // Forwarded event is not $> — initialize the destination
                        // with a fresh $>, then dispatch the forward.
                        let enter_event = alloc::rc::Rc::new(ExpressionParserFrameEvent::FrameEnter {});
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

        fn __router(&mut self, __e: &alloc::rc::Rc<ExpressionParserFrameEvent>) {
            let __ev: &ExpressionParserFrameEvent = __e;
            match self.__compartment.state.as_str() {
                "Start" => self._state_Start(__ev),
                "Primary" => self._state_Primary(__ev),
                "CallArgs" => self._state_CallArgs(__ev),
                "ParenInner" => self._state_ParenInner(__ev),
                "Done" => self._state_Done(__ev),
                _ => {}
            }
        }

        fn __transition(&mut self, next_compartment: ExpressionParserCompartment) {
            self.__next_compartment = Some(next_compartment);
        }

        pub fn parse(&mut self) {
            let __e = alloc::rc::Rc::new(ExpressionParserFrameEvent::Parse {});
            let mut __ctx = ExpressionParserFrameContext::new(alloc::rc::Rc::clone(&__e), None);
            self._context_stack.push(__ctx);
            self.__kernel(&__e);
            self._context_stack.pop();
        }

        fn _state_Start(&mut self, __e: &ExpressionParserFrameEvent) {
            match __e {
                ExpressionParserFrameEvent::Parse { .. } => { self._s_Start_hdl_user_parse(__e); }
                _ => {}
            }
        }

        // Parse one primary, then fold any trailing call-arg list onto it.
        // (The future precedence climb will wrap this base.)
        fn _state_Primary(&mut self, __e: &ExpressionParserFrameEvent) {
            match __e {
                ExpressionParserFrameEvent::FrameEnter { .. } => { self._s_Primary_hdl_frame_enter(__e); }
                _ => {}
            }
        }

        // Parse a (possibly empty) comma-separated argument list, having
        // already consumed the callee ident and the opening `(`. Each
        // argument is parsed by a fresh child ExpressionParser — the
        // recursion that makes this a real tree.
        fn _state_CallArgs(&mut self, __e: &ExpressionParserFrameEvent) {
            match __e {
                ExpressionParserFrameEvent::FrameEnter { .. } => { self._s_CallArgs_hdl_frame_enter(__e); }
                _ => {}
            }
        }

        // Parenthesized sub-expression: parse inner via a child, expect `)`.
        fn _state_ParenInner(&mut self, __e: &ExpressionParserFrameEvent) {
            match __e {
                ExpressionParserFrameEvent::FrameEnter { .. } => { self._s_ParenInner_hdl_frame_enter(__e); }
                _ => {}
            }
        }

        fn _state_Done(&mut self, __e: &ExpressionParserFrameEvent) {
            match __e {
                _ => {}
            }
        }

        fn _s_Start_hdl_user_parse(&mut self, __e: &ExpressionParserFrameEvent) {
            let mut __compartment = self.__prepareEnter("Primary");
            self.__transition(__compartment);
            return;
        }

        fn _s_Primary_hdl_frame_enter(&mut self, __e: &ExpressionParserFrameEvent) {
            let ts = self.tokens.as_mut().unwrap();
            let sp = ts.cur_span();
            
            match ts.peek_kind() {
                FsmTokenKind::KwTrue => {
                    ts.advance();
                    self.result = Some(Expression::Literal(Literal::Bool(true)));
                    let mut __compartment = self.__prepareEnter("Done");
                    self.__transition(__compartment);
                    return;
                }
                FsmTokenKind::KwFalse => {
                    ts.advance();
                    self.result = Some(Expression::Literal(Literal::Bool(false)));
                    let mut __compartment = self.__prepareEnter("Done");
                    self.__transition(__compartment);
                    return;
                }
                FsmTokenKind::IntLit(n) => {
                    ts.advance();
                    self.result = Some(Expression::Literal(Literal::Int(n)));
                    let mut __compartment = self.__prepareEnter("Done");
                    self.__transition(__compartment);
                    return;
                }
                FsmTokenKind::StringLit(s) => {
                    ts.advance();
                    self.result = Some(Expression::Literal(Literal::String(s)));
                    let mut __compartment = self.__prepareEnter("Done");
                    self.__transition(__compartment);
                    return;
                }
                FsmTokenKind::Probe(name) => {
                    ts.advance();
                    // A probe is a leaf reference; surface it as a Var
                    // carrying the `@@:`-qualified name. (A dedicated
                    // Expression::Probe variant may replace this later.)
                    self.result = Some(Expression::Var(format!("@@:{}", name)));
                    let mut __compartment = self.__prepareEnter("Done");
                    self.__transition(__compartment);
                    return;
                }
                FsmTokenKind::Ident(name) => {
                    ts.advance();
                    // Call expression `ident( args )` vs bare variable.
                    if ts.eat(&FsmTokenKind::LParen) {
                        self.pending_callee = name;
                        self.call_args = Vec::new();
                        let mut __compartment = self.__prepareEnter("CallArgs");
                        self.__transition(__compartment);
                        return;
                    }
                    self.result = Some(Expression::Var(name));
                    let mut __compartment = self.__prepareEnter("Done");
                    self.__transition(__compartment);
                    return;
                }
                FsmTokenKind::LParen => {
                    ts.advance();
                    // Parenthesized sub-expression: recurse via a child
                    // ExpressionParser, then expect `)`.
                    let mut __compartment = self.__prepareEnter("ParenInner");
                    self.__transition(__compartment);
                    return;
                }
                _ => {
                    self.error = Some(ParseError {
                        message: "expected an expression".to_string(),
                        span: sp,
                    });
                    let mut __compartment = self.__prepareEnter("Done");
                    self.__transition(__compartment);
                    return;
                }
            }
        }

        fn _s_CallArgs_hdl_frame_enter(&mut self, __e: &ExpressionParserFrameEvent) {
            // Empty arg list: `ident()`.
            {
                let ts = self.tokens.as_mut().unwrap();
                if ts.eat(&FsmTokenKind::RParen) {
                    self.result = Some(Expression::Call {
                        func: std::mem::take(&mut self.pending_callee),
                        args: std::mem::take(&mut self.call_args),
                    });
                    let mut __compartment = self.__prepareEnter("Done");
                    self.__transition(__compartment);
                    return;
                }
            }
            
            loop {
                // Parse one argument via a child parser (token shuttle).
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
                let arg = child
                    .result
                    .take()
                    .expect("child ExpressionParser sets result when no error");
                self.call_args.push(arg);
            
                let ts = self.tokens.as_mut().unwrap();
                if ts.eat(&FsmTokenKind::Comma) {
                    continue;
                }
                if ts.eat(&FsmTokenKind::RParen) {
                    self.result = Some(Expression::Call {
                        func: std::mem::take(&mut self.pending_callee),
                        args: std::mem::take(&mut self.call_args),
                    });
                    let mut __compartment = self.__prepareEnter("Done");
                    self.__transition(__compartment);
                    return;
                }
                self.error = Some(ParseError {
                    message: "expected `,` or `)` in call arguments".to_string(),
                    span: ts.cur_span(),
                });
                let mut __compartment = self.__prepareEnter("Done");
                self.__transition(__compartment);
                return;
            }
        }

        fn _s_ParenInner_hdl_frame_enter(&mut self, __e: &ExpressionParserFrameEvent) {
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
            let inner = child
                .result
                .take()
                .expect("child ExpressionParser sets result when no error");
            
            let ts = self.tokens.as_mut().unwrap();
            if ts.eat(&FsmTokenKind::RParen) {
                self.result = Some(inner);
                let mut __compartment = self.__prepareEnter("Done");
                self.__transition(__compartment);
                return;
            }
            self.error = Some(ParseError {
                message: "expected `)` to close parenthesized expression".to_string(),
                span: ts.cur_span(),
            });
            let mut __compartment = self.__prepareEnter("Done");
            self.__transition(__compartment);
            return;
        }
    }
}
pub use _expression_parser_framec::*;
