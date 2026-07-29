
// The RUST per-state message-dispatcher walk — the pilot-style twin of the shared `DispatchBody`.
// Rust's `_state_<S>` dispatcher is a `match` over a typed `<Sys>FrameEvent` enum, NOT an `if`-chain
// over a message string, so it does not (and cannot) share `DispatchBody` with Python/Java/C: the
// per-arm spelling is a variant pattern with an enter-time parent-chain ctx-climb, not a `Backend`
// seam any `if`-chain target could reuse. So rust gets its OWN system, driven from rust.rs's
// `Backend::dispatch`, over rust-only leaves (`rust_dispatch_open` / `_arm` / `_close`) — NOT the
// four `Backend` trait `dispatch_*` methods.
//
// THE WALK: emit the method header + `match __e {` (open), emit one arm per event message (arms),
// then the `_ => {}` default + the two closing braces (close). The per-arm bytes — the `$>` enter
// (climb the parent chain to the owning state, read each enter param off its typed ctx, pass
// positionally), the `<$` exit, and a user event's variant destructure — all live in the arm leaf;
// this walk only SEQUENCES them.
//
// THE HONEST MACHINE CLASS. §3 degenerate pole: a program-counter walk over data already decided
// upstream (`arms` pre-ordered by `StateDispatchWalk`; state read from the resolved symbol table).
// The one cursor (`ai`) is a pure program counter. Differential-gated byte-for-byte against the
// preserved `rust_dispatch_hand`. It rides the same read-only borrowed domain as the landed emit
// machines (`sym: &SystemSym`).
//
// The scan-guard (`if sym.scan.is_some() { return; }`) lives in the driver (`mod.rs`), not here: a
// scanner system dispatches directly in `route` and emits no `_state_<S>`, so the driver returns
// before ever building this machine.
//
// Regen: framec-ng -l rust --emit rust_dispatch.frs | grep -v '^#!\[allow' > rust_dispatch.gen.rs

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
mod _rust_dispatch_framec {
    use super::*;
    extern crate alloc;
    use alloc::{vec, format};
    #[derive(Clone)]
    #[allow(dead_code, non_camel_case_types)]
    enum RustDispatchFrameEvent {
        Step {  },
        FrameEnter {},
        FrameExit {},
    }

    #[derive(Clone)]
    #[allow(dead_code, non_camel_case_types)]
    enum RustDispatchFrameReturn {
        _Lifecycle(alloc::rc::Rc<dyn core::any::Any>),
    }

    #[allow(dead_code)]
    impl RustDispatchFrameEvent {
        fn name(&self) -> &'static str {
            match self {
                RustDispatchFrameEvent::Step { .. } => "step",
                RustDispatchFrameEvent::FrameEnter { .. } => "$>",
                RustDispatchFrameEvent::FrameExit { .. } => "<$",
            }
        }
    }

    #[derive(Clone, Debug)]
    #[allow(dead_code, non_camel_case_types)]
    enum RustDispatchFrameValue {
        Int(i64),
        Float(f64),
        Bool(bool),
        Str(String),
        List(Vec<Self>),
        Dict(alloc::collections::BTreeMap<String, Self>),
    }

    #[allow(dead_code, non_camel_case_types)]
    struct RustDispatchFrameContext {
        event: alloc::rc::Rc<RustDispatchFrameEvent>,
        _return: Option<RustDispatchFrameReturn>,
        _data: alloc::collections::BTreeMap<String, RustDispatchFrameValue>,
        _transitioned: bool,
    }

    impl RustDispatchFrameContext {
        fn new(event: alloc::rc::Rc<RustDispatchFrameEvent>, default_return: Option<RustDispatchFrameReturn>) -> Self {
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
    enum RustDispatchStateContext {
        Header,
        Arms,
        Close,
        Done,
        __NoContext,
    }

    impl Default for RustDispatchStateContext {
        fn default() -> Self {
            RustDispatchStateContext::Header
        }
    }

    #[allow(dead_code, non_camel_case_types)]
    #[derive(Clone)]
    struct RustDispatchCompartment {
        state: String,
        state_context: RustDispatchStateContext,
        forward_event: Option<RustDispatchFrameEvent>,
        parent_compartment: Option<Box<RustDispatchCompartment>>,
    }

    impl RustDispatchCompartment {
        fn new(state: &str) -> Self {
            let state_context = match state {
                "Header" => RustDispatchStateContext::Header,
                "Arms" => RustDispatchStateContext::Arms,
                "Close" => RustDispatchStateContext::Close,
                "Done" => RustDispatchStateContext::Done,
                _ => RustDispatchStateContext::__NoContext,
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
    pub struct RustDispatch<'a> {
        _state_stack: Vec<RustDispatchCompartment>,
        __compartment: RustDispatchCompartment,
        __next_compartment: Option<RustDispatchCompartment>,
        _context_stack: Vec<RustDispatchFrameContext>,
        pub sym: &'a SystemSym,
        pub state: &'a str,
        pub arms: EventVec,
        pub na: usize,
        pub out: Sink,
        pub ai: usize,
    }

    #[allow(non_snake_case)]
    impl<'a> RustDispatch<'a> {
        pub fn new(sym: &'a SystemSym, state: &'a str, arms: EventVec, na: usize, out: Sink) -> Self {
            Self {
                _state_stack: Vec::new(),
                _context_stack: Vec::new(),
                sym: sym,
                state: state,
                arms: arms,
                na: na,
                out: out,
                ai: 0,
                __compartment: RustDispatchCompartment::new("Header"),
                __next_compartment: None,
            }
        }

        pub fn __create(sym: &'a SystemSym, state: &'a str, arms: EventVec, na: usize, out: Sink) -> Self {
            let mut c = Self::new(sym, state, arms, na, out);
            c.__compartment = c.__prepareEnter("Header");
            let __e = alloc::rc::Rc::new(RustDispatchFrameEvent::FrameEnter {});
            let __ctx = RustDispatchFrameContext::new(alloc::rc::Rc::clone(&__e), None);
            c._context_stack.push(__ctx);
            c.__kernel(&__e);
            c._context_stack.pop();
            c
        }

        fn __hsm_chain(&mut self, leaf: &str) -> &'static [&'static str] {
            match leaf {
                "Header" => &["Header"],
                "Arms" => &["Arms"],
                "Close" => &["Close"],
                "Done" => &["Done"],
                _ => &[],
            }
        }

        fn __prepareEnter(&mut self, leaf: &str) -> RustDispatchCompartment {
            let chain = self.__hsm_chain(leaf);
            let mut comp: Option<RustDispatchCompartment> = None;
            for name in chain.iter() {
                let mut new_comp = RustDispatchCompartment::new(name);
                if let Some(parent) = comp.take() {
                    new_comp.parent_compartment = Some(Box::new(parent));
                }
                comp = Some(new_comp);
            }
            comp.expect("chain must contain at least the leaf state")
        }

        fn __kernel(&mut self, __e: &alloc::rc::Rc<RustDispatchFrameEvent>) {
            // Route event to current state.
            self.__router(__e);
            // Drain any transitions queued by the handler.
            while self.__next_compartment.is_some() {
                let next_compartment = self.__next_compartment.take().expect("invariant: while-loop guard checked is_some()");
                // Exit the current (leaf) state. RFC-0025.1: exit args live in the
                // source state's typed ctx (written at the transition site), so the
                // synthesized `<$` event carries no payload.
                let exit_event = alloc::rc::Rc::new(RustDispatchFrameEvent::FrameExit {});
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
                        let enter_event = alloc::rc::Rc::new(RustDispatchFrameEvent::FrameEnter {});
                        self.__router(&enter_event);
                    }
                    Some(fwd) if matches!(fwd, RustDispatchFrameEvent::FrameEnter { .. }) => {
                        // Forwarded event IS $> — dispatch directly so the
                        // destination's $> handler receives the caller's payload.
                        let fwd_rc = alloc::rc::Rc::new(fwd);
                        self.__router(&fwd_rc);
                    }
                    Some(fwd) => {
                        // Forwarded event is not $> — initialize the destination
                        // with a fresh $>, then dispatch the forward.
                        let enter_event = alloc::rc::Rc::new(RustDispatchFrameEvent::FrameEnter {});
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

        fn __router(&mut self, __e: &alloc::rc::Rc<RustDispatchFrameEvent>) {
            let __ev: &RustDispatchFrameEvent = __e;
            match self.__compartment.state.as_str() {
                "Header" => self._state_Header(__ev),
                "Arms" => self._state_Arms(__ev),
                "Close" => self._state_Close(__ev),
                "Done" => self._state_Done(__ev),
                _ => {}
            }
        }

        fn __transition(&mut self, next_compartment: RustDispatchCompartment) {
            self.__next_compartment = Some(next_compartment);
        }

        pub fn step(&mut self) {
            let __e = alloc::rc::Rc::new(RustDispatchFrameEvent::Step {});
            let mut __ctx = RustDispatchFrameContext::new(alloc::rc::Rc::clone(&__e), None);
            self._context_stack.push(__ctx);
            self.__kernel(&__e);
            self._context_stack.pop();
        }

        fn _state_Header(&mut self, __e: &RustDispatchFrameEvent) {
            match __e {
                RustDispatchFrameEvent::Step { .. } => { self._s_Header_hdl_user_step(__e); }
                _ => {}
            }
        }

        fn _state_Arms(&mut self, __e: &RustDispatchFrameEvent) {
            match __e {
                RustDispatchFrameEvent::Step { .. } => { self._s_Arms_hdl_user_step(__e); }
                _ => {}
            }
        }

        fn _state_Close(&mut self, __e: &RustDispatchFrameEvent) {
            match __e {
                RustDispatchFrameEvent::Step { .. } => { self._s_Close_hdl_user_step(__e); }
                _ => {}
            }
        }

        fn _state_Done(&mut self, __e: &RustDispatchFrameEvent) {
            match __e {
                _ => {}
            }
        }

        fn _s_Header_hdl_user_step(&mut self, __e: &RustDispatchFrameEvent) {
            rust_dispatch_open(self.sym, self.state, &mut self.out);
            let mut __compartment = self.__prepareEnter("Arms");
            self.__transition(__compartment);
            return;
        }

        fn _s_Arms_hdl_user_step(&mut self, __e: &RustDispatchFrameEvent) {
            if self.ai >= self.na {
                let mut __compartment = self.__prepareEnter("Close");
                self.__transition(__compartment);
                return;
            }
            rust_dispatch_arm(self.sym, self.state, &self.arms, self.ai, &mut self.out);
            self.ai = self.ai + 1;
            let mut __compartment = self.__prepareEnter("Arms");
            self.__transition(__compartment);
            return;
        }

        fn _s_Close_hdl_user_step(&mut self, __e: &RustDispatchFrameEvent) {
            rust_dispatch_close(&mut self.out);
            let mut __compartment = self.__prepareEnter("Done");
            self.__transition(__compartment);
            return;
        }
    }
}
pub use _rust_dispatch_framec::*;
