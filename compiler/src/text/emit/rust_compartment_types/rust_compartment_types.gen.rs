
// The RUST per-system TYPED-COMPARTMENT emitter — the `<Sys>Vars` / `<Sys>Args` enums and the
// `<Sys>Comp` struct that give every rust system a typed, per-state compartment (RFC-0056: the
// host serializer marshals the vars/args natively; framec writes no `downcast`, no `Box<dyn Any>`).
// A rust-only concern — no other backend spells this shape — so, like `RustDispatch`, it gets its
// OWN system, driven from rust.rs's free `emit_compartment_types`, over rust-only leaves
// (`ct_vars_open` / `ct_vars_variant` / `ct_args_open` / `ct_args_variant` / `ct_close` / `ct_comp`)
// — NOT any `Backend` trait seam.
//
// THE WALK: three fixed sections, two of them a per-state loop.
//   1. open the `<Sys>Vars` enum, emit one variant per state (its `$.` state vars), close it;
//   2. open the `<Sys>Args` enum, emit one variant per state (its `(param)` args), close it;
//   3. emit the `<Sys>Comp { state, vars, args }` struct (fixed fields), self-terminated.
// The per-variant bytes — a state's field list, the `state_var_ty` mapping, the serde `derive`
// recomputed from `sym.persist_reachable` — all live in the leaves; this walk only SEQUENCES them.
//
// THE HONEST MACHINE CLASS. §3 degenerate pole: a program-counter walk over data already decided
// upstream (`sym.states` from the resolved symbol table). Two cursors (`vi`, `ai`) are pure program
// counters over the same state list. Differential-gated byte-for-byte against the preserved
// `rust_compartment_types_hand`. It rides the same read-only borrowed domain as the landed emit
// machines (`sym: &SystemSym`).
//
// Regen: framec-ng -l rust --emit rust_compartment_types.frs | grep -v '^#!\[allow' >
// rust_compartment_types.gen.rs

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
mod _rust_compartment_types_framec {
    use super::*;
    extern crate alloc;
    use alloc::{vec, format};
    #[derive(Clone)]
    #[allow(dead_code, non_camel_case_types)]
    enum RustCompartmentTypesFrameEvent {
        Step {  },
        FrameEnter {},
        FrameExit {},
    }

    #[derive(Clone)]
    #[allow(dead_code, non_camel_case_types)]
    enum RustCompartmentTypesFrameReturn {
        _Lifecycle(alloc::rc::Rc<dyn core::any::Any>),
    }

    #[allow(dead_code)]
    impl RustCompartmentTypesFrameEvent {
        fn name(&self) -> &'static str {
            match self {
                RustCompartmentTypesFrameEvent::Step { .. } => "step",
                RustCompartmentTypesFrameEvent::FrameEnter { .. } => "$>",
                RustCompartmentTypesFrameEvent::FrameExit { .. } => "<$",
            }
        }
    }

    #[derive(Clone, Debug)]
    #[allow(dead_code, non_camel_case_types)]
    enum RustCompartmentTypesFrameValue {
        Int(i64),
        Float(f64),
        Bool(bool),
        Str(String),
        List(Vec<Self>),
        Dict(alloc::collections::BTreeMap<String, Self>),
    }

    #[allow(dead_code, non_camel_case_types)]
    struct RustCompartmentTypesFrameContext {
        event: alloc::rc::Rc<RustCompartmentTypesFrameEvent>,
        _return: Option<RustCompartmentTypesFrameReturn>,
        _data: alloc::collections::BTreeMap<String, RustCompartmentTypesFrameValue>,
        _transitioned: bool,
    }

    impl RustCompartmentTypesFrameContext {
        fn new(event: alloc::rc::Rc<RustCompartmentTypesFrameEvent>, default_return: Option<RustCompartmentTypesFrameReturn>) -> Self {
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
    enum RustCompartmentTypesStateContext {
        VarsOpen,
        VarsLoop,
        VarsClose,
        ArgsOpen,
        ArgsLoop,
        ArgsClose,
        Comp,
        Done,
        __NoContext,
    }

    impl Default for RustCompartmentTypesStateContext {
        fn default() -> Self {
            RustCompartmentTypesStateContext::VarsOpen
        }
    }

    #[allow(dead_code, non_camel_case_types)]
    #[derive(Clone)]
    struct RustCompartmentTypesCompartment {
        state: String,
        state_context: RustCompartmentTypesStateContext,
        forward_event: Option<RustCompartmentTypesFrameEvent>,
        parent_compartment: Option<Box<RustCompartmentTypesCompartment>>,
    }

    impl RustCompartmentTypesCompartment {
        fn new(state: &str) -> Self {
            let state_context = match state {
                "VarsOpen" => RustCompartmentTypesStateContext::VarsOpen,
                "VarsLoop" => RustCompartmentTypesStateContext::VarsLoop,
                "VarsClose" => RustCompartmentTypesStateContext::VarsClose,
                "ArgsOpen" => RustCompartmentTypesStateContext::ArgsOpen,
                "ArgsLoop" => RustCompartmentTypesStateContext::ArgsLoop,
                "ArgsClose" => RustCompartmentTypesStateContext::ArgsClose,
                "Comp" => RustCompartmentTypesStateContext::Comp,
                "Done" => RustCompartmentTypesStateContext::Done,
                _ => RustCompartmentTypesStateContext::__NoContext,
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
    pub struct RustCompartmentTypes<'a> {
        _state_stack: Vec<RustCompartmentTypesCompartment>,
        __compartment: RustCompartmentTypesCompartment,
        __next_compartment: Option<RustCompartmentTypesCompartment>,
        _context_stack: Vec<RustCompartmentTypesFrameContext>,
        pub sym: &'a SystemSym,
        pub ns: usize,
        pub out: Sink,
        pub vi: usize,
        pub ai: usize,
    }

    #[allow(non_snake_case)]
    impl<'a> RustCompartmentTypes<'a> {
        pub fn new(sym: &'a SystemSym, ns: usize, out: Sink) -> Self {
            Self {
                _state_stack: Vec::new(),
                _context_stack: Vec::new(),
                sym: sym,
                ns: ns,
                out: out,
                vi: 0,
                ai: 0,
                __compartment: RustCompartmentTypesCompartment::new("VarsOpen"),
                __next_compartment: None,
            }
        }

        pub fn __create(sym: &'a SystemSym, ns: usize, out: Sink) -> Self {
            let mut c = Self::new(sym, ns, out);
            c.__compartment = c.__prepareEnter("VarsOpen");
            let __e = alloc::rc::Rc::new(RustCompartmentTypesFrameEvent::FrameEnter {});
            let __ctx = RustCompartmentTypesFrameContext::new(alloc::rc::Rc::clone(&__e), None);
            c._context_stack.push(__ctx);
            c.__kernel(&__e);
            c._context_stack.pop();
            c
        }

        fn __hsm_chain(&mut self, leaf: &str) -> &'static [&'static str] {
            match leaf {
                "VarsOpen" => &["VarsOpen"],
                "VarsLoop" => &["VarsLoop"],
                "VarsClose" => &["VarsClose"],
                "ArgsOpen" => &["ArgsOpen"],
                "ArgsLoop" => &["ArgsLoop"],
                "ArgsClose" => &["ArgsClose"],
                "Comp" => &["Comp"],
                "Done" => &["Done"],
                _ => &[],
            }
        }

        fn __prepareEnter(&mut self, leaf: &str) -> RustCompartmentTypesCompartment {
            let chain = self.__hsm_chain(leaf);
            let mut comp: Option<RustCompartmentTypesCompartment> = None;
            for name in chain.iter() {
                let mut new_comp = RustCompartmentTypesCompartment::new(name);
                if let Some(parent) = comp.take() {
                    new_comp.parent_compartment = Some(Box::new(parent));
                }
                comp = Some(new_comp);
            }
            comp.expect("chain must contain at least the leaf state")
        }

        fn __kernel(&mut self, __e: &alloc::rc::Rc<RustCompartmentTypesFrameEvent>) {
            // Route event to current state.
            self.__router(__e);
            // Drain any transitions queued by the handler.
            while self.__next_compartment.is_some() {
                let next_compartment = self.__next_compartment.take().expect("invariant: while-loop guard checked is_some()");
                // Exit the current (leaf) state. RFC-0025.1: exit args live in the
                // source state's typed ctx (written at the transition site), so the
                // synthesized `<$` event carries no payload.
                let exit_event = alloc::rc::Rc::new(RustCompartmentTypesFrameEvent::FrameExit {});
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
                        let enter_event = alloc::rc::Rc::new(RustCompartmentTypesFrameEvent::FrameEnter {});
                        self.__router(&enter_event);
                    }
                    Some(fwd) if matches!(fwd, RustCompartmentTypesFrameEvent::FrameEnter { .. }) => {
                        // Forwarded event IS $> — dispatch directly so the
                        // destination's $> handler receives the caller's payload.
                        let fwd_rc = alloc::rc::Rc::new(fwd);
                        self.__router(&fwd_rc);
                    }
                    Some(fwd) => {
                        // Forwarded event is not $> — initialize the destination
                        // with a fresh $>, then dispatch the forward.
                        let enter_event = alloc::rc::Rc::new(RustCompartmentTypesFrameEvent::FrameEnter {});
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

        fn __router(&mut self, __e: &alloc::rc::Rc<RustCompartmentTypesFrameEvent>) {
            let __ev: &RustCompartmentTypesFrameEvent = __e;
            match self.__compartment.state.as_str() {
                "VarsOpen" => self._state_VarsOpen(__ev),
                "VarsLoop" => self._state_VarsLoop(__ev),
                "VarsClose" => self._state_VarsClose(__ev),
                "ArgsOpen" => self._state_ArgsOpen(__ev),
                "ArgsLoop" => self._state_ArgsLoop(__ev),
                "ArgsClose" => self._state_ArgsClose(__ev),
                "Comp" => self._state_Comp(__ev),
                "Done" => self._state_Done(__ev),
                _ => {}
            }
        }

        fn __transition(&mut self, next_compartment: RustCompartmentTypesCompartment) {
            self.__next_compartment = Some(next_compartment);
        }

        pub fn step(&mut self) {
            let __e = alloc::rc::Rc::new(RustCompartmentTypesFrameEvent::Step {});
            let mut __ctx = RustCompartmentTypesFrameContext::new(alloc::rc::Rc::clone(&__e), None);
            self._context_stack.push(__ctx);
            self.__kernel(&__e);
            self._context_stack.pop();
        }

        fn _state_VarsOpen(&mut self, __e: &RustCompartmentTypesFrameEvent) {
            match __e {
                RustCompartmentTypesFrameEvent::Step { .. } => { self._s_VarsOpen_hdl_user_step(__e); }
                _ => {}
            }
        }

        fn _state_VarsLoop(&mut self, __e: &RustCompartmentTypesFrameEvent) {
            match __e {
                RustCompartmentTypesFrameEvent::Step { .. } => { self._s_VarsLoop_hdl_user_step(__e); }
                _ => {}
            }
        }

        fn _state_VarsClose(&mut self, __e: &RustCompartmentTypesFrameEvent) {
            match __e {
                RustCompartmentTypesFrameEvent::Step { .. } => { self._s_VarsClose_hdl_user_step(__e); }
                _ => {}
            }
        }

        fn _state_ArgsOpen(&mut self, __e: &RustCompartmentTypesFrameEvent) {
            match __e {
                RustCompartmentTypesFrameEvent::Step { .. } => { self._s_ArgsOpen_hdl_user_step(__e); }
                _ => {}
            }
        }

        fn _state_ArgsLoop(&mut self, __e: &RustCompartmentTypesFrameEvent) {
            match __e {
                RustCompartmentTypesFrameEvent::Step { .. } => { self._s_ArgsLoop_hdl_user_step(__e); }
                _ => {}
            }
        }

        fn _state_ArgsClose(&mut self, __e: &RustCompartmentTypesFrameEvent) {
            match __e {
                RustCompartmentTypesFrameEvent::Step { .. } => { self._s_ArgsClose_hdl_user_step(__e); }
                _ => {}
            }
        }

        fn _state_Comp(&mut self, __e: &RustCompartmentTypesFrameEvent) {
            match __e {
                RustCompartmentTypesFrameEvent::Step { .. } => { self._s_Comp_hdl_user_step(__e); }
                _ => {}
            }
        }

        fn _state_Done(&mut self, __e: &RustCompartmentTypesFrameEvent) {
            match __e {
                _ => {}
            }
        }

        fn _s_VarsOpen_hdl_user_step(&mut self, __e: &RustCompartmentTypesFrameEvent) {
            ct_vars_open(self.sym, &mut self.out);
            let mut __compartment = self.__prepareEnter("VarsLoop");
            self.__transition(__compartment);
            return;
        }

        fn _s_VarsLoop_hdl_user_step(&mut self, __e: &RustCompartmentTypesFrameEvent) {
            if self.vi >= self.ns {
                let mut __compartment = self.__prepareEnter("VarsClose");
                self.__transition(__compartment);
                return;
            }
            ct_vars_variant(self.sym, self.vi, &mut self.out);
            self.vi = self.vi + 1;
            let mut __compartment = self.__prepareEnter("VarsLoop");
            self.__transition(__compartment);
            return;
        }

        fn _s_VarsClose_hdl_user_step(&mut self, __e: &RustCompartmentTypesFrameEvent) {
            ct_close(&mut self.out);
            let mut __compartment = self.__prepareEnter("ArgsOpen");
            self.__transition(__compartment);
            return;
        }

        fn _s_ArgsOpen_hdl_user_step(&mut self, __e: &RustCompartmentTypesFrameEvent) {
            ct_args_open(self.sym, &mut self.out);
            let mut __compartment = self.__prepareEnter("ArgsLoop");
            self.__transition(__compartment);
            return;
        }

        fn _s_ArgsLoop_hdl_user_step(&mut self, __e: &RustCompartmentTypesFrameEvent) {
            if self.ai >= self.ns {
                let mut __compartment = self.__prepareEnter("ArgsClose");
                self.__transition(__compartment);
                return;
            }
            ct_args_variant(self.sym, self.ai, &mut self.out);
            self.ai = self.ai + 1;
            let mut __compartment = self.__prepareEnter("ArgsLoop");
            self.__transition(__compartment);
            return;
        }

        fn _s_ArgsClose_hdl_user_step(&mut self, __e: &RustCompartmentTypesFrameEvent) {
            ct_close(&mut self.out);
            let mut __compartment = self.__prepareEnter("Comp");
            self.__transition(__compartment);
            return;
        }

        fn _s_Comp_hdl_user_step(&mut self, __e: &RustCompartmentTypesFrameEvent) {
            ct_comp(self.sym, &mut self.out);
            let mut __compartment = self.__prepareEnter("Done");
            self.__transition(__compartment);
            return;
        }
    }
}
pub use _rust_compartment_types_framec::*;
