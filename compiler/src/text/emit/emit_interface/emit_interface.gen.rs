
// The driver's INTERFACE/ROUTER walk, dogfooded as a plain `@@system` — the emit-side sequencer
// that reifies `emit`'s per-event router pass (the PUBLIC method per interface event that dispatches
// to the private handler methods). It rides the same READ-ONLY BORROWED DOMAIN as
// StmtWalk/BaseColumn/EmitHandlers: the system symbol and the `&dyn Backend` are SHARED BORROWS
// threaded through one lifetime `'a`; the OWNED domain is the accumulating output `out`, the two
// walk cursors (`mi`/`ai`) and their bounds (`ni`/`na`), plus the per-method arm accumulator `arms`.
//
// THE 2-LEVEL NESTING, EXPRESSED WITHOUT push$/pop$. The pass is a FIXED depth-2 walk — interface
// methods, then, per method, the machine's states (to resolve which state's handler runs) — so a
// stack is unnecessary (a stack buys UNBOUNDED depth; this depth is 2 and known). It is expressed
// instead as two NESTED CYCLE STATES with explicit up/down edges, one owned cursor per level:
//   $Method  cycles over `sym.interface` (`ni` methods); on a method it sets the arm bound `na`
//            (= state count), resets `ai`, CLEARS the arm accumulator, and descends `-> $Arm`; at
//            `mi >= ni` it halts `-> $Done`.
//   $Arm     cycles over `sym.states` (`na` states) STAMPING one `(state, owner)` arm per state
//            for which `resolve_handler(state, method)` is `Some` (HSM dispatch, resolved from the
//            symbol table); at `ai >= na` it computes the method's `is_async` and ROUTES — emits the
//            one public method via `be.route(...)` — then ASCENDS (`mi += 1`, `-> $Method`).
// The "mode" is the walk DEPTH (which of the two cycle states is live); the cursors advance it.
// This is the §3 degenerate pole — a program-counter walk over the ALREADY-RESOLVED symbol table,
// whose only fork is a structural table lookup (`resolve_handler` Some/None), not input recognition.
// `arms` is not a recognition register — it gates no transition; it is MATERIALIZATION being built,
// like `out`. `is_async` is a write-once/read-once local, not a carried mode. So this carries no
// recognition register; nothing is glossed. Its reify payoff is not a hidden mode but DOGFOOD
// UNIFORMITY (the maximal-rebuild campaign: the cleanroom emits its own driver as an @@system,
// differential-gated byte-for-byte vs the preserved `emit_interface_hand`).
//
// framec owns the WALK (the two cursors, the bounds, the descents/ascents, the halt, the per-method
// arm-accumulator reset). The un-Frame-able work is per-item NATIVE LEAVES: `state_count` (the arm
// bound), `stamp_arm` (the `resolve_handler` lookup + arm push — Frame cannot walk a symbol table),
// `clear_arms` (reset the accumulator per method), `method_is_async` (the `m.is_async || sym.is_async`
// disjunction), and `route_method`, which spells ONE public method: the verbatim `be.route(...)` the
// hand pass ran. Every materialization spelling stays native and byte-identical; the machine only
// sequences the walk.
//
// Regen: framec-ng -l rust --emit emit_interface.frs | grep -v '^#!\[allow' > emit_interface.gen.rs

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
mod _emit_interface_framec {
    use super::*;
    extern crate alloc;
    use alloc::{vec, format};
    #[derive(Clone)]
    #[allow(dead_code, non_camel_case_types)]
    enum EmitInterfaceFrameEvent {
        Step {  },
        FrameEnter {},
        FrameExit {},
    }

    #[derive(Clone)]
    #[allow(dead_code, non_camel_case_types)]
    enum EmitInterfaceFrameReturn {
        _Lifecycle(alloc::rc::Rc<dyn core::any::Any>),
    }

    #[allow(dead_code)]
    impl EmitInterfaceFrameEvent {
        fn name(&self) -> &'static str {
            match self {
                EmitInterfaceFrameEvent::Step { .. } => "step",
                EmitInterfaceFrameEvent::FrameEnter { .. } => "$>",
                EmitInterfaceFrameEvent::FrameExit { .. } => "<$",
            }
        }
    }

    #[derive(Clone, Debug)]
    #[allow(dead_code, non_camel_case_types)]
    enum EmitInterfaceFrameValue {
        Int(i64),
        Float(f64),
        Bool(bool),
        Str(String),
        List(Vec<Self>),
        Dict(alloc::collections::BTreeMap<String, Self>),
    }

    #[allow(dead_code, non_camel_case_types)]
    struct EmitInterfaceFrameContext {
        event: alloc::rc::Rc<EmitInterfaceFrameEvent>,
        _return: Option<EmitInterfaceFrameReturn>,
        _data: alloc::collections::BTreeMap<String, EmitInterfaceFrameValue>,
        _transitioned: bool,
    }

    impl EmitInterfaceFrameContext {
        fn new(event: alloc::rc::Rc<EmitInterfaceFrameEvent>, default_return: Option<EmitInterfaceFrameReturn>) -> Self {
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
    enum EmitInterfaceStateContext {
        Method,
        Arm,
        Done,
        __NoContext,
    }

    impl Default for EmitInterfaceStateContext {
        fn default() -> Self {
            EmitInterfaceStateContext::Method
        }
    }

    #[allow(dead_code, non_camel_case_types)]
    #[derive(Clone)]
    struct EmitInterfaceCompartment {
        state: String,
        state_context: EmitInterfaceStateContext,
        forward_event: Option<EmitInterfaceFrameEvent>,
        parent_compartment: Option<Box<EmitInterfaceCompartment>>,
    }

    impl EmitInterfaceCompartment {
        fn new(state: &str) -> Self {
            let state_context = match state {
                "Method" => EmitInterfaceStateContext::Method,
                "Arm" => EmitInterfaceStateContext::Arm,
                "Done" => EmitInterfaceStateContext::Done,
                _ => EmitInterfaceStateContext::__NoContext,
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
    pub struct EmitInterface<'a> {
        _state_stack: Vec<EmitInterfaceCompartment>,
        __compartment: EmitInterfaceCompartment,
        __next_compartment: Option<EmitInterfaceCompartment>,
        _context_stack: Vec<EmitInterfaceFrameContext>,
        pub sym: &'a SystemSym,
        pub be: &'a dyn Backend,
        pub ni: usize,
        pub arms: ArmVec,
        pub out: Sink,
        pub na: usize,
        pub mi: usize,
        pub ai: usize,
    }

    #[allow(non_snake_case)]
    impl<'a> EmitInterface<'a> {
        pub fn new(sym: &'a SystemSym, be: &'a dyn Backend, ni: usize, arms: ArmVec, out: Sink) -> Self {
            Self {
                _state_stack: Vec::new(),
                _context_stack: Vec::new(),
                sym: sym,
                be: be,
                ni: ni,
                arms: arms,
                out: out,
                na: 0,
                mi: 0,
                ai: 0,
                __compartment: EmitInterfaceCompartment::new("Method"),
                __next_compartment: None,
            }
        }

        pub fn __create(sym: &'a SystemSym, be: &'a dyn Backend, ni: usize, arms: ArmVec, out: Sink) -> Self {
            let mut c = Self::new(sym, be, ni, arms, out);
            c.__compartment = c.__prepareEnter("Method");
            let __e = alloc::rc::Rc::new(EmitInterfaceFrameEvent::FrameEnter {});
            let __ctx = EmitInterfaceFrameContext::new(alloc::rc::Rc::clone(&__e), None);
            c._context_stack.push(__ctx);
            c.__kernel(&__e);
            c._context_stack.pop();
            c
        }

        fn __hsm_chain(&mut self, leaf: &str) -> &'static [&'static str] {
            match leaf {
                "Method" => &["Method"],
                "Arm" => &["Arm"],
                "Done" => &["Done"],
                _ => &[],
            }
        }

        fn __prepareEnter(&mut self, leaf: &str) -> EmitInterfaceCompartment {
            let chain = self.__hsm_chain(leaf);
            let mut comp: Option<EmitInterfaceCompartment> = None;
            for name in chain.iter() {
                let mut new_comp = EmitInterfaceCompartment::new(name);
                if let Some(parent) = comp.take() {
                    new_comp.parent_compartment = Some(Box::new(parent));
                }
                comp = Some(new_comp);
            }
            comp.expect("chain must contain at least the leaf state")
        }

        fn __kernel(&mut self, __e: &alloc::rc::Rc<EmitInterfaceFrameEvent>) {
            // Route event to current state.
            self.__router(__e);
            // Drain any transitions queued by the handler.
            while self.__next_compartment.is_some() {
                let next_compartment = self.__next_compartment.take().expect("invariant: while-loop guard checked is_some()");
                // Exit the current (leaf) state. RFC-0025.1: exit args live in the
                // source state's typed ctx (written at the transition site), so the
                // synthesized `<$` event carries no payload.
                let exit_event = alloc::rc::Rc::new(EmitInterfaceFrameEvent::FrameExit {});
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
                        let enter_event = alloc::rc::Rc::new(EmitInterfaceFrameEvent::FrameEnter {});
                        self.__router(&enter_event);
                    }
                    Some(fwd) if matches!(fwd, EmitInterfaceFrameEvent::FrameEnter { .. }) => {
                        // Forwarded event IS $> — dispatch directly so the
                        // destination's $> handler receives the caller's payload.
                        let fwd_rc = alloc::rc::Rc::new(fwd);
                        self.__router(&fwd_rc);
                    }
                    Some(fwd) => {
                        // Forwarded event is not $> — initialize the destination
                        // with a fresh $>, then dispatch the forward.
                        let enter_event = alloc::rc::Rc::new(EmitInterfaceFrameEvent::FrameEnter {});
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

        fn __router(&mut self, __e: &alloc::rc::Rc<EmitInterfaceFrameEvent>) {
            let __ev: &EmitInterfaceFrameEvent = __e;
            match self.__compartment.state.as_str() {
                "Method" => self._state_Method(__ev),
                "Arm" => self._state_Arm(__ev),
                "Done" => self._state_Done(__ev),
                _ => {}
            }
        }

        fn __transition(&mut self, next_compartment: EmitInterfaceCompartment) {
            self.__next_compartment = Some(next_compartment);
        }

        pub fn step(&mut self) {
            let __e = alloc::rc::Rc::new(EmitInterfaceFrameEvent::Step {});
            let mut __ctx = EmitInterfaceFrameContext::new(alloc::rc::Rc::clone(&__e), None);
            self._context_stack.push(__ctx);
            self.__kernel(&__e);
            self._context_stack.pop();
        }

        fn _state_Method(&mut self, __e: &EmitInterfaceFrameEvent) {
            match __e {
                EmitInterfaceFrameEvent::Step { .. } => { self._s_Method_hdl_user_step(__e); }
                _ => {}
            }
        }

        fn _state_Arm(&mut self, __e: &EmitInterfaceFrameEvent) {
            match __e {
                EmitInterfaceFrameEvent::Step { .. } => { self._s_Arm_hdl_user_step(__e); }
                _ => {}
            }
        }

        fn _state_Done(&mut self, __e: &EmitInterfaceFrameEvent) {
            match __e {
                _ => {}
            }
        }

        fn _s_Method_hdl_user_step(&mut self, __e: &EmitInterfaceFrameEvent) {
            if self.mi >= self.ni {
                let mut __compartment = self.__prepareEnter("Done");
                self.__transition(__compartment);
                return;
            }
            self.na = state_count(self.sym);
            self.ai = 0;
            clear_arms(&mut self.arms);
            let mut __compartment = self.__prepareEnter("Arm");
            self.__transition(__compartment);
            return;
        }

        fn _s_Arm_hdl_user_step(&mut self, __e: &EmitInterfaceFrameEvent) {
            if self.ai >= self.na {
                let is_async = method_is_async(self.sym, self.mi);
                route_method(self.sym, self.be, self.mi, &self.arms, is_async, &mut self.out);
                self.mi = self.mi + 1;
                let mut __compartment = self.__prepareEnter("Method");
                self.__transition(__compartment);
                return;
            }
            stamp_arm(self.sym, self.mi, self.ai, &mut self.arms);
            self.ai = self.ai + 1;
            let mut __compartment = self.__prepareEnter("Arm");
            self.__transition(__compartment);
            return;
        }
    }
}
pub use _emit_interface_framec::*;
