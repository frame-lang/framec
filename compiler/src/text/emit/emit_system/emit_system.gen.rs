
// The driver's PER-SYSTEM PHASE SPINE, dogfooded as a plain `@@system` — the emit-side sequencer
// that reifies `emit`'s per-system run of passes (the interface router, the private handlers, the
// native-bodied actions/operations, and the `@@[persist]` save/restore). It rides the same
// READ-ONLY BORROWED DOMAIN as the five landed emit machines: the source, the symbol table, the
// system symbol, the section slice, and the `&dyn Backend` are SHARED BORROWS threaded through one
// lifetime `'a`; the OWNED domain is the accumulating output `out` and the derived `manifest` (the
// persist decision, computed once by the wrapper and carried in for the `$Persist` guard).
//
// A LINEAR 5-STATE SPINE (the decl_read mode-spine shape), NOT a cycle. Each phase is one state
// that calls the ALREADY-LANDED sub-system as a leaf and then advances UNCONDITIONALLY to the next
// phase — there is no cursor, no bound, no loop-back:
//   $Interface -> emit_interface::walk (the `(method, arm)` router pass)   -> $Dispatch
//   $Dispatch  -> state_dispatch_walk::walk (the per-state message dispatchers) -> $Handlers
//   $Handlers  -> emit_handlers::walk  (the `(section, state, handler)` pass) -> $Actions
//   $Actions   -> emit_actions::walk   (the `actions:` / `operations:` pass)  -> $Persist
//   $Persist   -> GUARDED: `manifest.enabled` ? `be.persist(&manifest, out)` : nothing  -> $Done
// The `open_system` / `close_system` bookends are NATIVE in the wrapper (`walk`), bracketing the
// spine exactly as the hand pass bracketed the phase run — they are backend spellings, not sub-
// systems, so they stay out of the 4-state spine.
//
// THE HONEST MACHINE CLASS. This is the §3 DEGENERATE POLE — a pure program-counter chain over the
// four phases, carrying no recognition register. The "mode" is only which phase has run; the
// sequence is fixed and history-free (the same system always runs Interface, then Handlers, then
// Actions, then the persist guard). Nothing is glossed: the one fork ($Persist's `manifest.enabled`)
// reads a FROZEN decision the persist derivation already made upstream, not a carried mode. Its
// reify payoff is not a hidden mode but DOGFOOD UNIFORMITY (the maximal-rebuild campaign: the
// cleanroom emits its own driver as an @@system, differential-gated byte-for-byte vs the preserved
// `emit_system_hand`). Calling it a machine is honest only in the Shadows sense that a straight-line
// chain is the trivial machine; the payoff is composition, not compression.
//
// framec owns the SPINE (the five unconditional advances + the persist guard). The un-Frame-able
// work is per-phase NATIVE LEAVES: `emit_iface_phase` / `emit_dispatch_phase` /
// `emit_handlers_phase` / `emit_actions_phase` each call ONE already-landed sub-system's `walk`
// (unchanged, NOT reinlined); `manifest_enabled` reads the persist flag; and `emit_persist` spells
// the one `be.persist(...)` the hand pass ran.
//
// $Dispatch sits BETWEEN $Interface and $Handlers because that is where its output belongs in the
// file: the public wrappers, then the per-state message dispatchers they route through, then the
// private handler methods those dispatchers call. It is unconditional like its neighbours — a
// target whose router calls `(state, event)` methods directly overrides no spelling, so the phase
// runs and emits nothing, and that target's bytes are unchanged.
//
// Regen: framec-ng -l rust --emit emit_system.frs | grep -v '^#!\[allow' > emit_system.gen.rs

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
mod _emit_system_framec {
    use super::*;
    extern crate alloc;
    use alloc::{vec, format};
    #[derive(Clone)]
    #[allow(dead_code, non_camel_case_types)]
    enum EmitSystemFrameEvent {
        Step {  },
        FrameEnter {},
        FrameExit {},
    }

    #[derive(Clone)]
    #[allow(dead_code, non_camel_case_types)]
    enum EmitSystemFrameReturn {
        _Lifecycle(alloc::rc::Rc<dyn core::any::Any>),
    }

    #[allow(dead_code)]
    impl EmitSystemFrameEvent {
        fn name(&self) -> &'static str {
            match self {
                EmitSystemFrameEvent::Step { .. } => "step",
                EmitSystemFrameEvent::FrameEnter { .. } => "$>",
                EmitSystemFrameEvent::FrameExit { .. } => "<$",
            }
        }
    }

    #[derive(Clone, Debug)]
    #[allow(dead_code, non_camel_case_types)]
    enum EmitSystemFrameValue {
        Int(i64),
        Float(f64),
        Bool(bool),
        Str(String),
        List(Vec<Self>),
        Dict(alloc::collections::BTreeMap<String, Self>),
    }

    #[allow(dead_code, non_camel_case_types)]
    struct EmitSystemFrameContext {
        event: alloc::rc::Rc<EmitSystemFrameEvent>,
        _return: Option<EmitSystemFrameReturn>,
        _data: alloc::collections::BTreeMap<String, EmitSystemFrameValue>,
        _transitioned: bool,
    }

    impl EmitSystemFrameContext {
        fn new(event: alloc::rc::Rc<EmitSystemFrameEvent>, default_return: Option<EmitSystemFrameReturn>) -> Self {
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
    enum EmitSystemStateContext {
        Interface,
        Dispatch,
        Handlers,
        Actions,
        Persist,
        Done,
        __NoContext,
    }

    impl Default for EmitSystemStateContext {
        fn default() -> Self {
            EmitSystemStateContext::Interface
        }
    }

    #[allow(dead_code, non_camel_case_types)]
    #[derive(Clone)]
    struct EmitSystemCompartment {
        state: String,
        state_context: EmitSystemStateContext,
        forward_event: Option<EmitSystemFrameEvent>,
        parent_compartment: Option<Box<EmitSystemCompartment>>,
    }

    impl EmitSystemCompartment {
        fn new(state: &str) -> Self {
            let state_context = match state {
                "Interface" => EmitSystemStateContext::Interface,
                "Dispatch" => EmitSystemStateContext::Dispatch,
                "Handlers" => EmitSystemStateContext::Handlers,
                "Actions" => EmitSystemStateContext::Actions,
                "Persist" => EmitSystemStateContext::Persist,
                "Done" => EmitSystemStateContext::Done,
                _ => EmitSystemStateContext::__NoContext,
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
    pub struct EmitSystem<'a> {
        _state_stack: Vec<EmitSystemCompartment>,
        __compartment: EmitSystemCompartment,
        __next_compartment: Option<EmitSystemCompartment>,
        _context_stack: Vec<EmitSystemFrameContext>,
        pub src: &'a Source,
        pub syms: &'a SymbolTable,
        pub sym: &'a SystemSym,
        pub sections: &'a [Section],
        pub be: &'a dyn Backend,
        pub manifest: PersistManifest,
        pub out: Sink,
    }

    #[allow(non_snake_case)]
    impl<'a> EmitSystem<'a> {
        pub fn new(src: &'a Source, syms: &'a SymbolTable, sym: &'a SystemSym, sections: &'a [Section], be: &'a dyn Backend, manifest: PersistManifest, out: Sink) -> Self {
            Self {
                _state_stack: Vec::new(),
                _context_stack: Vec::new(),
                src: src,
                syms: syms,
                sym: sym,
                sections: sections,
                be: be,
                manifest: manifest,
                out: out,
                __compartment: EmitSystemCompartment::new("Interface"),
                __next_compartment: None,
            }
        }

        pub fn __create(src: &'a Source, syms: &'a SymbolTable, sym: &'a SystemSym, sections: &'a [Section], be: &'a dyn Backend, manifest: PersistManifest, out: Sink) -> Self {
            let mut c = Self::new(src, syms, sym, sections, be, manifest, out);
            c.__compartment = c.__prepareEnter("Interface");
            let __e = alloc::rc::Rc::new(EmitSystemFrameEvent::FrameEnter {});
            let __ctx = EmitSystemFrameContext::new(alloc::rc::Rc::clone(&__e), None);
            c._context_stack.push(__ctx);
            c.__kernel(&__e);
            c._context_stack.pop();
            c
        }

        fn __hsm_chain(&mut self, leaf: &str) -> &'static [&'static str] {
            match leaf {
                "Interface" => &["Interface"],
                "Dispatch" => &["Dispatch"],
                "Handlers" => &["Handlers"],
                "Actions" => &["Actions"],
                "Persist" => &["Persist"],
                "Done" => &["Done"],
                _ => &[],
            }
        }

        fn __prepareEnter(&mut self, leaf: &str) -> EmitSystemCompartment {
            let chain = self.__hsm_chain(leaf);
            let mut comp: Option<EmitSystemCompartment> = None;
            for name in chain.iter() {
                let mut new_comp = EmitSystemCompartment::new(name);
                if let Some(parent) = comp.take() {
                    new_comp.parent_compartment = Some(Box::new(parent));
                }
                comp = Some(new_comp);
            }
            comp.expect("chain must contain at least the leaf state")
        }

        fn __kernel(&mut self, __e: &alloc::rc::Rc<EmitSystemFrameEvent>) {
            // Route event to current state.
            self.__router(__e);
            // Drain any transitions queued by the handler.
            while self.__next_compartment.is_some() {
                let next_compartment = self.__next_compartment.take().expect("invariant: while-loop guard checked is_some()");
                // Exit the current (leaf) state. RFC-0025.1: exit args live in the
                // source state's typed ctx (written at the transition site), so the
                // synthesized `<$` event carries no payload.
                let exit_event = alloc::rc::Rc::new(EmitSystemFrameEvent::FrameExit {});
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
                        let enter_event = alloc::rc::Rc::new(EmitSystemFrameEvent::FrameEnter {});
                        self.__router(&enter_event);
                    }
                    Some(fwd) if matches!(fwd, EmitSystemFrameEvent::FrameEnter { .. }) => {
                        // Forwarded event IS $> — dispatch directly so the
                        // destination's $> handler receives the caller's payload.
                        let fwd_rc = alloc::rc::Rc::new(fwd);
                        self.__router(&fwd_rc);
                    }
                    Some(fwd) => {
                        // Forwarded event is not $> — initialize the destination
                        // with a fresh $>, then dispatch the forward.
                        let enter_event = alloc::rc::Rc::new(EmitSystemFrameEvent::FrameEnter {});
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

        fn __router(&mut self, __e: &alloc::rc::Rc<EmitSystemFrameEvent>) {
            let __ev: &EmitSystemFrameEvent = __e;
            match self.__compartment.state.as_str() {
                "Interface" => self._state_Interface(__ev),
                "Dispatch" => self._state_Dispatch(__ev),
                "Handlers" => self._state_Handlers(__ev),
                "Actions" => self._state_Actions(__ev),
                "Persist" => self._state_Persist(__ev),
                "Done" => self._state_Done(__ev),
                _ => {}
            }
        }

        fn __transition(&mut self, next_compartment: EmitSystemCompartment) {
            self.__next_compartment = Some(next_compartment);
        }

        pub fn step(&mut self) {
            let __e = alloc::rc::Rc::new(EmitSystemFrameEvent::Step {});
            let mut __ctx = EmitSystemFrameContext::new(alloc::rc::Rc::clone(&__e), None);
            self._context_stack.push(__ctx);
            self.__kernel(&__e);
            self._context_stack.pop();
        }

        fn _state_Interface(&mut self, __e: &EmitSystemFrameEvent) {
            match __e {
                EmitSystemFrameEvent::Step { .. } => { self._s_Interface_hdl_user_step(__e); }
                _ => {}
            }
        }

        fn _state_Dispatch(&mut self, __e: &EmitSystemFrameEvent) {
            match __e {
                EmitSystemFrameEvent::Step { .. } => { self._s_Dispatch_hdl_user_step(__e); }
                _ => {}
            }
        }

        fn _state_Handlers(&mut self, __e: &EmitSystemFrameEvent) {
            match __e {
                EmitSystemFrameEvent::Step { .. } => { self._s_Handlers_hdl_user_step(__e); }
                _ => {}
            }
        }

        fn _state_Actions(&mut self, __e: &EmitSystemFrameEvent) {
            match __e {
                EmitSystemFrameEvent::Step { .. } => { self._s_Actions_hdl_user_step(__e); }
                _ => {}
            }
        }

        fn _state_Persist(&mut self, __e: &EmitSystemFrameEvent) {
            match __e {
                EmitSystemFrameEvent::Step { .. } => { self._s_Persist_hdl_user_step(__e); }
                _ => {}
            }
        }

        fn _state_Done(&mut self, __e: &EmitSystemFrameEvent) {
            match __e {
                _ => {}
            }
        }

        fn _s_Interface_hdl_user_step(&mut self, __e: &EmitSystemFrameEvent) {
            emit_iface_phase(self.sym, self.be, &mut self.out);
            let mut __compartment = self.__prepareEnter("Dispatch");
            self.__transition(__compartment);
            return;
        }

        fn _s_Dispatch_hdl_user_step(&mut self, __e: &EmitSystemFrameEvent) {
            emit_dispatch_phase(self.sym, self.be, &mut self.out);
            let mut __compartment = self.__prepareEnter("Handlers");
            self.__transition(__compartment);
            return;
        }

        fn _s_Handlers_hdl_user_step(&mut self, __e: &EmitSystemFrameEvent) {
            emit_handlers_phase(self.src, self.syms, self.sym, self.sections, self.be, &mut self.out);
            let mut __compartment = self.__prepareEnter("Actions");
            self.__transition(__compartment);
            return;
        }

        fn _s_Actions_hdl_user_step(&mut self, __e: &EmitSystemFrameEvent) {
            emit_actions_phase(self.src, self.syms, self.sym, self.sections, self.be, &mut self.out);
            let mut __compartment = self.__prepareEnter("Persist");
            self.__transition(__compartment);
            return;
        }

        fn _s_Persist_hdl_user_step(&mut self, __e: &EmitSystemFrameEvent) {
            let en = manifest_enabled(&self.manifest);
            if en == false {
                let mut __compartment = self.__prepareEnter("Done");
                self.__transition(__compartment);
                return;
            }
            emit_persist(self.be, &self.manifest, &mut self.out);
            let mut __compartment = self.__prepareEnter("Done");
            self.__transition(__compartment);
            return;
        }
    }
}
pub use _emit_system_framec::*;
