use std::collections::HashMap;
use std::any::Any;


// The driver's HANDLER-EMISSION walk, dogfooded as a plain `@@system` — the emit-side sequencer
// that reifies `emit`'s `(section, state, handler)` nested pass (the private per-handler methods).
// It rides the same READ-ONLY BORROWED DOMAIN as StmtWalk/BaseColumn: the section slice, the
// source, the symbol table, the system symbol, and the `&dyn Backend` are SHARED BORROWS threaded
// through one lifetime `'a`; the OWNED domain is the accumulating output `out`, the three walk
// cursors (`si`/`sti`/`hi`), and their bounds (`nsec`/`nst`/`nh`).
//
// THE 3-LEVEL NESTING, EXPRESSED WITHOUT push$/pop$. The pass is a FIXED depth-3 walk — sections,
// then that section's states, then that state's handlers — so a stack is unnecessary (a stack buys
// UNBOUNDED depth; this depth is 3 and known). It is expressed instead as three NESTED CYCLE
// STATES with explicit up/down edges, one owned cursor per level:
//   $Section  cycles over `sections` (fork: only `Section::Machine` descends); on a machine
//             section it sets the state bound `nst`, resets `sti`, and descends `-> $State`; at
//             `si >= nsec` it halts `-> $Done`.
//   $State    cycles over the current section's `members` (fork: only `MachineMember::State`
//             descends); on a state it sets the handler bound `nh`, resets `hi`, and descends
//             `-> $Handler`; at `sti >= nst` it ASCENDS (`si += 1`, `-> $Section`).
//   $Handler  cycles over the current state's `members` (fork: only `StateMember::Handler`
//             emits); on a handler it emits one private method; at `hi >= nh` it ASCENDS
//             (`sti += 1`, `-> $State`).
// The "mode" is the walk DEPTH (which of the three cycle states is live); the cursors advance it.
// This is the §3 degenerate pole — a program-counter walk over ALREADY-PARSED tree data, whose
// forks are structural type-dispatch (`Section::Machine`? `MachineMember::State`? …), not input
// recognition. It carries no recognition register; nothing is glossed. Its reify payoff is not a
// hidden mode but DOGFOOD UNIFORMITY (the maximal-rebuild campaign: the cleanroom emits its own
// driver as an @@system, differential-gated byte-for-byte vs the preserved `emit_handlers_hand`).
//
// framec owns the WALK (the three cursors, the bounds, the descents/ascents, the halt). The
// un-Frame-able work is per-item NATIVE LEAVES: the structural forks/bounds (`is_machine_section`,
// `member_count`, `is_state_member`, `state_member_count`, `is_handler_member` — Frame cannot match
// a Rust enum), the two per-handler forks the pass computes (`handler_is_async`, `handler_ret` —
// the is_async disjunction and the return-type-inheritance `or_else`), and `emit_handler`, which
// spells ONE private method: `be.open_handler(...)`, then the StmtWalk body walk (`emit_body`,
// unchanged, called as a leaf — NOT reinlined), then `be.close_handler(...)`. Every materialization
// spelling stays native and byte-identical; the machine only sequences the walk.
//
// Regen: framec-ng -l rust --emit emit_handlers.frs | grep -v '^#!\[allow' > emit_handlers.gen.rs

#[derive(Clone)]
enum EmitHandlersVars {
    Section {  },
    State {  },
    Handler {  },
    Done {  },
}
#[derive(Clone)]
enum EmitHandlersArgs {
    Section {  },
    State {  },
    Handler {  },
    Done {  },
}
#[derive(Clone)]
struct EmitHandlersComp {
    state: String,
    vars: EmitHandlersVars,
    args: EmitHandlersArgs,
}

pub struct EmitHandlers<'a> {
    compartment: EmitHandlersComp,
    stack: Vec<EmitHandlersComp>,
    pub src: &'a Source,
    pub syms: &'a SymbolTable,
    pub sym: &'a SystemSym,
    pub sections: &'a [Section],
    pub be: &'a dyn Backend,
    pub nsec: usize,
    pub out: Sink,
    pub nst: usize,
    pub nh: usize,
    pub si: usize,
    pub sti: usize,
    pub hi: usize,
}

impl<'a> EmitHandlers<'a> {
    pub fn new(src: &'a Source, syms: &'a SymbolTable, sym: &'a SystemSym, sections: &'a [Section], be: &'a dyn Backend, nsec: usize, out: Sink) -> EmitHandlers<'a> {
        let compartment = EmitHandlersComp { state: "Section".to_string(), vars: EmitHandlersVars::Section {  }, args: EmitHandlersArgs::Section {  } };
        EmitHandlers { compartment, stack: Vec::new(), src: src, syms: syms, sym: sym, sections: sections, be: be, nsec: nsec, out: out, nst: 0, nh: 0, si: 0, sti: 0, hi: 0 }
    }

    pub fn step(&mut self) {
        match self.compartment.state.as_str() {
            "Section" => self.Section_step(),
            "State" => self.State_step(),
            "Handler" => self.Handler_step(),
            _ => {}
        }
    }

    fn Section_step(&mut self) {
        if self.si >= self.nsec {
            let mut __next = EmitHandlersComp { state: "Done".to_string(), vars: EmitHandlersVars::Done {  }, args: EmitHandlersArgs::Done { } };
            self.compartment = __next;
            return Default::default();
        }
        let ism = is_machine_section(self.sections, self.si);
        if ism == false {
            self.si = self.si + 1;
            let mut __next = EmitHandlersComp { state: "Section".to_string(), vars: EmitHandlersVars::Section {  }, args: EmitHandlersArgs::Section { } };
            self.compartment = __next;
            return Default::default();
        }
        self.nst = member_count(self.sections, self.si);
        self.sti = 0;
        let mut __next = EmitHandlersComp { state: "State".to_string(), vars: EmitHandlersVars::State {  }, args: EmitHandlersArgs::State { } };
        self.compartment = __next;
        return Default::default();
    }

    fn State_step(&mut self) {
        if self.sti >= self.nst {
            self.si = self.si + 1;
            let mut __next = EmitHandlersComp { state: "Section".to_string(), vars: EmitHandlersVars::Section {  }, args: EmitHandlersArgs::Section { } };
            self.compartment = __next;
            return Default::default();
        }
        let iss = is_state_member(self.sections, self.si, self.sti);
        if iss == false {
            self.sti = self.sti + 1;
            let mut __next = EmitHandlersComp { state: "State".to_string(), vars: EmitHandlersVars::State {  }, args: EmitHandlersArgs::State { } };
            self.compartment = __next;
            return Default::default();
        }
        self.nh = state_member_count(self.sections, self.si, self.sti);
        self.hi = 0;
        let mut __next = EmitHandlersComp { state: "Handler".to_string(), vars: EmitHandlersVars::Handler {  }, args: EmitHandlersArgs::Handler { } };
        self.compartment = __next;
        return Default::default();
    }

    fn Handler_step(&mut self) {
        if self.hi >= self.nh {
            self.sti = self.sti + 1;
            let mut __next = EmitHandlersComp { state: "State".to_string(), vars: EmitHandlersVars::State {  }, args: EmitHandlersArgs::State { } };
            self.compartment = __next;
            return Default::default();
        }
        let ish = is_handler_member(self.sections, self.si, self.sti, self.hi);
        if ish == false {
            self.hi = self.hi + 1;
            let mut __next = EmitHandlersComp { state: "Handler".to_string(), vars: EmitHandlersVars::Handler {  }, args: EmitHandlersArgs::Handler { } };
            self.compartment = __next;
            return Default::default();
        }
        let is_async = handler_is_async(self.sym, self.sections, self.si, self.sti, self.hi, self.be);
        let ret = handler_ret(self.sym, self.sections, self.si, self.sti, self.hi, self.be);
        emit_handler(self.src, self.syms, self.sym, self.be, self.sections, self.si, self.sti, self.hi, is_async, ret, &mut self.out);
        self.hi = self.hi + 1;
        let mut __next = EmitHandlersComp { state: "Handler".to_string(), vars: EmitHandlersVars::Handler {  }, args: EmitHandlersArgs::Handler { } };
        self.compartment = __next;
        return Default::default();
    }

}

