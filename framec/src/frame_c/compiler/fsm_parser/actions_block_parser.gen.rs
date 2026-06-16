
// RFC-0042 §3.7 — the `actions:` section parser as a Frame @@system child
// of the fsm parser tree. Parses a sequence of action declarations:
//
//   action_decl ::= name "(" (param ("," param)*)? ")" (":" type)? block
//   param       ::= name ":" type
//
// into an FsmActionsBlock. The `actions:` keyword is consumed by the
// caller (FsmDeclParser); this parser begins at the first declaration and
// stops at `domain:` (KwDomain) or the body-closing `}` (both left
// unconsumed). Each action body is parsed by an ActionBlockParser child.
//
// Composition contract:
//   - input:  `tokens` set by the parent, positioned at the first decl.
//   - output: `result` (Some<FsmActionsBlock>) or `error`.

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
mod _actions_block_parser_framec {
    use super::*;
    extern crate alloc;
    use alloc::{vec, format};
    #[derive(Clone, Debug)]
    #[allow(dead_code, non_camel_case_types)]
    enum ActionsBlockParserFrameEvent {
        Parse {  },
        FrameEnter {},
        FrameExit {},
    }

    #[derive(Clone)]
    #[allow(dead_code, non_camel_case_types)]
    enum ActionsBlockParserFrameReturn {
        _Lifecycle(alloc::rc::Rc<dyn core::any::Any>),
    }

    #[allow(dead_code)]
    impl ActionsBlockParserFrameEvent {
        fn name(&self) -> &'static str {
            match self {
                ActionsBlockParserFrameEvent::Parse { .. } => "parse",
                ActionsBlockParserFrameEvent::FrameEnter { .. } => "$>",
                ActionsBlockParserFrameEvent::FrameExit { .. } => "<$",
            }
        }
    }

    #[derive(Clone, Debug)]
    #[allow(dead_code, non_camel_case_types)]
    enum ActionsBlockParserFrameValue {
        Int(i64),
        Float(f64),
        Bool(bool),
        Str(String),
        List(Vec<Self>),
        Dict(alloc::collections::BTreeMap<String, Self>),
    }

    #[allow(dead_code, non_camel_case_types)]
    struct ActionsBlockParserFrameContext {
        event: alloc::rc::Rc<ActionsBlockParserFrameEvent>,
        _return: Option<ActionsBlockParserFrameReturn>,
        _data: alloc::collections::BTreeMap<String, ActionsBlockParserFrameValue>,
        _transitioned: bool,
    }

    impl ActionsBlockParserFrameContext {
        fn new(event: alloc::rc::Rc<ActionsBlockParserFrameEvent>, default_return: Option<ActionsBlockParserFrameReturn>) -> Self {
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
    enum ActionsBlockParserStateContext {
        Start,
        Decls,
        Done,
        __NoContext,
    }

    impl Default for ActionsBlockParserStateContext {
        fn default() -> Self {
            ActionsBlockParserStateContext::Start
        }
    }

    #[allow(dead_code, non_camel_case_types)]
    #[derive(Clone)]
    struct ActionsBlockParserCompartment {
        state: String,
        state_context: ActionsBlockParserStateContext,
        forward_event: Option<ActionsBlockParserFrameEvent>,
        parent_compartment: Option<Box<ActionsBlockParserCompartment>>,
    }

    impl ActionsBlockParserCompartment {
        fn new(state: &str) -> Self {
            let state_context = match state {
                "Start" => ActionsBlockParserStateContext::Start,
                "Decls" => ActionsBlockParserStateContext::Decls,
                "Done" => ActionsBlockParserStateContext::Done,
                _ => ActionsBlockParserStateContext::__NoContext,
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
    pub struct ActionsBlockParser {
        _state_stack: Vec<ActionsBlockParserCompartment>,
        __compartment: ActionsBlockParserCompartment,
        __next_compartment: Option<ActionsBlockParserCompartment>,
        _context_stack: Vec<ActionsBlockParserFrameContext>,
        pub tokens: Option<FsmTokenStream>,
        pub decls: Vec<FsmActionDecl>,
        pub span_start: Span,
        pub result: Option<FsmActionsBlock>,
        pub error: Option<ParseError>,
        pub error_code: Option<&'static str>,
    }

    #[allow(non_snake_case)]
    impl ActionsBlockParser {
        pub fn new() -> Self {
            Self {
                _state_stack: Vec::new(),
                _context_stack: Vec::new(),
                tokens: None,
                decls: Vec::new(),
                span_start: Span::new(0, 0),
                result: None,
                error: None,
                error_code: None,
                __compartment: ActionsBlockParserCompartment::new("Start"),
                __next_compartment: None,
            }
        }

        pub fn __create() -> Self {
            let mut c = Self::new();
            c.__compartment = c.__prepareEnter("Start");
            let __e = alloc::rc::Rc::new(ActionsBlockParserFrameEvent::FrameEnter {});
            let __ctx = ActionsBlockParserFrameContext::new(alloc::rc::Rc::clone(&__e), None);
            c._context_stack.push(__ctx);
            c.__kernel(&__e);
            c._context_stack.pop();
            c
        }

        fn __hsm_chain(&mut self, leaf: &str) -> &'static [&'static str] {
            match leaf {
                "Start" => &["Start"],
                "Decls" => &["Decls"],
                "Done" => &["Done"],
                _ => &[],
            }
        }

        fn __prepareEnter(&mut self, leaf: &str) -> ActionsBlockParserCompartment {
            let chain = self.__hsm_chain(leaf);
            let mut comp: Option<ActionsBlockParserCompartment> = None;
            for name in chain.iter() {
                let mut new_comp = ActionsBlockParserCompartment::new(name);
                if let Some(parent) = comp.take() {
                    new_comp.parent_compartment = Some(Box::new(parent));
                }
                comp = Some(new_comp);
            }
            comp.expect("chain must contain at least the leaf state")
        }

        fn __kernel(&mut self, __e: &alloc::rc::Rc<ActionsBlockParserFrameEvent>) {
            // Route event to current state.
            self.__router(__e);
            // Drain any transitions queued by the handler.
            while self.__next_compartment.is_some() {
                let next_compartment = self.__next_compartment.take().expect("invariant: while-loop guard checked is_some()");
                // Exit the current (leaf) state. RFC-0025.1: exit args live in the
                // source state's typed ctx (written at the transition site), so the
                // synthesized `<$` event carries no payload.
                let exit_event = alloc::rc::Rc::new(ActionsBlockParserFrameEvent::FrameExit {});
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
                        let enter_event = alloc::rc::Rc::new(ActionsBlockParserFrameEvent::FrameEnter {});
                        self.__router(&enter_event);
                    }
                    Some(fwd) if matches!(fwd, ActionsBlockParserFrameEvent::FrameEnter { .. }) => {
                        // Forwarded event IS $> — dispatch directly so the
                        // destination's $> handler receives the caller's payload.
                        let fwd_rc = alloc::rc::Rc::new(fwd);
                        self.__router(&fwd_rc);
                    }
                    Some(fwd) => {
                        // Forwarded event is not $> — initialize the destination
                        // with a fresh $>, then dispatch the forward.
                        let enter_event = alloc::rc::Rc::new(ActionsBlockParserFrameEvent::FrameEnter {});
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

        fn __router(&mut self, __e: &alloc::rc::Rc<ActionsBlockParserFrameEvent>) {
            let __ev: &ActionsBlockParserFrameEvent = __e;
            match self.__compartment.state.as_str() {
                "Start" => self._state_Start(__ev),
                "Decls" => self._state_Decls(__ev),
                "Done" => self._state_Done(__ev),
                _ => {}
            }
        }

        fn __transition(&mut self, next_compartment: ActionsBlockParserCompartment) {
            self.__next_compartment = Some(next_compartment);
        }

        pub fn parse(&mut self) {
            let __e = alloc::rc::Rc::new(ActionsBlockParserFrameEvent::Parse {});
            let mut __ctx = ActionsBlockParserFrameContext::new(alloc::rc::Rc::clone(&__e), None);
            self._context_stack.push(__ctx);
            self.__kernel(&__e);
            self._context_stack.pop();
        }

        fn _state_Start(&mut self, __e: &ActionsBlockParserFrameEvent) {
            match __e {
                ActionsBlockParserFrameEvent::Parse { .. } => { self._s_Start_hdl_user_parse(__e); }
                _ => {}
            }
        }

        fn _state_Decls(&mut self, __e: &ActionsBlockParserFrameEvent) {
            match __e {
                ActionsBlockParserFrameEvent::FrameEnter { .. } => { self._s_Decls_hdl_frame_enter(__e); }
                _ => {}
            }
        }

        fn _state_Done(&mut self, __e: &ActionsBlockParserFrameEvent) {
            match __e {
                _ => {}
            }
        }

        fn _s_Start_hdl_user_parse(&mut self, __e: &ActionsBlockParserFrameEvent) {
            let mut __compartment = self.__prepareEnter("Decls");
            self.__transition(__compartment);
            return;
        }

        fn _s_Decls_hdl_frame_enter(&mut self, __e: &ActionsBlockParserFrameEvent) {
            loop {
                // An action declaration begins with its name (Ident).
                // Anything else — a following section keyword
                // (`domain:`/`actions:`), a state label, the body close
                // `}`, or EOF — ends the section. We leave that token
                // unconsumed so FsmDeclParser regains control and applies
                // the canonical-order (E710) and at-most-once (E711) rules.
                if !matches!(
                    self.tokens.as_ref().unwrap().peek_kind(),
                    FsmTokenKind::Ident(_)
                ) {
                    let span = self.span_start.clone();
                    self.result = Some(FsmActionsBlock {
                        actions: std::mem::take(&mut self.decls),
                        span,
                    });
                    let mut __compartment = self.__prepareEnter("Done");
                    self.__transition(__compartment);
                    return;
                }
            
                let dsp = self.tokens.as_ref().unwrap().cur_span();
            
                // action name
                let name = match self.tokens.as_ref().unwrap().peek_kind() {
                    FsmTokenKind::Ident(n) => n,
                    _ => {
                        self.error = Some(ParseError {
                            message: "expected an action name".to_string(),
                            span: dsp,
                        });
                        let mut __compartment = self.__prepareEnter("Done");
                        self.__transition(__compartment);
                        return;
                    }
                };
                self.tokens.as_mut().unwrap().advance();
            
                // `(`
                if !self.tokens.as_mut().unwrap().eat(&FsmTokenKind::LParen) {
                    self.error = Some(ParseError {
                        message: format!("expected `(` after action `{}`", name),
                        span: self.tokens.as_ref().unwrap().cur_span(),
                    });
                    let mut __compartment = self.__prepareEnter("Done");
                    self.__transition(__compartment);
                    return;
                }
            
                // parameter list: name : type (, ...)*
                let mut params: Vec<FsmParameter> = Vec::new();
                if !self.tokens.as_ref().unwrap().at(&FsmTokenKind::RParen) {
                    loop {
                        let psp = self.tokens.as_ref().unwrap().cur_span();
                        let pname = match self.tokens.as_ref().unwrap().peek_kind() {
                            FsmTokenKind::Ident(p) => p,
                            _ => {
                                self.error = Some(ParseError {
                                    message: "expected a parameter name".to_string(),
                                    span: self.tokens.as_ref().unwrap().cur_span(),
                                });
                                let mut __compartment = self.__prepareEnter("Done");
                                self.__transition(__compartment);
                                return;
                            }
                        };
                        self.tokens.as_mut().unwrap().advance();
                        if !self.tokens.as_mut().unwrap().eat(&FsmTokenKind::Colon) {
                            self.error = Some(ParseError {
                                message: format!("expected `:` after parameter `{}`", pname),
                                span: self.tokens.as_ref().unwrap().cur_span(),
                            });
                            let mut __compartment = self.__prepareEnter("Done");
                            self.__transition(__compartment);
                            return;
                        }
                        let ptype = match self.tokens.as_ref().unwrap().peek_kind() {
                            FsmTokenKind::Ident(t) => Type::Custom(t),
                            _ => {
                                self.error = Some(ParseError {
                                    message: format!("expected a type for parameter `{}`", pname),
                                    span: self.tokens.as_ref().unwrap().cur_span(),
                                });
                                let mut __compartment = self.__prepareEnter("Done");
                                self.__transition(__compartment);
                                return;
                            }
                        };
                        self.tokens.as_mut().unwrap().advance();
                        params.push(FsmParameter {
                            name: pname,
                            param_type: ptype,
                            default: None,
                            span: psp,
                        });
                        if self.tokens.as_mut().unwrap().eat(&FsmTokenKind::Comma) {
                            continue;
                        }
                        break;
                    }
                }
            
                // `)`
                if !self.tokens.as_mut().unwrap().eat(&FsmTokenKind::RParen) {
                    self.error = Some(ParseError {
                        message: format!("expected `)` to close `{}`'s parameter list", name),
                        span: self.tokens.as_ref().unwrap().cur_span(),
                    });
                    let mut __compartment = self.__prepareEnter("Done");
                    self.__transition(__compartment);
                    return;
                }
            
                // optional `: type` return type
                let return_type = if self.tokens.as_mut().unwrap().eat(&FsmTokenKind::Colon) {
                    match self.tokens.as_ref().unwrap().peek_kind() {
                        FsmTokenKind::Ident(t) => {
                            self.tokens.as_mut().unwrap().advance();
                            Some(Type::Custom(t))
                        }
                        _ => {
                            self.error = Some(ParseError {
                                message: format!("expected a return type for action `{}`", name),
                                span: self.tokens.as_ref().unwrap().cur_span(),
                            });
                            let mut __compartment = self.__prepareEnter("Done");
                            self.__transition(__compartment);
                            return;
                        }
                    }
                } else {
                    None
                };
            
                // body — an action block.
                let mut child = ActionBlockParser::__create();
                child.tokens = self.tokens.take();
                child.parse();
                self.tokens = child.tokens.take();
                if let Some(e) = child.error.take() {
                    self.error_code = child.error_code.take();
                    self.error = Some(e);
                    let mut __compartment = self.__prepareEnter("Done");
                    self.__transition(__compartment);
                    return;
                }
                let body = child
                    .result
                    .take()
                    .expect("child ActionBlockParser sets result when no error");
            
                self.decls.push(FsmActionDecl {
                    name,
                    params,
                    return_type,
                    body,
                    span: dsp,
                });
            }
        }
    }
}
pub use _actions_block_parser_framec::*;
