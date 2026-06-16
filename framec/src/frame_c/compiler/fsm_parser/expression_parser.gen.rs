
// RFC-0043 — the expression parser as a Frame @@system child of the
// fsm parser tree.
//
// Consumes tokens from a shared FsmTokenStream (moved in by the parent
// via Option::take, moved back out when done) and builds one Expression.
// This is the first CHILD parser in the tree — it proves the
// cooperating-systems composition pattern (linear token-stream
// ownership shuttling).
//
// Precedence is handled by precedence climbing expressed across states
// plus a binding-power table (parse_helpers::binding_power):
//
//   $Atom    — one atom: literal | probe | ident | call | parenthesized.
//   $Postfix — fold `.field` member accesses onto the atom.
//   $Climb   — the precedence loop: while the next operator's left
//              binding power ≥ self.min_bp, consume it and parse the
//              right operand with a CHILD ExpressionParser whose min_bp
//              is the operator's right binding power. Left-associative
//              via the (2k-1, 2k) binding-power pairs.
//   $Done    — `result` holds the Expression, or `error` is set.
//
// The caller sets `min_bp` before `parse()` (default 0 = a full
// expression). `$Climb` recursing through children at higher min_bp is
// what makes precedence + associativity fall out — no per-level state.
//
// SCOPE: literals, probes, vars, calls `ident(args)`, parenthesized
// sub-expressions, `.field` member access, and the binary operators in
// binding_power (||, &&, ==/!=, </<=/>/>=, +/-, *///%). Unary prefix
// (!x, -x) lands when a fixture first needs it (a child at min_bp above
// the tightest binary, wrapping Expression::Unary).
//
// Composition contract:
//   - input:  `tokens` set by the parent; `min_bp` (default 0).
//   - output: `result` (Some on success) or `error`.

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
        Atom,
        Unary,
        CallArgs,
        ParenInner,
        ConciseReturn,
        Postfix,
        Climb,
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
                "Atom" => ExpressionParserStateContext::Atom,
                "Unary" => ExpressionParserStateContext::Unary,
                "CallArgs" => ExpressionParserStateContext::CallArgs,
                "ParenInner" => ExpressionParserStateContext::ParenInner,
                "ConciseReturn" => ExpressionParserStateContext::ConciseReturn,
                "Postfix" => ExpressionParserStateContext::Postfix,
                "Climb" => ExpressionParserStateContext::Climb,
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
        pub min_bp: u8,
        pub left: Option<Expression>,
        pub pending_callee: String,
        pub call_args: Vec<Expression>,
        pub pending_unary: Option<UnaryOp>,
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
                min_bp: 0,
                left: None,
                pending_callee: String::new(),
                call_args: Vec::new(),
                pending_unary: None,
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
                "Atom" => &["Atom"],
                "Unary" => &["Unary"],
                "CallArgs" => &["CallArgs"],
                "ParenInner" => &["ParenInner"],
                "ConciseReturn" => &["ConciseReturn"],
                "Postfix" => &["Postfix"],
                "Climb" => &["Climb"],
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
                "Atom" => self._state_Atom(__ev),
                "Unary" => self._state_Unary(__ev),
                "CallArgs" => self._state_CallArgs(__ev),
                "ParenInner" => self._state_ParenInner(__ev),
                "ConciseReturn" => self._state_ConciseReturn(__ev),
                "Postfix" => self._state_Postfix(__ev),
                "Climb" => self._state_Climb(__ev),
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

        // Parse one atom into `self.left`, then fold postfixes.
        fn _state_Atom(&mut self, __e: &ExpressionParserFrameEvent) {
            match __e {
                ExpressionParserFrameEvent::FrameEnter { .. } => { self._s_Atom_hdl_frame_enter(__e); }
                _ => {}
            }
        }

        // Parse a prefix-unary operand via a child at UNARY_BP, then wrap.
        fn _state_Unary(&mut self, __e: &ExpressionParserFrameEvent) {
            match __e {
                ExpressionParserFrameEvent::FrameEnter { .. } => { self._s_Unary_hdl_frame_enter(__e); }
                _ => {}
            }
        }

        // Parse a (possibly empty) comma-separated argument list, having
        // already consumed the callee ident and the opening `(`. Each
        // argument is a fresh child ExpressionParser at min_bp 0.
        fn _state_CallArgs(&mut self, __e: &ExpressionParserFrameEvent) {
            match __e {
                ExpressionParserFrameEvent::FrameEnter { .. } => { self._s_CallArgs_hdl_frame_enter(__e); }
                _ => {}
            }
        }

        // Parenthesized sub-expression: parse inner via a child at min_bp
        // 0 (a full expression resets precedence), expect `)`.
        fn _state_ParenInner(&mut self, __e: &ExpressionParserFrameEvent) {
            match __e {
                ExpressionParserFrameEvent::FrameEnter { .. } => { self._s_ParenInner_hdl_frame_enter(__e); }
                _ => {}
            }
        }

        // `@@:( expr )` concise return setter — the `@@:` and `(` are
        // already consumed. Parse the inner expression, expect `)`, and
        // desugar to `@@:return = expr`.
        fn _state_ConciseReturn(&mut self, __e: &ExpressionParserFrameEvent) {
            match __e {
                ExpressionParserFrameEvent::FrameEnter { .. } => { self._s_ConciseReturn_hdl_frame_enter(__e); }
                _ => {}
            }
        }

        // Fold `.field` member accesses onto `self.left` (tightest
        // binding, left-associative: a.b.c = (a.b).c).
        fn _state_Postfix(&mut self, __e: &ExpressionParserFrameEvent) {
            match __e {
                ExpressionParserFrameEvent::FrameEnter { .. } => { self._s_Postfix_hdl_frame_enter(__e); }
                _ => {}
            }
        }

        // Precedence climb. While the next operator binds at least as
        // tightly as our floor, consume it and fold in a right operand
        // parsed by a child at the operator's right binding power.
        fn _state_Climb(&mut self, __e: &ExpressionParserFrameEvent) {
            match __e {
                ExpressionParserFrameEvent::FrameEnter { .. } => { self._s_Climb_hdl_frame_enter(__e); }
                _ => {}
            }
        }

        fn _state_Done(&mut self, __e: &ExpressionParserFrameEvent) {
            match __e {
                _ => {}
            }
        }

        fn _s_Start_hdl_user_parse(&mut self, __e: &ExpressionParserFrameEvent) {
            let mut __compartment = self.__prepareEnter("Atom");
            self.__transition(__compartment);
            return;
        }

        fn _s_Atom_hdl_frame_enter(&mut self, __e: &ExpressionParserFrameEvent) {
            let ts = self.tokens.as_mut().unwrap();
            let sp = ts.cur_span();
            
            match ts.peek_kind() {
                FsmTokenKind::KwTrue => {
                    ts.advance();
                    self.left = Some(Expression::Literal(Literal::Bool(true)));
                    let mut __compartment = self.__prepareEnter("Postfix");
                    self.__transition(__compartment);
                    return;
                }
                FsmTokenKind::KwFalse => {
                    ts.advance();
                    self.left = Some(Expression::Literal(Literal::Bool(false)));
                    let mut __compartment = self.__prepareEnter("Postfix");
                    self.__transition(__compartment);
                    return;
                }
                FsmTokenKind::IntLit(n) => {
                    ts.advance();
                    self.left = Some(Expression::Literal(Literal::Int(n)));
                    let mut __compartment = self.__prepareEnter("Postfix");
                    self.__transition(__compartment);
                    return;
                }
                FsmTokenKind::StringLit(s) => {
                    ts.advance();
                    self.left = Some(Expression::Literal(Literal::String(s)));
                    let mut __compartment = self.__prepareEnter("Postfix");
                    self.__transition(__compartment);
                    return;
                }
                FsmTokenKind::Probe(name) => {
                    ts.advance();
                    // A probe is a leaf reference; surface it as a Var
                    // carrying the `@@:`-qualified name.
                    self.left = Some(Expression::Var(format!("@@:{}", name)));
                    let mut __compartment = self.__prepareEnter("Postfix");
                    self.__transition(__compartment);
                    return;
                }
                // `$state.stage` stage-capture reference (§3.5.2). Surfaced
                // as a Var carrying the qualified name; a chained
                // `.return_value` (Mode C, §8.3) then folds on via $Postfix.
                FsmTokenKind::StageRef { state, stage } => {
                    ts.advance();
                    self.left = Some(Expression::Var(format!("${}.{}", state, stage)));
                    let mut __compartment = self.__prepareEnter("Postfix");
                    self.__transition(__compartment);
                    return;
                }
                FsmTokenKind::Ident(name) => {
                    ts.advance();
                    // Call `ident(args)` vs bare variable.
                    if ts.eat(&FsmTokenKind::LParen) {
                        self.pending_callee = name;
                        self.call_args = Vec::new();
                        let mut __compartment = self.__prepareEnter("CallArgs");
                        self.__transition(__compartment);
                        return;
                    }
                    self.left = Some(Expression::Var(name));
                    let mut __compartment = self.__prepareEnter("Postfix");
                    self.__transition(__compartment);
                    return;
                }
                FsmTokenKind::LParen => {
                    ts.advance();
                    let mut __compartment = self.__prepareEnter("ParenInner");
                    self.__transition(__compartment);
                    return;
                }
                // `@@:( expr )` — concise return setter. Desugars to
                // `@@:return = expr`, i.e. Assign{ Var("@@:return"), expr }.
                FsmTokenKind::ConciseReturn => {
                    ts.advance(); // `@@:`
                    if !ts.eat(&FsmTokenKind::LParen) {
                        self.error = Some(ParseError {
                            message: "expected `(` after `@@:`".to_string(),
                            span: ts.cur_span(),
                        });
                        let mut __compartment = self.__prepareEnter("Done");
                        self.__transition(__compartment);
                        return;
                    }
                    let mut __compartment = self.__prepareEnter("ConciseReturn");
                    self.__transition(__compartment);
                    return;
                }
                // Prefix unary `!x` / `-x`. In atom position `-` is
                // unary (binary subtraction is handled in $Climb).
                // The operand is parsed by a child at UNARY_BP — above
                // every binary left-power — so member/call postfixes
                // bind into the operand (`-a.b` = -(a.b)) but no binary
                // operator folds inside it (`-a * b` = (-a) * b).
                FsmTokenKind::Bang => {
                    ts.advance();
                    self.pending_unary = Some(UnaryOp::Not);
                    let mut __compartment = self.__prepareEnter("Unary");
                    self.__transition(__compartment);
                    return;
                }
                FsmTokenKind::Minus => {
                    ts.advance();
                    self.pending_unary = Some(UnaryOp::Neg);
                    let mut __compartment = self.__prepareEnter("Unary");
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

        fn _s_Unary_hdl_frame_enter(&mut self, __e: &ExpressionParserFrameEvent) {
            let mut child = ExpressionParser::__create();
            child.min_bp = 13; // UNARY_BP — above the tightest binary l_bp (11)
            child.tokens = self.tokens.take();
            child.parse();
            self.tokens = child.tokens.take();
            if let Some(e) = child.error.take() {
                self.error = Some(e);
                let mut __compartment = self.__prepareEnter("Done");
                self.__transition(__compartment);
                return;
            }
            let operand = child
                .result
                .take()
                .expect("child ExpressionParser sets result when no error");
            let op = self.pending_unary.take().unwrap();
            self.left = Some(Expression::Unary {
                op,
                expr: Box::new(operand),
            });
            let mut __compartment = self.__prepareEnter("Climb");
            self.__transition(__compartment);
            return;
        }

        fn _s_CallArgs_hdl_frame_enter(&mut self, __e: &ExpressionParserFrameEvent) {
            {
                let ts = self.tokens.as_mut().unwrap();
                if ts.eat(&FsmTokenKind::RParen) {
                    self.left = Some(Expression::Call {
                        func: std::mem::take(&mut self.pending_callee),
                        args: std::mem::take(&mut self.call_args),
                    });
                    let mut __compartment = self.__prepareEnter("Postfix");
                    self.__transition(__compartment);
                    return;
                }
            }
            
            loop {
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
                    self.left = Some(Expression::Call {
                        func: std::mem::take(&mut self.pending_callee),
                        args: std::mem::take(&mut self.call_args),
                    });
                    let mut __compartment = self.__prepareEnter("Postfix");
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
                self.left = Some(inner);
                let mut __compartment = self.__prepareEnter("Postfix");
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

        fn _s_ConciseReturn_hdl_frame_enter(&mut self, __e: &ExpressionParserFrameEvent) {
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
            if !ts.eat(&FsmTokenKind::RParen) {
                self.error = Some(ParseError {
                    message: "expected `)` to close `@@:(...)`".to_string(),
                    span: ts.cur_span(),
                });
                let mut __compartment = self.__prepareEnter("Done");
                self.__transition(__compartment);
                return;
            }
            self.left = Some(Expression::Assign {
                target: Box::new(Expression::Var("@@:return".to_string())),
                value: Box::new(inner),
            });
            let mut __compartment = self.__prepareEnter("Postfix");
            self.__transition(__compartment);
            return;
        }

        fn _s_Postfix_hdl_frame_enter(&mut self, __e: &ExpressionParserFrameEvent) {
            loop {
                if !self.tokens.as_ref().unwrap().at(&FsmTokenKind::Dot) {
                    let mut __compartment = self.__prepareEnter("Climb");
                    self.__transition(__compartment);
                    return;
                }
                self.tokens.as_mut().unwrap().advance(); // `.`
                let field = match self.tokens.as_ref().unwrap().peek_kind() {
                    FsmTokenKind::Ident(f) => f,
                    _ => {
                        self.error = Some(ParseError {
                            message: "expected field name after `.`".to_string(),
                            span: self.tokens.as_ref().unwrap().cur_span(),
                        });
                        let mut __compartment = self.__prepareEnter("Done");
                        self.__transition(__compartment);
                        return;
                    }
                };
                self.tokens.as_mut().unwrap().advance(); // field ident
                let object = self.left.take().unwrap();
                self.left = Some(Expression::Member {
                    object: Box::new(object),
                    field,
                });
            }
        }

        fn _s_Climb_hdl_frame_enter(&mut self, __e: &ExpressionParserFrameEvent) {
            loop {
                let op = self.tokens.as_ref().unwrap().peek_kind();
                let (l_bp, r_bp) = match binding_power(&op) {
                    Some(bps) => bps,
                    None => {
                        self.result = self.left.take();
                        let mut __compartment = self.__prepareEnter("Done");
                        self.__transition(__compartment);
                        return;
                    }
                };
                if l_bp < self.min_bp {
                    self.result = self.left.take();
                    let mut __compartment = self.__prepareEnter("Done");
                    self.__transition(__compartment);
                    return;
                }
            
                self.tokens.as_mut().unwrap().advance(); // operator
            
                let mut child = ExpressionParser::__create();
                child.min_bp = r_bp;
                child.tokens = self.tokens.take();
                child.parse();
                self.tokens = child.tokens.take();
                if let Some(e) = child.error.take() {
                    self.error = Some(e);
                    let mut __compartment = self.__prepareEnter("Done");
                    self.__transition(__compartment);
                    return;
                }
                let right = child
                    .result
                    .take()
                    .expect("child ExpressionParser sets result when no error");
            
                let left = self.left.take().unwrap();
                self.left = Some(Expression::Binary {
                    left: Box::new(left),
                    op: binary_op(&op),
                    right: Box::new(right),
                });
            }
        }
    }
}
pub use _expression_parser_framec::*;
