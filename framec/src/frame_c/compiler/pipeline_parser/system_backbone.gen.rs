
// RFC-0039 Stage 0/1 — the system-section loop as a Frame backbone.
//
// Replaces the hand-written `Parser::parse_system` section loop. The backbone
// OWNS the parser (B1) and drives it live; each section is delegated to the
// existing `take_*_section` oracle method on `Parser` (which consumes the
// keyword + `:` and runs that section's parser). The single `$Sections` state
// self-loops — a flat list of sections, no nesting, no push$/pop$ — the
// constructive proof that the section grammar is regular.
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
        Machine,
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
                "Machine" => SystemBackboneStateContext::Machine,
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
                "Machine" => &["Machine"],
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
                "Machine" => self._state_Machine(__ev),
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

        // The machine state loop: one state per `$StateRef`, until a section
        // keyword / Eof. Self-loop = a flat list of states. Each state is
        // delegated to the existing `parse_state` oracle; the stopping token is
        // left unconsumed for `$Sections` to handle.
        fn _state_Machine(&mut self, __e: &SystemBackboneFrameEvent) {
            match __e {
                SystemBackboneFrameEvent::FrameEnter { .. } => { self._s_Machine_hdl_frame_enter(__e); }
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
                    match self.parser.as_mut().unwrap().take_interface_section() {
                        Ok(v) => {
                            let s = self.system.as_mut().unwrap();
                            s.section_order.push(SystemSectionKind::Interface);
                            s.interface = v;
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
                    match self.parser.as_mut().unwrap().take_actions_section() {
                        Ok(v) => {
                            let s = self.system.as_mut().unwrap();
                            s.section_order.push(SystemSectionKind::Actions);
                            s.actions = v;
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
                Ok(4) => {
                    match self.parser.as_mut().unwrap().take_operations_section() {
                        Ok(v) => {
                            let s = self.system.as_mut().unwrap();
                            s.section_order.push(SystemSectionKind::Operations);
                            s.operations = v;
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
                    match self.parser.as_mut().unwrap().parse_state() {
                        Ok(st) => { self.machine_states.push(st);
                        let mut __compartment = self.__prepareEnter("Machine");
                        self.__transition(__compartment);
                        return; }
                        Err(e) => { self.error = Some(e);
                        let mut __compartment = self.__prepareEnter("Done");
                        self.__transition(__compartment);
                        return; }
                    }
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
    }
}
pub use _system_backbone_framec::*;
