
// The SHARED per-state HANDLER-OPENER walk, for the targets whose private `(state, handler)` method
// is a header + two positional binding loops — Python, Java, C. It SPELLS the opening of one handler
// method, one fragment at a time, through the `Backend` seam: emit the method HEADER, bind the
// state's own params off the live compartment (`state_args`, one per param), bind the event's own
// params off its slot (`enter_args` for `$>`, `exit_args` for `<$`, `__e._parameters` for a user
// event, one per param), then run the language TAIL (C seeds a `$>` state's `$.x` vars into the
// compartment; Python/Java have no tail). Rust's opener is structurally different — a scan-branch
// plus a header-only kernel branch, with no body binding loops — so it does NOT use this walk; it
// is a SEPARATE future milestone.
//
// THE WALK IS IDENTICAL ACROSS PYTHON/JAVA/C: header, state-param binds, event-param binds, tail.
// Only the per-fragment SPELLING differs (Python untyped `x = slot[i]`; Java
// `{ty} x = java_unbox(slot.get(i))`; C `{ty} x = ({ty})(intptr_t)FrameVec_get(slot, i)`), and that
// is exactly what the `be.handler_*` leaves carry. The one target-specific tail — C's `$>`
// state-var seeding versus Python/Java's nothing — folds entirely into `be.handler_seeds`, so this
// walk needs no per-language guard.
//
// THE HONEST MACHINE CLASS. Degenerate pole: a program-counter walk over data already decided
// upstream (params read from the resolved symbol table; the two cursors `si`, `ei` are pure program
// counters). The payoff is DOGFOOD UNIFORMITY, and — because it is SHARED — the elimination of three
// copies of one walk. Differential-gated byte-for-byte against each target's preserved
// `*_open_handler_hand`. It rides the same read-only borrowed domain as the landed emit machines
// (`be: &dyn Backend`, `sym: &SystemSym`). (`params` is carried OWNED, like `DispatchBody`'s `arms`.)
//
// Regen: framec-ng -l rust --emit handler_open.frs | grep -v '^#!\[allow' > handler_open.gen.rs

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
mod _handler_open_framec {
    use super::*;
    extern crate alloc;
    use alloc::{vec, format};
    #[derive(Clone)]
    #[allow(dead_code, non_camel_case_types)]
    enum HandlerOpenFrameEvent {
        Step {  },
        FrameEnter {},
        FrameExit {},
    }

    #[derive(Clone)]
    #[allow(dead_code, non_camel_case_types)]
    enum HandlerOpenFrameReturn {
        _Lifecycle(alloc::rc::Rc<dyn core::any::Any>),
    }

    #[allow(dead_code)]
    impl HandlerOpenFrameEvent {
        fn name(&self) -> &'static str {
            match self {
                HandlerOpenFrameEvent::Step { .. } => "step",
                HandlerOpenFrameEvent::FrameEnter { .. } => "$>",
                HandlerOpenFrameEvent::FrameExit { .. } => "<$",
            }
        }
    }

    #[derive(Clone, Debug)]
    #[allow(dead_code, non_camel_case_types)]
    enum HandlerOpenFrameValue {
        Int(i64),
        Float(f64),
        Bool(bool),
        Str(String),
        List(Vec<Self>),
        Dict(alloc::collections::BTreeMap<String, Self>),
    }

    #[allow(dead_code, non_camel_case_types)]
    struct HandlerOpenFrameContext {
        event: alloc::rc::Rc<HandlerOpenFrameEvent>,
        _return: Option<HandlerOpenFrameReturn>,
        _data: alloc::collections::BTreeMap<String, HandlerOpenFrameValue>,
        _transitioned: bool,
    }

    impl HandlerOpenFrameContext {
        fn new(event: alloc::rc::Rc<HandlerOpenFrameEvent>, default_return: Option<HandlerOpenFrameReturn>) -> Self {
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
    enum HandlerOpenStateContext {
        Header,
        StateParams,
        EventParams,
        Seeds,
        Done,
        __NoContext,
    }

    impl Default for HandlerOpenStateContext {
        fn default() -> Self {
            HandlerOpenStateContext::Header
        }
    }

    #[allow(dead_code, non_camel_case_types)]
    #[derive(Clone)]
    struct HandlerOpenCompartment {
        state: String,
        state_context: HandlerOpenStateContext,
        forward_event: Option<HandlerOpenFrameEvent>,
        parent_compartment: Option<Box<HandlerOpenCompartment>>,
    }

    impl HandlerOpenCompartment {
        fn new(state: &str) -> Self {
            let state_context = match state {
                "Header" => HandlerOpenStateContext::Header,
                "StateParams" => HandlerOpenStateContext::StateParams,
                "EventParams" => HandlerOpenStateContext::EventParams,
                "Seeds" => HandlerOpenStateContext::Seeds,
                "Done" => HandlerOpenStateContext::Done,
                _ => HandlerOpenStateContext::__NoContext,
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
    pub struct HandlerOpen<'a> {
        _state_stack: Vec<HandlerOpenCompartment>,
        __compartment: HandlerOpenCompartment,
        __next_compartment: Option<HandlerOpenCompartment>,
        _context_stack: Vec<HandlerOpenFrameContext>,
        pub be: &'a dyn Backend,
        pub sym: &'a SystemSym,
        pub state: &'a str,
        pub event: &'a str,
        pub params: String,
        pub ns: usize,
        pub ne: usize,
        pub out: Sink,
        pub si: usize,
        pub ei: usize,
    }

    #[allow(non_snake_case)]
    impl<'a> HandlerOpen<'a> {
        pub fn new(be: &'a dyn Backend, sym: &'a SystemSym, state: &'a str, event: &'a str, params: String, ns: usize, ne: usize, out: Sink) -> Self {
            Self {
                _state_stack: Vec::new(),
                _context_stack: Vec::new(),
                be: be,
                sym: sym,
                state: state,
                event: event,
                params: params,
                ns: ns,
                ne: ne,
                out: out,
                si: 0,
                ei: 0,
                __compartment: HandlerOpenCompartment::new("Header"),
                __next_compartment: None,
            }
        }

        pub fn __create(be: &'a dyn Backend, sym: &'a SystemSym, state: &'a str, event: &'a str, params: String, ns: usize, ne: usize, out: Sink) -> Self {
            let mut c = Self::new(be, sym, state, event, params, ns, ne, out);
            c.__compartment = c.__prepareEnter("Header");
            let __e = alloc::rc::Rc::new(HandlerOpenFrameEvent::FrameEnter {});
            let __ctx = HandlerOpenFrameContext::new(alloc::rc::Rc::clone(&__e), None);
            c._context_stack.push(__ctx);
            c.__kernel(&__e);
            c._context_stack.pop();
            c
        }

        fn __hsm_chain(&mut self, leaf: &str) -> &'static [&'static str] {
            match leaf {
                "Header" => &["Header"],
                "StateParams" => &["StateParams"],
                "EventParams" => &["EventParams"],
                "Seeds" => &["Seeds"],
                "Done" => &["Done"],
                _ => &[],
            }
        }

        fn __prepareEnter(&mut self, leaf: &str) -> HandlerOpenCompartment {
            let chain = self.__hsm_chain(leaf);
            let mut comp: Option<HandlerOpenCompartment> = None;
            for name in chain.iter() {
                let mut new_comp = HandlerOpenCompartment::new(name);
                if let Some(parent) = comp.take() {
                    new_comp.parent_compartment = Some(Box::new(parent));
                }
                comp = Some(new_comp);
            }
            comp.expect("chain must contain at least the leaf state")
        }

        fn __kernel(&mut self, __e: &alloc::rc::Rc<HandlerOpenFrameEvent>) {
            // Route event to current state.
            self.__router(__e);
            // Drain any transitions queued by the handler.
            while self.__next_compartment.is_some() {
                let next_compartment = self.__next_compartment.take().expect("invariant: while-loop guard checked is_some()");
                // Exit the current (leaf) state. RFC-0025.1: exit args live in the
                // source state's typed ctx (written at the transition site), so the
                // synthesized `<$` event carries no payload.
                let exit_event = alloc::rc::Rc::new(HandlerOpenFrameEvent::FrameExit {});
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
                        let enter_event = alloc::rc::Rc::new(HandlerOpenFrameEvent::FrameEnter {});
                        self.__router(&enter_event);
                    }
                    Some(fwd) if matches!(fwd, HandlerOpenFrameEvent::FrameEnter { .. }) => {
                        // Forwarded event IS $> — dispatch directly so the
                        // destination's $> handler receives the caller's payload.
                        let fwd_rc = alloc::rc::Rc::new(fwd);
                        self.__router(&fwd_rc);
                    }
                    Some(fwd) => {
                        // Forwarded event is not $> — initialize the destination
                        // with a fresh $>, then dispatch the forward.
                        let enter_event = alloc::rc::Rc::new(HandlerOpenFrameEvent::FrameEnter {});
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

        fn __router(&mut self, __e: &alloc::rc::Rc<HandlerOpenFrameEvent>) {
            let __ev: &HandlerOpenFrameEvent = __e;
            match self.__compartment.state.as_str() {
                "Header" => self._state_Header(__ev),
                "StateParams" => self._state_StateParams(__ev),
                "EventParams" => self._state_EventParams(__ev),
                "Seeds" => self._state_Seeds(__ev),
                "Done" => self._state_Done(__ev),
                _ => {}
            }
        }

        fn __transition(&mut self, next_compartment: HandlerOpenCompartment) {
            self.__next_compartment = Some(next_compartment);
        }

        pub fn step(&mut self) {
            let __e = alloc::rc::Rc::new(HandlerOpenFrameEvent::Step {});
            let mut __ctx = HandlerOpenFrameContext::new(alloc::rc::Rc::clone(&__e), None);
            self._context_stack.push(__ctx);
            self.__kernel(&__e);
            self._context_stack.pop();
        }

        fn _state_Header(&mut self, __e: &HandlerOpenFrameEvent) {
            match __e {
                HandlerOpenFrameEvent::Step { .. } => { self._s_Header_hdl_user_step(__e); }
                _ => {}
            }
        }

        fn _state_StateParams(&mut self, __e: &HandlerOpenFrameEvent) {
            match __e {
                HandlerOpenFrameEvent::Step { .. } => { self._s_StateParams_hdl_user_step(__e); }
                _ => {}
            }
        }

        fn _state_EventParams(&mut self, __e: &HandlerOpenFrameEvent) {
            match __e {
                HandlerOpenFrameEvent::Step { .. } => { self._s_EventParams_hdl_user_step(__e); }
                _ => {}
            }
        }

        fn _state_Seeds(&mut self, __e: &HandlerOpenFrameEvent) {
            match __e {
                HandlerOpenFrameEvent::Step { .. } => { self._s_Seeds_hdl_user_step(__e); }
                _ => {}
            }
        }

        fn _state_Done(&mut self, __e: &HandlerOpenFrameEvent) {
            match __e {
                _ => {}
            }
        }

        fn _s_Header_hdl_user_step(&mut self, __e: &HandlerOpenFrameEvent) {
            handler_open(self.be, self.sym, self.state, self.event, &self.params, &mut self.out);
            let mut __compartment = self.__prepareEnter("StateParams");
            self.__transition(__compartment);
            return;
        }

        fn _s_StateParams_hdl_user_step(&mut self, __e: &HandlerOpenFrameEvent) {
            if self.si >= self.ns {
                let mut __compartment = self.__prepareEnter("EventParams");
                self.__transition(__compartment);
                return;
            }
            handler_state_param(self.be, self.sym, self.state, self.si, &mut self.out);
            self.si = self.si + 1;
            let mut __compartment = self.__prepareEnter("StateParams");
            self.__transition(__compartment);
            return;
        }

        fn _s_EventParams_hdl_user_step(&mut self, __e: &HandlerOpenFrameEvent) {
            if self.ei >= self.ne {
                let mut __compartment = self.__prepareEnter("Seeds");
                self.__transition(__compartment);
                return;
            }
            handler_event_param(self.be, self.sym, self.state, self.event, &self.params, self.ei, &mut self.out);
            self.ei = self.ei + 1;
            let mut __compartment = self.__prepareEnter("EventParams");
            self.__transition(__compartment);
            return;
        }

        fn _s_Seeds_hdl_user_step(&mut self, __e: &HandlerOpenFrameEvent) {
            handler_seeds(self.be, self.sym, self.state, self.event, &mut self.out);
            let mut __compartment = self.__prepareEnter("Done");
            self.__transition(__compartment);
            return;
        }
    }
}
pub use _handler_open_framec::*;
