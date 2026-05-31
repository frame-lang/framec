
// RFC-0043 — the action-block parser as a Frame @@system child of the
// fsm parser tree. Parses a `{ statement* }` block into a BlockAst.
// Statements are separated by `;` or by whitespace (RFC-0043 §3.1); a
// trailing `;` before `}` is permitted. Each statement is parsed by a
// StatementParser child (token-stream shuttle). Mutually recursive with
// StatementParser (an `if` branch is itself an action block).
//
// Composition contract:
//   - input:  `tokens` set by the parent, positioned at the opening `{`.
//   - output: `result` (Some<BlockAst>) or `error`.

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
mod _action_block_parser_framec {
    use super::*;
    extern crate alloc;
    use alloc::{vec, format};
    #[derive(Clone, Debug)]
    #[allow(dead_code, non_camel_case_types)]
    enum ActionBlockParserFrameEvent {
        Parse {  },
        FrameEnter {},
        FrameExit {},
    }

    #[derive(Clone)]
    #[allow(dead_code, non_camel_case_types)]
    enum ActionBlockParserFrameReturn {
        _Lifecycle(alloc::rc::Rc<dyn core::any::Any>),
    }

    #[allow(dead_code)]
    impl ActionBlockParserFrameEvent {
        fn name(&self) -> &'static str {
            match self {
                ActionBlockParserFrameEvent::Parse { .. } => "parse",
                ActionBlockParserFrameEvent::FrameEnter { .. } => "$>",
                ActionBlockParserFrameEvent::FrameExit { .. } => "<$",
            }
        }
    }

    #[derive(Clone, Debug)]
    #[allow(dead_code, non_camel_case_types)]
    enum ActionBlockParserFrameValue {
        Int(i64),
        Float(f64),
        Bool(bool),
        Str(String),
        List(Vec<Self>),
        Dict(alloc::collections::BTreeMap<String, Self>),
    }

    #[allow(dead_code, non_camel_case_types)]
    struct ActionBlockParserFrameContext {
        event: alloc::rc::Rc<ActionBlockParserFrameEvent>,
        _return: Option<ActionBlockParserFrameReturn>,
        _data: alloc::collections::BTreeMap<String, ActionBlockParserFrameValue>,
        _transitioned: bool,
    }

    impl ActionBlockParserFrameContext {
        fn new(event: alloc::rc::Rc<ActionBlockParserFrameEvent>, default_return: Option<ActionBlockParserFrameReturn>) -> Self {
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
    enum ActionBlockParserStateContext {
        Start,
        Open,
        Stmts,
        Done,
        __NoContext,
    }

    impl Default for ActionBlockParserStateContext {
        fn default() -> Self {
            ActionBlockParserStateContext::Start
        }
    }

    #[allow(dead_code, non_camel_case_types)]
    #[derive(Clone)]
    struct ActionBlockParserCompartment {
        state: String,
        state_context: ActionBlockParserStateContext,
        forward_event: Option<ActionBlockParserFrameEvent>,
        parent_compartment: Option<Box<ActionBlockParserCompartment>>,
    }

    impl ActionBlockParserCompartment {
        fn new(state: &str) -> Self {
            let state_context = match state {
                "Start" => ActionBlockParserStateContext::Start,
                "Open" => ActionBlockParserStateContext::Open,
                "Stmts" => ActionBlockParserStateContext::Stmts,
                "Done" => ActionBlockParserStateContext::Done,
                _ => ActionBlockParserStateContext::__NoContext,
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
    pub struct ActionBlockParser {
        _state_stack: Vec<ActionBlockParserCompartment>,
        __compartment: ActionBlockParserCompartment,
        __next_compartment: Option<ActionBlockParserCompartment>,
        _context_stack: Vec<ActionBlockParserFrameContext>,
        pub tokens: Option<FsmTokenStream>,
        pub statements: Vec<Statement>,
        pub span_start: Span,
        pub result: Option<BlockAst>,
        pub error: Option<ParseError>,
    }

    #[allow(non_snake_case)]
    impl ActionBlockParser {
        pub fn new() -> Self {
            Self {
                _state_stack: Vec::new(),
                _context_stack: Vec::new(),
                tokens: None,
                statements: Vec::new(),
                span_start: Span::new(0, 0),
                result: None,
                error: None,
                __compartment: ActionBlockParserCompartment::new("Start"),
                __next_compartment: None,
            }
        }

        pub fn __create() -> Self {
            let mut c = Self::new();
            c.__compartment = c.__prepareEnter("Start");
            let __e = alloc::rc::Rc::new(ActionBlockParserFrameEvent::FrameEnter {});
            let __ctx = ActionBlockParserFrameContext::new(alloc::rc::Rc::clone(&__e), None);
            c._context_stack.push(__ctx);
            c.__kernel(&__e);
            c._context_stack.pop();
            c
        }

        fn __hsm_chain(&mut self, leaf: &str) -> &'static [&'static str] {
            match leaf {
                "Start" => &["Start"],
                "Open" => &["Open"],
                "Stmts" => &["Stmts"],
                "Done" => &["Done"],
                _ => &[],
            }
        }

        fn __prepareEnter(&mut self, leaf: &str) -> ActionBlockParserCompartment {
            let chain = self.__hsm_chain(leaf);
            let mut comp: Option<ActionBlockParserCompartment> = None;
            for name in chain.iter() {
                let mut new_comp = ActionBlockParserCompartment::new(name);
                if let Some(parent) = comp.take() {
                    new_comp.parent_compartment = Some(Box::new(parent));
                }
                comp = Some(new_comp);
            }
            comp.expect("chain must contain at least the leaf state")
        }

        fn __kernel(&mut self, __e: &alloc::rc::Rc<ActionBlockParserFrameEvent>) {
            // Route event to current state.
            self.__router(__e);
            // Drain any transitions queued by the handler.
            while self.__next_compartment.is_some() {
                let next_compartment = self.__next_compartment.take().expect("invariant: while-loop guard checked is_some()");
                // Exit the current (leaf) state. RFC-0025.1: exit args live in the
                // source state's typed ctx (written at the transition site), so the
                // synthesized `<$` event carries no payload.
                let exit_event = alloc::rc::Rc::new(ActionBlockParserFrameEvent::FrameExit {});
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
                        let enter_event = alloc::rc::Rc::new(ActionBlockParserFrameEvent::FrameEnter {});
                        self.__router(&enter_event);
                    }
                    Some(fwd) if matches!(fwd, ActionBlockParserFrameEvent::FrameEnter { .. }) => {
                        // Forwarded event IS $> — dispatch directly so the
                        // destination's $> handler receives the caller's payload.
                        let fwd_rc = alloc::rc::Rc::new(fwd);
                        self.__router(&fwd_rc);
                    }
                    Some(fwd) => {
                        // Forwarded event is not $> — initialize the destination
                        // with a fresh $>, then dispatch the forward.
                        let enter_event = alloc::rc::Rc::new(ActionBlockParserFrameEvent::FrameEnter {});
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

        fn __router(&mut self, __e: &alloc::rc::Rc<ActionBlockParserFrameEvent>) {
            let __ev: &ActionBlockParserFrameEvent = __e;
            match self.__compartment.state.as_str() {
                "Start" => self._state_Start(__ev),
                "Open" => self._state_Open(__ev),
                "Stmts" => self._state_Stmts(__ev),
                "Done" => self._state_Done(__ev),
                _ => {}
            }
        }

        fn __transition(&mut self, next_compartment: ActionBlockParserCompartment) {
            self.__next_compartment = Some(next_compartment);
        }

        pub fn parse(&mut self) {
            let __e = alloc::rc::Rc::new(ActionBlockParserFrameEvent::Parse {});
            let mut __ctx = ActionBlockParserFrameContext::new(alloc::rc::Rc::clone(&__e), None);
            self._context_stack.push(__ctx);
            self.__kernel(&__e);
            self._context_stack.pop();
        }

        fn _state_Start(&mut self, __e: &ActionBlockParserFrameEvent) {
            match __e {
                ActionBlockParserFrameEvent::Parse { .. } => { self._s_Start_hdl_user_parse(__e); }
                _ => {}
            }
        }

        fn _state_Open(&mut self, __e: &ActionBlockParserFrameEvent) {
            match __e {
                ActionBlockParserFrameEvent::FrameEnter { .. } => { self._s_Open_hdl_frame_enter(__e); }
                _ => {}
            }
        }

        fn _state_Stmts(&mut self, __e: &ActionBlockParserFrameEvent) {
            match __e {
                ActionBlockParserFrameEvent::FrameEnter { .. } => { self._s_Stmts_hdl_frame_enter(__e); }
                _ => {}
            }
        }

        fn _state_Done(&mut self, __e: &ActionBlockParserFrameEvent) {
            match __e {
                _ => {}
            }
        }

        fn _s_Start_hdl_user_parse(&mut self, __e: &ActionBlockParserFrameEvent) {
            let mut __compartment = self.__prepareEnter("Open");
            self.__transition(__compartment);
            return;
        }

        fn _s_Open_hdl_frame_enter(&mut self, __e: &ActionBlockParserFrameEvent) {
            self.span_start = self.tokens.as_ref().unwrap().cur_span();
            if !self.tokens.as_mut().unwrap().eat(&FsmTokenKind::LBrace) {
                self.error = Some(ParseError {
                    message: "expected `{` to open an action block".to_string(),
                    span: self.tokens.as_ref().unwrap().cur_span(),
                });
                let mut __compartment = self.__prepareEnter("Done");
                self.__transition(__compartment);
                return;
            }
            let mut __compartment = self.__prepareEnter("Stmts");
            self.__transition(__compartment);
            return;
        }

        fn _s_Stmts_hdl_frame_enter(&mut self, __e: &ActionBlockParserFrameEvent) {
            loop {
                match self.tokens.as_ref().unwrap().peek_kind() {
                    FsmTokenKind::RBrace => {
                        self.tokens.as_mut().unwrap().advance(); // `}`
                        let span = self.span_start.clone();
                        self.result = Some(BlockAst {
                            statements: std::mem::take(&mut self.statements),
                            span,
                        });
                        let mut __compartment = self.__prepareEnter("Done");
                        self.__transition(__compartment);
                        return;
                    }
                    FsmTokenKind::Eof => {
                        self.error = Some(ParseError {
                            message: "unexpected end of input; expected `}` to close action block".to_string(),
                            span: self.tokens.as_ref().unwrap().cur_span(),
                        });
                        let mut __compartment = self.__prepareEnter("Done");
                        self.__transition(__compartment);
                        return;
                    }
                    // Stray `;` (e.g. a trailing or leading separator).
                    FsmTokenKind::Semi => {
                        self.tokens.as_mut().unwrap().advance();
                    }
                    _ => {
                        let mut child = StatementParser::__create();
                        child.tokens = self.tokens.take();
                        child.parse();
                        self.tokens = child.tokens.take();
                        if let Some(e) = child.error.take() {
                            self.error = Some(e);
                            let mut __compartment = self.__prepareEnter("Done");
                            self.__transition(__compartment);
                            return;
                        }
                        let s = child
                            .result
                            .take()
                            .expect("child StatementParser sets result when no error");
                        self.statements.push(s);
                        // Optional `;` separator between statements.
                        self.tokens.as_mut().unwrap().eat(&FsmTokenKind::Semi);
                    }
                }
            }
        }
    }
}
pub use _action_block_parser_framec::*;
