
// RFC-0042 — the state parser as a Frame @@system child of the fsm
// parser tree. Parses ONE state declaration: an optional `$Label`, a
// sequence of match elements (stages + bare expressions), and an
// optional transition clause (`-> target : -> target`).
//
// FsmDeclParser loops, spawning a StateParser per state until `}`. The
// state ends where the next state begins (a `$Label:`) or at `}` / EOF;
// StateParser stops at those boundaries WITHOUT consuming them, leaving
// them for the parent's loop.
//
// SCOPE: optional label, one match (element sequence), optional static
// transition clause. Ordered-choice `|` matches and conditional `when`
// targets land when a fixture needs them. Bare expressions delegate to
// the (complete) ExpressionParser child; stages capture the regex body
// string and an optional `.label`.
//
// Composition contract:
//   - input:  `tokens` set by the parent.
//   - output: `result` (Some<FsmStateAst>) or `error`.

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
mod _state_parser_framec {
    use super::*;
    extern crate alloc;
    use alloc::{vec, format};
    #[derive(Clone, Debug)]
    #[allow(dead_code, non_camel_case_types)]
    enum StateParserFrameEvent {
        Parse {  },
        FrameEnter {},
        FrameExit {},
    }

    #[derive(Clone)]
    #[allow(dead_code, non_camel_case_types)]
    enum StateParserFrameReturn {
        _Lifecycle(alloc::rc::Rc<dyn core::any::Any>),
    }

    #[allow(dead_code)]
    impl StateParserFrameEvent {
        fn name(&self) -> &'static str {
            match self {
                StateParserFrameEvent::Parse { .. } => "parse",
                StateParserFrameEvent::FrameEnter { .. } => "$>",
                StateParserFrameEvent::FrameExit { .. } => "<$",
            }
        }
    }

    #[derive(Clone, Debug)]
    #[allow(dead_code, non_camel_case_types)]
    enum StateParserFrameValue {
        Int(i64),
        Float(f64),
        Bool(bool),
        Str(String),
        List(Vec<Self>),
        Dict(alloc::collections::BTreeMap<String, Self>),
    }

    #[allow(dead_code, non_camel_case_types)]
    struct StateParserFrameContext {
        event: alloc::rc::Rc<StateParserFrameEvent>,
        _return: Option<StateParserFrameReturn>,
        _data: alloc::collections::BTreeMap<String, StateParserFrameValue>,
        _transitioned: bool,
    }

    impl StateParserFrameContext {
        fn new(event: alloc::rc::Rc<StateParserFrameEvent>, default_return: Option<StateParserFrameReturn>) -> Self {
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
    enum StateParserStateContext {
        Start,
        Label,
        Elements,
        Transition,
        Done,
        __NoContext,
    }

    impl Default for StateParserStateContext {
        fn default() -> Self {
            StateParserStateContext::Start
        }
    }

    #[allow(dead_code, non_camel_case_types)]
    #[derive(Clone)]
    struct StateParserCompartment {
        state: String,
        state_context: StateParserStateContext,
        forward_event: Option<StateParserFrameEvent>,
        parent_compartment: Option<Box<StateParserCompartment>>,
    }

    impl StateParserCompartment {
        fn new(state: &str) -> Self {
            let state_context = match state {
                "Start" => StateParserStateContext::Start,
                "Label" => StateParserStateContext::Label,
                "Elements" => StateParserStateContext::Elements,
                "Transition" => StateParserStateContext::Transition,
                "Done" => StateParserStateContext::Done,
                _ => StateParserStateContext::__NoContext,
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
    pub struct StateParser {
        _state_stack: Vec<StateParserCompartment>,
        __compartment: StateParserCompartment,
        __next_compartment: Option<StateParserCompartment>,
        _context_stack: Vec<StateParserFrameContext>,
        pub tokens: Option<FsmTokenStream>,
        pub label: Option<String>,
        pub elements: Vec<MatchElement>,
        pub matches: Vec<MatchAst>,
        pub span_start: Span,
        pub result: Option<FsmStateAst>,
        pub error: Option<ParseError>,
    }

    #[allow(non_snake_case)]
    impl StateParser {
        pub fn new() -> Self {
            Self {
                _state_stack: Vec::new(),
                _context_stack: Vec::new(),
                tokens: None,
                label: None,
                elements: Vec::new(),
                matches: Vec::new(),
                span_start: Span::new(0, 0),
                result: None,
                error: None,
                __compartment: StateParserCompartment::new("Start"),
                __next_compartment: None,
            }
        }

        pub fn __create() -> Self {
            let mut c = Self::new();
            c.__compartment = c.__prepareEnter("Start");
            let __e = alloc::rc::Rc::new(StateParserFrameEvent::FrameEnter {});
            let __ctx = StateParserFrameContext::new(alloc::rc::Rc::clone(&__e), None);
            c._context_stack.push(__ctx);
            c.__kernel(&__e);
            c._context_stack.pop();
            c
        }

        fn __hsm_chain(&mut self, leaf: &str) -> &'static [&'static str] {
            match leaf {
                "Start" => &["Start"],
                "Label" => &["Label"],
                "Elements" => &["Elements"],
                "Transition" => &["Transition"],
                "Done" => &["Done"],
                _ => &[],
            }
        }

        fn __prepareEnter(&mut self, leaf: &str) -> StateParserCompartment {
            let chain = self.__hsm_chain(leaf);
            let mut comp: Option<StateParserCompartment> = None;
            for name in chain.iter() {
                let mut new_comp = StateParserCompartment::new(name);
                if let Some(parent) = comp.take() {
                    new_comp.parent_compartment = Some(Box::new(parent));
                }
                comp = Some(new_comp);
            }
            comp.expect("chain must contain at least the leaf state")
        }

        fn __kernel(&mut self, __e: &alloc::rc::Rc<StateParserFrameEvent>) {
            // Route event to current state.
            self.__router(__e);
            // Drain any transitions queued by the handler.
            while self.__next_compartment.is_some() {
                let next_compartment = self.__next_compartment.take().expect("invariant: while-loop guard checked is_some()");
                // Exit the current (leaf) state. RFC-0025.1: exit args live in the
                // source state's typed ctx (written at the transition site), so the
                // synthesized `<$` event carries no payload.
                let exit_event = alloc::rc::Rc::new(StateParserFrameEvent::FrameExit {});
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
                        let enter_event = alloc::rc::Rc::new(StateParserFrameEvent::FrameEnter {});
                        self.__router(&enter_event);
                    }
                    Some(fwd) if matches!(fwd, StateParserFrameEvent::FrameEnter { .. }) => {
                        // Forwarded event IS $> — dispatch directly so the
                        // destination's $> handler receives the caller's payload.
                        let fwd_rc = alloc::rc::Rc::new(fwd);
                        self.__router(&fwd_rc);
                    }
                    Some(fwd) => {
                        // Forwarded event is not $> — initialize the destination
                        // with a fresh $>, then dispatch the forward.
                        let enter_event = alloc::rc::Rc::new(StateParserFrameEvent::FrameEnter {});
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

        fn __router(&mut self, __e: &alloc::rc::Rc<StateParserFrameEvent>) {
            let __ev: &StateParserFrameEvent = __e;
            match self.__compartment.state.as_str() {
                "Start" => self._state_Start(__ev),
                "Label" => self._state_Label(__ev),
                "Elements" => self._state_Elements(__ev),
                "Transition" => self._state_Transition(__ev),
                "Done" => self._state_Done(__ev),
                _ => {}
            }
        }

        fn __transition(&mut self, next_compartment: StateParserCompartment) {
            self.__next_compartment = Some(next_compartment);
        }

        pub fn parse(&mut self) {
            let __e = alloc::rc::Rc::new(StateParserFrameEvent::Parse {});
            let mut __ctx = StateParserFrameContext::new(alloc::rc::Rc::clone(&__e), None);
            self._context_stack.push(__ctx);
            self.__kernel(&__e);
            self._context_stack.pop();
        }

        fn _state_Start(&mut self, __e: &StateParserFrameEvent) {
            match __e {
                StateParserFrameEvent::Parse { .. } => { self._s_Start_hdl_user_parse(__e); }
                _ => {}
            }
        }

        // Optional `$Label:` — consume if present.
        fn _state_Label(&mut self, __e: &StateParserFrameEvent) {
            match __e {
                StateParserFrameEvent::FrameEnter { .. } => { self._s_Label_hdl_frame_enter(__e); }
                _ => {}
            }
        }

        // Collect match elements until a transition (`->`/`:`), an
        // ordered-choice `|`, the next state (`$Label:`), or `}` / EOF.
        fn _state_Elements(&mut self, __e: &StateParserFrameEvent) {
            match __e {
                StateParserFrameEvent::FrameEnter { .. } => { self._s_Elements_hdl_frame_enter(__e); }
                _ => {}
            }
        }

        // Optional transition clause for the current match: `-> target`
        // then optional `: -> target`. Then either start the next match
        // (ordered-choice `|`) or finish the state.
        fn _state_Transition(&mut self, __e: &StateParserFrameEvent) {
            match __e {
                StateParserFrameEvent::FrameEnter { .. } => { self._s_Transition_hdl_frame_enter(__e); }
                _ => {}
            }
        }

        fn _state_Done(&mut self, __e: &StateParserFrameEvent) {
            match __e {
                _ => {}
            }
        }

        fn _s_Start_hdl_user_parse(&mut self, __e: &StateParserFrameEvent) {
            let mut __compartment = self.__prepareEnter("Label");
            self.__transition(__compartment);
            return;
        }

        fn _s_Label_hdl_frame_enter(&mut self, __e: &StateParserFrameEvent) {
            let lbl = match self.tokens.as_ref().unwrap().peek_kind() {
                FsmTokenKind::StateLabel(name) => Some(name),
                _ => None,
            };
            if lbl.is_some() {
                self.tokens.as_mut().unwrap().advance();
                self.label = lbl;
            }
            self.span_start = self.tokens.as_ref().unwrap().cur_span();
            let mut __compartment = self.__prepareEnter("Elements");
            self.__transition(__compartment);
            return;
        }

        fn _s_Elements_hdl_frame_enter(&mut self, __e: &StateParserFrameEvent) {
            loop {
                let next = self.tokens.as_ref().unwrap().peek_kind();
                match next {
                    // Boundaries — stop collecting, move to transition.
                    FsmTokenKind::Arrow
                    | FsmTokenKind::Colon
                    | FsmTokenKind::Pipe
                    | FsmTokenKind::RBrace
                    | FsmTokenKind::Eof => {
                        let mut __compartment = self.__prepareEnter("Transition");
                        self.__transition(__compartment);
                        return;
                    }
                    // Next state begins — this state is done, no transition.
                    FsmTokenKind::StateLabel(_) => {
                        let mut __compartment = self.__prepareEnter("Transition");
                        self.__transition(__compartment);
                        return;
                    }
                    // `.label /regex/` — a labeled stage.
                    FsmTokenKind::StageLabel(name) => {
                        let sp = self.tokens.as_ref().unwrap().cur_span();
                        self.tokens.as_mut().unwrap().advance();
                        let body = match self.tokens.as_ref().unwrap().peek_kind() {
                            FsmTokenKind::RegexLiteral(b) => b,
                            _ => {
                                self.error = Some(ParseError {
                                    message: "expected a regex literal after stage label".to_string(),
                                    span: self.tokens.as_ref().unwrap().cur_span(),
                                });
                                let mut __compartment = self.__prepareEnter("Done");
                                self.__transition(__compartment);
                                return;
                            }
                        };
                        self.tokens.as_mut().unwrap().advance();
                        self.elements.push(MatchElement::Stage(StageAst {
                            label: Some(name),
                            regex: body,
                            embedding_actions: Vec::new(),
                            span: sp,
                        }));
                    }
                    // `/regex/` — an unlabeled stage.
                    FsmTokenKind::RegexLiteral(body) => {
                        let sp = self.tokens.as_ref().unwrap().cur_span();
                        self.tokens.as_mut().unwrap().advance();
                        self.elements.push(MatchElement::Stage(StageAst {
                            label: None,
                            regex: body,
                            embedding_actions: Vec::new(),
                            span: sp,
                        }));
                    }
                    // Anything else — a bare expression. Delegate to the
                    // ExpressionParser child (token-stream shuttle).
                    _ => {
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
                        self.elements.push(MatchElement::BareExpression { expr, span: sp });
                    }
                }
            }
        }

        fn _s_Transition_hdl_frame_enter(&mut self, __e: &StateParserFrameEvent) {
            let mut transition: Option<FsmTransitionClauseAst> = None;
            
            if self.tokens.as_ref().unwrap().at(&FsmTokenKind::Arrow) {
                let tsp = self.tokens.as_ref().unwrap().cur_span();
                self.tokens.as_mut().unwrap().advance(); // `->`
            
                let success = match parse_target(self.tokens.as_mut().unwrap()) {
                    Ok(t) => t,
                    Err(e) => { self.error = Some(e);
                    let mut __compartment = self.__prepareEnter("Done");
                    self.__transition(__compartment);
                    return; }
                };
            
                // Optional failure branch `: -> target`.
                let failure = if self.tokens.as_mut().unwrap().eat(&FsmTokenKind::Colon) {
                    if !self.tokens.as_mut().unwrap().eat(&FsmTokenKind::Arrow) {
                        self.error = Some(ParseError {
                            message: "expected `->` after `:` in failure branch".to_string(),
                            span: self.tokens.as_ref().unwrap().cur_span(),
                        });
                        let mut __compartment = self.__prepareEnter("Done");
                        self.__transition(__compartment);
                        return;
                    }
                    match parse_target(self.tokens.as_mut().unwrap()) {
                        Ok(t) => Some(t),
                        Err(e) => { self.error = Some(e);
                        let mut __compartment = self.__prepareEnter("Done");
                        self.__transition(__compartment);
                        return; }
                    }
                } else {
                    None
                };
            
                transition = Some(FsmTransitionClauseAst {
                    success,
                    failure,
                    span: tsp,
                });
            }
            
            // Commit the current match.
            let span = self.span_start.clone();
            self.matches.push(MatchAst {
                elements: std::mem::take(&mut self.elements),
                transition,
                span: span.clone(),
            });
            
            // Ordered-choice `|` starts another match in this state.
            if self.tokens.as_mut().unwrap().eat(&FsmTokenKind::Pipe) {
                let mut __compartment = self.__prepareEnter("Elements");
                self.__transition(__compartment);
                return;
            }
            
            self.result = Some(FsmStateAst {
                label: self.label.take(),
                matches: std::mem::take(&mut self.matches),
                span,
            });
            let mut __compartment = self.__prepareEnter("Done");
            self.__transition(__compartment);
            return;
        }
    }
}
pub use _state_parser_framec::*;
