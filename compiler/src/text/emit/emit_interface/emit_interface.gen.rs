use std::collections::HashMap;
use std::any::Any;


// The driver's INTERFACE/ROUTER walk, dogfooded as a plain `@@system` — the emit-side sequencer
// that reifies `emit`'s per-event router pass (the PUBLIC method per interface event that dispatches
// to the private handler methods). It rides the same READ-ONLY BORROWED DOMAIN as
// StmtWalk/BaseColumn/EmitHandlers: the system symbol and the `&dyn Backend` are SHARED BORROWS
// threaded through one lifetime `'a`; the OWNED domain is the accumulating output `out`, the two
// walk cursors (`mi`/`ai`) and their bounds (`ni`/`na`), plus the per-method arm accumulator `arms`.
//
// THE 2-LEVEL NESTING, EXPRESSED WITHOUT push$/pop$. The pass is a FIXED depth-2 walk — interface
// methods, then, per method, the machine's states (to resolve which state's handler runs) — so a
// stack is unnecessary (a stack buys UNBOUNDED depth; this depth is 2 and known). It is expressed
// instead as two NESTED CYCLE STATES with explicit up/down edges, one owned cursor per level:
//   $Method  cycles over `sym.interface` (`ni` methods); on a method it sets the arm bound `na`
//            (= state count), resets `ai`, CLEARS the arm accumulator, and descends `-> $Arm`; at
//            `mi >= ni` it halts `-> $Done`.
//   $Arm     cycles over `sym.states` (`na` states) STAMPING one `(state, owner)` arm per state
//            for which `resolve_handler(state, method)` is `Some` (HSM dispatch, resolved from the
//            symbol table); at `ai >= na` it computes the method's `is_async` and ROUTES — emits the
//            one public method via `be.route(...)` — then ASCENDS (`mi += 1`, `-> $Method`).
// The "mode" is the walk DEPTH (which of the two cycle states is live); the cursors advance it.
// This is the §3 degenerate pole — a program-counter walk over the ALREADY-RESOLVED symbol table,
// whose only fork is a structural table lookup (`resolve_handler` Some/None), not input recognition.
// `arms` is not a recognition register — it gates no transition; it is MATERIALIZATION being built,
// like `out`. `is_async` is a write-once/read-once local, not a carried mode. So this carries no
// recognition register; nothing is glossed. Its reify payoff is not a hidden mode but DOGFOOD
// UNIFORMITY (the maximal-rebuild campaign: the cleanroom emits its own driver as an @@system,
// differential-gated byte-for-byte vs the preserved `emit_interface_hand`).
//
// framec owns the WALK (the two cursors, the bounds, the descents/ascents, the halt, the per-method
// arm-accumulator reset). The un-Frame-able work is per-item NATIVE LEAVES: `state_count` (the arm
// bound), `stamp_arm` (the `resolve_handler` lookup + arm push — Frame cannot walk a symbol table),
// `clear_arms` (reset the accumulator per method), `method_is_async` (the `m.is_async || sym.is_async`
// disjunction), and `route_method`, which spells ONE public method: the verbatim `be.route(...)` the
// hand pass ran. Every materialization spelling stays native and byte-identical; the machine only
// sequences the walk.
//
// Regen: framec-ng -l rust --emit emit_interface.frs | grep -v '^#!\[allow' > emit_interface.gen.rs

#[derive(Clone)]
enum EmitInterfaceVars {
    Method {  },
    Arm {  },
    Done {  },
}
#[derive(Clone)]
enum EmitInterfaceArgs {
    Method {  },
    Arm {  },
    Done {  },
}
#[derive(Clone)]
struct EmitInterfaceComp {
    state: String,
    vars: EmitInterfaceVars,
    args: EmitInterfaceArgs,
}

pub struct EmitInterface<'a> {
    compartment: EmitInterfaceComp,
    stack: Vec<EmitInterfaceComp>,
    pub sym: &'a SystemSym,
    pub be: &'a dyn Backend,
    pub ni: usize,
    pub arms: ArmVec,
    pub out: Sink,
    pub na: usize,
    pub mi: usize,
    pub ai: usize,
}

impl<'a> EmitInterface<'a> {
    pub fn new(sym: &'a SystemSym, be: &'a dyn Backend, ni: usize, arms: ArmVec, out: Sink) -> EmitInterface<'a> {
        let compartment = EmitInterfaceComp { state: "Method".to_string(), vars: EmitInterfaceVars::Method {  }, args: EmitInterfaceArgs::Method {  } };
        EmitInterface { compartment, stack: Vec::new(), sym: sym, be: be, ni: ni, arms: arms, out: out, na: 0, mi: 0, ai: 0 }
    }

    pub fn step(&mut self) {
        match self.compartment.state.as_str() {
            "Method" => self.Method_step(),
            "Arm" => self.Arm_step(),
            _ => {}
        }
    }

    fn Method_step(&mut self) {
        if self.mi >= self.ni {
            let mut __next = EmitInterfaceComp { state: "Done".to_string(), vars: EmitInterfaceVars::Done {  }, args: EmitInterfaceArgs::Done { } };
            self.compartment = __next;
            return Default::default();
        }
        self.na = state_count(self.sym);
        self.ai = 0;
        clear_arms(&mut self.arms);
        let mut __next = EmitInterfaceComp { state: "Arm".to_string(), vars: EmitInterfaceVars::Arm {  }, args: EmitInterfaceArgs::Arm { } };
        self.compartment = __next;
        return Default::default();
    }

    fn Arm_step(&mut self) {
        if self.ai >= self.na {
            let is_async = method_is_async(self.sym, self.mi);
            route_method(self.sym, self.be, self.mi, &self.arms, is_async, &mut self.out);
            self.mi = self.mi + 1;
            let mut __next = EmitInterfaceComp { state: "Method".to_string(), vars: EmitInterfaceVars::Method {  }, args: EmitInterfaceArgs::Method { } };
            self.compartment = __next;
            return Default::default();
        }
        stamp_arm(self.sym, self.mi, self.ai, &mut self.arms);
        self.ai = self.ai + 1;
        let mut __next = EmitInterfaceComp { state: "Arm".to_string(), vars: EmitInterfaceVars::Arm {  }, args: EmitInterfaceArgs::Arm { } };
        self.compartment = __next;
        return Default::default();
    }

}
