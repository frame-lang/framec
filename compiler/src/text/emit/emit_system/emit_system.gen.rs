use std::collections::HashMap;
use std::any::Any;


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

#[derive(Clone)]
enum EmitSystemVars {
    Interface {  },
    Dispatch {  },
    Handlers {  },
    Actions {  },
    Persist {  },
    Done {  },
}
#[derive(Clone)]
enum EmitSystemArgs {
    Interface {  },
    Dispatch {  },
    Handlers {  },
    Actions {  },
    Persist {  },
    Done {  },
}
#[derive(Clone)]
struct EmitSystemComp {
    state: String,
    vars: EmitSystemVars,
    args: EmitSystemArgs,
}

pub struct EmitSystem<'a> {
    compartment: EmitSystemComp,
    stack: Vec<EmitSystemComp>,
    pub src: &'a Source,
    pub syms: &'a SymbolTable,
    pub sym: &'a SystemSym,
    pub sections: &'a [Section],
    pub be: &'a dyn Backend,
    pub manifest: PersistManifest,
    pub out: Sink,
}

impl<'a> EmitSystem<'a> {
    pub fn new(src: &'a Source, syms: &'a SymbolTable, sym: &'a SystemSym, sections: &'a [Section], be: &'a dyn Backend, manifest: PersistManifest, out: Sink) -> EmitSystem<'a> {
        let compartment = EmitSystemComp { state: "Interface".to_string(), vars: EmitSystemVars::Interface {  }, args: EmitSystemArgs::Interface {  } };
        EmitSystem { compartment, stack: Vec::new(), src: src, syms: syms, sym: sym, sections: sections, be: be, manifest: manifest, out: out }
    }

    pub fn step(&mut self) {
        match self.compartment.state.as_str() {
            "Interface" => self.Interface_step(),
            "Dispatch" => self.Dispatch_step(),
            "Handlers" => self.Handlers_step(),
            "Actions" => self.Actions_step(),
            "Persist" => self.Persist_step(),
            _ => {}
        }
    }

    fn Interface_step(&mut self) {
        emit_iface_phase(self.sym, self.be, &mut self.out);
        let mut __next = EmitSystemComp { state: "Dispatch".to_string(), vars: EmitSystemVars::Dispatch {  }, args: EmitSystemArgs::Dispatch { } };
        self.compartment = __next;
        return Default::default();
    }

    fn Dispatch_step(&mut self) {
        emit_dispatch_phase(self.sym, self.be, &mut self.out);
        let mut __next = EmitSystemComp { state: "Handlers".to_string(), vars: EmitSystemVars::Handlers {  }, args: EmitSystemArgs::Handlers { } };
        self.compartment = __next;
        return Default::default();
    }

    fn Handlers_step(&mut self) {
        emit_handlers_phase(self.src, self.syms, self.sym, self.sections, self.be, &mut self.out);
        let mut __next = EmitSystemComp { state: "Actions".to_string(), vars: EmitSystemVars::Actions {  }, args: EmitSystemArgs::Actions { } };
        self.compartment = __next;
        return Default::default();
    }

    fn Actions_step(&mut self) {
        emit_actions_phase(self.src, self.syms, self.sym, self.sections, self.be, &mut self.out);
        let mut __next = EmitSystemComp { state: "Persist".to_string(), vars: EmitSystemVars::Persist {  }, args: EmitSystemArgs::Persist { } };
        self.compartment = __next;
        return Default::default();
    }

    fn Persist_step(&mut self) {
        let en = manifest_enabled(&self.manifest);
        if en == false {
            let mut __next = EmitSystemComp { state: "Done".to_string(), vars: EmitSystemVars::Done {  }, args: EmitSystemArgs::Done { } };
            self.compartment = __next;
            return Default::default();
        }
        emit_persist(self.be, &self.manifest, &mut self.out);
        let mut __next = EmitSystemComp { state: "Done".to_string(), vars: EmitSystemVars::Done {  }, args: EmitSystemArgs::Done { } };
        self.compartment = __next;
        return Default::default();
    }

}

