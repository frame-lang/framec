
// The driver's HANDLER-EMISSION walk, dogfooded as a plain `@@system` — the emit-side sequencer
// that reifies `emit`'s `(section, state, handler)` nested pass (the private per-handler methods).
// It rides the same READ-ONLY BORROWED DOMAIN as StmtWalk/BaseColumn: the section slice, the
// source, the symbol table, the system symbol, and the `&dyn Backend` are SHARED BORROWS threaded
// through one lifetime `'a`; the OWNED domain is the accumulating output `out`, the three walk
// cursors (`si`/`sti`/`hi`), and their bounds (`nsec`/`nst`/`nh`).
//
// THE 3-LEVEL NESTING, EXPRESSED WITHOUT push$/pop$. The pass is a FIXED depth-3 walk — sections,
// then that section's states, then that state's handlers — so a stack is unnecessary (a stack buys
// UNBOUNDED depth; this depth is 3 and known). It is expressed instead as three NESTED CYCLE
// STATES with explicit up/down edges, one owned cursor per level:
//   $Section  cycles over `sections` (fork: only `Section::Machine` descends); on a machine
//             section it sets the state bound `nst`, resets `sti`, and descends `-> $State`; at
//             `si >= nsec` it halts `-> $Done`.
//   $State    cycles over the current section's `members` (fork: only `MachineMember::State`
//             descends); on a state it sets the handler bound `nh`, resets `hi`, and descends
//             `-> $Handler`; at `sti >= nst` it ASCENDS (`si += 1`, `-> $Section`).
//   $Handler  cycles over the current state's `members` (fork: only `StateMember::Handler`
//             emits); on a handler it emits one private method; at `hi >= nh` it ASCENDS
//             (`sti += 1`, `-> $State`).
// The "mode" is the walk DEPTH (which of the three cycle states is live); the cursors advance it.
// This is the §3 degenerate pole — a program-counter walk over ALREADY-PARSED tree data, whose
// forks are structural type-dispatch (`Section::Machine`? `MachineMember::State`? …), not input
// recognition. It carries no recognition register; nothing is glossed. Its reify payoff is not a
// hidden mode but DOGFOOD UNIFORMITY (the maximal-rebuild campaign: the cleanroom emits its own
// driver as an @@system, differential-gated byte-for-byte vs the preserved `emit_handlers_hand`).
//
// framec owns the WALK (the three cursors, the bounds, the descents/ascents, the halt). The
// un-Frame-able work is per-item NATIVE LEAVES: the structural forks/bounds (`is_machine_section`,
// `member_count`, `is_state_member`, `state_member_count`, `is_handler_member` — Frame cannot match
// a Rust enum), the two per-handler forks the pass computes (`handler_is_async`, `handler_ret` —
// the is_async disjunction and the return-type-inheritance `or_else`), and `emit_handler`, which
// spells ONE private method: `be.open_handler(...)`, then the StmtWalk body walk (`emit_body`,
// unchanged, called as a leaf — NOT reinlined), then `be.close_handler(...)`. Every materialization
// spelling stays native and byte-identical; the machine only sequences the walk.
//
// Regen: framec-ng -l rust --emit emit_handlers.frs | grep -v '^#!\[allow' > emit_handlers.gen.rs

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
mod _emit_handlers_framec {
    use super::*;
    extern crate alloc;
    use alloc::{vec, format};
    #[derive(Clone)]
    #[allow(dead_code, non_camel_case_types)]
    enum EmitHandlersFrameEvent {
        Step {  },
        FrameEnter {},
        FrameExit {},
    }

    #[derive(Clone)]
    #[allow(dead_code, non_camel_case_types)]
    enum EmitHandlersFrameReturn {
        _Lifecycle(alloc::rc::Rc<dyn core::any::Any>),
    }

    #[allow(dead_code)]
    impl EmitHandlersFrameEvent {
        fn name(&self) -> &'static str {
            match self {
                EmitHandlersFrameEvent::Step { .. } => "step",
                EmitHandlersFrameEvent::FrameEnter { .. } => "$>",
                EmitHandlersFrameEvent::FrameExit { .. } => "<$",
            }
        }
    }

    #[derive(Clone, Debug)]
    #[allow(dead_code, non_camel_case_types)]
    enum EmitHandlersFrameValue {
        Int(i64),
        Float(f64),
        Bool(bool),
        Str(String),
        List(Vec<Self>),
        Dict(alloc::collections::BTreeMap<String, Self>),
    }

    #[allow(dead_code, non_camel_case_types)]
    struct EmitHandlersFrameContext {
        event: alloc::rc::Rc<EmitHandlersFrameEvent>,
        _return: Option<EmitHandlersFrameReturn>,
        _data: alloc::collections::BTreeMap<String, EmitHandlersFrameValue>,
        _transitioned: bool,
    }

    impl EmitHandlersFrameContext {
        fn new(event: alloc::rc::Rc<EmitHandlersFrameEvent>, default_return: Option<EmitHandlersFrameReturn>) -> Self {
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
    enum EmitHandlersStateContext {
        Section,
        State,
        Handler,
        Done,
        __NoContext,
    }

    impl Default for EmitHandlersStateContext {
        fn default() -> Self {
            EmitHandlersStateContext::Section
        }
    }

    #[allow(dead_code, non_camel_case_types)]
    #[derive(Clone)]
    struct EmitHandlersCompartment {
        state: String,
        state_context: EmitHandlersStateContext,
        forward_event: Option<EmitHandlersFrameEvent>,
        parent_compartment: Option<Box<EmitHandlersCompartment>>,
    }

    impl EmitHandlersCompartment {
        fn new(state: &str) -> Self {
            let state_context = match state {
                "Section" => EmitHandlersStateContext::Section,
                "State" => EmitHandlersStateContext::State,
                "Handler" => EmitHandlersStateContext::Handler,
                "Done" => EmitHandlersStateContext::Done,
                _ => EmitHandlersStateContext::__NoContext,
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
    pub struct EmitHandlers<'a> {
        _state_stack: Vec<EmitHandlersCompartment>,
        __compartment: EmitHandlersCompartment,
        __next_compartment: Option<EmitHandlersCompartment>,
        _context_stack: Vec<EmitHandlersFrameContext>,
        pub src: &'a Source,
        pub syms: &'a SymbolTable,
        pub sym: &'a SystemSym,
        pub sections: &'a [Section],
        pub be: &'a dyn Backend,
        pub nsec: usize,
        pub out: Sink,
        pub nst: usize,
        pub nh: usize,
        pub si: usize,
        pub sti: usize,
        pub hi: usize,
    }

    #[allow(non_snake_case)]
    impl<'a> EmitHandlers<'a> {
        pub fn new(src: &'a Source, syms: &'a SymbolTable, sym: &'a SystemSym, sections: &'a [Section], be: &'a dyn Backend, nsec: usize, out: Sink) -> Self {
            Self {
                _state_stack: Vec::new(),
                _context_stack: Vec::new(),
                src: src,
                syms: syms,
                sym: sym,
                sections: sections,
                be: be,
                nsec: nsec,
                out: out,
                nst: 0,
                nh: 0,
                si: 0,
                sti: 0,
                hi: 0,
                __compartment: EmitHandlersCompartment::new("Section"),
                __next_compartment: None,
            }
        }

        pub fn __create(src: &'a Source, syms: &'a SymbolTable, sym: &'a SystemSym, sections: &'a [Section], be: &'a dyn Backend, nsec: usize, out: Sink) -> Self {
            let mut c = Self::new(src, syms, sym, sections, be, nsec, out);
            c.__compartment = c.__prepareEnter("Section");
            let __e = alloc::rc::Rc::new(EmitHandlersFrameEvent::FrameEnter {});
            let __ctx = EmitHandlersFrameContext::new(alloc::rc::Rc::clone(&__e), None);
            c._context_stack.push(__ctx);
            c.__kernel(&__e);
            c._context_stack.pop();
            c
        }

        fn __hsm_chain(&mut self, leaf: &str) -> &'static [&'static str] {
            match leaf {
                "Section" => &["Section"],
                "State" => &["State"],
                "Handler" => &["Handler"],
                "Done" => &["Done"],
                _ => &[],
            }
        }

        fn __prepareEnter(&mut self, leaf: &str) -> EmitHandlersCompartment {
            let chain = self.__hsm_chain(leaf);
            let mut comp: Option<EmitHandlersCompartment> = None;
            for name in chain.iter() {
                let mut new_comp = EmitHandlersCompartment::new(name);
                if let Some(parent) = comp.take() {
                    new_comp.parent_compartment = Some(Box::new(parent));
                }
                comp = Some(new_comp);
            }
            comp.expect("chain must contain at least the leaf state")
        }

        fn __kernel(&mut self, __e: &alloc::rc::Rc<EmitHandlersFrameEvent>) {
            // Route event to current state.
            self.__router(__e);
            // Drain any transitions queued by the handler.
            while self.__next_compartment.is_some() {
                let next_compartment = self.__next_compartment.take().expect("invariant: while-loop guard checked is_some()");
                // Exit the current (leaf) state. RFC-0025.1: exit args live in the
                // source state's typed ctx (written at the transition site), so the
                // synthesized `<$` event carries no payload.
                let exit_event = alloc::rc::Rc::new(EmitHandlersFrameEvent::FrameExit {});
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
                        let enter_event = alloc::rc::Rc::new(EmitHandlersFrameEvent::FrameEnter {});
                        self.__router(&enter_event);
                    }
                    Some(fwd) if matches!(fwd, EmitHandlersFrameEvent::FrameEnter { .. }) => {
                        // Forwarded event IS $> — dispatch directly so the
                        // destination's $> handler receives the caller's payload.
                        let fwd_rc = alloc::rc::Rc::new(fwd);
                        self.__router(&fwd_rc);
                    }
                    Some(fwd) => {
                        // Forwarded event is not $> — initialize the destination
                        // with a fresh $>, then dispatch the forward.
                        let enter_event = alloc::rc::Rc::new(EmitHandlersFrameEvent::FrameEnter {});
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

        fn __router(&mut self, __e: &alloc::rc::Rc<EmitHandlersFrameEvent>) {
            let __ev: &EmitHandlersFrameEvent = __e;
            match self.__compartment.state.as_str() {
                "Section" => self._state_Section(__ev),
                "State" => self._state_State(__ev),
                "Handler" => self._state_Handler(__ev),
                "Done" => self._state_Done(__ev),
                _ => {}
            }
        }

        fn __transition(&mut self, next_compartment: EmitHandlersCompartment) {
            self.__next_compartment = Some(next_compartment);
        }

        pub fn step(&mut self) {
            let __e = alloc::rc::Rc::new(EmitHandlersFrameEvent::Step {});
            let mut __ctx = EmitHandlersFrameContext::new(alloc::rc::Rc::clone(&__e), None);
            self._context_stack.push(__ctx);
            self.__kernel(&__e);
            self._context_stack.pop();
        }

        fn _state_Section(&mut self, __e: &EmitHandlersFrameEvent) {
            match __e {
                EmitHandlersFrameEvent::Step { .. } => { self._s_Section_hdl_user_step(__e); }
                _ => {}
            }
        }

        fn _state_State(&mut self, __e: &EmitHandlersFrameEvent) {
            match __e {
                EmitHandlersFrameEvent::Step { .. } => { self._s_State_hdl_user_step(__e); }
                _ => {}
            }
        }

        fn _state_Handler(&mut self, __e: &EmitHandlersFrameEvent) {
            match __e {
                EmitHandlersFrameEvent::Step { .. } => { self._s_Handler_hdl_user_step(__e); }
                _ => {}
            }
        }

        fn _state_Done(&mut self, __e: &EmitHandlersFrameEvent) {
            match __e {
                _ => {}
            }
        }

        fn _s_Section_hdl_user_step(&mut self, __e: &EmitHandlersFrameEvent) {
            if self.si >= self.nsec {
                let mut __compartment = self.__prepareEnter("Done");
                self.__transition(__compartment);
                return;
            }
            let ism = is_machine_section(self.sections, self.si);
            if ism == false {
                self.si = self.si + 1;
                let mut __compartment = self.__prepareEnter("Section");
                self.__transition(__compartment);
                return;
            }
            self.nst = member_count(self.sections, self.si);
            self.sti = 0;
            let mut __compartment = self.__prepareEnter("State");
            self.__transition(__compartment);
            return;
        }

        fn _s_State_hdl_user_step(&mut self, __e: &EmitHandlersFrameEvent) {
            if self.sti >= self.nst {
                self.si = self.si + 1;
                let mut __compartment = self.__prepareEnter("Section");
                self.__transition(__compartment);
                return;
            }
            let iss = is_state_member(self.sections, self.si, self.sti);
            if iss == false {
                self.sti = self.sti + 1;
                let mut __compartment = self.__prepareEnter("State");
                self.__transition(__compartment);
                return;
            }
            self.nh = state_member_count(self.sections, self.si, self.sti);
            self.hi = 0;
            let mut __compartment = self.__prepareEnter("Handler");
            self.__transition(__compartment);
            return;
        }

        fn _s_Handler_hdl_user_step(&mut self, __e: &EmitHandlersFrameEvent) {
            if self.hi >= self.nh {
                self.sti = self.sti + 1;
                let mut __compartment = self.__prepareEnter("State");
                self.__transition(__compartment);
                return;
            }
            let ish = is_handler_member(self.sections, self.si, self.sti, self.hi);
            if ish == false {
                self.hi = self.hi + 1;
                let mut __compartment = self.__prepareEnter("Handler");
                self.__transition(__compartment);
                return;
            }
            let is_async = handler_is_async(self.sym, self.sections, self.si, self.sti, self.hi, self.be);
            let ret = handler_ret(self.sym, self.sections, self.si, self.sti, self.hi, self.be);
            emit_handler(self.src, self.syms, self.sym, self.be, self.sections, self.si, self.sti, self.hi, is_async, ret, &mut self.out);
            self.hi = self.hi + 1;
            let mut __compartment = self.__prepareEnter("Handler");
            self.__transition(__compartment);
            return;
        }
    }
}
pub use _emit_handlers_framec::*;
