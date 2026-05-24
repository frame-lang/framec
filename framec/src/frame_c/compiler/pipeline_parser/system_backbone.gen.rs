
// RFC-0039 Stage 0/1/2 — the parser's outer grammar as a Frame backbone.
//
// Replaces the hand-written `Parser::parse_system` loops. The backbone OWNS the
// parser (B1) and drives it live. Every level is a self-looping state that
// proves its sub-grammar is regular:
//   $Sections    — the flat list of system sections, dispatched by keyword.
//   $Interface   — the flat list of interface methods.
//   $Actions     — the flat list of action declarations.
//   $Operations  — the flat list of operation declarations.
//   $Machine     — the flat list of `$State` declarations in `machine:`.
//   $StateHeader/$StateBody — one state's header + its unordered member list.
// Only the `domain:` section is still delegated whole (`take_domain_section`).
// Each terminal is delegated to an existing oracle method on `Parser`
// (`take_state_header`, `parse_interface_method`, `parse_action`,
// `parse_operation`, `parse_state_var_decl`, `parse_{enter,exit,event}_handler`);
// no push$/pop$ anywhere, because none of these sub-grammars is recursive.
//
// Errors thread through `error` (Frame handlers return ()). Byte-output is
// identical to the recursive-descent loop; the snapshot/matrix/fuzz suites are
// the parity gate.

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
mod _system_backbone_framec {
    use super::*;
    extern crate alloc;
    use alloc::{vec, format};
    #[derive(Clone, Debug)]
    #[allow(dead_code, non_camel_case_types)]
    enum SystemBackboneFrameEvent {
        Parse {  },
        FrameEnter {},
        FrameExit {},
    }

    #[derive(Clone)]
    #[allow(dead_code, non_camel_case_types)]
    enum SystemBackboneFrameReturn {
        _Lifecycle(alloc::rc::Rc<dyn core::any::Any>),
    }

    #[allow(dead_code)]
    impl SystemBackboneFrameEvent {
        fn name(&self) -> &'static str {
            match self {
                SystemBackboneFrameEvent::Parse { .. } => "parse",
                SystemBackboneFrameEvent::FrameEnter { .. } => "$>",
                SystemBackboneFrameEvent::FrameExit { .. } => "<$",
            }
        }
    }

    #[derive(Clone, Debug)]
    #[allow(dead_code, non_camel_case_types)]
    enum SystemBackboneFrameValue {
        Int(i64),
        Float(f64),
        Bool(bool),
        Str(String),
        List(Vec<Self>),
        Dict(alloc::collections::BTreeMap<String, Self>),
    }

    #[allow(dead_code, non_camel_case_types)]
    struct SystemBackboneFrameContext {
        event: alloc::rc::Rc<SystemBackboneFrameEvent>,
        _return: Option<SystemBackboneFrameReturn>,
        _data: alloc::collections::BTreeMap<String, SystemBackboneFrameValue>,
        _transitioned: bool,
    }

    impl SystemBackboneFrameContext {
        fn new(event: alloc::rc::Rc<SystemBackboneFrameEvent>, default_return: Option<SystemBackboneFrameReturn>) -> Self {
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
    enum SystemBackboneStateContext {
        Start,
        Sections,
        Interface,
        Actions,
        Operations,
        Machine,
        StateHeader,
        StateBody,
        Done,
        __NoContext,
    }

    impl Default for SystemBackboneStateContext {
        fn default() -> Self {
            SystemBackboneStateContext::Start
        }
    }

    #[allow(dead_code, non_camel_case_types)]
    #[derive(Clone)]
    struct SystemBackboneCompartment {
        state: String,
        state_context: SystemBackboneStateContext,
        forward_event: Option<SystemBackboneFrameEvent>,
        parent_compartment: Option<Box<SystemBackboneCompartment>>,
    }

    impl SystemBackboneCompartment {
        fn new(state: &str) -> Self {
            let state_context = match state {
                "Start" => SystemBackboneStateContext::Start,
                "Sections" => SystemBackboneStateContext::Sections,
                "Interface" => SystemBackboneStateContext::Interface,
                "Actions" => SystemBackboneStateContext::Actions,
                "Operations" => SystemBackboneStateContext::Operations,
                "Machine" => SystemBackboneStateContext::Machine,
                "StateHeader" => SystemBackboneStateContext::StateHeader,
                "StateBody" => SystemBackboneStateContext::StateBody,
                "Done" => SystemBackboneStateContext::Done,
                _ => SystemBackboneStateContext::__NoContext,
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
    pub struct SystemBackbone {
        _state_stack: Vec<SystemBackboneCompartment>,
        __compartment: SystemBackboneCompartment,
        __next_compartment: Option<SystemBackboneCompartment>,
        _context_stack: Vec<SystemBackboneFrameContext>,
        pub parser: Option<Parser>,
        pub system: Option<SystemAst>,
        pub error: Option<ParseError>,
        pub start: usize,
        pub machine_states: Vec<StateAst>,
        pub machine_start: usize,
        pub current_state: Option<StateAst>,
        pub pending_attrs: Vec<crate::frame_c::compiler::frame_ast::Attribute>,
        pub state_body_close: usize,
        pub iface_methods: Vec<InterfaceMethod>,
        pub action_decls: Vec<ActionAst>,
        pub ops: Vec<OperationAst>,
    }

    #[allow(non_snake_case)]
    impl SystemBackbone {
        pub fn new() -> Self {
            Self {
                _state_stack: Vec::new(),
                _context_stack: Vec::new(),
                parser: None,
                system: None,
                error: None,
                start: 0,
                machine_states: Vec::new(),
                machine_start: 0,
                current_state: None,
                pending_attrs: Vec::new(),
                state_body_close: 0,
                iface_methods: Vec::new(),
                action_decls: Vec::new(),
                ops: Vec::new(),
                __compartment: SystemBackboneCompartment::new("Start"),
                __next_compartment: None,
            }
        }

        pub fn __create() -> Self {
            let mut c = Self::new();
            c.__compartment = c.__prepareEnter("Start");
            let __e = alloc::rc::Rc::new(SystemBackboneFrameEvent::FrameEnter {});
            let __ctx = SystemBackboneFrameContext::new(alloc::rc::Rc::clone(&__e), None);
            c._context_stack.push(__ctx);
            c.__kernel(&__e);
            c._context_stack.pop();
            c
        }

        fn __hsm_chain(&mut self, leaf: &str) -> &'static [&'static str] {
            match leaf {
                "Start" => &["Start"],
                "Sections" => &["Sections"],
                "Interface" => &["Interface"],
                "Actions" => &["Actions"],
                "Operations" => &["Operations"],
                "Machine" => &["Machine"],
                "StateHeader" => &["StateHeader"],
                "StateBody" => &["StateBody"],
                "Done" => &["Done"],
                _ => &[],
            }
        }

        fn __prepareEnter(&mut self, leaf: &str) -> SystemBackboneCompartment {
            let chain = self.__hsm_chain(leaf);
            let mut comp: Option<SystemBackboneCompartment> = None;
            for name in chain.iter() {
                let mut new_comp = SystemBackboneCompartment::new(name);
                if let Some(parent) = comp.take() {
                    new_comp.parent_compartment = Some(Box::new(parent));
                }
                comp = Some(new_comp);
            }
            comp.expect("chain must contain at least the leaf state")
        }

        fn __kernel(&mut self, __e: &alloc::rc::Rc<SystemBackboneFrameEvent>) {
            // Route event to current state.
            self.__router(__e);
            // Drain any transitions queued by the handler.
            while self.__next_compartment.is_some() {
                let next_compartment = self.__next_compartment.take().expect("invariant: while-loop guard checked is_some()");
                // Exit the current (leaf) state. RFC-0025.1: exit args live in the
                // source state's typed ctx (written at the transition site), so the
                // synthesized `<$` event carries no payload.
                let exit_event = alloc::rc::Rc::new(SystemBackboneFrameEvent::FrameExit {});
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
                        let enter_event = alloc::rc::Rc::new(SystemBackboneFrameEvent::FrameEnter {});
                        self.__router(&enter_event);
                    }
                    Some(fwd) if matches!(fwd, SystemBackboneFrameEvent::FrameEnter { .. }) => {
                        // Forwarded event IS $> — dispatch directly so the
                        // destination's $> handler receives the caller's payload.
                        let fwd_rc = alloc::rc::Rc::new(fwd);
                        self.__router(&fwd_rc);
                    }
                    Some(fwd) => {
                        // Forwarded event is not $> — initialize the destination
                        // with a fresh $>, then dispatch the forward.
                        let enter_event = alloc::rc::Rc::new(SystemBackboneFrameEvent::FrameEnter {});
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

        fn __router(&mut self, __e: &alloc::rc::Rc<SystemBackboneFrameEvent>) {
            let __ev: &SystemBackboneFrameEvent = __e;
            match self.__compartment.state.as_str() {
                "Start" => self._state_Start(__ev),
                "Sections" => self._state_Sections(__ev),
                "Interface" => self._state_Interface(__ev),
                "Actions" => self._state_Actions(__ev),
                "Operations" => self._state_Operations(__ev),
                "Machine" => self._state_Machine(__ev),
                "StateHeader" => self._state_StateHeader(__ev),
                "StateBody" => self._state_StateBody(__ev),
                "Done" => self._state_Done(__ev),
                _ => {}
            }
        }

        fn __transition(&mut self, next_compartment: SystemBackboneCompartment) {
            self.__next_compartment = Some(next_compartment);
        }

        pub fn parse(&mut self) {
            let __e = alloc::rc::Rc::new(SystemBackboneFrameEvent::Parse {});
            let mut __ctx = SystemBackboneFrameContext::new(alloc::rc::Rc::clone(&__e), None);
            self._context_stack.push(__ctx);
            self.__kernel(&__e);
            self._context_stack.pop();
        }

        fn _state_Start(&mut self, __e: &SystemBackboneFrameEvent) {
            match __e {
                SystemBackboneFrameEvent::Parse { .. } => { self._s_Start_hdl_user_parse(__e); }
                _ => {}
            }
        }

        fn _state_Sections(&mut self, __e: &SystemBackboneFrameEvent) {
            match __e {
                SystemBackboneFrameEvent::FrameEnter { .. } => { self._s_Sections_hdl_frame_enter(__e); }
                _ => {}
            }
        }

        // The interface method loop: one `InterfaceMethod` per `Ident`, until a
        // section keyword / Eof. Self-loop = a flat list of methods, attributes
        // accumulating into `pending_attrs` until the next method claims them —
        // the same regular shape as $Machine, proving the interface grammar is
        // regular too.
        fn _state_Interface(&mut self, __e: &SystemBackboneFrameEvent) {
            match __e {
                SystemBackboneFrameEvent::FrameEnter { .. } => { self._s_Interface_hdl_frame_enter(__e); }
                _ => {}
            }
        }

        // The actions loop: one `ActionAst` per `Ident`, until a section
        // keyword / Eof. No attributes (actions take none); unknown tokens are
        // skipped, matching the recursive `parse_actions`.
        fn _state_Actions(&mut self, __e: &SystemBackboneFrameEvent) {
            match __e {
                SystemBackboneFrameEvent::FrameEnter { .. } => { self._s_Actions_hdl_frame_enter(__e); }
                _ => {}
            }
        }

        // The operations loop: one `OperationAst` per `Ident`, with `@@[...]`
        // attributes (RFC-0012 amendment: `@@[save]`/`@@[load]`) accumulating in
        // `pending_attrs` until the next operation claims them. Unknown tokens
        // are skipped, matching the recursive `parse_operations`.
        fn _state_Operations(&mut self, __e: &SystemBackboneFrameEvent) {
            match __e {
                SystemBackboneFrameEvent::FrameEnter { .. } => { self._s_Operations_hdl_frame_enter(__e); }
                _ => {}
            }
        }

        // The machine state loop: one state per `$StateRef`, until a section
        // keyword / Eof. Self-loop = a flat list of states. Each `$StateRef`
        // hands off to `$StateHeader` (the state's own parse phase); the
        // stopping token is left unconsumed for `$Sections` to handle.
        fn _state_Machine(&mut self, __e: &SystemBackboneFrameEvent) {
            match __e {
                SystemBackboneFrameEvent::FrameEnter { .. } => { self._s_Machine_hdl_frame_enter(__e); }
                _ => {}
            }
        }

        // A single state's header: name / optional `=> $Parent` / optional
        // `(params)` / the opening brace. The whole bounded sequence is one
        // oracle verdict (`take_state_header`) — it is regular, so it is a
        // single state that delegates and advances to the body loop.
        fn _state_StateHeader(&mut self, __e: &SystemBackboneFrameEvent) {
            match __e {
                SystemBackboneFrameEvent::FrameEnter { .. } => { self._s_StateHeader_hdl_frame_enter(__e); }
                _ => {}
            }
        }

        // The state-body loop: state-vars, the `$>`/`<$` lifecycle handlers,
        // event handlers (with their accumulated `@@[...]` attributes), and the
        // `=> $^` default forward — in any order until the closing `}`. A
        // self-loop over an unordered member list: regular, no nesting. Each
        // member is delegated to its existing parser oracle; attributes
        // accumulate in `pending_attrs` until the next event handler claims
        // them, exactly as the recursive `parse_state_body` does.
        fn _state_StateBody(&mut self, __e: &SystemBackboneFrameEvent) {
            match __e {
                SystemBackboneFrameEvent::FrameEnter { .. } => { self._s_StateBody_hdl_frame_enter(__e); }
                _ => {}
            }
        }

        fn _state_Done(&mut self, __e: &SystemBackboneFrameEvent) {
            match __e {
                _ => {}
            }
        }

        fn _s_Start_hdl_user_parse(&mut self, __e: &SystemBackboneFrameEvent) {
            let mut __compartment = self.__prepareEnter("Sections");
            self.__transition(__compartment);
            return;
        }

        fn _s_Sections_hdl_frame_enter(&mut self, __e: &SystemBackboneFrameEvent) {
            // Classify the next section; the block scopes the peek borrow.
            let pk: Result<u8, ParseError> = {
                let p = self.parser.as_mut().unwrap();
                match p.peek() {
                    Ok(Token::Eof) => Ok(0),
                    Ok(Token::Interface) => Ok(1),
                    Ok(Token::Machine) => Ok(2),
                    Ok(Token::Actions) => Ok(3),
                    Ok(Token::Operations) => Ok(4),
                    Ok(Token::Domain) => Ok(5),
                    Ok(_) => Ok(6),
                    Err(e) => Err(ParseError::from(e.clone())),
                }
            };
            match pk {
                Err(e) => {
                    self.error = Some(e);
                    let mut __compartment = self.__prepareEnter("Done");
                    self.__transition(__compartment);
                    return;
                }
                Ok(0) => {
                    let c = self.parser.as_ref().unwrap().cursor();
                    let st = self.start;
                    self.system.as_mut().unwrap().span = Span::new(st, c);
                    let mut __compartment = self.__prepareEnter("Done");
                    self.__transition(__compartment);
                    return;
                }
                Ok(1) => {
                    // Consume `interface:`, then drive the method loop
                    // ourselves ($Interface) rather than delegating the
                    // whole section to parse_interface_methods.
                    match self.parser.as_mut().unwrap().consume_section_header() {
                        Ok(()) => {
                            self.system.as_mut().unwrap().section_order.push(SystemSectionKind::Interface);
                            self.iface_methods = Vec::new();
                            self.pending_attrs = Vec::new();
                            let mut __compartment = self.__prepareEnter("Interface");
                            self.__transition(__compartment);
                            return;
                        }
                        Err(e) => { self.error = Some(e);
                        let mut __compartment = self.__prepareEnter("Done");
                        self.__transition(__compartment);
                        return; }
                    }
                }
                Ok(2) => {
                    // Consume `machine:`, then drive the state loop ourselves
                    // ($Machine) rather than delegating to parse_machine.
                    match self.parser.as_mut().unwrap().consume_section_header() {
                        Ok(()) => {
                            self.system.as_mut().unwrap().section_order.push(SystemSectionKind::Machine);
                            self.machine_start = self.parser.as_ref().unwrap().cursor();
                            self.machine_states = Vec::new();
                            let mut __compartment = self.__prepareEnter("Machine");
                            self.__transition(__compartment);
                            return;
                        }
                        Err(e) => { self.error = Some(e);
                        let mut __compartment = self.__prepareEnter("Done");
                        self.__transition(__compartment);
                        return; }
                    }
                }
                Ok(3) => {
                    match self.parser.as_mut().unwrap().consume_section_header() {
                        Ok(()) => {
                            self.system.as_mut().unwrap().section_order.push(SystemSectionKind::Actions);
                            self.action_decls = Vec::new();
                            let mut __compartment = self.__prepareEnter("Actions");
                            self.__transition(__compartment);
                            return;
                        }
                        Err(e) => { self.error = Some(e);
                        let mut __compartment = self.__prepareEnter("Done");
                        self.__transition(__compartment);
                        return; }
                    }
                }
                Ok(4) => {
                    match self.parser.as_mut().unwrap().consume_section_header() {
                        Ok(()) => {
                            self.system.as_mut().unwrap().section_order.push(SystemSectionKind::Operations);
                            self.ops = Vec::new();
                            self.pending_attrs = Vec::new();
                            let mut __compartment = self.__prepareEnter("Operations");
                            self.__transition(__compartment);
                            return;
                        }
                        Err(e) => { self.error = Some(e);
                        let mut __compartment = self.__prepareEnter("Done");
                        self.__transition(__compartment);
                        return; }
                    }
                }
                Ok(5) => {
                    match self.parser.as_mut().unwrap().take_domain_section() {
                        Ok(v) => {
                            let s = self.system.as_mut().unwrap();
                            s.section_order.push(SystemSectionKind::Domain);
                            s.domain = v;
                            let mut __compartment = self.__prepareEnter("Sections");
                            self.__transition(__compartment);
                            return;
                        }
                        Err(e) => { self.error = Some(e);
                        let mut __compartment = self.__prepareEnter("Done");
                        self.__transition(__compartment);
                        return; }
                    }
                }
                Ok(_) => {
                    // Unexpected token — same diagnostic as the recursive form.
                    match self.parser.as_mut().unwrap().advance() {
                        Ok(sp) => {
                            self.error = Some(ParseError {
                                message: format!("Expected section keyword, found {:?}", sp.token),
                                span: sp.span,
                            });
                        }
                        Err(e) => {
                            self.error = Some(ParseError::from(e));
                        }
                    }
                    let mut __compartment = self.__prepareEnter("Done");
                    self.__transition(__compartment);
                    return;
                }
            }
        }

        fn _s_Interface_hdl_frame_enter(&mut self, __e: &SystemBackboneFrameEvent) {
            let pk: Result<u8, ParseError> = {
                let p = self.parser.as_mut().unwrap();
                match p.peek() {
                    Ok(Token::Interface)
                    | Ok(Token::Machine)
                    | Ok(Token::Actions)
                    | Ok(Token::Operations)
                    | Ok(Token::Domain)
                    | Ok(Token::Eof) => Ok(0),
                    Ok(Token::Attribute { .. }) => Ok(1),
                    Ok(Token::Ident(_)) => Ok(2),
                    Ok(_) => Ok(3),
                    Err(e) => Err(ParseError::from(e.clone())),
                }
            };
            match pk {
                Err(e) => { self.error = Some(e);
                let mut __compartment = self.__prepareEnter("Done");
                self.__transition(__compartment);
                return; }
                Ok(0) => {
                    // End of interface section — store the methods and hand
                    // the stopping token back to $Sections (unconsumed).
                    let methods = std::mem::take(&mut self.iface_methods);
                    self.system.as_mut().unwrap().interface = methods;
                    let mut __compartment = self.__prepareEnter("Sections");
                    self.__transition(__compartment);
                    return;
                }
                Ok(1) => {
                    match self.parser.as_mut().unwrap().advance() {
                        Ok(spanned) => {
                            if let Token::Attribute { name, args } = spanned.token {
                                self.pending_attrs.push(
                                    crate::frame_c::compiler::frame_ast::Attribute {
                                        name,
                                        args,
                                        span: spanned.span,
                                    },
                                );
                            }
                            let mut __compartment = self.__prepareEnter("Interface");
                            self.__transition(__compartment);
                            return;
                        }
                        Err(e) => { self.error = Some(ParseError::from(e));
                        let mut __compartment = self.__prepareEnter("Done");
                        self.__transition(__compartment);
                        return; }
                    }
                }
                Ok(2) => {
                    match self.parser.as_mut().unwrap().parse_interface_method() {
                        Ok(mut m) => {
                            m.attributes = std::mem::take(&mut self.pending_attrs);
                            self.iface_methods.push(m);
                            let mut __compartment = self.__prepareEnter("Interface");
                            self.__transition(__compartment);
                            return;
                        }
                        Err(e) => { self.error = Some(e);
                        let mut __compartment = self.__prepareEnter("Done");
                        self.__transition(__compartment);
                        return; }
                    }
                }
                Ok(_) => {
                    match self.parser.as_mut().unwrap().advance() {
                        Ok(spanned) => {
                            self.error = Some(ParseError {
                                message: format!(
                                    "Expected method name in interface, found {:?}",
                                    spanned.token
                                ),
                                span: spanned.span,
                            });
                        }
                        Err(e) => { self.error = Some(ParseError::from(e)); }
                    }
                    let mut __compartment = self.__prepareEnter("Done");
                    self.__transition(__compartment);
                    return;
                }
            }
        }

        fn _s_Actions_hdl_frame_enter(&mut self, __e: &SystemBackboneFrameEvent) {
            let pk: Result<u8, ParseError> = {
                let p = self.parser.as_mut().unwrap();
                match p.peek() {
                    Ok(Token::Interface)
                    | Ok(Token::Machine)
                    | Ok(Token::Actions)
                    | Ok(Token::Operations)
                    | Ok(Token::Domain)
                    | Ok(Token::Eof) => Ok(0),
                    Ok(Token::Ident(_)) => Ok(1),
                    Ok(_) => Ok(2),
                    Err(e) => Err(ParseError::from(e.clone())),
                }
            };
            match pk {
                Err(e) => { self.error = Some(e);
                let mut __compartment = self.__prepareEnter("Done");
                self.__transition(__compartment);
                return; }
                Ok(0) => {
                    let actions = std::mem::take(&mut self.action_decls);
                    self.system.as_mut().unwrap().actions = actions;
                    let mut __compartment = self.__prepareEnter("Sections");
                    self.__transition(__compartment);
                    return;
                }
                Ok(1) => {
                    match self.parser.as_mut().unwrap().parse_action() {
                        Ok(a) => { self.action_decls.push(a);
                        let mut __compartment = self.__prepareEnter("Actions");
                        self.__transition(__compartment);
                        return; }
                        Err(e) => { self.error = Some(e);
                        let mut __compartment = self.__prepareEnter("Done");
                        self.__transition(__compartment);
                        return; }
                    }
                }
                Ok(_) => {
                    match self.parser.as_mut().unwrap().advance() {
                        Ok(_) => {
                            let mut __compartment = self.__prepareEnter("Actions");
                            self.__transition(__compartment);
                            return; }
                        Err(e) => { self.error = Some(ParseError::from(e));
                        let mut __compartment = self.__prepareEnter("Done");
                        self.__transition(__compartment);
                        return; }
                    }
                }
            }
        }

        fn _s_Operations_hdl_frame_enter(&mut self, __e: &SystemBackboneFrameEvent) {
            let pk: Result<u8, ParseError> = {
                let p = self.parser.as_mut().unwrap();
                match p.peek() {
                    Ok(Token::Interface)
                    | Ok(Token::Machine)
                    | Ok(Token::Actions)
                    | Ok(Token::Operations)
                    | Ok(Token::Domain)
                    | Ok(Token::Eof) => Ok(0),
                    Ok(Token::Attribute { .. }) => Ok(1),
                    Ok(Token::Ident(_)) => Ok(2),
                    Ok(_) => Ok(3),
                    Err(e) => Err(ParseError::from(e.clone())),
                }
            };
            match pk {
                Err(e) => { self.error = Some(e);
                let mut __compartment = self.__prepareEnter("Done");
                self.__transition(__compartment);
                return; }
                Ok(0) => {
                    let ops = std::mem::take(&mut self.ops);
                    self.system.as_mut().unwrap().operations = ops;
                    let mut __compartment = self.__prepareEnter("Sections");
                    self.__transition(__compartment);
                    return;
                }
                Ok(1) => {
                    match self.parser.as_mut().unwrap().advance() {
                        Ok(spanned) => {
                            if let Token::Attribute { name, args } = spanned.token {
                                self.pending_attrs.push(
                                    crate::frame_c::compiler::frame_ast::Attribute {
                                        name,
                                        args,
                                        span: spanned.span,
                                    },
                                );
                            }
                            let mut __compartment = self.__prepareEnter("Operations");
                            self.__transition(__compartment);
                            return;
                        }
                        Err(e) => { self.error = Some(ParseError::from(e));
                        let mut __compartment = self.__prepareEnter("Done");
                        self.__transition(__compartment);
                        return; }
                    }
                }
                Ok(2) => {
                    match self.parser.as_mut().unwrap().parse_operation() {
                        Ok(mut op) => {
                            op.attributes = std::mem::take(&mut self.pending_attrs);
                            self.ops.push(op);
                            let mut __compartment = self.__prepareEnter("Operations");
                            self.__transition(__compartment);
                            return;
                        }
                        Err(e) => { self.error = Some(e);
                        let mut __compartment = self.__prepareEnter("Done");
                        self.__transition(__compartment);
                        return; }
                    }
                }
                Ok(_) => {
                    match self.parser.as_mut().unwrap().advance() {
                        Ok(_) => {
                            let mut __compartment = self.__prepareEnter("Operations");
                            self.__transition(__compartment);
                            return; }
                        Err(e) => { self.error = Some(ParseError::from(e));
                        let mut __compartment = self.__prepareEnter("Done");
                        self.__transition(__compartment);
                        return; }
                    }
                }
            }
        }

        fn _s_Machine_hdl_frame_enter(&mut self, __e: &SystemBackboneFrameEvent) {
            let pk: Result<u8, ParseError> = {
                let p = self.parser.as_mut().unwrap();
                match p.peek() {
                    Ok(Token::StateRef(_)) => Ok(1),
                    Ok(Token::Interface)
                    | Ok(Token::Actions)
                    | Ok(Token::Operations)
                    | Ok(Token::Domain)
                    | Ok(Token::Machine)
                    | Ok(Token::Eof) => Ok(2),
                    Ok(_) => Ok(3),
                    Err(e) => Err(ParseError::from(e.clone())),
                }
            };
            match pk {
                Err(e) => { self.error = Some(e);
                let mut __compartment = self.__prepareEnter("Done");
                self.__transition(__compartment);
                return; }
                Ok(1) => {
                    // A state declaration — parse its header + body as the
                    // dedicated $StateHeader/$StateBody phases.
                    let mut __compartment = self.__prepareEnter("StateHeader");
                    self.__transition(__compartment);
                    return;
                }
                Ok(2) => {
                    // End of machine section — finalize and hand the
                    // stopping token back to $Sections (unconsumed).
                    let c = self.parser.as_ref().unwrap().cursor();
                    let states = std::mem::take(&mut self.machine_states);
                    self.system.as_mut().unwrap().machine =
                        Some(MachineAst { states, span: Span::new(self.machine_start, c) });
                    let mut __compartment = self.__prepareEnter("Sections");
                    self.__transition(__compartment);
                    return;
                }
                Ok(_) => {
                    match self.parser.as_mut().unwrap().advance() {
                        Ok(sp) => {
                            self.error = Some(ParseError {
                                message: format!(
                                    "Expected state declaration in machine, found {:?}",
                                    sp.token
                                ),
                                span: sp.span,
                            });
                        }
                        Err(e) => { self.error = Some(ParseError::from(e)); }
                    }
                    let mut __compartment = self.__prepareEnter("Done");
                    self.__transition(__compartment);
                    return;
                }
            }
        }

        fn _s_StateHeader_hdl_frame_enter(&mut self, __e: &SystemBackboneFrameEvent) {
            match self.parser.as_mut().unwrap().take_state_header() {
                Ok((st, bc)) => {
                    self.current_state = Some(st);
                    self.state_body_close = bc;
                    self.pending_attrs = Vec::new();
                    let mut __compartment = self.__prepareEnter("StateBody");
                    self.__transition(__compartment);
                    return;
                }
                Err(e) => { self.error = Some(e);
                let mut __compartment = self.__prepareEnter("Done");
                self.__transition(__compartment);
                return; }
            }
        }

        fn _s_StateBody_hdl_frame_enter(&mut self, __e: &SystemBackboneFrameEvent) {
            let pk: Result<u8, ParseError> = {
                let p = self.parser.as_mut().unwrap();
                match p.peek() {
                    Ok(Token::RBrace) | Ok(Token::Eof) => Ok(0),
                    Ok(Token::Attribute { .. }) => Ok(1),
                    Ok(Token::StateVarRef(_)) => Ok(2),
                    Ok(Token::EnterHandler) => Ok(3),
                    Ok(Token::ExitHandler) => Ok(4),
                    Ok(Token::Ident(_)) => Ok(5),
                    Ok(Token::FatArrow) => Ok(6),
                    Ok(_) => Ok(7),
                    Err(e) => Err(ParseError::from(e.clone())),
                }
            };
            match pk {
                Err(e) => { self.error = Some(e);
                let mut __compartment = self.__prepareEnter("Done");
                self.__transition(__compartment);
                return; }
                Ok(0) => {
                    // End of body: skip past `}` and hand the finished
                    // state back to the $Machine loop.
                    let bc = self.state_body_close;
                    self.parser.as_mut().unwrap().set_cursor(bc + 1);
                    let st = self.current_state.take().unwrap();
                    self.machine_states.push(st);
                    let mut __compartment = self.__prepareEnter("Machine");
                    self.__transition(__compartment);
                    return;
                }
                Ok(1) => {
                    // RFC-0013 attribute: accumulate for the next handler.
                    match self.parser.as_mut().unwrap().advance() {
                        Ok(spanned) => {
                            if let Token::Attribute { name, args } = spanned.token {
                                self.pending_attrs.push(
                                    crate::frame_c::compiler::frame_ast::Attribute {
                                        name,
                                        args,
                                        span: spanned.span,
                                    },
                                );
                            }
                            let mut __compartment = self.__prepareEnter("StateBody");
                            self.__transition(__compartment);
                            return;
                        }
                        Err(e) => { self.error = Some(ParseError::from(e));
                        let mut __compartment = self.__prepareEnter("Done");
                        self.__transition(__compartment);
                        return; }
                    }
                }
                Ok(2) => {
                    match self.parser.as_mut().unwrap().parse_state_var_decl() {
                        Ok(sv) => {
                            self.current_state.as_mut().unwrap().state_vars.push(sv);
                            let mut __compartment = self.__prepareEnter("StateBody");
                            self.__transition(__compartment);
                            return;
                        }
                        Err(e) => { self.error = Some(e);
                        let mut __compartment = self.__prepareEnter("Done");
                        self.__transition(__compartment);
                        return; }
                    }
                }
                Ok(3) => {
                    let bc = self.state_body_close;
                    match self.parser.as_mut().unwrap().parse_enter_handler(bc) {
                        Ok(h) => {
                            self.current_state.as_mut().unwrap().enter = Some(h);
                            let mut __compartment = self.__prepareEnter("StateBody");
                            self.__transition(__compartment);
                            return;
                        }
                        Err(e) => { self.error = Some(e);
                        let mut __compartment = self.__prepareEnter("Done");
                        self.__transition(__compartment);
                        return; }
                    }
                }
                Ok(4) => {
                    let bc = self.state_body_close;
                    match self.parser.as_mut().unwrap().parse_exit_handler(bc) {
                        Ok(h) => {
                            self.current_state.as_mut().unwrap().exit = Some(h);
                            let mut __compartment = self.__prepareEnter("StateBody");
                            self.__transition(__compartment);
                            return;
                        }
                        Err(e) => { self.error = Some(e);
                        let mut __compartment = self.__prepareEnter("Done");
                        self.__transition(__compartment);
                        return; }
                    }
                }
                Ok(5) => {
                    let bc = self.state_body_close;
                    match self.parser.as_mut().unwrap().parse_event_handler(bc) {
                        Ok(mut h) => {
                            h.attributes = std::mem::take(&mut self.pending_attrs);
                            self.current_state.as_mut().unwrap().handlers.push(h);
                            let mut __compartment = self.__prepareEnter("StateBody");
                            self.__transition(__compartment);
                            return;
                        }
                        Err(e) => { self.error = Some(e);
                        let mut __compartment = self.__prepareEnter("Done");
                        self.__transition(__compartment);
                        return; }
                    }
                }
                Ok(6) => {
                    // Default forward: `=> $^`.
                    let r: Result<bool, ParseError> = {
                        let p = self.parser.as_mut().unwrap();
                        match p.advance() {
                            Ok(_) => match p.check(&Token::ParentRef) {
                                Ok(true) => match p.advance() {
                                    Ok(_) => Ok(true),
                                    Err(e) => Err(ParseError::from(e)),
                                },
                                Ok(false) => Ok(false),
                                Err(e) => Err(ParseError::from(e)),
                            },
                            Err(e) => Err(ParseError::from(e)),
                        }
                    };
                    match r {
                        Ok(true) => {
                            self.current_state.as_mut().unwrap().default_forward = true;
                            let mut __compartment = self.__prepareEnter("StateBody");
                            self.__transition(__compartment);
                            return;
                        }
                        Ok(false) => {
                            let mut __compartment = self.__prepareEnter("StateBody");
                            self.__transition(__compartment);
                            return; }
                        Err(e) => { self.error = Some(e);
                        let mut __compartment = self.__prepareEnter("Done");
                        self.__transition(__compartment);
                        return; }
                    }
                }
                Ok(_) => {
                    // Skip unknown tokens in the state body.
                    match self.parser.as_mut().unwrap().advance() {
                        Ok(_) => {
                            let mut __compartment = self.__prepareEnter("StateBody");
                            self.__transition(__compartment);
                            return; }
                        Err(e) => { self.error = Some(ParseError::from(e));
                        let mut __compartment = self.__prepareEnter("Done");
                        self.__transition(__compartment);
                        return; }
                    }
                }
            }
        }
    }
}
pub use _system_backbone_framec::*;
