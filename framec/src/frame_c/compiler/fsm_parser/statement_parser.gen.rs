
// RFC-0043 §3.2 — the statement parser as a Frame @@system child of the
// fsm parser tree. Parses ONE statement from a shared FsmTokenStream:
//
//   assignment  ::= lvalue "=" expression     (lvalue parsed as an expr)
//   call / expr ::= expression                (a bare call/expression stmt)
//   if_stmt     ::= "if" cond block ("else" "if" cond block)* ("else" block)?
//
// Assignment and expression statements are disambiguated by a one-token
// lookahead after the leading expression: a following `=` makes it an
// assignment (the leading expression is the lvalue), otherwise it is an
// expression statement.
//
// `if` branches are blocks, parsed by ActionBlockParser and wrapped as
// `Statement::Block`; `else if` recurses StatementParser. StatementParser
// and ActionBlockParser are mutually recursive (each module `use`s the
// other; circular `use` across sibling modules is fine in Rust).
//
// SCOPE: assignment, expression statement, if/else(-if). Deferred:
// `@@:return = expr` / `@@:(expr)` return-statement forms (need probe/
// return lexing refinement). Composition contract:
//   - input:  `tokens` set by the parent.
//   - output: `result` (Some<Statement>) or `error`.

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
mod _statement_parser_framec {
    use super::*;
    extern crate alloc;
    use alloc::{vec, format};
    #[derive(Clone)]
    #[allow(dead_code, non_camel_case_types)]
    enum StatementParserFrameEvent {
        Parse {  },
        FrameEnter {},
        FrameExit {},
    }

    #[derive(Clone)]
    #[allow(dead_code, non_camel_case_types)]
    enum StatementParserFrameReturn {
        _Lifecycle(alloc::rc::Rc<dyn core::any::Any>),
    }

    #[allow(dead_code)]
    impl StatementParserFrameEvent {
        fn name(&self) -> &'static str {
            match self {
                StatementParserFrameEvent::Parse { .. } => "parse",
                StatementParserFrameEvent::FrameEnter { .. } => "$>",
                StatementParserFrameEvent::FrameExit { .. } => "<$",
            }
        }
    }

    #[derive(Clone, Debug)]
    #[allow(dead_code, non_camel_case_types)]
    enum StatementParserFrameValue {
        Int(i64),
        Float(f64),
        Bool(bool),
        Str(String),
        List(Vec<Self>),
        Dict(alloc::collections::BTreeMap<String, Self>),
    }

    #[allow(dead_code, non_camel_case_types)]
    struct StatementParserFrameContext {
        event: alloc::rc::Rc<StatementParserFrameEvent>,
        _return: Option<StatementParserFrameReturn>,
        _data: alloc::collections::BTreeMap<String, StatementParserFrameValue>,
        _transitioned: bool,
    }

    impl StatementParserFrameContext {
        fn new(event: alloc::rc::Rc<StatementParserFrameEvent>, default_return: Option<StatementParserFrameReturn>) -> Self {
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
    enum StatementParserStateContext {
        Start,
        Dispatch,
        AfterExpr,
        IfCond,
        IfThen,
        IfElse,
        Done,
        __NoContext,
    }

    impl Default for StatementParserStateContext {
        fn default() -> Self {
            StatementParserStateContext::Start
        }
    }

    #[allow(dead_code, non_camel_case_types)]
    #[derive(Clone)]
    struct StatementParserCompartment {
        state: String,
        state_context: StatementParserStateContext,
        forward_event: Option<StatementParserFrameEvent>,
        parent_compartment: Option<Box<StatementParserCompartment>>,
    }

    impl StatementParserCompartment {
        fn new(state: &str) -> Self {
            let state_context = match state {
                "Start" => StatementParserStateContext::Start,
                "Dispatch" => StatementParserStateContext::Dispatch,
                "AfterExpr" => StatementParserStateContext::AfterExpr,
                "IfCond" => StatementParserStateContext::IfCond,
                "IfThen" => StatementParserStateContext::IfThen,
                "IfElse" => StatementParserStateContext::IfElse,
                "Done" => StatementParserStateContext::Done,
                _ => StatementParserStateContext::__NoContext,
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
    pub struct StatementParser {
        _state_stack: Vec<StatementParserCompartment>,
        __compartment: StatementParserCompartment,
        __next_compartment: Option<StatementParserCompartment>,
        _context_stack: Vec<StatementParserFrameContext>,
        pub tokens: Option<FsmTokenStream>,
        pub lhs: Option<Expression>,
        pub cond: Option<Expression>,
        pub then_block: Option<BlockAst>,
        pub span_start: Span,
        pub result: Option<Statement>,
        pub error: Option<ParseError>,
        pub error_code: Option<&'static str>,
    }

    #[allow(non_snake_case)]
    impl StatementParser {
        pub fn new() -> Self {
            Self {
                _state_stack: Vec::new(),
                _context_stack: Vec::new(),
                tokens: None,
                lhs: None,
                cond: None,
                then_block: None,
                span_start: Span::new(0, 0),
                result: None,
                error: None,
                error_code: None,
                __compartment: StatementParserCompartment::new("Start"),
                __next_compartment: None,
            }
        }

        pub fn __create() -> Self {
            let mut c = Self::new();
            c.__compartment = c.__prepareEnter("Start");
            let __e = alloc::rc::Rc::new(StatementParserFrameEvent::FrameEnter {});
            let __ctx = StatementParserFrameContext::new(alloc::rc::Rc::clone(&__e), None);
            c._context_stack.push(__ctx);
            c.__kernel(&__e);
            c._context_stack.pop();
            c
        }

        fn __hsm_chain(&mut self, leaf: &str) -> &'static [&'static str] {
            match leaf {
                "Start" => &["Start"],
                "Dispatch" => &["Dispatch"],
                "AfterExpr" => &["AfterExpr"],
                "IfCond" => &["IfCond"],
                "IfThen" => &["IfThen"],
                "IfElse" => &["IfElse"],
                "Done" => &["Done"],
                _ => &[],
            }
        }

        fn __prepareEnter(&mut self, leaf: &str) -> StatementParserCompartment {
            let chain = self.__hsm_chain(leaf);
            let mut comp: Option<StatementParserCompartment> = None;
            for name in chain.iter() {
                let mut new_comp = StatementParserCompartment::new(name);
                if let Some(parent) = comp.take() {
                    new_comp.parent_compartment = Some(Box::new(parent));
                }
                comp = Some(new_comp);
            }
            comp.expect("chain must contain at least the leaf state")
        }

        fn __kernel(&mut self, __e: &alloc::rc::Rc<StatementParserFrameEvent>) {
            // Route event to current state.
            self.__router(__e);
            // Drain any transitions queued by the handler.
            while self.__next_compartment.is_some() {
                let next_compartment = self.__next_compartment.take().expect("invariant: while-loop guard checked is_some()");
                // Exit the current (leaf) state. RFC-0025.1: exit args live in the
                // source state's typed ctx (written at the transition site), so the
                // synthesized `<$` event carries no payload.
                let exit_event = alloc::rc::Rc::new(StatementParserFrameEvent::FrameExit {});
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
                        let enter_event = alloc::rc::Rc::new(StatementParserFrameEvent::FrameEnter {});
                        self.__router(&enter_event);
                    }
                    Some(fwd) if matches!(fwd, StatementParserFrameEvent::FrameEnter { .. }) => {
                        // Forwarded event IS $> — dispatch directly so the
                        // destination's $> handler receives the caller's payload.
                        let fwd_rc = alloc::rc::Rc::new(fwd);
                        self.__router(&fwd_rc);
                    }
                    Some(fwd) => {
                        // Forwarded event is not $> — initialize the destination
                        // with a fresh $>, then dispatch the forward.
                        let enter_event = alloc::rc::Rc::new(StatementParserFrameEvent::FrameEnter {});
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

        fn __router(&mut self, __e: &alloc::rc::Rc<StatementParserFrameEvent>) {
            let __ev: &StatementParserFrameEvent = __e;
            match self.__compartment.state.as_str() {
                "Start" => self._state_Start(__ev),
                "Dispatch" => self._state_Dispatch(__ev),
                "AfterExpr" => self._state_AfterExpr(__ev),
                "IfCond" => self._state_IfCond(__ev),
                "IfThen" => self._state_IfThen(__ev),
                "IfElse" => self._state_IfElse(__ev),
                "Done" => self._state_Done(__ev),
                _ => {}
            }
        }

        fn __transition(&mut self, next_compartment: StatementParserCompartment) {
            self.__next_compartment = Some(next_compartment);
        }

        pub fn parse(&mut self) {
            let __e = alloc::rc::Rc::new(StatementParserFrameEvent::Parse {});
            let mut __ctx = StatementParserFrameContext::new(alloc::rc::Rc::clone(&__e), None);
            self._context_stack.push(__ctx);
            self.__kernel(&__e);
            self._context_stack.pop();
        }

        fn _state_Start(&mut self, __e: &StatementParserFrameEvent) {
            match __e {
                StatementParserFrameEvent::Parse { .. } => { self._s_Start_hdl_user_parse(__e); }
                _ => {}
            }
        }

        fn _state_Dispatch(&mut self, __e: &StatementParserFrameEvent) {
            match __e {
                StatementParserFrameEvent::FrameEnter { .. } => { self._s_Dispatch_hdl_frame_enter(__e); }
                _ => {}
            }
        }

        fn _state_AfterExpr(&mut self, __e: &StatementParserFrameEvent) {
            match __e {
                StatementParserFrameEvent::FrameEnter { .. } => { self._s_AfterExpr_hdl_frame_enter(__e); }
                _ => {}
            }
        }

        fn _state_IfCond(&mut self, __e: &StatementParserFrameEvent) {
            match __e {
                StatementParserFrameEvent::FrameEnter { .. } => { self._s_IfCond_hdl_frame_enter(__e); }
                _ => {}
            }
        }

        fn _state_IfThen(&mut self, __e: &StatementParserFrameEvent) {
            match __e {
                StatementParserFrameEvent::FrameEnter { .. } => { self._s_IfThen_hdl_frame_enter(__e); }
                _ => {}
            }
        }

        fn _state_IfElse(&mut self, __e: &StatementParserFrameEvent) {
            match __e {
                StatementParserFrameEvent::FrameEnter { .. } => { self._s_IfElse_hdl_frame_enter(__e); }
                _ => {}
            }
        }

        fn _state_Done(&mut self, __e: &StatementParserFrameEvent) {
            match __e {
                _ => {}
            }
        }

        fn _s_Start_hdl_user_parse(&mut self, __e: &StatementParserFrameEvent) {
            let mut __compartment = self.__prepareEnter("Dispatch");
            self.__transition(__compartment);
            return;
        }

        fn _s_Dispatch_hdl_frame_enter(&mut self, __e: &StatementParserFrameEvent) {
            self.span_start = self.tokens.as_ref().unwrap().cur_span();
            if self.tokens.as_ref().unwrap().at(&FsmTokenKind::KwIf) {
                self.tokens.as_mut().unwrap().advance(); // `if`;
                let mut __compartment = self.__prepareEnter("IfCond");
                self.__transition(__compartment);
                return;
            }
            // Leading expression: either an expression statement or the
            // lvalue of an assignment (decided in $AfterExpr).
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
            self.lhs = child.result.take();
            let mut __compartment = self.__prepareEnter("AfterExpr");
            self.__transition(__compartment);
            return;
        }

        fn _s_AfterExpr_hdl_frame_enter(&mut self, __e: &StatementParserFrameEvent) {
            let span = self.span_start.clone();
            if self.tokens.as_mut().unwrap().eat(&FsmTokenKind::Eq) {
                // Assignment: leading expr was the lvalue; parse the RHS.
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
                let rhs = child
                    .result
                    .take()
                    .expect("child ExpressionParser sets result when no error");
                let lhs = self.lhs.take().unwrap();
                self.result = Some(Statement::Expression(ExpressionAst {
                    expr: Expression::Assign {
                        target: Box::new(lhs),
                        value: Box::new(rhs),
                    },
                    span,
                }));
                let mut __compartment = self.__prepareEnter("Done");
                self.__transition(__compartment);
                return;
            }
            // Expression statement.
            let lhs = self.lhs.take().unwrap();
            self.result = Some(Statement::Expression(ExpressionAst { expr: lhs, span }));
            let mut __compartment = self.__prepareEnter("Done");
            self.__transition(__compartment);
            return;
        }

        fn _s_IfCond_hdl_frame_enter(&mut self, __e: &StatementParserFrameEvent) {
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
            self.cond = child.result.take();
            let mut __compartment = self.__prepareEnter("IfThen");
            self.__transition(__compartment);
            return;
        }

        fn _s_IfThen_hdl_frame_enter(&mut self, __e: &StatementParserFrameEvent) {
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
            self.then_block = child.result.take();
            let mut __compartment = self.__prepareEnter("IfElse");
            self.__transition(__compartment);
            return;
        }

        fn _s_IfElse_hdl_frame_enter(&mut self, __e: &StatementParserFrameEvent) {
            let mut else_branch: Option<Box<Statement>> = None;
            
            if self.tokens.as_mut().unwrap().eat(&FsmTokenKind::KwElse) {
                if self.tokens.as_ref().unwrap().at(&FsmTokenKind::KwIf) {
                    // `else if ...` — recurse StatementParser.
                    let mut child = StatementParser::__create();
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
                    let s = child
                        .result
                        .take()
                        .expect("child StatementParser sets result when no error");
                    else_branch = Some(Box::new(s));
                } else {
                    // `else { ... }`
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
                    let blk = child
                        .result
                        .take()
                        .expect("child ActionBlockParser sets result when no error");
                    else_branch = Some(Box::new(Statement::Block(blk)));
                }
            }
            
            let span = self.span_start.clone();
            let cond = self.cond.take().unwrap();
            let then_block = self.then_block.take().unwrap();
            self.result = Some(Statement::If(IfAst {
                condition: cond,
                then_branch: Box::new(Statement::Block(then_block)),
                else_branch,
                span,
            }));
            let mut __compartment = self.__prepareEnter("Done");
            self.__transition(__compartment);
            return;
        }
    }
}
pub use _statement_parser_framec::*;
