
// The generated runtime's STATE-CHAIN TABLE walk, dogfooded as a plain `@@system` — the emit-side
// sequencer that produces, for every leaf state, the ROOT..LEAF path the target's compartment
// factory walks when it enters that state. It rides the same READ-ONLY BORROWED DOMAIN as the
// landed emit machines: the system symbol and the `&dyn Backend` are SHARED BORROWS threaded
// through one lifetime `'a`; the OWNED domain is the accumulating output `out`, the outer cursor
// `si` with its bound `ns`, the climb cursor `ci`, the climb depth `depth`, and the per-state path
// accumulator `chain`.
//
// THE 2-LEVEL NESTING, EXPRESSED WITHOUT push$/pop$ (the EmitInterface shape). The outer level is a
// cursor over `sym.states`; the inner level is an ANCESTOR CLIMB whose depth is bounded by the state
// count, so a stack is unnecessary (a stack buys UNBOUNDED depth; this depth is bounded and known).
// Three cycle states with explicit down/across/up edges:
//   $State  cycles over `sym.states` (`ns` states); on a state it CLEARS the path accumulator, seeds
//           the climb cursor (`ci = si`), and descends `-> $Climb`; at `si >= ns` it halts `-> $Done`.
//   $Climb  pushes the current node's NAME onto `chain` and looks up its parent's INDEX; a parent
//           (`p >= 0`) moves the cursor and loops; no parent (`p < 0`) — or a depth past `ns`, the
//           defensive cycle guard — crosses `-> $Emit`.
//   $Emit   asks the backend to spell ONE table entry from the (reversed, root-first) path, then
//           ASCENDS: `si += 1`, `depth = 0`, `-> $State`.
//
// THE HONEST MACHINE CLASS. §3 degenerate pole again, and the classification is worth stating
// because the climb *looks* like it carries something: `ci` is a MONOTONE CURSOR over an already-
// resolved parent link, not a recognition register — it is read out of the symbol table's frozen
// `parent` field, never advanced by input, and no later behaviour is gated on its value beyond the
// halt. `depth` is a bound, not a mode. Nothing is glossed; the payoff claimed is DOGFOOD UNIFORMITY,
// differential-gated byte-for-byte against the preserved `hsm_chain_hand`.
//
// framec owns the WALK (both cursors, both bounds, the clear, the descents/ascents, the halt). The
// un-Frame-able work is per-item NATIVE LEAVES: `clear_chain` (the per-state accumulator reset),
// `push_state_name` + `parent_index` (symbol-table reads Frame cannot do), and `stamp_chain`, which
// reverses the leaf-first path into root-first order and hands it to `be.hsm_chain_entry` — the
// SPELLING is the target's, so a target with no such table overrides nothing and this walk emits
// nothing for it.
//
// Regen: framec-ng -l rust --emit hsm_chain_walk.frs | grep -v '^#!\[allow' > hsm_chain_walk.gen.rs

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
mod _hsm_chain_walk_framec {
    use super::*;
    extern crate alloc;
    use alloc::{vec, format};
    #[derive(Clone)]
    #[allow(dead_code, non_camel_case_types)]
    enum HsmChainWalkFrameEvent {
        Step {  },
        FrameEnter {},
        FrameExit {},
    }

    #[derive(Clone)]
    #[allow(dead_code, non_camel_case_types)]
    enum HsmChainWalkFrameReturn {
        _Lifecycle(alloc::rc::Rc<dyn core::any::Any>),
    }

    #[allow(dead_code)]
    impl HsmChainWalkFrameEvent {
        fn name(&self) -> &'static str {
            match self {
                HsmChainWalkFrameEvent::Step { .. } => "step",
                HsmChainWalkFrameEvent::FrameEnter { .. } => "$>",
                HsmChainWalkFrameEvent::FrameExit { .. } => "<$",
            }
        }
    }

    #[derive(Clone, Debug)]
    #[allow(dead_code, non_camel_case_types)]
    enum HsmChainWalkFrameValue {
        Int(i64),
        Float(f64),
        Bool(bool),
        Str(String),
        List(Vec<Self>),
        Dict(alloc::collections::BTreeMap<String, Self>),
    }

    #[allow(dead_code, non_camel_case_types)]
    struct HsmChainWalkFrameContext {
        event: alloc::rc::Rc<HsmChainWalkFrameEvent>,
        _return: Option<HsmChainWalkFrameReturn>,
        _data: alloc::collections::BTreeMap<String, HsmChainWalkFrameValue>,
        _transitioned: bool,
    }

    impl HsmChainWalkFrameContext {
        fn new(event: alloc::rc::Rc<HsmChainWalkFrameEvent>, default_return: Option<HsmChainWalkFrameReturn>) -> Self {
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
    enum HsmChainWalkStateContext {
        State,
        Climb,
        Emit,
        Done,
        __NoContext,
    }

    impl Default for HsmChainWalkStateContext {
        fn default() -> Self {
            HsmChainWalkStateContext::State
        }
    }

    #[allow(dead_code, non_camel_case_types)]
    #[derive(Clone)]
    struct HsmChainWalkCompartment {
        state: String,
        state_context: HsmChainWalkStateContext,
        forward_event: Option<HsmChainWalkFrameEvent>,
        parent_compartment: Option<Box<HsmChainWalkCompartment>>,
    }

    impl HsmChainWalkCompartment {
        fn new(state: &str) -> Self {
            let state_context = match state {
                "State" => HsmChainWalkStateContext::State,
                "Climb" => HsmChainWalkStateContext::Climb,
                "Emit" => HsmChainWalkStateContext::Emit,
                "Done" => HsmChainWalkStateContext::Done,
                _ => HsmChainWalkStateContext::__NoContext,
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
    pub struct HsmChainWalk<'a> {
        _state_stack: Vec<HsmChainWalkCompartment>,
        __compartment: HsmChainWalkCompartment,
        __next_compartment: Option<HsmChainWalkCompartment>,
        _context_stack: Vec<HsmChainWalkFrameContext>,
        pub sym: &'a SystemSym,
        pub be: &'a dyn Backend,
        pub ns: usize,
        pub chain: ChainVec,
        pub out: Sink,
        pub si: usize,
        pub ci: usize,
        pub depth: usize,
    }

    #[allow(non_snake_case)]
    impl<'a> HsmChainWalk<'a> {
        pub fn new(sym: &'a SystemSym, be: &'a dyn Backend, ns: usize, chain: ChainVec, out: Sink) -> Self {
            Self {
                _state_stack: Vec::new(),
                _context_stack: Vec::new(),
                sym: sym,
                be: be,
                ns: ns,
                chain: chain,
                out: out,
                si: 0,
                ci: 0,
                depth: 0,
                __compartment: HsmChainWalkCompartment::new("State"),
                __next_compartment: None,
            }
        }

        pub fn __create(sym: &'a SystemSym, be: &'a dyn Backend, ns: usize, chain: ChainVec, out: Sink) -> Self {
            let mut c = Self::new(sym, be, ns, chain, out);
            c.__compartment = c.__prepareEnter("State");
            let __e = alloc::rc::Rc::new(HsmChainWalkFrameEvent::FrameEnter {});
            let __ctx = HsmChainWalkFrameContext::new(alloc::rc::Rc::clone(&__e), None);
            c._context_stack.push(__ctx);
            c.__kernel(&__e);
            c._context_stack.pop();
            c
        }

        fn __hsm_chain(&mut self, leaf: &str) -> &'static [&'static str] {
            match leaf {
                "State" => &["State"],
                "Climb" => &["Climb"],
                "Emit" => &["Emit"],
                "Done" => &["Done"],
                _ => &[],
            }
        }

        fn __prepareEnter(&mut self, leaf: &str) -> HsmChainWalkCompartment {
            let chain = self.__hsm_chain(leaf);
            let mut comp: Option<HsmChainWalkCompartment> = None;
            for name in chain.iter() {
                let mut new_comp = HsmChainWalkCompartment::new(name);
                if let Some(parent) = comp.take() {
                    new_comp.parent_compartment = Some(Box::new(parent));
                }
                comp = Some(new_comp);
            }
            comp.expect("chain must contain at least the leaf state")
        }

        fn __kernel(&mut self, __e: &alloc::rc::Rc<HsmChainWalkFrameEvent>) {
            // Route event to current state.
            self.__router(__e);
            // Drain any transitions queued by the handler.
            while self.__next_compartment.is_some() {
                let next_compartment = self.__next_compartment.take().expect("invariant: while-loop guard checked is_some()");
                // Exit the current (leaf) state. RFC-0025.1: exit args live in the
                // source state's typed ctx (written at the transition site), so the
                // synthesized `<$` event carries no payload.
                let exit_event = alloc::rc::Rc::new(HsmChainWalkFrameEvent::FrameExit {});
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
                        let enter_event = alloc::rc::Rc::new(HsmChainWalkFrameEvent::FrameEnter {});
                        self.__router(&enter_event);
                    }
                    Some(fwd) if matches!(fwd, HsmChainWalkFrameEvent::FrameEnter { .. }) => {
                        // Forwarded event IS $> — dispatch directly so the
                        // destination's $> handler receives the caller's payload.
                        let fwd_rc = alloc::rc::Rc::new(fwd);
                        self.__router(&fwd_rc);
                    }
                    Some(fwd) => {
                        // Forwarded event is not $> — initialize the destination
                        // with a fresh $>, then dispatch the forward.
                        let enter_event = alloc::rc::Rc::new(HsmChainWalkFrameEvent::FrameEnter {});
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

        fn __router(&mut self, __e: &alloc::rc::Rc<HsmChainWalkFrameEvent>) {
            let __ev: &HsmChainWalkFrameEvent = __e;
            match self.__compartment.state.as_str() {
                "State" => self._state_State(__ev),
                "Climb" => self._state_Climb(__ev),
                "Emit" => self._state_Emit(__ev),
                "Done" => self._state_Done(__ev),
                _ => {}
            }
        }

        fn __transition(&mut self, next_compartment: HsmChainWalkCompartment) {
            self.__next_compartment = Some(next_compartment);
        }

        pub fn step(&mut self) {
            let __e = alloc::rc::Rc::new(HsmChainWalkFrameEvent::Step {});
            let mut __ctx = HsmChainWalkFrameContext::new(alloc::rc::Rc::clone(&__e), None);
            self._context_stack.push(__ctx);
            self.__kernel(&__e);
            self._context_stack.pop();
        }

        fn _state_State(&mut self, __e: &HsmChainWalkFrameEvent) {
            match __e {
                HsmChainWalkFrameEvent::Step { .. } => { self._s_State_hdl_user_step(__e); }
                _ => {}
            }
        }

        fn _state_Climb(&mut self, __e: &HsmChainWalkFrameEvent) {
            match __e {
                HsmChainWalkFrameEvent::Step { .. } => { self._s_Climb_hdl_user_step(__e); }
                _ => {}
            }
        }

        fn _state_Emit(&mut self, __e: &HsmChainWalkFrameEvent) {
            match __e {
                HsmChainWalkFrameEvent::Step { .. } => { self._s_Emit_hdl_user_step(__e); }
                _ => {}
            }
        }

        fn _state_Done(&mut self, __e: &HsmChainWalkFrameEvent) {
            match __e {
                _ => {}
            }
        }

        fn _s_State_hdl_user_step(&mut self, __e: &HsmChainWalkFrameEvent) {
            if self.si >= self.ns {
                let mut __compartment = self.__prepareEnter("Done");
                self.__transition(__compartment);
                return;
            }
            clear_chain(&mut self.chain);
            self.ci = self.si;
            self.depth = 0;
            let mut __compartment = self.__prepareEnter("Climb");
            self.__transition(__compartment);
            return;
        }

        fn _s_Climb_hdl_user_step(&mut self, __e: &HsmChainWalkFrameEvent) {
            if self.depth > self.ns {
                let mut __compartment = self.__prepareEnter("Emit");
                self.__transition(__compartment);
                return;
            }
            push_state_name(self.sym, self.ci, &mut self.chain);
            self.depth = self.depth + 1;
            let p = parent_index(self.sym, self.ci);
            if p < 0 {
                let mut __compartment = self.__prepareEnter("Emit");
                self.__transition(__compartment);
                return;
            }
            self.ci = p as usize;
            let mut __compartment = self.__prepareEnter("Climb");
            self.__transition(__compartment);
            return;
        }

        fn _s_Emit_hdl_user_step(&mut self, __e: &HsmChainWalkFrameEvent) {
            stamp_chain(self.sym, self.be, self.si, &mut self.chain, &mut self.out);
            self.si = self.si + 1;
            let mut __compartment = self.__prepareEnter("State");
            self.__transition(__compartment);
            return;
        }
    }
}
pub use _hsm_chain_walk_framec::*;
