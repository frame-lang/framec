
// The generated runtime's PER-STATE MESSAGE-DISPATCH walk, dogfooded as a plain `@@system` — the
// emit-side sequencer that produces, for every state, the private method the router hands an event
// to, which matches the event's message against the handlers that state declares. It rides the same
// READ-ONLY BORROWED DOMAIN as the landed emit machines: the system symbol and the `&dyn Backend`
// are SHARED BORROWS threaded through one lifetime `'a`; the OWNED domain is the accumulating
// output `out`, the two walk cursors (`si`/`hi`) and their bounds (`ns`/`nh`), plus the per-state
// arm accumulator `arms`.
//
// THE 2-LEVEL NESTING, EXPRESSED WITHOUT push$/pop$ (the EmitInterface shape). The pass is a FIXED
// depth-2 walk — the machine's states, then, per state, that state's declared handlers — so a stack
// is unnecessary (a stack buys UNBOUNDED depth; this depth is 2 and known). Two nested CYCLE STATES
// with explicit up/down edges, one owned cursor per level:
//   $State   cycles over `sym.states` (`ns` states); on a state it sets the handler bound `nh`,
//            resets `hi`, CLEARS the arm accumulator, and descends `-> $Handler`; at `si >= ns` it
//            halts `-> $Done`.
//   $Handler cycles over that state's handlers (`nh`), STAMPING one event message per handler; at
//            `hi >= nh` it DISPATCHES — asks the backend to spell the one method from the stamped
//            arms — then ASCENDS (`si += 1`, `-> $State`).
//
// THE HONEST MACHINE CLASS. §3 degenerate pole: a program-counter walk over the ALREADY-RESOLVED
// symbol table. `arms` is not a recognition register — it gates no transition; it is MATERIALIZATION
// being built, exactly like `out` (the ENGINE that decided which handlers a state has is the
// resolver, upstream and already shipped; this walk only reads that frozen decision). Nothing is
// glossed. The payoff claimed is DOGFOOD UNIFORMITY, differential-gated byte-for-byte against the
// preserved `state_dispatch_hand`.
//
// framec owns the WALK (both cursors, both bounds, the per-state accumulator reset, the
// descents/ascents, the halt). The un-Frame-able work is per-item NATIVE LEAVES: `handler_count`
// (the inner bound), `clear_arms` (the reset), `stamp_handler` (the symbol-table read Frame cannot
// do), and `dispatch_state`, which hands `(state, arms)` to `be.dispatch` — the SPELLING is the
// target's, so a target whose router calls `(state, event)` methods directly (Java, Rust, C)
// overrides nothing and this walk emits nothing for it.
//
// THE ARM ORDER IS THE WALK'S, NOT A BACKEND'S. The shipped compiler emits dispatch arms in
// handler-KEY order (exit, enter, then user events alphabetically) in EVERY target — measured
// against the 4.6.1 oracle for python_3, java, rust and c on one source. That is a decision, so it
// rides `stamp_handler`'s slot projection (`handler_slot`, the twin of EmitHandlers' `member_slot`)
// and `be` is threaded in for it. It used to live inside Python's `dispatch` spelling, where a
// backend re-sorted a list the walk had already built — the wrong layer, and three more copies
// waiting to be written.
//
// Regen: framec-ng -l rust --emit state_dispatch_walk.frs | grep -v '^#!\[allow' > state_dispatch_walk.gen.rs

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
mod _state_dispatch_walk_framec {
    use super::*;
    extern crate alloc;
    use alloc::{vec, format};
    #[derive(Clone)]
    #[allow(dead_code, non_camel_case_types)]
    enum StateDispatchWalkFrameEvent {
        Step {  },
        FrameEnter {},
        FrameExit {},
    }

    #[derive(Clone)]
    #[allow(dead_code, non_camel_case_types)]
    enum StateDispatchWalkFrameReturn {
        _Lifecycle(alloc::rc::Rc<dyn core::any::Any>),
    }

    #[allow(dead_code)]
    impl StateDispatchWalkFrameEvent {
        fn name(&self) -> &'static str {
            match self {
                StateDispatchWalkFrameEvent::Step { .. } => "step",
                StateDispatchWalkFrameEvent::FrameEnter { .. } => "$>",
                StateDispatchWalkFrameEvent::FrameExit { .. } => "<$",
            }
        }
    }

    #[derive(Clone, Debug)]
    #[allow(dead_code, non_camel_case_types)]
    enum StateDispatchWalkFrameValue {
        Int(i64),
        Float(f64),
        Bool(bool),
        Str(String),
        List(Vec<Self>),
        Dict(alloc::collections::BTreeMap<String, Self>),
    }

    #[allow(dead_code, non_camel_case_types)]
    struct StateDispatchWalkFrameContext {
        event: alloc::rc::Rc<StateDispatchWalkFrameEvent>,
        _return: Option<StateDispatchWalkFrameReturn>,
        _data: alloc::collections::BTreeMap<String, StateDispatchWalkFrameValue>,
        _transitioned: bool,
    }

    impl StateDispatchWalkFrameContext {
        fn new(event: alloc::rc::Rc<StateDispatchWalkFrameEvent>, default_return: Option<StateDispatchWalkFrameReturn>) -> Self {
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
    enum StateDispatchWalkStateContext {
        State,
        Handler,
        Done,
        __NoContext,
    }

    impl Default for StateDispatchWalkStateContext {
        fn default() -> Self {
            StateDispatchWalkStateContext::State
        }
    }

    #[allow(dead_code, non_camel_case_types)]
    #[derive(Clone)]
    struct StateDispatchWalkCompartment {
        state: String,
        state_context: StateDispatchWalkStateContext,
        forward_event: Option<StateDispatchWalkFrameEvent>,
        parent_compartment: Option<Box<StateDispatchWalkCompartment>>,
    }

    impl StateDispatchWalkCompartment {
        fn new(state: &str) -> Self {
            let state_context = match state {
                "State" => StateDispatchWalkStateContext::State,
                "Handler" => StateDispatchWalkStateContext::Handler,
                "Done" => StateDispatchWalkStateContext::Done,
                _ => StateDispatchWalkStateContext::__NoContext,
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
    pub struct StateDispatchWalk<'a> {
        _state_stack: Vec<StateDispatchWalkCompartment>,
        __compartment: StateDispatchWalkCompartment,
        __next_compartment: Option<StateDispatchWalkCompartment>,
        _context_stack: Vec<StateDispatchWalkFrameContext>,
        pub sym: &'a SystemSym,
        pub be: &'a dyn Backend,
        pub ns: usize,
        pub arms: EventVec,
        pub out: Sink,
        pub nh: usize,
        pub si: usize,
        pub hi: usize,
    }

    #[allow(non_snake_case)]
    impl<'a> StateDispatchWalk<'a> {
        pub fn new(sym: &'a SystemSym, be: &'a dyn Backend, ns: usize, arms: EventVec, out: Sink) -> Self {
            Self {
                _state_stack: Vec::new(),
                _context_stack: Vec::new(),
                sym: sym,
                be: be,
                ns: ns,
                arms: arms,
                out: out,
                nh: 0,
                si: 0,
                hi: 0,
                __compartment: StateDispatchWalkCompartment::new("State"),
                __next_compartment: None,
            }
        }

        pub fn __create(sym: &'a SystemSym, be: &'a dyn Backend, ns: usize, arms: EventVec, out: Sink) -> Self {
            let mut c = Self::new(sym, be, ns, arms, out);
            c.__compartment = c.__prepareEnter("State");
            let __e = alloc::rc::Rc::new(StateDispatchWalkFrameEvent::FrameEnter {});
            let __ctx = StateDispatchWalkFrameContext::new(alloc::rc::Rc::clone(&__e), None);
            c._context_stack.push(__ctx);
            c.__kernel(&__e);
            c._context_stack.pop();
            c
        }

        fn __hsm_chain(&mut self, leaf: &str) -> &'static [&'static str] {
            match leaf {
                "State" => &["State"],
                "Handler" => &["Handler"],
                "Done" => &["Done"],
                _ => &[],
            }
        }

        fn __prepareEnter(&mut self, leaf: &str) -> StateDispatchWalkCompartment {
            let chain = self.__hsm_chain(leaf);
            let mut comp: Option<StateDispatchWalkCompartment> = None;
            for name in chain.iter() {
                let mut new_comp = StateDispatchWalkCompartment::new(name);
                if let Some(parent) = comp.take() {
                    new_comp.parent_compartment = Some(Box::new(parent));
                }
                comp = Some(new_comp);
            }
            comp.expect("chain must contain at least the leaf state")
        }

        fn __kernel(&mut self, __e: &alloc::rc::Rc<StateDispatchWalkFrameEvent>) {
            // Route event to current state.
            self.__router(__e);
            // Drain any transitions queued by the handler.
            while self.__next_compartment.is_some() {
                let next_compartment = self.__next_compartment.take().expect("invariant: while-loop guard checked is_some()");
                // Exit the current (leaf) state. RFC-0025.1: exit args live in the
                // source state's typed ctx (written at the transition site), so the
                // synthesized `<$` event carries no payload.
                let exit_event = alloc::rc::Rc::new(StateDispatchWalkFrameEvent::FrameExit {});
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
                        let enter_event = alloc::rc::Rc::new(StateDispatchWalkFrameEvent::FrameEnter {});
                        self.__router(&enter_event);
                    }
                    Some(fwd) if matches!(fwd, StateDispatchWalkFrameEvent::FrameEnter { .. }) => {
                        // Forwarded event IS $> — dispatch directly so the
                        // destination's $> handler receives the caller's payload.
                        let fwd_rc = alloc::rc::Rc::new(fwd);
                        self.__router(&fwd_rc);
                    }
                    Some(fwd) => {
                        // Forwarded event is not $> — initialize the destination
                        // with a fresh $>, then dispatch the forward.
                        let enter_event = alloc::rc::Rc::new(StateDispatchWalkFrameEvent::FrameEnter {});
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

        fn __router(&mut self, __e: &alloc::rc::Rc<StateDispatchWalkFrameEvent>) {
            let __ev: &StateDispatchWalkFrameEvent = __e;
            match self.__compartment.state.as_str() {
                "State" => self._state_State(__ev),
                "Handler" => self._state_Handler(__ev),
                "Done" => self._state_Done(__ev),
                _ => {}
            }
        }

        fn __transition(&mut self, next_compartment: StateDispatchWalkCompartment) {
            self.__next_compartment = Some(next_compartment);
        }

        pub fn step(&mut self) {
            let __e = alloc::rc::Rc::new(StateDispatchWalkFrameEvent::Step {});
            let mut __ctx = StateDispatchWalkFrameContext::new(alloc::rc::Rc::clone(&__e), None);
            self._context_stack.push(__ctx);
            self.__kernel(&__e);
            self._context_stack.pop();
        }

        fn _state_State(&mut self, __e: &StateDispatchWalkFrameEvent) {
            match __e {
                StateDispatchWalkFrameEvent::Step { .. } => { self._s_State_hdl_user_step(__e); }
                _ => {}
            }
        }

        fn _state_Handler(&mut self, __e: &StateDispatchWalkFrameEvent) {
            match __e {
                StateDispatchWalkFrameEvent::Step { .. } => { self._s_Handler_hdl_user_step(__e); }
                _ => {}
            }
        }

        fn _state_Done(&mut self, __e: &StateDispatchWalkFrameEvent) {
            match __e {
                _ => {}
            }
        }

        fn _s_State_hdl_user_step(&mut self, __e: &StateDispatchWalkFrameEvent) {
            if self.si >= self.ns {
                let mut __compartment = self.__prepareEnter("Done");
                self.__transition(__compartment);
                return;
            }
            self.nh = handler_count(self.sym, self.si);
            self.hi = 0;
            clear_arms(&mut self.arms);
            let mut __compartment = self.__prepareEnter("Handler");
            self.__transition(__compartment);
            return;
        }

        fn _s_Handler_hdl_user_step(&mut self, __e: &StateDispatchWalkFrameEvent) {
            if self.hi >= self.nh {
                dispatch_state(self.sym, self.be, self.si, &self.arms, &mut self.out);
                self.si = self.si + 1;
                let mut __compartment = self.__prepareEnter("State");
                self.__transition(__compartment);
                return;
            }
            stamp_handler(self.sym, self.be, self.si, self.hi, &mut self.arms);
            self.hi = self.hi + 1;
            let mut __compartment = self.__prepareEnter("Handler");
            self.__transition(__compartment);
            return;
        }
    }
}
pub use _state_dispatch_walk_framec::*;
