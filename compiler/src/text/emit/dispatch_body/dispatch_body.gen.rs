
// The SHARED per-state message-dispatcher BODY walk, for the targets whose dispatcher is an
// `if`-chain over the event message — Python, Java, C. It is the inner half of dispatch: the shared
// `StateDispatchWalk` decides WHICH arms a state dispatches and in WHAT ORDER; this walk SPELLS the
// method body those arms produce, one fragment at a time, through the `Backend` seam. Rust's
// dispatcher is a `match` over a typed event enum — a different control structure — so it does NOT
// use this walk; it gets its own system.
//
// THE WALK IS IDENTICAL ACROSS PYTHON/JAVA/C: emit the method header, bind the state's params off
// the live compartment (one per param), emit one arm per event message, then close. Only the
// per-fragment SPELLING differs (Python `if __e._message == "m":` / untyped bind; Java
// `if (__e._message.equals("m"))` / typed unbox; C `if (strcmp(...))` / typed cast), and that is
// exactly what the `be.dispatch_*` leaves carry. The one target-specific tail — Python's `pass` on an
// empty dispatcher and its `=> $^` default-forward fall-through, versus Java/C's closing brace —
// folds entirely into `be.dispatch_close`, so this walk needs no per-language guard.
//
// THE HONEST MACHINE CLASS. §3 degenerate pole: a program-counter walk over data already decided
// upstream (`arms` pre-ordered by `StateDispatchWalk`; params read from the resolved symbol table).
// The two cursors (`pi`, `ai`) are pure program counters. The payoff is DOGFOOD UNIFORMITY, and —
// because it is SHARED — the elimination of three copies of one walk. Differential-gated byte-for-byte
// against each target's preserved `*_dispatch_hand`. It rides the same read-only borrowed domain as
// the landed emit machines (`be: &dyn Backend`, `sym: &SystemSym`).
//
// Regen: framec-ng -l rust --emit dispatch_body.frs | grep -v '^#!\[allow' > dispatch_body.gen.rs

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
mod _dispatch_body_framec {
    use super::*;
    extern crate alloc;
    use alloc::{vec, format};
    #[derive(Clone)]
    #[allow(dead_code, non_camel_case_types)]
    enum DispatchBodyFrameEvent {
        Step {  },
        FrameEnter {},
        FrameExit {},
    }

    #[derive(Clone)]
    #[allow(dead_code, non_camel_case_types)]
    enum DispatchBodyFrameReturn {
        _Lifecycle(alloc::rc::Rc<dyn core::any::Any>),
    }

    #[allow(dead_code)]
    impl DispatchBodyFrameEvent {
        fn name(&self) -> &'static str {
            match self {
                DispatchBodyFrameEvent::Step { .. } => "step",
                DispatchBodyFrameEvent::FrameEnter { .. } => "$>",
                DispatchBodyFrameEvent::FrameExit { .. } => "<$",
            }
        }
    }

    #[derive(Clone, Debug)]
    #[allow(dead_code, non_camel_case_types)]
    enum DispatchBodyFrameValue {
        Int(i64),
        Float(f64),
        Bool(bool),
        Str(String),
        List(Vec<Self>),
        Dict(alloc::collections::BTreeMap<String, Self>),
    }

    #[allow(dead_code, non_camel_case_types)]
    struct DispatchBodyFrameContext {
        event: alloc::rc::Rc<DispatchBodyFrameEvent>,
        _return: Option<DispatchBodyFrameReturn>,
        _data: alloc::collections::BTreeMap<String, DispatchBodyFrameValue>,
        _transitioned: bool,
    }

    impl DispatchBodyFrameContext {
        fn new(event: alloc::rc::Rc<DispatchBodyFrameEvent>, default_return: Option<DispatchBodyFrameReturn>) -> Self {
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
    enum DispatchBodyStateContext {
        Header,
        Params,
        Arms,
        Close,
        Done,
        __NoContext,
    }

    impl Default for DispatchBodyStateContext {
        fn default() -> Self {
            DispatchBodyStateContext::Header
        }
    }

    #[allow(dead_code, non_camel_case_types)]
    #[derive(Clone)]
    struct DispatchBodyCompartment {
        state: String,
        state_context: DispatchBodyStateContext,
        forward_event: Option<DispatchBodyFrameEvent>,
        parent_compartment: Option<Box<DispatchBodyCompartment>>,
    }

    impl DispatchBodyCompartment {
        fn new(state: &str) -> Self {
            let state_context = match state {
                "Header" => DispatchBodyStateContext::Header,
                "Params" => DispatchBodyStateContext::Params,
                "Arms" => DispatchBodyStateContext::Arms,
                "Close" => DispatchBodyStateContext::Close,
                "Done" => DispatchBodyStateContext::Done,
                _ => DispatchBodyStateContext::__NoContext,
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
    pub struct DispatchBody<'a> {
        _state_stack: Vec<DispatchBodyCompartment>,
        __compartment: DispatchBodyCompartment,
        __next_compartment: Option<DispatchBodyCompartment>,
        _context_stack: Vec<DispatchBodyFrameContext>,
        pub be: &'a dyn Backend,
        pub sym: &'a SystemSym,
        pub state: &'a str,
        pub arms: EventVec,
        pub np: usize,
        pub na: usize,
        pub out: Sink,
        pub pi: usize,
        pub ai: usize,
    }

    #[allow(non_snake_case)]
    impl<'a> DispatchBody<'a> {
        pub fn new(be: &'a dyn Backend, sym: &'a SystemSym, state: &'a str, arms: EventVec, np: usize, na: usize, out: Sink) -> Self {
            Self {
                _state_stack: Vec::new(),
                _context_stack: Vec::new(),
                be: be,
                sym: sym,
                state: state,
                arms: arms,
                np: np,
                na: na,
                out: out,
                pi: 0,
                ai: 0,
                __compartment: DispatchBodyCompartment::new("Header"),
                __next_compartment: None,
            }
        }

        pub fn __create(be: &'a dyn Backend, sym: &'a SystemSym, state: &'a str, arms: EventVec, np: usize, na: usize, out: Sink) -> Self {
            let mut c = Self::new(be, sym, state, arms, np, na, out);
            c.__compartment = c.__prepareEnter("Header");
            let __e = alloc::rc::Rc::new(DispatchBodyFrameEvent::FrameEnter {});
            let __ctx = DispatchBodyFrameContext::new(alloc::rc::Rc::clone(&__e), None);
            c._context_stack.push(__ctx);
            c.__kernel(&__e);
            c._context_stack.pop();
            c
        }

        fn __hsm_chain(&mut self, leaf: &str) -> &'static [&'static str] {
            match leaf {
                "Header" => &["Header"],
                "Params" => &["Params"],
                "Arms" => &["Arms"],
                "Close" => &["Close"],
                "Done" => &["Done"],
                _ => &[],
            }
        }

        fn __prepareEnter(&mut self, leaf: &str) -> DispatchBodyCompartment {
            let chain = self.__hsm_chain(leaf);
            let mut comp: Option<DispatchBodyCompartment> = None;
            for name in chain.iter() {
                let mut new_comp = DispatchBodyCompartment::new(name);
                if let Some(parent) = comp.take() {
                    new_comp.parent_compartment = Some(Box::new(parent));
                }
                comp = Some(new_comp);
            }
            comp.expect("chain must contain at least the leaf state")
        }

        fn __kernel(&mut self, __e: &alloc::rc::Rc<DispatchBodyFrameEvent>) {
            // Route event to current state.
            self.__router(__e);
            // Drain any transitions queued by the handler.
            while self.__next_compartment.is_some() {
                let next_compartment = self.__next_compartment.take().expect("invariant: while-loop guard checked is_some()");
                // Exit the current (leaf) state. RFC-0025.1: exit args live in the
                // source state's typed ctx (written at the transition site), so the
                // synthesized `<$` event carries no payload.
                let exit_event = alloc::rc::Rc::new(DispatchBodyFrameEvent::FrameExit {});
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
                        let enter_event = alloc::rc::Rc::new(DispatchBodyFrameEvent::FrameEnter {});
                        self.__router(&enter_event);
                    }
                    Some(fwd) if matches!(fwd, DispatchBodyFrameEvent::FrameEnter { .. }) => {
                        // Forwarded event IS $> — dispatch directly so the
                        // destination's $> handler receives the caller's payload.
                        let fwd_rc = alloc::rc::Rc::new(fwd);
                        self.__router(&fwd_rc);
                    }
                    Some(fwd) => {
                        // Forwarded event is not $> — initialize the destination
                        // with a fresh $>, then dispatch the forward.
                        let enter_event = alloc::rc::Rc::new(DispatchBodyFrameEvent::FrameEnter {});
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

        fn __router(&mut self, __e: &alloc::rc::Rc<DispatchBodyFrameEvent>) {
            let __ev: &DispatchBodyFrameEvent = __e;
            match self.__compartment.state.as_str() {
                "Header" => self._state_Header(__ev),
                "Params" => self._state_Params(__ev),
                "Arms" => self._state_Arms(__ev),
                "Close" => self._state_Close(__ev),
                "Done" => self._state_Done(__ev),
                _ => {}
            }
        }

        fn __transition(&mut self, next_compartment: DispatchBodyCompartment) {
            self.__next_compartment = Some(next_compartment);
        }

        pub fn step(&mut self) {
            let __e = alloc::rc::Rc::new(DispatchBodyFrameEvent::Step {});
            let mut __ctx = DispatchBodyFrameContext::new(alloc::rc::Rc::clone(&__e), None);
            self._context_stack.push(__ctx);
            self.__kernel(&__e);
            self._context_stack.pop();
        }

        fn _state_Header(&mut self, __e: &DispatchBodyFrameEvent) {
            match __e {
                DispatchBodyFrameEvent::Step { .. } => { self._s_Header_hdl_user_step(__e); }
                _ => {}
            }
        }

        fn _state_Params(&mut self, __e: &DispatchBodyFrameEvent) {
            match __e {
                DispatchBodyFrameEvent::Step { .. } => { self._s_Params_hdl_user_step(__e); }
                _ => {}
            }
        }

        fn _state_Arms(&mut self, __e: &DispatchBodyFrameEvent) {
            match __e {
                DispatchBodyFrameEvent::Step { .. } => { self._s_Arms_hdl_user_step(__e); }
                _ => {}
            }
        }

        fn _state_Close(&mut self, __e: &DispatchBodyFrameEvent) {
            match __e {
                DispatchBodyFrameEvent::Step { .. } => { self._s_Close_hdl_user_step(__e); }
                _ => {}
            }
        }

        fn _state_Done(&mut self, __e: &DispatchBodyFrameEvent) {
            match __e {
                _ => {}
            }
        }

        fn _s_Header_hdl_user_step(&mut self, __e: &DispatchBodyFrameEvent) {
            dispatch_open(self.be, self.sym, self.state, &mut self.out);
            let mut __compartment = self.__prepareEnter("Params");
            self.__transition(__compartment);
            return;
        }

        fn _s_Params_hdl_user_step(&mut self, __e: &DispatchBodyFrameEvent) {
            if self.pi >= self.np {
                let mut __compartment = self.__prepareEnter("Arms");
                self.__transition(__compartment);
                return;
            }
            dispatch_param(self.be, self.sym, self.state, self.pi, &mut self.out);
            self.pi = self.pi + 1;
            let mut __compartment = self.__prepareEnter("Params");
            self.__transition(__compartment);
            return;
        }

        fn _s_Arms_hdl_user_step(&mut self, __e: &DispatchBodyFrameEvent) {
            if self.ai >= self.na {
                let mut __compartment = self.__prepareEnter("Close");
                self.__transition(__compartment);
                return;
            }
            dispatch_arm(self.be, self.sym, self.state, &self.arms, self.ai, &mut self.out);
            self.ai = self.ai + 1;
            let mut __compartment = self.__prepareEnter("Arms");
            self.__transition(__compartment);
            return;
        }

        fn _s_Close_hdl_user_step(&mut self, __e: &DispatchBodyFrameEvent) {
            dispatch_close(self.be, self.sym, self.state, &self.arms, self.np, &mut self.out);
            let mut __compartment = self.__prepareEnter("Done");
            self.__transition(__compartment);
            return;
        }
    }
}
pub use _dispatch_body_framec::*;
