
// The handler/action BODY statement walk, dogfooded as a plain `@@system` TRANSDUCER — the
// emit-side analogue of the back-half graph walkers (Reachability, HsmCycle), and the first
// machine to ride the READ-ONLY BORROWED DOMAIN (the plain-`@@system` twin of a scanner's
// `&'a [u8]`): its input — the statement slice, the source, the symbol table, the current
// system/state, and the `&dyn Backend` — are all SHARED BORROWS threaded through one lifetime
// `'a`; its OWNED domain is the accumulating output `out`, the cursor `i`, and the one-bit
// `terminated` latch. It reifies `emit_body`.
//
// This is a genuine Mealy transducer: it consumes the statements in order, emits target text for
// each (through the backend's SPELLINGS, unchanged, in native leaves), and carries ONE bit of
// state — `terminated` — set when a BASE-NESTING terminal (`depth == 0 && rel == 0`
// transition / stack-push / pop / `@@:return`) fires. That bit is read back two ways, exactly as
// the hand walk read it: it HALTS the walk (`-> $Done`, so nothing after a base-nesting terminal
// is spelled — the dead code the old compiler stripped from text it had already emitted), and it
// selects the body's terminal (`Terminated` vs `Fell`) for the wrapper.
//
// WHICH KIND OF BODY this is rides the domain as `role: BodyRole` — Handler or Action. It is a
// TAG framec put on the tree (the body came out of a state's HandlerNode or out of an
// `actions:`/`operations:` Decl), not a sentinel decoded from `state == ""`, and it reaches
// exactly one arm: `@@:(expr)` parks its value on the live FrameContext in a HANDLER and spells
// the target's own `return` in an ACTION, which has no context because the user may call it
// directly.
//
// WHAT COUNTS AS TERMINAL is the backend's answer, not the walk's: a statement only ends the body
// if that target's SPELLING of it actually returns. `@@:(expr)` returns on Java/Rust/C and does
// NOT on Python (where it assigns the context's return slot and execution continues), so
// `emit_return_call` asks `Backend::return_call_terminates` before latching. Calling it terminal
// on a target that keeps running would DELETE LIVE CODE — the statements after it are reachable.
//
// framec owns the WALK (the cursor, the terminated latch, the halt); the 10-way Stmt DISPATCH is
// a per-item function surfaced here as a `kind`-keyed branch, and each arm's leaf holds the EXACT
// byte-for-byte spelling sequence of its `emit_body` match arm (Transition's exit->build->enter->
// return lifecycle via `has_lifecycle` guards, StackPush/StackPop/StackPopBare/Forward, the
// Lowering-backed Native/Assign/ReturnCall). `kind_at` returns -1 at end-of-slice (the loop
// bound), 0=Trivia, 1=Native, 2=Transition, 3=StackPush, 4=StackPopBare, 5=StackPop, 6=Assign,
// 7=ReturnCall, 8=SelfCall, 9=Forward — the hand match order. The wrapper drives `step()` a
// bounded number of times and reads `out` + `terminated`.
//
// Regen: framec-ng -l rust --emit stmt_walk.frs | grep -v '^#!\[allow' > stmt_walk.gen.rs

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
mod _stmt_walk_framec {
    use super::*;
    extern crate alloc;
    use alloc::{vec, format};
    #[derive(Clone)]
    #[allow(dead_code, non_camel_case_types)]
    enum StmtWalkFrameEvent {
        Step {  },
        FrameEnter {},
        FrameExit {},
    }

    #[derive(Clone)]
    #[allow(dead_code, non_camel_case_types)]
    enum StmtWalkFrameReturn {
        _Lifecycle(alloc::rc::Rc<dyn core::any::Any>),
    }

    #[allow(dead_code)]
    impl StmtWalkFrameEvent {
        fn name(&self) -> &'static str {
            match self {
                StmtWalkFrameEvent::Step { .. } => "step",
                StmtWalkFrameEvent::FrameEnter { .. } => "$>",
                StmtWalkFrameEvent::FrameExit { .. } => "<$",
            }
        }
    }

    #[derive(Clone, Debug)]
    #[allow(dead_code, non_camel_case_types)]
    enum StmtWalkFrameValue {
        Int(i64),
        Float(f64),
        Bool(bool),
        Str(String),
        List(Vec<Self>),
        Dict(alloc::collections::BTreeMap<String, Self>),
    }

    #[allow(dead_code, non_camel_case_types)]
    struct StmtWalkFrameContext {
        event: alloc::rc::Rc<StmtWalkFrameEvent>,
        _return: Option<StmtWalkFrameReturn>,
        _data: alloc::collections::BTreeMap<String, StmtWalkFrameValue>,
        _transitioned: bool,
    }

    impl StmtWalkFrameContext {
        fn new(event: alloc::rc::Rc<StmtWalkFrameEvent>, default_return: Option<StmtWalkFrameReturn>) -> Self {
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
    enum StmtWalkStateContext {
        Walk,
        Done,
        __NoContext,
    }

    impl Default for StmtWalkStateContext {
        fn default() -> Self {
            StmtWalkStateContext::Walk
        }
    }

    #[allow(dead_code, non_camel_case_types)]
    #[derive(Clone)]
    struct StmtWalkCompartment {
        state: String,
        state_context: StmtWalkStateContext,
        forward_event: Option<StmtWalkFrameEvent>,
        parent_compartment: Option<Box<StmtWalkCompartment>>,
    }

    impl StmtWalkCompartment {
        fn new(state: &str) -> Self {
            let state_context = match state {
                "Walk" => StmtWalkStateContext::Walk,
                "Done" => StmtWalkStateContext::Done,
                _ => StmtWalkStateContext::__NoContext,
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
    pub struct StmtWalk<'a> {
        _state_stack: Vec<StmtWalkCompartment>,
        __compartment: StmtWalkCompartment,
        __next_compartment: Option<StmtWalkCompartment>,
        _context_stack: Vec<StmtWalkFrameContext>,
        pub src: &'a Source,
        pub syms: &'a SymbolTable,
        pub sym: &'a SystemSym,
        pub role: BodyRole,
        pub stmts: &'a [Stmt],
        pub state: &'a str,
        pub event: &'a str,
        pub is_async: bool,
        pub base: u32,
        pub be: &'a dyn Backend,
        pub out: Sink,
        pub terminated: bool,
        pub i: usize,
    }

    #[allow(non_snake_case)]
    impl<'a> StmtWalk<'a> {
        pub fn new(src: &'a Source, syms: &'a SymbolTable, sym: &'a SystemSym, role: BodyRole, stmts: &'a [Stmt], state: &'a str, event: &'a str, is_async: bool, base: u32, be: &'a dyn Backend, out: Sink) -> Self {
            Self {
                _state_stack: Vec::new(),
                _context_stack: Vec::new(),
                src: src,
                syms: syms,
                sym: sym,
                role: role,
                stmts: stmts,
                state: state,
                event: event,
                is_async: is_async,
                base: base,
                be: be,
                out: out,
                terminated: false,
                i: 0,
                __compartment: StmtWalkCompartment::new("Walk"),
                __next_compartment: None,
            }
        }

        pub fn __create(src: &'a Source, syms: &'a SymbolTable, sym: &'a SystemSym, role: BodyRole, stmts: &'a [Stmt], state: &'a str, event: &'a str, is_async: bool, base: u32, be: &'a dyn Backend, out: Sink) -> Self {
            let mut c = Self::new(src, syms, sym, role, stmts, state, event, is_async, base, be, out);
            c.__compartment = c.__prepareEnter("Walk");
            let __e = alloc::rc::Rc::new(StmtWalkFrameEvent::FrameEnter {});
            let __ctx = StmtWalkFrameContext::new(alloc::rc::Rc::clone(&__e), None);
            c._context_stack.push(__ctx);
            c.__kernel(&__e);
            c._context_stack.pop();
            c
        }

        fn __hsm_chain(&mut self, leaf: &str) -> &'static [&'static str] {
            match leaf {
                "Walk" => &["Walk"],
                "Done" => &["Done"],
                _ => &[],
            }
        }

        fn __prepareEnter(&mut self, leaf: &str) -> StmtWalkCompartment {
            let chain = self.__hsm_chain(leaf);
            let mut comp: Option<StmtWalkCompartment> = None;
            for name in chain.iter() {
                let mut new_comp = StmtWalkCompartment::new(name);
                if let Some(parent) = comp.take() {
                    new_comp.parent_compartment = Some(Box::new(parent));
                }
                comp = Some(new_comp);
            }
            comp.expect("chain must contain at least the leaf state")
        }

        fn __kernel(&mut self, __e: &alloc::rc::Rc<StmtWalkFrameEvent>) {
            // Route event to current state.
            self.__router(__e);
            // Drain any transitions queued by the handler.
            while self.__next_compartment.is_some() {
                let next_compartment = self.__next_compartment.take().expect("invariant: while-loop guard checked is_some()");
                // Exit the current (leaf) state. RFC-0025.1: exit args live in the
                // source state's typed ctx (written at the transition site), so the
                // synthesized `<$` event carries no payload.
                let exit_event = alloc::rc::Rc::new(StmtWalkFrameEvent::FrameExit {});
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
                        let enter_event = alloc::rc::Rc::new(StmtWalkFrameEvent::FrameEnter {});
                        self.__router(&enter_event);
                    }
                    Some(fwd) if matches!(fwd, StmtWalkFrameEvent::FrameEnter { .. }) => {
                        // Forwarded event IS $> — dispatch directly so the
                        // destination's $> handler receives the caller's payload.
                        let fwd_rc = alloc::rc::Rc::new(fwd);
                        self.__router(&fwd_rc);
                    }
                    Some(fwd) => {
                        // Forwarded event is not $> — initialize the destination
                        // with a fresh $>, then dispatch the forward.
                        let enter_event = alloc::rc::Rc::new(StmtWalkFrameEvent::FrameEnter {});
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

        fn __router(&mut self, __e: &alloc::rc::Rc<StmtWalkFrameEvent>) {
            let __ev: &StmtWalkFrameEvent = __e;
            match self.__compartment.state.as_str() {
                "Walk" => self._state_Walk(__ev),
                "Done" => self._state_Done(__ev),
                _ => {}
            }
        }

        fn __transition(&mut self, next_compartment: StmtWalkCompartment) {
            self.__next_compartment = Some(next_compartment);
        }

        pub fn step(&mut self) {
            let __e = alloc::rc::Rc::new(StmtWalkFrameEvent::Step {});
            let mut __ctx = StmtWalkFrameContext::new(alloc::rc::Rc::clone(&__e), None);
            self._context_stack.push(__ctx);
            self.__kernel(&__e);
            self._context_stack.pop();
        }

        fn _state_Walk(&mut self, __e: &StmtWalkFrameEvent) {
            match __e {
                StmtWalkFrameEvent::Step { .. } => { self._s_Walk_hdl_user_step(__e); }
                _ => {}
            }
        }

        fn _state_Done(&mut self, __e: &StmtWalkFrameEvent) {
            match __e {
                _ => {}
            }
        }

        fn _s_Walk_hdl_user_step(&mut self, __e: &StmtWalkFrameEvent) {
            let k = kind_at(self.stmts, self.i);
            if k < 0 {
                let mut __compartment = self.__prepareEnter("Done");
                self.__transition(__compartment);
                return;
            }
            if k == 1 {
                emit_native(self.src, self.syms, self.sym, self.state, self.be, self.base, self.stmts, self.i, &mut self.out);
                self.i = self.i + 1;
                let mut __compartment = self.__prepareEnter("Walk");
                self.__transition(__compartment);
                return;
            }
            if k == 2 {
                let term = emit_transition(self.src, self.syms, self.sym, self.state, self.be, self.base, self.stmts, self.i, &mut self.out);
                if term {
                    self.terminated = true;
                    let mut __compartment = self.__prepareEnter("Done");
                    self.__transition(__compartment);
                    return;
                }
                self.i = self.i + 1;
                let mut __compartment = self.__prepareEnter("Walk");
                self.__transition(__compartment);
                return;
            }
            if k == 3 {
                let term = emit_stack_push(self.src, self.syms, self.sym, self.state, self.be, self.base, self.stmts, self.i, &mut self.out);
                if term {
                    self.terminated = true;
                    let mut __compartment = self.__prepareEnter("Done");
                    self.__transition(__compartment);
                    return;
                }
                self.i = self.i + 1;
                let mut __compartment = self.__prepareEnter("Walk");
                self.__transition(__compartment);
                return;
            }
            if k == 4 {
                emit_stack_pop_bare(self.be, self.base, self.stmts, self.i, &mut self.out);
                self.i = self.i + 1;
                let mut __compartment = self.__prepareEnter("Walk");
                self.__transition(__compartment);
                return;
            }
            if k == 5 {
                let term = emit_stack_pop(self.src, self.syms, self.sym, self.state, self.be, self.base, self.stmts, self.i, &mut self.out);
                if term {
                    self.terminated = true;
                    let mut __compartment = self.__prepareEnter("Done");
                    self.__transition(__compartment);
                    return;
                }
                self.i = self.i + 1;
                let mut __compartment = self.__prepareEnter("Walk");
                self.__transition(__compartment);
                return;
            }
            if k == 6 {
                emit_assign(self.src, self.syms, self.sym, self.state, self.be, self.base, self.stmts, self.i, &mut self.out);
                self.i = self.i + 1;
                let mut __compartment = self.__prepareEnter("Walk");
                self.__transition(__compartment);
                return;
            }
            if k == 7 {
                let term = emit_return_call(self.src, self.syms, self.sym, self.role, self.state, self.event, self.be, self.base, self.is_async, self.stmts, self.i, &mut self.out);
                if term {
                    self.terminated = true;
                    let mut __compartment = self.__prepareEnter("Done");
                    self.__transition(__compartment);
                    return;
                }
                self.i = self.i + 1;
                let mut __compartment = self.__prepareEnter("Walk");
                self.__transition(__compartment);
                return;
            }
            if k == 8 {
                emit_self_call(self.src, self.syms, self.sym, self.state, self.be, self.base, self.is_async, self.stmts, self.i, &mut self.out);
                self.i = self.i + 1;
                let mut __compartment = self.__prepareEnter("Walk");
                self.__transition(__compartment);
                return;
            }
            if k == 9 {
                emit_forward(self.sym, self.state, self.event, self.be, self.base, self.stmts, self.i, &mut self.out);
                self.i = self.i + 1;
                let mut __compartment = self.__prepareEnter("Walk");
                self.__transition(__compartment);
                return;
            }
            self.i = self.i + 1;
            let mut __compartment = self.__prepareEnter("Walk");
            self.__transition(__compartment);
            return;
        }
    }
}
pub use _stmt_walk_framec::*;
