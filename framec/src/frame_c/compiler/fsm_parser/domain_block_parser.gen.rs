
// RFC-0042 §3.8 — the `domain:` section parser as a Frame @@system child
// of the fsm parser tree. Parses a sequence of `name: type = default`
// field declarations into an FsmDomainBlock. The `domain:` keyword is
// consumed by the caller (FsmDeclParser); this parser begins at the
// first field and stops at the body-closing `}` (left unconsumed).
//
// Each field's default initializer is a parsed expression (ExpressionParser
// child, token-stream shuttle). Field types are simple identifiers in
// v0.1 (Type::Custom); compound types (Vec<T>, etc.) land later.
//
// Composition contract:
//   - input:  `tokens` set by the parent, positioned at the first field.
//   - output: `result` (Some<FsmDomainBlock>) or `error`.

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
mod _domain_block_parser_framec {
    use super::*;
    extern crate alloc;
    use alloc::{vec, format};
    #[derive(Clone, Debug)]
    #[allow(dead_code, non_camel_case_types)]
    enum DomainBlockParserFrameEvent {
        Parse {  },
        FrameEnter {},
        FrameExit {},
    }

    #[derive(Clone)]
    #[allow(dead_code, non_camel_case_types)]
    enum DomainBlockParserFrameReturn {
        _Lifecycle(alloc::rc::Rc<dyn core::any::Any>),
    }

    #[allow(dead_code)]
    impl DomainBlockParserFrameEvent {
        fn name(&self) -> &'static str {
            match self {
                DomainBlockParserFrameEvent::Parse { .. } => "parse",
                DomainBlockParserFrameEvent::FrameEnter { .. } => "$>",
                DomainBlockParserFrameEvent::FrameExit { .. } => "<$",
            }
        }
    }

    #[derive(Clone, Debug)]
    #[allow(dead_code, non_camel_case_types)]
    enum DomainBlockParserFrameValue {
        Int(i64),
        Float(f64),
        Bool(bool),
        Str(String),
        List(Vec<Self>),
        Dict(alloc::collections::BTreeMap<String, Self>),
    }

    #[allow(dead_code, non_camel_case_types)]
    struct DomainBlockParserFrameContext {
        event: alloc::rc::Rc<DomainBlockParserFrameEvent>,
        _return: Option<DomainBlockParserFrameReturn>,
        _data: alloc::collections::BTreeMap<String, DomainBlockParserFrameValue>,
        _transitioned: bool,
    }

    impl DomainBlockParserFrameContext {
        fn new(event: alloc::rc::Rc<DomainBlockParserFrameEvent>, default_return: Option<DomainBlockParserFrameReturn>) -> Self {
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
    enum DomainBlockParserStateContext {
        Start,
        Vars,
        Done,
        __NoContext,
    }

    impl Default for DomainBlockParserStateContext {
        fn default() -> Self {
            DomainBlockParserStateContext::Start
        }
    }

    #[allow(dead_code, non_camel_case_types)]
    #[derive(Clone)]
    struct DomainBlockParserCompartment {
        state: String,
        state_context: DomainBlockParserStateContext,
        forward_event: Option<DomainBlockParserFrameEvent>,
        parent_compartment: Option<Box<DomainBlockParserCompartment>>,
    }

    impl DomainBlockParserCompartment {
        fn new(state: &str) -> Self {
            let state_context = match state {
                "Start" => DomainBlockParserStateContext::Start,
                "Vars" => DomainBlockParserStateContext::Vars,
                "Done" => DomainBlockParserStateContext::Done,
                _ => DomainBlockParserStateContext::__NoContext,
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
    pub struct DomainBlockParser {
        _state_stack: Vec<DomainBlockParserCompartment>,
        __compartment: DomainBlockParserCompartment,
        __next_compartment: Option<DomainBlockParserCompartment>,
        _context_stack: Vec<DomainBlockParserFrameContext>,
        pub tokens: Option<FsmTokenStream>,
        pub vars: Vec<FsmDomainVar>,
        pub span_start: Span,
        pub result: Option<FsmDomainBlock>,
        pub error: Option<ParseError>,
    }

    #[allow(non_snake_case)]
    impl DomainBlockParser {
        pub fn new() -> Self {
            Self {
                _state_stack: Vec::new(),
                _context_stack: Vec::new(),
                tokens: None,
                vars: Vec::new(),
                span_start: Span::new(0, 0),
                result: None,
                error: None,
                __compartment: DomainBlockParserCompartment::new("Start"),
                __next_compartment: None,
            }
        }

        pub fn __create() -> Self {
            let mut c = Self::new();
            c.__compartment = c.__prepareEnter("Start");
            let __e = alloc::rc::Rc::new(DomainBlockParserFrameEvent::FrameEnter {});
            let __ctx = DomainBlockParserFrameContext::new(alloc::rc::Rc::clone(&__e), None);
            c._context_stack.push(__ctx);
            c.__kernel(&__e);
            c._context_stack.pop();
            c
        }

        fn __hsm_chain(&mut self, leaf: &str) -> &'static [&'static str] {
            match leaf {
                "Start" => &["Start"],
                "Vars" => &["Vars"],
                "Done" => &["Done"],
                _ => &[],
            }
        }

        fn __prepareEnter(&mut self, leaf: &str) -> DomainBlockParserCompartment {
            let chain = self.__hsm_chain(leaf);
            let mut comp: Option<DomainBlockParserCompartment> = None;
            for name in chain.iter() {
                let mut new_comp = DomainBlockParserCompartment::new(name);
                if let Some(parent) = comp.take() {
                    new_comp.parent_compartment = Some(Box::new(parent));
                }
                comp = Some(new_comp);
            }
            comp.expect("chain must contain at least the leaf state")
        }

        fn __kernel(&mut self, __e: &alloc::rc::Rc<DomainBlockParserFrameEvent>) {
            // Route event to current state.
            self.__router(__e);
            // Drain any transitions queued by the handler.
            while self.__next_compartment.is_some() {
                let next_compartment = self.__next_compartment.take().expect("invariant: while-loop guard checked is_some()");
                // Exit the current (leaf) state. RFC-0025.1: exit args live in the
                // source state's typed ctx (written at the transition site), so the
                // synthesized `<$` event carries no payload.
                let exit_event = alloc::rc::Rc::new(DomainBlockParserFrameEvent::FrameExit {});
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
                        let enter_event = alloc::rc::Rc::new(DomainBlockParserFrameEvent::FrameEnter {});
                        self.__router(&enter_event);
                    }
                    Some(fwd) if matches!(fwd, DomainBlockParserFrameEvent::FrameEnter { .. }) => {
                        // Forwarded event IS $> — dispatch directly so the
                        // destination's $> handler receives the caller's payload.
                        let fwd_rc = alloc::rc::Rc::new(fwd);
                        self.__router(&fwd_rc);
                    }
                    Some(fwd) => {
                        // Forwarded event is not $> — initialize the destination
                        // with a fresh $>, then dispatch the forward.
                        let enter_event = alloc::rc::Rc::new(DomainBlockParserFrameEvent::FrameEnter {});
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

        fn __router(&mut self, __e: &alloc::rc::Rc<DomainBlockParserFrameEvent>) {
            let __ev: &DomainBlockParserFrameEvent = __e;
            match self.__compartment.state.as_str() {
                "Start" => self._state_Start(__ev),
                "Vars" => self._state_Vars(__ev),
                "Done" => self._state_Done(__ev),
                _ => {}
            }
        }

        fn __transition(&mut self, next_compartment: DomainBlockParserCompartment) {
            self.__next_compartment = Some(next_compartment);
        }

        pub fn parse(&mut self) {
            let __e = alloc::rc::Rc::new(DomainBlockParserFrameEvent::Parse {});
            let mut __ctx = DomainBlockParserFrameContext::new(alloc::rc::Rc::clone(&__e), None);
            self._context_stack.push(__ctx);
            self.__kernel(&__e);
            self._context_stack.pop();
        }

        fn _state_Start(&mut self, __e: &DomainBlockParserFrameEvent) {
            match __e {
                DomainBlockParserFrameEvent::Parse { .. } => { self._s_Start_hdl_user_parse(__e); }
                _ => {}
            }
        }

        fn _state_Vars(&mut self, __e: &DomainBlockParserFrameEvent) {
            match __e {
                DomainBlockParserFrameEvent::FrameEnter { .. } => { self._s_Vars_hdl_frame_enter(__e); }
                _ => {}
            }
        }

        fn _state_Done(&mut self, __e: &DomainBlockParserFrameEvent) {
            match __e {
                _ => {}
            }
        }

        fn _s_Start_hdl_user_parse(&mut self, __e: &DomainBlockParserFrameEvent) {
            let mut __compartment = self.__prepareEnter("Vars");
            self.__transition(__compartment);
            return;
        }

        fn _s_Vars_hdl_frame_enter(&mut self, __e: &DomainBlockParserFrameEvent) {
            loop {
                match self.tokens.as_ref().unwrap().peek_kind() {
                    // Body close ends the section (left unconsumed for
                    // FsmDeclParser's `}` handling).
                    FsmTokenKind::RBrace | FsmTokenKind::Eof => {
                        let span = self.span_start.clone();
                        self.result = Some(FsmDomainBlock {
                            vars: std::mem::take(&mut self.vars),
                            span,
                        });
                        let mut __compartment = self.__prepareEnter("Done");
                        self.__transition(__compartment);
                        return;
                    }
                    _ => {}
                }
            
                let vsp = self.tokens.as_ref().unwrap().cur_span();
            
                // field name
                let name = match self.tokens.as_ref().unwrap().peek_kind() {
                    FsmTokenKind::Ident(n) => n,
                    _ => {
                        self.error = Some(ParseError {
                            message: "expected a domain field name".to_string(),
                            span: vsp,
                        });
                        let mut __compartment = self.__prepareEnter("Done");
                        self.__transition(__compartment);
                        return;
                    }
                };
                self.tokens.as_mut().unwrap().advance();
            
                // `:`
                if !self.tokens.as_mut().unwrap().eat(&FsmTokenKind::Colon) {
                    self.error = Some(ParseError {
                        message: format!("expected `:` after domain field `{}`", name),
                        span: self.tokens.as_ref().unwrap().cur_span(),
                    });
                    let mut __compartment = self.__prepareEnter("Done");
                    self.__transition(__compartment);
                    return;
                }
            
                // type (simple identifier in v0.1)
                let var_type = match self.tokens.as_ref().unwrap().peek_kind() {
                    FsmTokenKind::Ident(t) => Type::Custom(t),
                    _ => {
                        self.error = Some(ParseError {
                            message: format!("expected a type for domain field `{}`", name),
                            span: self.tokens.as_ref().unwrap().cur_span(),
                        });
                        let mut __compartment = self.__prepareEnter("Done");
                        self.__transition(__compartment);
                        return;
                    }
                };
                self.tokens.as_mut().unwrap().advance();
            
                // `=` mandatory default (RFC-0042 §3.8 / E705)
                if !self.tokens.as_mut().unwrap().eat(&FsmTokenKind::Eq) {
                    self.error = Some(ParseError {
                        message: format!("domain field `{}` is missing its `= <default>` initializer (E705)", name),
                        span: self.tokens.as_ref().unwrap().cur_span(),
                    });
                    let mut __compartment = self.__prepareEnter("Done");
                    self.__transition(__compartment);
                    return;
                }
            
                // default initializer (parsed expression)
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
                let default = child
                    .result
                    .take()
                    .expect("child ExpressionParser sets result when no error");
            
                self.vars.push(FsmDomainVar {
                    name,
                    var_type,
                    default,
                    span: vsp,
                });
            }
        }
    }
}
pub use _domain_block_parser_framec::*;
