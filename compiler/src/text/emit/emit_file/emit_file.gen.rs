
// The driver's TOP-LEVEL ITEM WALK, dogfooded as a plain `@@system` — the OUTERMOST emit sequencer,
// the body of the public `emit` fn. It reifies `emit`'s file-item loop: the pass that walks
// `ast.items` and, for each, either passes the user's top-level native code through verbatim (the
// "water") or delegates a system to the `EmitSystem` phase spine. It rides the same READ-ONLY
// BORROWED DOMAIN as the six landed emit machines: the source, the file AST, and the symbol table
// are SHARED BORROWS threaded through one lifetime `'a`, alongside the `&dyn Backend`; the OWNED
// domain is the accumulating output `out` and the single walk cursor `i` (bound `n`).
//
// A SINGLE CYCLE STATE — the reachability-style top walk. `$Item` cycles `ast.items`, and on each
// item FORKS structurally:
//   Item::Native  -> render the water (verbatim, minus `@@Sys(...)` islands) via a native leaf; SELF-LOOP.
//   otherwise     -> delegate to the `EmitSystem` phase spine (a System resolves to its symbol and
//                    runs; a Bom/Pragma/Efsm item resolves to nothing and emits nothing, exactly as
//                    the hand loop's `else { continue }` did); back to `$Item`.
// At `i >= n` it halts `-> $Done`. The `file_header` preamble is a NATIVE bookend in the wrapper
// (`walk`), emitted once before the cycle — a backend spelling, not a sub-system, so it stays out of
// the cycle. There is no closer: the wrapper's `out.finish()` is the terminal.
//
// THE HONEST MACHINE CLASS. This is the §3 DEGENERATE POLE — a program-counter walk over the
// ALREADY-PARSED item list, whose only fork is a structural type-dispatch (`Item::Native`? — Frame
// cannot match a Rust enum), NOT input recognition. The cursor `i` carries no recognition register;
// the same item list always walks the same way. Nothing is glossed. Its reify payoff is not a hidden
// mode but DOGFOOD UNIFORMITY: with this machine landed, the ENTIRE emit driver — from the file, down
// through each system's phases, its handlers, its statements, to the base column — runs through
// @@systems, differential-gated byte-for-byte vs the preserved `emit_file_hand`.
//
// framec owns the WALK (the cursor, the bound, the self-loops, the halt). The un-Frame-able work is
// per-item NATIVE LEAVES: `is_native_item` (the structural fork), `emit_native_item` (the water
// render — shared with the oracle via `driver::render_native_item`), and `emit_system_item` (resolve
// the system's symbol and call the landed `EmitSystem` `walk`, unchanged). Every spelling stays
// native and byte-identical; the machine only sequences the walk.
//
// Regen: framec-ng -l rust --emit emit_file.frs | grep -v '^#!\[allow' > emit_file.gen.rs

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
mod _emit_file_framec {
    use super::*;
    extern crate alloc;
    use alloc::{vec, format};
    #[derive(Clone)]
    #[allow(dead_code, non_camel_case_types)]
    enum EmitFileFrameEvent {
        Step {  },
        FrameEnter {},
        FrameExit {},
    }

    #[derive(Clone)]
    #[allow(dead_code, non_camel_case_types)]
    enum EmitFileFrameReturn {
        _Lifecycle(alloc::rc::Rc<dyn core::any::Any>),
    }

    #[allow(dead_code)]
    impl EmitFileFrameEvent {
        fn name(&self) -> &'static str {
            match self {
                EmitFileFrameEvent::Step { .. } => "step",
                EmitFileFrameEvent::FrameEnter { .. } => "$>",
                EmitFileFrameEvent::FrameExit { .. } => "<$",
            }
        }
    }

    #[derive(Clone, Debug)]
    #[allow(dead_code, non_camel_case_types)]
    enum EmitFileFrameValue {
        Int(i64),
        Float(f64),
        Bool(bool),
        Str(String),
        List(Vec<Self>),
        Dict(alloc::collections::BTreeMap<String, Self>),
    }

    #[allow(dead_code, non_camel_case_types)]
    struct EmitFileFrameContext {
        event: alloc::rc::Rc<EmitFileFrameEvent>,
        _return: Option<EmitFileFrameReturn>,
        _data: alloc::collections::BTreeMap<String, EmitFileFrameValue>,
        _transitioned: bool,
    }

    impl EmitFileFrameContext {
        fn new(event: alloc::rc::Rc<EmitFileFrameEvent>, default_return: Option<EmitFileFrameReturn>) -> Self {
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
    enum EmitFileStateContext {
        Item,
        Done,
        __NoContext,
    }

    impl Default for EmitFileStateContext {
        fn default() -> Self {
            EmitFileStateContext::Item
        }
    }

    #[allow(dead_code, non_camel_case_types)]
    #[derive(Clone)]
    struct EmitFileCompartment {
        state: String,
        state_context: EmitFileStateContext,
        forward_event: Option<EmitFileFrameEvent>,
        parent_compartment: Option<Box<EmitFileCompartment>>,
    }

    impl EmitFileCompartment {
        fn new(state: &str) -> Self {
            let state_context = match state {
                "Item" => EmitFileStateContext::Item,
                "Done" => EmitFileStateContext::Done,
                _ => EmitFileStateContext::__NoContext,
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
    pub struct EmitFile<'a> {
        _state_stack: Vec<EmitFileCompartment>,
        __compartment: EmitFileCompartment,
        __next_compartment: Option<EmitFileCompartment>,
        _context_stack: Vec<EmitFileFrameContext>,
        pub src: &'a Source,
        pub ast: &'a FileAst,
        pub syms: &'a SymbolTable,
        pub be: &'a dyn Backend,
        pub n: usize,
        pub out: Sink,
        pub i: usize,
    }

    #[allow(non_snake_case)]
    impl<'a> EmitFile<'a> {
        pub fn new(src: &'a Source, ast: &'a FileAst, syms: &'a SymbolTable, be: &'a dyn Backend, n: usize, out: Sink) -> Self {
            Self {
                _state_stack: Vec::new(),
                _context_stack: Vec::new(),
                src: src,
                ast: ast,
                syms: syms,
                be: be,
                n: n,
                out: out,
                i: 0,
                __compartment: EmitFileCompartment::new("Item"),
                __next_compartment: None,
            }
        }

        pub fn __create(src: &'a Source, ast: &'a FileAst, syms: &'a SymbolTable, be: &'a dyn Backend, n: usize, out: Sink) -> Self {
            let mut c = Self::new(src, ast, syms, be, n, out);
            c.__compartment = c.__prepareEnter("Item");
            let __e = alloc::rc::Rc::new(EmitFileFrameEvent::FrameEnter {});
            let __ctx = EmitFileFrameContext::new(alloc::rc::Rc::clone(&__e), None);
            c._context_stack.push(__ctx);
            c.__kernel(&__e);
            c._context_stack.pop();
            c
        }

        fn __hsm_chain(&mut self, leaf: &str) -> &'static [&'static str] {
            match leaf {
                "Item" => &["Item"],
                "Done" => &["Done"],
                _ => &[],
            }
        }

        fn __prepareEnter(&mut self, leaf: &str) -> EmitFileCompartment {
            let chain = self.__hsm_chain(leaf);
            let mut comp: Option<EmitFileCompartment> = None;
            for name in chain.iter() {
                let mut new_comp = EmitFileCompartment::new(name);
                if let Some(parent) = comp.take() {
                    new_comp.parent_compartment = Some(Box::new(parent));
                }
                comp = Some(new_comp);
            }
            comp.expect("chain must contain at least the leaf state")
        }

        fn __kernel(&mut self, __e: &alloc::rc::Rc<EmitFileFrameEvent>) {
            // Route event to current state.
            self.__router(__e);
            // Drain any transitions queued by the handler.
            while self.__next_compartment.is_some() {
                let next_compartment = self.__next_compartment.take().expect("invariant: while-loop guard checked is_some()");
                // Exit the current (leaf) state. RFC-0025.1: exit args live in the
                // source state's typed ctx (written at the transition site), so the
                // synthesized `<$` event carries no payload.
                let exit_event = alloc::rc::Rc::new(EmitFileFrameEvent::FrameExit {});
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
                        let enter_event = alloc::rc::Rc::new(EmitFileFrameEvent::FrameEnter {});
                        self.__router(&enter_event);
                    }
                    Some(fwd) if matches!(fwd, EmitFileFrameEvent::FrameEnter { .. }) => {
                        // Forwarded event IS $> — dispatch directly so the
                        // destination's $> handler receives the caller's payload.
                        let fwd_rc = alloc::rc::Rc::new(fwd);
                        self.__router(&fwd_rc);
                    }
                    Some(fwd) => {
                        // Forwarded event is not $> — initialize the destination
                        // with a fresh $>, then dispatch the forward.
                        let enter_event = alloc::rc::Rc::new(EmitFileFrameEvent::FrameEnter {});
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

        fn __router(&mut self, __e: &alloc::rc::Rc<EmitFileFrameEvent>) {
            let __ev: &EmitFileFrameEvent = __e;
            match self.__compartment.state.as_str() {
                "Item" => self._state_Item(__ev),
                "Done" => self._state_Done(__ev),
                _ => {}
            }
        }

        fn __transition(&mut self, next_compartment: EmitFileCompartment) {
            self.__next_compartment = Some(next_compartment);
        }

        pub fn step(&mut self) {
            let __e = alloc::rc::Rc::new(EmitFileFrameEvent::Step {});
            let mut __ctx = EmitFileFrameContext::new(alloc::rc::Rc::clone(&__e), None);
            self._context_stack.push(__ctx);
            self.__kernel(&__e);
            self._context_stack.pop();
        }

        fn _state_Item(&mut self, __e: &EmitFileFrameEvent) {
            match __e {
                EmitFileFrameEvent::Step { .. } => { self._s_Item_hdl_user_step(__e); }
                _ => {}
            }
        }

        fn _state_Done(&mut self, __e: &EmitFileFrameEvent) {
            match __e {
                _ => {}
            }
        }

        fn _s_Item_hdl_user_step(&mut self, __e: &EmitFileFrameEvent) {
            if self.i >= self.n {
                let mut __compartment = self.__prepareEnter("Done");
                self.__transition(__compartment);
                return;
            }
            let isn = is_native_item(self.ast, self.i);
            if isn {
                emit_native_item(self.src, self.syms, self.be, self.ast, self.i, &mut self.out);
                self.i = self.i + 1;
                let mut __compartment = self.__prepareEnter("Item");
                self.__transition(__compartment);
                return;
            }
            emit_system_item(self.src, self.syms, self.be, self.ast, self.i, &mut self.out);
            self.i = self.i + 1;
            let mut __compartment = self.__prepareEnter("Item");
            self.__transition(__compartment);
            return;
        }
    }
}
pub use _emit_file_framec::*;
