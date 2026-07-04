
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
    #[derive(Clone)]
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
        StageEmbeds,
        Transition,
        CondTarget,
        FailureBranch,
        CommitMatch,
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
                "StageEmbeds" => StateParserStateContext::StageEmbeds,
                "Transition" => StateParserStateContext::Transition,
                "CondTarget" => StateParserStateContext::CondTarget,
                "FailureBranch" => StateParserStateContext::FailureBranch,
                "CommitMatch" => StateParserStateContext::CommitMatch,
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
        pub pending_stage_label: Option<String>,
        pub pending_stage_regex: String,
        pub pending_stage_span: Span,
        pub pending_embeds: Vec<EmbeddingActionAst>,
        pub has_arrow: bool,
        pub success_target: Option<FsmTransitionTarget>,
        pub failure_target: Option<FsmTransitionTarget>,
        pub cond_alts: Vec<FsmCondAlt>,
        pub transition_span: Span,
        pub span_start: Span,
        pub result: Option<FsmStateAst>,
        pub error: Option<ParseError>,
        pub error_code: Option<&'static str>,
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
                pending_stage_label: None,
                pending_stage_regex: String::new(),
                pending_stage_span: Span::new(0, 0),
                pending_embeds: Vec::new(),
                has_arrow: false,
                success_target: None,
                failure_target: None,
                cond_alts: Vec::new(),
                transition_span: Span::new(0, 0),
                span_start: Span::new(0, 0),
                result: None,
                error: None,
                error_code: None,
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
                "StageEmbeds" => &["StageEmbeds"],
                "Transition" => &["Transition"],
                "CondTarget" => &["CondTarget"],
                "FailureBranch" => &["FailureBranch"],
                "CommitMatch" => &["CommitMatch"],
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
                "StageEmbeds" => self._state_StageEmbeds(__ev),
                "Transition" => self._state_Transition(__ev),
                "CondTarget" => self._state_CondTarget(__ev),
                "FailureBranch" => self._state_FailureBranch(__ev),
                "CommitMatch" => self._state_CommitMatch(__ev),
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

        // Collect zero or more embedding actions on the stage just parsed,
        // then push the complete Stage and return to $Elements. Each
        // embedding sigil (`>{` `@{` `${` `%{` `@eof{`) is followed by an
        // action block (ActionBlockParser child).
        fn _state_StageEmbeds(&mut self, __e: &StateParserFrameEvent) {
            match __e {
                StateParserFrameEvent::FrameEnter { .. } => { self._s_StageEmbeds_hdl_frame_enter(__e); }
                _ => {}
            }
        }

        // Optional transition clause for the current match: `-> target`
        // then optional `: -> target`. Then either start the next match
        // (ordered-choice `|`) or finish the state.
        // Begin an optional transition clause. Parses the success target,
        // which is either a static `$State`/`$State.stage` or a conditional
        // `( $A when cond, ... )`. The failure branch and the match commit
        // follow in $FailureBranch / $CommitMatch.
        fn _state_Transition(&mut self, __e: &StateParserFrameEvent) {
            match __e {
                StateParserFrameEvent::FrameEnter { .. } => { self._s_Transition_hdl_frame_enter(__e); }
                _ => {}
            }
        }

        // Parse a conditional target's `cond_alt` list: `static when expr`
        // entries, comma-separated, until `)`. Each condition is parsed by
        // the ExpressionParser child (token-stream shuttle). Per §3.5.4.1
        // every alternative requires its `when` guard.
        fn _state_CondTarget(&mut self, __e: &StateParserFrameEvent) {
            match __e {
                StateParserFrameEvent::FrameEnter { .. } => { self._s_CondTarget_hdl_frame_enter(__e); }
                _ => {}
            }
        }

        // Optional failure branch `: -> target` (static target).
        fn _state_FailureBranch(&mut self, __e: &StateParserFrameEvent) {
            match __e {
                StateParserFrameEvent::FrameEnter { .. } => { self._s_FailureBranch_hdl_frame_enter(__e); }
                _ => {}
            }
        }

        // Commit the current match (elements + optional transition), then
        // either start the next ordered-choice match (`|`) or finish.
        fn _state_CommitMatch(&mut self, __e: &StateParserFrameEvent) {
            match __e {
                StateParserFrameEvent::FrameEnter { .. } => { self._s_CommitMatch_hdl_frame_enter(__e); }
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
                    // KwActions/KwDomain end the state list (a section
                    // follows); the next StateLabel begins another state.
                    FsmTokenKind::Arrow
                    | FsmTokenKind::Colon
                    | FsmTokenKind::Pipe
                    | FsmTokenKind::RBrace
                    | FsmTokenKind::Eof
                    | FsmTokenKind::KwActions
                    | FsmTokenKind::KwDomain
                    | FsmTokenKind::StateLabel(_) => {
                        let mut __compartment = self.__prepareEnter("Transition");
                        self.__transition(__compartment);
                        return;
                    }
                    // `.label /regex/` — a labeled stage. Stash the base
                    // stage and collect any embedding actions in $StageEmbeds.
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
                        self.pending_stage_label = Some(name);
                        self.pending_stage_regex = body;
                        self.pending_stage_span = sp;
                        self.pending_embeds = Vec::new();
                        let mut __compartment = self.__prepareEnter("StageEmbeds");
                        self.__transition(__compartment);
                        return;
                    }
                    // `/regex/` — an unlabeled stage.
                    FsmTokenKind::RegexLiteral(body) => {
                        let sp = self.tokens.as_ref().unwrap().cur_span();
                        self.tokens.as_mut().unwrap().advance();
                        self.pending_stage_label = None;
                        self.pending_stage_regex = body;
                        self.pending_stage_span = sp;
                        self.pending_embeds = Vec::new();
                        let mut __compartment = self.__prepareEnter("StageEmbeds");
                        self.__transition(__compartment);
                        return;
                    }
                    // `{ ... }` — an action block element. Delegate to
                    // ActionBlockParser (token-stream shuttle).
                    FsmTokenKind::LBrace => {
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
                        let block = child
                            .result
                            .take()
                            .expect("child ActionBlockParser sets result when no error");
                        self.elements.push(MatchElement::ActionBlock(block));
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

        fn _s_StageEmbeds_hdl_frame_enter(&mut self, __e: &StateParserFrameEvent) {
            loop {
                let op = match self.tokens.as_ref().unwrap().peek_kind() {
                    FsmTokenKind::EmbedStart => Some(EmbeddingOp::Start),
                    FsmTokenKind::EmbedAccept => Some(EmbeddingOp::Accept),
                    FsmTokenKind::EmbedEvery => Some(EmbeddingOp::EveryTransition),
                    FsmTokenKind::EmbedLeave => Some(EmbeddingOp::LeaveAccept),
                    FsmTokenKind::EmbedEof => Some(EmbeddingOp::Eof),
                    _ => None,
                };
                match op {
                    Some(op) => {
                        let esp = self.tokens.as_ref().unwrap().cur_span();
                        self.tokens.as_mut().unwrap().advance(); // the sigil
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
                        self.pending_embeds.push(EmbeddingActionAst {
                            op,
                            body,
                            span: esp,
                        });
                    }
                    None => {
                        // No more embeds — emit the complete stage.
                        self.elements.push(MatchElement::Stage(StageAst {
                            label: self.pending_stage_label.take(),
                            regex: std::mem::take(&mut self.pending_stage_regex),
                            embedding_actions: std::mem::take(&mut self.pending_embeds),
                            span: self.pending_stage_span.clone(),
                        }));
                        let mut __compartment = self.__prepareEnter("Elements");
                        self.__transition(__compartment);
                        return;
                    }
                }
            }
        }

        fn _s_Transition_hdl_frame_enter(&mut self, __e: &StateParserFrameEvent) {
            if !self.tokens.as_ref().unwrap().at(&FsmTokenKind::Arrow) {
                // A leading `:` (no `->`) is a failure-only clause: the
                // success path is the implicit-terminal match (§4.3),
                // success_target stays None. Anything else ends the match.
                if self.tokens.as_ref().unwrap().at(&FsmTokenKind::Colon) {
                    self.transition_span = self.tokens.as_ref().unwrap().cur_span();
                    self.has_arrow = true;
                    let mut __compartment = self.__prepareEnter("FailureBranch");
                    self.__transition(__compartment);
                    return;
                }
                self.has_arrow = false;
                let mut __compartment = self.__prepareEnter("CommitMatch");
                self.__transition(__compartment);
                return;
            }
            self.transition_span = self.tokens.as_ref().unwrap().cur_span();
            self.has_arrow = true;
            self.tokens.as_mut().unwrap().advance(); // `->`
            
            // Conditional target: `( $A when cond, ... )`.
            if self.tokens.as_ref().unwrap().at(&FsmTokenKind::LParen) {
                self.tokens.as_mut().unwrap().advance(); // `(`
                self.cond_alts = Vec::new();
                let mut __compartment = self.__prepareEnter("CondTarget");
                self.__transition(__compartment);
                return;
            }
            
            // Static target.
            match parse_target(self.tokens.as_mut().unwrap()) {
                Ok(t) => { self.success_target = Some(t);
                let mut __compartment = self.__prepareEnter("FailureBranch");
                self.__transition(__compartment);
                return; }
                Err(e) => { self.error = Some(e);
                let mut __compartment = self.__prepareEnter("Done");
                self.__transition(__compartment);
                return; }
            }
        }

        fn _s_CondTarget_hdl_frame_enter(&mut self, __e: &StateParserFrameEvent) {
            loop {
                let alt_span = self.tokens.as_ref().unwrap().cur_span();
                let target = match parse_target(self.tokens.as_mut().unwrap()) {
                    Ok(t) => t,
                    Err(e) => { self.error = Some(e);
                    let mut __compartment = self.__prepareEnter("Done");
                    self.__transition(__compartment);
                    return; }
                };
                if !self.tokens.as_mut().unwrap().eat(&FsmTokenKind::KwWhen) {
                    // E715: every conditional alternative needs a `when`.
                    self.error = Some(ParseError {
                        message: "conditional_target alternative is missing its `when` guard (E715)".to_string(),
                        span: self.tokens.as_ref().unwrap().cur_span(),
                    });
                    let mut __compartment = self.__prepareEnter("Done");
                    self.__transition(__compartment);
                    return;
                }
            
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
                let condition = child
                    .result
                    .take()
                    .expect("child ExpressionParser sets result when no error");
                self.cond_alts.push(FsmCondAlt {
                    target,
                    condition,
                    span: alt_span,
                });
            
                if self.tokens.as_mut().unwrap().eat(&FsmTokenKind::Comma) {
                    continue;
                }
                if self.tokens.as_mut().unwrap().eat(&FsmTokenKind::RParen) {
                    self.success_target =
                        Some(FsmTransitionTarget::Conditional(std::mem::take(&mut self.cond_alts)));
                    let mut __compartment = self.__prepareEnter("FailureBranch");
                    self.__transition(__compartment);
                    return;
                }
                self.error = Some(ParseError {
                    message: "expected `,` or `)` in conditional transition target".to_string(),
                    span: self.tokens.as_ref().unwrap().cur_span(),
                });
                let mut __compartment = self.__prepareEnter("Done");
                self.__transition(__compartment);
                return;
            }
        }

        fn _s_FailureBranch_hdl_frame_enter(&mut self, __e: &StateParserFrameEvent) {
            if self.tokens.as_mut().unwrap().eat(&FsmTokenKind::Colon) {
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
                    Ok(t) => { self.failure_target = Some(t); }
                    Err(e) => { self.error = Some(e);
                    let mut __compartment = self.__prepareEnter("Done");
                    self.__transition(__compartment);
                    return; }
                }
            }
            let mut __compartment = self.__prepareEnter("CommitMatch");
            self.__transition(__compartment);
            return;
        }

        fn _s_CommitMatch_hdl_frame_enter(&mut self, __e: &StateParserFrameEvent) {
            let transition = if self.has_arrow {
                Some(FsmTransitionClauseAst {
                    // `None` for a failure-only clause (`: -> $Err`).
                    success: self.success_target.take(),
                    failure: self.failure_target.take(),
                    span: self.transition_span.clone(),
                })
            } else {
                None
            };
            
            let span = self.span_start.clone();
            self.matches.push(MatchAst {
                elements: std::mem::take(&mut self.elements),
                transition,
                span: span.clone(),
            });
            
            // Ordered-choice `|` starts another match in this state.
            if self.tokens.as_mut().unwrap().eat(&FsmTokenKind::Pipe) {
                self.has_arrow = false;
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
