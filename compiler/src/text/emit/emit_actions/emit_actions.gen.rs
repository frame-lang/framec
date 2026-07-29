
// The driver's ACTIONS/OPERATIONS walk, dogfooded as a plain `@@system` — the emit-side sequencer
// that reifies `emit`'s `actions:` / `operations:` pass (one method per user-bodied member; the
// signature is Frame's, the body is the user's). It rides the same READ-ONLY BORROWED DOMAIN as
// StmtWalk/BaseColumn/EmitHandlers: the section slice, the source, the symbol table, the system
// symbol, and the `&dyn Backend` are SHARED BORROWS threaded through one lifetime `'a`; the OWNED
// domain is the accumulating output `out`, the two walk cursors (`si`/`mi`) and their bounds
// (`nsec`/`nm`).
//
// THE 2-LEVEL NESTING, EXPRESSED WITHOUT push$/pop$. The pass is a FIXED depth-2 walk — sections,
// then, for each `actions:`/`operations:` section, its member decls — so a stack is unnecessary (a
// stack buys UNBOUNDED depth; this depth is 2 and known). It is expressed instead as two NESTED
// CYCLE STATES with explicit up/down edges, one owned cursor per level:
//   $Section  cycles over `sections` (fork: only the sections ADMITTED IN THIS PASS descend); on
//             such a section it sets the member bound `nm`, resets `mi`, and descends
//             `-> $Member`; at `si >= nsec` it advances the PASS (below) or halts `-> $Done`.
//   $Member   cycles over the current section's `members` (fork: only `Decl::WithBody` emits); on a
//             bodied member it opens one action, walks its body via the StmtWalk leaf, and closes
//             it; at `mi >= nm` it ASCENDS (`si += 1`, `-> $Section`).
// THE PASS CURSOR. Some targets emit `actions:` members BEFORE `operations:` members whatever
// order the two sections were declared in (the shipped compiler holds them in two collections and
// runs two passes; Frame's canonical block order actually puts `operations:` FIRST, so source order
// is not it). That is expressed as a third cursor, `phase`, over a FIXED, KNOWN pass count `nphase`
// (1 = one pass admitting both kinds in source order; 2 = actions pass, then operations pass): when
// `si` runs off the end and another pass remains, `phase` advances and `si` resets.
//
// `phase` is a CURSOR, not a mode worth naming as a state (Shadows §3 degenerate pole): like `si`
// and `mi` it is a program counter over ALREADY-PARSED tree data, it gates no recognition, and
// deleting it would change output only by reordering. It becomes a state to reify the day a pass
// carries something forward INTO the next one — a name table, a dedup set, an emitted-count read
// back to gate the second pass. Nothing does today.
//
// The "mode" is the walk DEPTH (which of the two cycle states is live); the cursors advance it.
// This is the §3 degenerate pole — a program-counter walk over ALREADY-PARSED tree data, whose
// forks are structural type-dispatch (`Section::Actions`? `Decl::WithBody`? — Frame cannot match a
// Rust enum), not input recognition. It carries no recognition register; nothing is glossed. Its
// reify payoff is not a hidden mode but DOGFOOD UNIFORMITY (the maximal-rebuild campaign: the
// cleanroom emits its own driver as an @@system, differential-gated byte-for-byte vs the preserved
// `emit_actions_hand`).
//
// framec owns the WALK (the two cursors, the bounds, the descents/ascents, the halt). The
// un-Frame-able work is per-item NATIVE LEAVES: the structural forks/bounds (`is_action_section`,
// `action_member_count`, `is_withbody_member`), and `emit_action`, which spells ONE method:
// `be.open_action(...)`, then the StmtWalk body walk (`emit_body`, unchanged, called as a leaf —
// NOT reinlined, its `BodyEnd` discarded exactly as the hand pass discarded it), then
// `be.close_action(...)`. Every materialization spelling stays native and byte-identical; the
// machine only sequences the walk.
//
// Regen: framec-ng -l rust --emit emit_actions.frs | grep -v '^#!\[allow' > emit_actions.gen.rs

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
mod _emit_actions_framec {
    use super::*;
    extern crate alloc;
    use alloc::{vec, format};
    #[derive(Clone)]
    #[allow(dead_code, non_camel_case_types)]
    enum EmitActionsFrameEvent {
        Step {  },
        FrameEnter {},
        FrameExit {},
    }

    #[derive(Clone)]
    #[allow(dead_code, non_camel_case_types)]
    enum EmitActionsFrameReturn {
        _Lifecycle(alloc::rc::Rc<dyn core::any::Any>),
    }

    #[allow(dead_code)]
    impl EmitActionsFrameEvent {
        fn name(&self) -> &'static str {
            match self {
                EmitActionsFrameEvent::Step { .. } => "step",
                EmitActionsFrameEvent::FrameEnter { .. } => "$>",
                EmitActionsFrameEvent::FrameExit { .. } => "<$",
            }
        }
    }

    #[derive(Clone, Debug)]
    #[allow(dead_code, non_camel_case_types)]
    enum EmitActionsFrameValue {
        Int(i64),
        Float(f64),
        Bool(bool),
        Str(String),
        List(Vec<Self>),
        Dict(alloc::collections::BTreeMap<String, Self>),
    }

    #[allow(dead_code, non_camel_case_types)]
    struct EmitActionsFrameContext {
        event: alloc::rc::Rc<EmitActionsFrameEvent>,
        _return: Option<EmitActionsFrameReturn>,
        _data: alloc::collections::BTreeMap<String, EmitActionsFrameValue>,
        _transitioned: bool,
    }

    impl EmitActionsFrameContext {
        fn new(event: alloc::rc::Rc<EmitActionsFrameEvent>, default_return: Option<EmitActionsFrameReturn>) -> Self {
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
    enum EmitActionsStateContext {
        Section,
        Member,
        Done,
        __NoContext,
    }

    impl Default for EmitActionsStateContext {
        fn default() -> Self {
            EmitActionsStateContext::Section
        }
    }

    #[allow(dead_code, non_camel_case_types)]
    #[derive(Clone)]
    struct EmitActionsCompartment {
        state: String,
        state_context: EmitActionsStateContext,
        forward_event: Option<EmitActionsFrameEvent>,
        parent_compartment: Option<Box<EmitActionsCompartment>>,
    }

    impl EmitActionsCompartment {
        fn new(state: &str) -> Self {
            let state_context = match state {
                "Section" => EmitActionsStateContext::Section,
                "Member" => EmitActionsStateContext::Member,
                "Done" => EmitActionsStateContext::Done,
                _ => EmitActionsStateContext::__NoContext,
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
    pub struct EmitActions<'a> {
        _state_stack: Vec<EmitActionsCompartment>,
        __compartment: EmitActionsCompartment,
        __next_compartment: Option<EmitActionsCompartment>,
        _context_stack: Vec<EmitActionsFrameContext>,
        pub src: &'a Source,
        pub syms: &'a SymbolTable,
        pub sym: &'a SystemSym,
        pub sections: &'a [Section],
        pub be: &'a dyn Backend,
        pub nsec: usize,
        pub nphase: usize,
        pub out: Sink,
        pub nm: usize,
        pub si: usize,
        pub phase: usize,
        pub mi: usize,
    }

    #[allow(non_snake_case)]
    impl<'a> EmitActions<'a> {
        pub fn new(src: &'a Source, syms: &'a SymbolTable, sym: &'a SystemSym, sections: &'a [Section], be: &'a dyn Backend, nsec: usize, nphase: usize, out: Sink) -> Self {
            Self {
                _state_stack: Vec::new(),
                _context_stack: Vec::new(),
                src: src,
                syms: syms,
                sym: sym,
                sections: sections,
                be: be,
                nsec: nsec,
                nphase: nphase,
                out: out,
                nm: 0,
                si: 0,
                phase: 0,
                mi: 0,
                __compartment: EmitActionsCompartment::new("Section"),
                __next_compartment: None,
            }
        }

        pub fn __create(src: &'a Source, syms: &'a SymbolTable, sym: &'a SystemSym, sections: &'a [Section], be: &'a dyn Backend, nsec: usize, nphase: usize, out: Sink) -> Self {
            let mut c = Self::new(src, syms, sym, sections, be, nsec, nphase, out);
            c.__compartment = c.__prepareEnter("Section");
            let __e = alloc::rc::Rc::new(EmitActionsFrameEvent::FrameEnter {});
            let __ctx = EmitActionsFrameContext::new(alloc::rc::Rc::clone(&__e), None);
            c._context_stack.push(__ctx);
            c.__kernel(&__e);
            c._context_stack.pop();
            c
        }

        fn __hsm_chain(&mut self, leaf: &str) -> &'static [&'static str] {
            match leaf {
                "Section" => &["Section"],
                "Member" => &["Member"],
                "Done" => &["Done"],
                _ => &[],
            }
        }

        fn __prepareEnter(&mut self, leaf: &str) -> EmitActionsCompartment {
            let chain = self.__hsm_chain(leaf);
            let mut comp: Option<EmitActionsCompartment> = None;
            for name in chain.iter() {
                let mut new_comp = EmitActionsCompartment::new(name);
                if let Some(parent) = comp.take() {
                    new_comp.parent_compartment = Some(Box::new(parent));
                }
                comp = Some(new_comp);
            }
            comp.expect("chain must contain at least the leaf state")
        }

        fn __kernel(&mut self, __e: &alloc::rc::Rc<EmitActionsFrameEvent>) {
            // Route event to current state.
            self.__router(__e);
            // Drain any transitions queued by the handler.
            while self.__next_compartment.is_some() {
                let next_compartment = self.__next_compartment.take().expect("invariant: while-loop guard checked is_some()");
                // Exit the current (leaf) state. RFC-0025.1: exit args live in the
                // source state's typed ctx (written at the transition site), so the
                // synthesized `<$` event carries no payload.
                let exit_event = alloc::rc::Rc::new(EmitActionsFrameEvent::FrameExit {});
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
                        let enter_event = alloc::rc::Rc::new(EmitActionsFrameEvent::FrameEnter {});
                        self.__router(&enter_event);
                    }
                    Some(fwd) if matches!(fwd, EmitActionsFrameEvent::FrameEnter { .. }) => {
                        // Forwarded event IS $> — dispatch directly so the
                        // destination's $> handler receives the caller's payload.
                        let fwd_rc = alloc::rc::Rc::new(fwd);
                        self.__router(&fwd_rc);
                    }
                    Some(fwd) => {
                        // Forwarded event is not $> — initialize the destination
                        // with a fresh $>, then dispatch the forward.
                        let enter_event = alloc::rc::Rc::new(EmitActionsFrameEvent::FrameEnter {});
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

        fn __router(&mut self, __e: &alloc::rc::Rc<EmitActionsFrameEvent>) {
            let __ev: &EmitActionsFrameEvent = __e;
            match self.__compartment.state.as_str() {
                "Section" => self._state_Section(__ev),
                "Member" => self._state_Member(__ev),
                "Done" => self._state_Done(__ev),
                _ => {}
            }
        }

        fn __transition(&mut self, next_compartment: EmitActionsCompartment) {
            self.__next_compartment = Some(next_compartment);
        }

        pub fn step(&mut self) {
            let __e = alloc::rc::Rc::new(EmitActionsFrameEvent::Step {});
            let mut __ctx = EmitActionsFrameContext::new(alloc::rc::Rc::clone(&__e), None);
            self._context_stack.push(__ctx);
            self.__kernel(&__e);
            self._context_stack.pop();
        }

        fn _state_Section(&mut self, __e: &EmitActionsFrameEvent) {
            match __e {
                EmitActionsFrameEvent::Step { .. } => { self._s_Section_hdl_user_step(__e); }
                _ => {}
            }
        }

        fn _state_Member(&mut self, __e: &EmitActionsFrameEvent) {
            match __e {
                EmitActionsFrameEvent::Step { .. } => { self._s_Member_hdl_user_step(__e); }
                _ => {}
            }
        }

        fn _state_Done(&mut self, __e: &EmitActionsFrameEvent) {
            match __e {
                _ => {}
            }
        }

        fn _s_Section_hdl_user_step(&mut self, __e: &EmitActionsFrameEvent) {
            if self.si >= self.nsec {
                if self.phase + 1 >= self.nphase {
                    let mut __compartment = self.__prepareEnter("Done");
                    self.__transition(__compartment);
                    return;
                }
                self.phase = self.phase + 1;
                self.si = 0;
                let mut __compartment = self.__prepareEnter("Section");
                self.__transition(__compartment);
                return;
            }
            let isa = is_action_section(self.sections, self.si, self.phase, self.nphase);
            if isa == false {
                self.si = self.si + 1;
                let mut __compartment = self.__prepareEnter("Section");
                self.__transition(__compartment);
                return;
            }
            self.nm = action_member_count(self.sections, self.si, self.phase, self.nphase);
            self.mi = 0;
            let mut __compartment = self.__prepareEnter("Member");
            self.__transition(__compartment);
            return;
        }

        fn _s_Member_hdl_user_step(&mut self, __e: &EmitActionsFrameEvent) {
            if self.mi >= self.nm {
                self.si = self.si + 1;
                let mut __compartment = self.__prepareEnter("Section");
                self.__transition(__compartment);
                return;
            }
            let iswb = is_withbody_member(self.sections, self.si, self.mi, self.phase, self.nphase);
            if iswb == false {
                emit_action_trivia(self.src, self.be, self.sections, self.si, self.mi, self.phase, self.nphase, &mut self.out);
                self.mi = self.mi + 1;
                let mut __compartment = self.__prepareEnter("Member");
                self.__transition(__compartment);
                return;
            }
            emit_action(self.src, self.syms, self.sym, self.be, self.sections, self.si, self.mi, self.phase, self.nphase, &mut self.out);
            self.mi = self.mi + 1;
            let mut __compartment = self.__prepareEnter("Member");
            self.__transition(__compartment);
            return;
        }
    }
}
pub use _emit_actions_framec::*;
