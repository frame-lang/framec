
// The system CONSTRUCTOR's domain-field initializer walk, dogfooded as a plain `@@system` — the
// emit-side sequencer that reifies the `for f in &sym.domain` loop `open_system` ran inline to seed
// each declared domain field in the generated constructor. It rides the same READ-ONLY BORROWED
// DOMAIN as the six landed emit machines: the system symbol and the `&dyn Backend` are SHARED
// BORROWS threaded through one lifetime `'a`; the OWNED domain is the accumulating output `out` and
// the cursor `i` with its bound `nd`.
//
// A ONE-LEVEL CYCLE, the simplest shape in the family (the `BaseColumn` shape, but materializing
// instead of folding): `$Field` cycles over `sym.domain` (`nd` fields), stamping ONE field's
// initializer per iteration and advancing; at `i >= nd` it halts to `$Done`. No stack (depth 1), no
// bound recomputation, no accumulator to clear.
//
// THE HONEST MACHINE CLASS. This is the §3 DEGENERATE POLE — a pure program-counter walk over the
// ALREADY-RESOLVED symbol table. Nothing forks on input; `i` is a MONOTONE CURSOR, not a
// recognition register (it gates no transition other than the halt, and no later behaviour reads it
// back). Nothing is glossed: there is no hidden mode here to name. Its reify payoff is not
// compression but DOGFOOD UNIFORMITY — the cleanroom emits its own driver as `@@system`s,
// differential-gated byte-for-byte against the preserved `domain_init_hand`.
//
// framec owns the WALK (the cursor, the bound, the halt). The un-Frame-able work is the single
// per-item NATIVE LEAF `stamp_domain_init`, which asks the BACKEND to spell field `i`'s initializer
// (`be.domain_init(sym, i, out)`) — Frame cannot walk a symbol table, and the SPELLING is the
// target's, not the walk's. A target with no constructor-time domain seeding overrides nothing and
// this walk emits nothing for it.
//
// Regen: framec-ng -l rust --emit domain_init_walk.frs | grep -v '^#!\[allow' > domain_init_walk.gen.rs

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
mod _domain_init_walk_framec {
    use super::*;
    extern crate alloc;
    use alloc::{vec, format};
    #[derive(Clone)]
    #[allow(dead_code, non_camel_case_types)]
    enum DomainInitWalkFrameEvent {
        Step {  },
        FrameEnter {},
        FrameExit {},
    }

    #[derive(Clone)]
    #[allow(dead_code, non_camel_case_types)]
    enum DomainInitWalkFrameReturn {
        _Lifecycle(alloc::rc::Rc<dyn core::any::Any>),
    }

    #[allow(dead_code)]
    impl DomainInitWalkFrameEvent {
        fn name(&self) -> &'static str {
            match self {
                DomainInitWalkFrameEvent::Step { .. } => "step",
                DomainInitWalkFrameEvent::FrameEnter { .. } => "$>",
                DomainInitWalkFrameEvent::FrameExit { .. } => "<$",
            }
        }
    }

    #[derive(Clone, Debug)]
    #[allow(dead_code, non_camel_case_types)]
    enum DomainInitWalkFrameValue {
        Int(i64),
        Float(f64),
        Bool(bool),
        Str(String),
        List(Vec<Self>),
        Dict(alloc::collections::BTreeMap<String, Self>),
    }

    #[allow(dead_code, non_camel_case_types)]
    struct DomainInitWalkFrameContext {
        event: alloc::rc::Rc<DomainInitWalkFrameEvent>,
        _return: Option<DomainInitWalkFrameReturn>,
        _data: alloc::collections::BTreeMap<String, DomainInitWalkFrameValue>,
        _transitioned: bool,
    }

    impl DomainInitWalkFrameContext {
        fn new(event: alloc::rc::Rc<DomainInitWalkFrameEvent>, default_return: Option<DomainInitWalkFrameReturn>) -> Self {
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
    enum DomainInitWalkStateContext {
        Field,
        Done,
        __NoContext,
    }

    impl Default for DomainInitWalkStateContext {
        fn default() -> Self {
            DomainInitWalkStateContext::Field
        }
    }

    #[allow(dead_code, non_camel_case_types)]
    #[derive(Clone)]
    struct DomainInitWalkCompartment {
        state: String,
        state_context: DomainInitWalkStateContext,
        forward_event: Option<DomainInitWalkFrameEvent>,
        parent_compartment: Option<Box<DomainInitWalkCompartment>>,
    }

    impl DomainInitWalkCompartment {
        fn new(state: &str) -> Self {
            let state_context = match state {
                "Field" => DomainInitWalkStateContext::Field,
                "Done" => DomainInitWalkStateContext::Done,
                _ => DomainInitWalkStateContext::__NoContext,
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
    pub struct DomainInitWalk<'a> {
        _state_stack: Vec<DomainInitWalkCompartment>,
        __compartment: DomainInitWalkCompartment,
        __next_compartment: Option<DomainInitWalkCompartment>,
        _context_stack: Vec<DomainInitWalkFrameContext>,
        pub sym: &'a SystemSym,
        pub be: &'a dyn Backend,
        pub nd: usize,
        pub out: Sink,
        pub i: usize,
    }

    #[allow(non_snake_case)]
    impl<'a> DomainInitWalk<'a> {
        pub fn new(sym: &'a SystemSym, be: &'a dyn Backend, nd: usize, out: Sink) -> Self {
            Self {
                _state_stack: Vec::new(),
                _context_stack: Vec::new(),
                sym: sym,
                be: be,
                nd: nd,
                out: out,
                i: 0,
                __compartment: DomainInitWalkCompartment::new("Field"),
                __next_compartment: None,
            }
        }

        pub fn __create(sym: &'a SystemSym, be: &'a dyn Backend, nd: usize, out: Sink) -> Self {
            let mut c = Self::new(sym, be, nd, out);
            c.__compartment = c.__prepareEnter("Field");
            let __e = alloc::rc::Rc::new(DomainInitWalkFrameEvent::FrameEnter {});
            let __ctx = DomainInitWalkFrameContext::new(alloc::rc::Rc::clone(&__e), None);
            c._context_stack.push(__ctx);
            c.__kernel(&__e);
            c._context_stack.pop();
            c
        }

        fn __hsm_chain(&mut self, leaf: &str) -> &'static [&'static str] {
            match leaf {
                "Field" => &["Field"],
                "Done" => &["Done"],
                _ => &[],
            }
        }

        fn __prepareEnter(&mut self, leaf: &str) -> DomainInitWalkCompartment {
            let chain = self.__hsm_chain(leaf);
            let mut comp: Option<DomainInitWalkCompartment> = None;
            for name in chain.iter() {
                let mut new_comp = DomainInitWalkCompartment::new(name);
                if let Some(parent) = comp.take() {
                    new_comp.parent_compartment = Some(Box::new(parent));
                }
                comp = Some(new_comp);
            }
            comp.expect("chain must contain at least the leaf state")
        }

        fn __kernel(&mut self, __e: &alloc::rc::Rc<DomainInitWalkFrameEvent>) {
            // Route event to current state.
            self.__router(__e);
            // Drain any transitions queued by the handler.
            while self.__next_compartment.is_some() {
                let next_compartment = self.__next_compartment.take().expect("invariant: while-loop guard checked is_some()");
                // Exit the current (leaf) state. RFC-0025.1: exit args live in the
                // source state's typed ctx (written at the transition site), so the
                // synthesized `<$` event carries no payload.
                let exit_event = alloc::rc::Rc::new(DomainInitWalkFrameEvent::FrameExit {});
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
                        let enter_event = alloc::rc::Rc::new(DomainInitWalkFrameEvent::FrameEnter {});
                        self.__router(&enter_event);
                    }
                    Some(fwd) if matches!(fwd, DomainInitWalkFrameEvent::FrameEnter { .. }) => {
                        // Forwarded event IS $> — dispatch directly so the
                        // destination's $> handler receives the caller's payload.
                        let fwd_rc = alloc::rc::Rc::new(fwd);
                        self.__router(&fwd_rc);
                    }
                    Some(fwd) => {
                        // Forwarded event is not $> — initialize the destination
                        // with a fresh $>, then dispatch the forward.
                        let enter_event = alloc::rc::Rc::new(DomainInitWalkFrameEvent::FrameEnter {});
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

        fn __router(&mut self, __e: &alloc::rc::Rc<DomainInitWalkFrameEvent>) {
            let __ev: &DomainInitWalkFrameEvent = __e;
            match self.__compartment.state.as_str() {
                "Field" => self._state_Field(__ev),
                "Done" => self._state_Done(__ev),
                _ => {}
            }
        }

        fn __transition(&mut self, next_compartment: DomainInitWalkCompartment) {
            self.__next_compartment = Some(next_compartment);
        }

        pub fn step(&mut self) {
            let __e = alloc::rc::Rc::new(DomainInitWalkFrameEvent::Step {});
            let mut __ctx = DomainInitWalkFrameContext::new(alloc::rc::Rc::clone(&__e), None);
            self._context_stack.push(__ctx);
            self.__kernel(&__e);
            self._context_stack.pop();
        }

        fn _state_Field(&mut self, __e: &DomainInitWalkFrameEvent) {
            match __e {
                DomainInitWalkFrameEvent::Step { .. } => { self._s_Field_hdl_user_step(__e); }
                _ => {}
            }
        }

        fn _state_Done(&mut self, __e: &DomainInitWalkFrameEvent) {
            match __e {
                _ => {}
            }
        }

        fn _s_Field_hdl_user_step(&mut self, __e: &DomainInitWalkFrameEvent) {
            if self.i >= self.nd {
                let mut __compartment = self.__prepareEnter("Done");
                self.__transition(__compartment);
                return;
            }
            stamp_domain_init(self.sym, self.be, self.i, &mut self.out);
            self.i = self.i + 1;
            let mut __compartment = self.__prepareEnter("Field");
            self.__transition(__compartment);
            return;
        }
    }
}
pub use _domain_init_walk_framec::*;
