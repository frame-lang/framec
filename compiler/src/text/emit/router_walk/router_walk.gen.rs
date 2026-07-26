
// The generated runtime's STATE ROUTER walk, dogfooded as a plain `@@system` — the emit-side
// sequencer that produces one arm per state of "if the live compartment is in this state, hand the
// event to that state's dispatcher". It rides the same READ-ONLY BORROWED DOMAIN as the landed emit
// machines: the system symbol and the `&dyn Backend` are SHARED BORROWS threaded through one
// lifetime `'a`; the OWNED domain is the accumulating output `out`, the cursor `si` with its bound
// `ns`, and the `first` bit.
//
// A ONE-LEVEL CYCLE: `$Arm` stamps one state's arm per iteration and advances; at `si >= ns` it
// halts to `$Done`.
//
// THE ONE BIT WORTH NAMING — and why it is NOT a recognition register. `first` distinguishes the
// leading arm (`if`) from every later one (`elif` / `else if`). It is a WRITE-ONCE latch: true at
// entry, cleared by the first stamp, never read back to change which transition fires. It is
// carried here for one reason — so the SPELLING never has to re-derive "have I written an arm yet?"
// by looking at what it already wrote, which is precisely the emitted-text oracle RFC-0056 P6
// forbids. The old compiler's answer to this question was a `.is_empty()` on the output buffer; the
// answer here is a bool the walk owns. §3 degenerate pole otherwise: a program-counter cursor over
// the already-resolved symbol table, gated on nothing the input says.
//
// framec owns the WALK (the cursor, the bound, the latch, the halt). The un-Frame-able work is the
// single per-item NATIVE LEAF `stamp_router_arm`, which hands `(state, first)` to
// `be.router_arm` — the SPELLING is the target's, so a target whose dispatch is direct (Java, Rust,
// C) overrides nothing and this walk emits nothing for it.
//
// Regen: framec-ng -l rust --emit router_walk.frs | grep -v '^#!\[allow' > router_walk.gen.rs

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
mod _router_walk_framec {
    use super::*;
    extern crate alloc;
    use alloc::{vec, format};
    #[derive(Clone)]
    #[allow(dead_code, non_camel_case_types)]
    enum RouterWalkFrameEvent {
        Step {  },
        FrameEnter {},
        FrameExit {},
    }

    #[derive(Clone)]
    #[allow(dead_code, non_camel_case_types)]
    enum RouterWalkFrameReturn {
        _Lifecycle(alloc::rc::Rc<dyn core::any::Any>),
    }

    #[allow(dead_code)]
    impl RouterWalkFrameEvent {
        fn name(&self) -> &'static str {
            match self {
                RouterWalkFrameEvent::Step { .. } => "step",
                RouterWalkFrameEvent::FrameEnter { .. } => "$>",
                RouterWalkFrameEvent::FrameExit { .. } => "<$",
            }
        }
    }

    #[derive(Clone, Debug)]
    #[allow(dead_code, non_camel_case_types)]
    enum RouterWalkFrameValue {
        Int(i64),
        Float(f64),
        Bool(bool),
        Str(String),
        List(Vec<Self>),
        Dict(alloc::collections::BTreeMap<String, Self>),
    }

    #[allow(dead_code, non_camel_case_types)]
    struct RouterWalkFrameContext {
        event: alloc::rc::Rc<RouterWalkFrameEvent>,
        _return: Option<RouterWalkFrameReturn>,
        _data: alloc::collections::BTreeMap<String, RouterWalkFrameValue>,
        _transitioned: bool,
    }

    impl RouterWalkFrameContext {
        fn new(event: alloc::rc::Rc<RouterWalkFrameEvent>, default_return: Option<RouterWalkFrameReturn>) -> Self {
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
    enum RouterWalkStateContext {
        Arm,
        Done,
        __NoContext,
    }

    impl Default for RouterWalkStateContext {
        fn default() -> Self {
            RouterWalkStateContext::Arm
        }
    }

    #[allow(dead_code, non_camel_case_types)]
    #[derive(Clone)]
    struct RouterWalkCompartment {
        state: String,
        state_context: RouterWalkStateContext,
        forward_event: Option<RouterWalkFrameEvent>,
        parent_compartment: Option<Box<RouterWalkCompartment>>,
    }

    impl RouterWalkCompartment {
        fn new(state: &str) -> Self {
            let state_context = match state {
                "Arm" => RouterWalkStateContext::Arm,
                "Done" => RouterWalkStateContext::Done,
                _ => RouterWalkStateContext::__NoContext,
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
    pub struct RouterWalk<'a> {
        _state_stack: Vec<RouterWalkCompartment>,
        __compartment: RouterWalkCompartment,
        __next_compartment: Option<RouterWalkCompartment>,
        _context_stack: Vec<RouterWalkFrameContext>,
        pub sym: &'a SystemSym,
        pub be: &'a dyn Backend,
        pub ns: usize,
        pub out: Sink,
        pub si: usize,
        pub first: bool,
    }

    #[allow(non_snake_case)]
    impl<'a> RouterWalk<'a> {
        pub fn new(sym: &'a SystemSym, be: &'a dyn Backend, ns: usize, out: Sink) -> Self {
            Self {
                _state_stack: Vec::new(),
                _context_stack: Vec::new(),
                sym: sym,
                be: be,
                ns: ns,
                out: out,
                si: 0,
                first: true,
                __compartment: RouterWalkCompartment::new("Arm"),
                __next_compartment: None,
            }
        }

        pub fn __create(sym: &'a SystemSym, be: &'a dyn Backend, ns: usize, out: Sink) -> Self {
            let mut c = Self::new(sym, be, ns, out);
            c.__compartment = c.__prepareEnter("Arm");
            let __e = alloc::rc::Rc::new(RouterWalkFrameEvent::FrameEnter {});
            let __ctx = RouterWalkFrameContext::new(alloc::rc::Rc::clone(&__e), None);
            c._context_stack.push(__ctx);
            c.__kernel(&__e);
            c._context_stack.pop();
            c
        }

        fn __hsm_chain(&mut self, leaf: &str) -> &'static [&'static str] {
            match leaf {
                "Arm" => &["Arm"],
                "Done" => &["Done"],
                _ => &[],
            }
        }

        fn __prepareEnter(&mut self, leaf: &str) -> RouterWalkCompartment {
            let chain = self.__hsm_chain(leaf);
            let mut comp: Option<RouterWalkCompartment> = None;
            for name in chain.iter() {
                let mut new_comp = RouterWalkCompartment::new(name);
                if let Some(parent) = comp.take() {
                    new_comp.parent_compartment = Some(Box::new(parent));
                }
                comp = Some(new_comp);
            }
            comp.expect("chain must contain at least the leaf state")
        }

        fn __kernel(&mut self, __e: &alloc::rc::Rc<RouterWalkFrameEvent>) {
            // Route event to current state.
            self.__router(__e);
            // Drain any transitions queued by the handler.
            while self.__next_compartment.is_some() {
                let next_compartment = self.__next_compartment.take().expect("invariant: while-loop guard checked is_some()");
                // Exit the current (leaf) state. RFC-0025.1: exit args live in the
                // source state's typed ctx (written at the transition site), so the
                // synthesized `<$` event carries no payload.
                let exit_event = alloc::rc::Rc::new(RouterWalkFrameEvent::FrameExit {});
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
                        let enter_event = alloc::rc::Rc::new(RouterWalkFrameEvent::FrameEnter {});
                        self.__router(&enter_event);
                    }
                    Some(fwd) if matches!(fwd, RouterWalkFrameEvent::FrameEnter { .. }) => {
                        // Forwarded event IS $> — dispatch directly so the
                        // destination's $> handler receives the caller's payload.
                        let fwd_rc = alloc::rc::Rc::new(fwd);
                        self.__router(&fwd_rc);
                    }
                    Some(fwd) => {
                        // Forwarded event is not $> — initialize the destination
                        // with a fresh $>, then dispatch the forward.
                        let enter_event = alloc::rc::Rc::new(RouterWalkFrameEvent::FrameEnter {});
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

        fn __router(&mut self, __e: &alloc::rc::Rc<RouterWalkFrameEvent>) {
            let __ev: &RouterWalkFrameEvent = __e;
            match self.__compartment.state.as_str() {
                "Arm" => self._state_Arm(__ev),
                "Done" => self._state_Done(__ev),
                _ => {}
            }
        }

        fn __transition(&mut self, next_compartment: RouterWalkCompartment) {
            self.__next_compartment = Some(next_compartment);
        }

        pub fn step(&mut self) {
            let __e = alloc::rc::Rc::new(RouterWalkFrameEvent::Step {});
            let mut __ctx = RouterWalkFrameContext::new(alloc::rc::Rc::clone(&__e), None);
            self._context_stack.push(__ctx);
            self.__kernel(&__e);
            self._context_stack.pop();
        }

        fn _state_Arm(&mut self, __e: &RouterWalkFrameEvent) {
            match __e {
                RouterWalkFrameEvent::Step { .. } => { self._s_Arm_hdl_user_step(__e); }
                _ => {}
            }
        }

        fn _state_Done(&mut self, __e: &RouterWalkFrameEvent) {
            match __e {
                _ => {}
            }
        }

        fn _s_Arm_hdl_user_step(&mut self, __e: &RouterWalkFrameEvent) {
            if self.si >= self.ns {
                let mut __compartment = self.__prepareEnter("Done");
                self.__transition(__compartment);
                return;
            }
            stamp_router_arm(self.sym, self.be, self.si, self.first, &mut self.out);
            self.first = false;
            self.si = self.si + 1;
            let mut __compartment = self.__prepareEnter("Arm");
            self.__transition(__compartment);
            return;
        }
    }
}
pub use _router_walk_framec::*;
