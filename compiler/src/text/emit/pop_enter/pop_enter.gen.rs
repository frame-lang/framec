
// The RUST pop-enter emitter — MECHANIZED from rust.rs::pop_enter by Cauldron's mechanizer
// (increment 3b), then hand-adjusted at ONE leaf: `self.pad(rel)` (a pure Backend method, no
// self-state) is inlined as `format!("        {}", " ".repeat(self.rel as usize))`, because the
// mechanizer does not yet lift `self.method(..)` calls to free-function leaves (a later increment).
// Everything else is verbatim mechanizer output: the `for st in &sym.states { if has_lifecycle {..} }`
// loop became a For4/Fork3/Step2/Next1 cycle over the cursor `i0`, entry $Step5.
//
// pop_enter is a per-backend Backend method (java/python/c stay native); this rust-only system is
// driven from rust.rs's `pop_enter` one-line driver, and its byte-for-byte ORACLE is the preserved
// frozen `super::rust::pop_enter_hand`, gated in tests/emit_scaffold_walks.rs via
// `super::driver::pop_enter_parity_report`.
//
// Regen: framec-ng -l rust --emit pop_enter.frs | grep -v '^#!\[allow' > pop_enter.gen.rs
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
mod _pop_enter_framec {
    use super::*;
    extern crate alloc;
    use alloc::{vec, format};
    #[derive(Clone)]
    #[allow(dead_code, non_camel_case_types)]
    enum PopEnterFrameEvent {
        Step {  },
        FrameEnter {},
        FrameExit {},
    }

    #[derive(Clone)]
    #[allow(dead_code, non_camel_case_types)]
    enum PopEnterFrameReturn {
        _Lifecycle(alloc::rc::Rc<dyn core::any::Any>),
    }

    #[allow(dead_code)]
    impl PopEnterFrameEvent {
        fn name(&self) -> &'static str {
            match self {
                PopEnterFrameEvent::Step { .. } => "step",
                PopEnterFrameEvent::FrameEnter { .. } => "$>",
                PopEnterFrameEvent::FrameExit { .. } => "<$",
            }
        }
    }

    #[derive(Clone, Debug)]
    #[allow(dead_code, non_camel_case_types)]
    enum PopEnterFrameValue {
        Int(i64),
        Float(f64),
        Bool(bool),
        Str(String),
        List(Vec<Self>),
        Dict(alloc::collections::BTreeMap<String, Self>),
    }

    #[allow(dead_code, non_camel_case_types)]
    struct PopEnterFrameContext {
        event: alloc::rc::Rc<PopEnterFrameEvent>,
        _return: Option<PopEnterFrameReturn>,
        _data: alloc::collections::BTreeMap<String, PopEnterFrameValue>,
        _transitioned: bool,
    }

    impl PopEnterFrameContext {
        fn new(event: alloc::rc::Rc<PopEnterFrameEvent>, default_return: Option<PopEnterFrameReturn>) -> Self {
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
    enum PopEnterStateContext {
        Step5,
        Step2,
        Fork3,
        For4,
        Next1,
        Done,
        __NoContext,
    }

    impl Default for PopEnterStateContext {
        fn default() -> Self {
            PopEnterStateContext::Step5
        }
    }

    #[allow(dead_code, non_camel_case_types)]
    #[derive(Clone)]
    struct PopEnterCompartment {
        state: String,
        state_context: PopEnterStateContext,
        forward_event: Option<PopEnterFrameEvent>,
        parent_compartment: Option<Box<PopEnterCompartment>>,
    }

    impl PopEnterCompartment {
        fn new(state: &str) -> Self {
            let state_context = match state {
                "Step5" => PopEnterStateContext::Step5,
                "Step2" => PopEnterStateContext::Step2,
                "Fork3" => PopEnterStateContext::Fork3,
                "For4" => PopEnterStateContext::For4,
                "Next1" => PopEnterStateContext::Next1,
                "Done" => PopEnterStateContext::Done,
                _ => PopEnterStateContext::__NoContext,
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
    pub struct PopEnter<'a> {
        _state_stack: Vec<PopEnterCompartment>,
        __compartment: PopEnterCompartment,
        __next_compartment: Option<PopEnterCompartment>,
        _context_stack: Vec<PopEnterFrameContext>,
        pub rel: u32,
        pub sym: &'a SystemSym,
        pub enter_args: Option < &'a str >,
        pub i0: usize,
        pub out: Sink,
    }

    #[allow(non_snake_case)]
    impl<'a> PopEnter<'a> {
        pub fn new(rel: u32, sym: &'a SystemSym, enter_args: Option < &'a str >, out: Sink) -> Self {
            Self {
                _state_stack: Vec::new(),
                _context_stack: Vec::new(),
                rel: rel,
                sym: sym,
                enter_args: enter_args,
                i0: 0,
                out: out,
                __compartment: PopEnterCompartment::new("Step5"),
                __next_compartment: None,
            }
        }

        pub fn __create(rel: u32, sym: &'a SystemSym, enter_args: Option < &'a str >, out: Sink) -> Self {
            let mut c = Self::new(rel, sym, enter_args, out);
            c.__compartment = c.__prepareEnter("Step5");
            let __e = alloc::rc::Rc::new(PopEnterFrameEvent::FrameEnter {});
            let __ctx = PopEnterFrameContext::new(alloc::rc::Rc::clone(&__e), None);
            c._context_stack.push(__ctx);
            c.__kernel(&__e);
            c._context_stack.pop();
            c
        }

        fn __hsm_chain(&mut self, leaf: &str) -> &'static [&'static str] {
            match leaf {
                "Step5" => &["Step5"],
                "Step2" => &["Step2"],
                "Fork3" => &["Fork3"],
                "For4" => &["For4"],
                "Next1" => &["Next1"],
                "Done" => &["Done"],
                _ => &[],
            }
        }

        fn __prepareEnter(&mut self, leaf: &str) -> PopEnterCompartment {
            let chain = self.__hsm_chain(leaf);
            let mut comp: Option<PopEnterCompartment> = None;
            for name in chain.iter() {
                let mut new_comp = PopEnterCompartment::new(name);
                if let Some(parent) = comp.take() {
                    new_comp.parent_compartment = Some(Box::new(parent));
                }
                comp = Some(new_comp);
            }
            comp.expect("chain must contain at least the leaf state")
        }

        fn __kernel(&mut self, __e: &alloc::rc::Rc<PopEnterFrameEvent>) {
            // Route event to current state.
            self.__router(__e);
            // Drain any transitions queued by the handler.
            while self.__next_compartment.is_some() {
                let next_compartment = self.__next_compartment.take().expect("invariant: while-loop guard checked is_some()");
                // Exit the current (leaf) state. RFC-0025.1: exit args live in the
                // source state's typed ctx (written at the transition site), so the
                // synthesized `<$` event carries no payload.
                let exit_event = alloc::rc::Rc::new(PopEnterFrameEvent::FrameExit {});
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
                        let enter_event = alloc::rc::Rc::new(PopEnterFrameEvent::FrameEnter {});
                        self.__router(&enter_event);
                    }
                    Some(fwd) if matches!(fwd, PopEnterFrameEvent::FrameEnter { .. }) => {
                        // Forwarded event IS $> — dispatch directly so the
                        // destination's $> handler receives the caller's payload.
                        let fwd_rc = alloc::rc::Rc::new(fwd);
                        self.__router(&fwd_rc);
                    }
                    Some(fwd) => {
                        // Forwarded event is not $> — initialize the destination
                        // with a fresh $>, then dispatch the forward.
                        let enter_event = alloc::rc::Rc::new(PopEnterFrameEvent::FrameEnter {});
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

        fn __router(&mut self, __e: &alloc::rc::Rc<PopEnterFrameEvent>) {
            let __ev: &PopEnterFrameEvent = __e;
            match self.__compartment.state.as_str() {
                "Step5" => self._state_Step5(__ev),
                "Step2" => self._state_Step2(__ev),
                "Fork3" => self._state_Fork3(__ev),
                "For4" => self._state_For4(__ev),
                "Next1" => self._state_Next1(__ev),
                "Done" => self._state_Done(__ev),
                _ => {}
            }
        }

        fn __transition(&mut self, next_compartment: PopEnterCompartment) {
            self.__next_compartment = Some(next_compartment);
        }

        pub fn step(&mut self) {
            let __e = alloc::rc::Rc::new(PopEnterFrameEvent::Step {});
            let mut __ctx = PopEnterFrameContext::new(alloc::rc::Rc::clone(&__e), None);
            self._context_stack.push(__ctx);
            self.__kernel(&__e);
            self._context_stack.pop();
        }

        fn _state_Step5(&mut self, __e: &PopEnterFrameEvent) {
            match __e {
                PopEnterFrameEvent::Step { .. } => { self._s_Step5_hdl_user_step(__e); }
                _ => {}
            }
        }

        fn _state_Step2(&mut self, __e: &PopEnterFrameEvent) {
            match __e {
                PopEnterFrameEvent::Step { .. } => { self._s_Step2_hdl_user_step(__e); }
                _ => {}
            }
        }

        fn _state_Fork3(&mut self, __e: &PopEnterFrameEvent) {
            match __e {
                PopEnterFrameEvent::Step { .. } => { self._s_Fork3_hdl_user_step(__e); }
                _ => {}
            }
        }

        fn _state_For4(&mut self, __e: &PopEnterFrameEvent) {
            match __e {
                PopEnterFrameEvent::Step { .. } => { self._s_For4_hdl_user_step(__e); }
                _ => {}
            }
        }

        fn _state_Next1(&mut self, __e: &PopEnterFrameEvent) {
            match __e {
                PopEnterFrameEvent::Step { .. } => { self._s_Next1_hdl_user_step(__e); }
                _ => {}
            }
        }

        fn _state_Done(&mut self, __e: &PopEnterFrameEvent) {
            match __e {
                _ => {}
            }
        }

        fn _s_Step5_hdl_user_step(&mut self, __e: &PopEnterFrameEvent) {
            let mut __compartment = self.__prepareEnter("For4");
            self.__transition(__compartment);
            return;
        }

        fn _s_Step2_hdl_user_step(&mut self, __e: &PopEnterFrameEvent) {
            let p = format ! ("        {}" , " " . repeat (self.rel as usize));
            let a = self . enter_args . unwrap_or ("");
            let st = & self . sym . states [self.i0];
            self . out . frame (& format ! ("{p}if self.compartment.state == \"{}\" {{ self.{}_{}({a}); }}\n" , st . name , st . name , rust_ident ("$>"))) ;
            let mut __compartment = self.__prepareEnter("Next1");
            self.__transition(__compartment);
            return;
        }

        fn _s_Fork3_hdl_user_step(&mut self, __e: &PopEnterFrameEvent) {
            let st = & self . sym . states [self.i0];
            if has_lifecycle (self . sym , & st . name , "$>") {
                let mut __compartment = self.__prepareEnter("Step2");
                self.__transition(__compartment);
                return;
            }
            let mut __compartment = self.__prepareEnter("Next1");
            self.__transition(__compartment);
            return;
        }

        fn _s_For4_hdl_user_step(&mut self, __e: &PopEnterFrameEvent) {
            if self.i0 >= self . sym . states.len() {
                let mut __compartment = self.__prepareEnter("Done");
                self.__transition(__compartment);
                return;
            }
            let mut __compartment = self.__prepareEnter("Fork3");
            self.__transition(__compartment);
            return;
        }

        fn _s_Next1_hdl_user_step(&mut self, __e: &PopEnterFrameEvent) {
            self.i0 = self.i0 + 1;
            let mut __compartment = self.__prepareEnter("For4");
            self.__transition(__compartment);
            return;
        }
    }
}
pub use _pop_enter_framec::*;
