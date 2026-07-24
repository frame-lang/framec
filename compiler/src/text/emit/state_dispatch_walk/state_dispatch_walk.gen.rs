use std::collections::HashMap;
use std::any::Any;


// The generated runtime's PER-STATE MESSAGE-DISPATCH walk, dogfooded as a plain `@@system` — the
// emit-side sequencer that produces, for every state, the private method the router hands an event
// to, which matches the event's message against the handlers that state declares. It rides the same
// READ-ONLY BORROWED DOMAIN as the landed emit machines: the system symbol and the `&dyn Backend`
// are SHARED BORROWS threaded through one lifetime `'a`; the OWNED domain is the accumulating
// output `out`, the two walk cursors (`si`/`hi`) and their bounds (`ns`/`nh`), plus the per-state
// arm accumulator `arms`.
//
// THE 2-LEVEL NESTING, EXPRESSED WITHOUT push$/pop$ (the EmitInterface shape). The pass is a FIXED
// depth-2 walk — the machine's states, then, per state, that state's declared handlers — so a stack
// is unnecessary (a stack buys UNBOUNDED depth; this depth is 2 and known). Two nested CYCLE STATES
// with explicit up/down edges, one owned cursor per level:
//   $State   cycles over `sym.states` (`ns` states); on a state it sets the handler bound `nh`,
//            resets `hi`, CLEARS the arm accumulator, and descends `-> $Handler`; at `si >= ns` it
//            halts `-> $Done`.
//   $Handler cycles over that state's handlers (`nh`), STAMPING one event message per handler; at
//            `hi >= nh` it DISPATCHES — asks the backend to spell the one method from the stamped
//            arms — then ASCENDS (`si += 1`, `-> $State`).
//
// THE HONEST MACHINE CLASS. §3 degenerate pole: a program-counter walk over the ALREADY-RESOLVED
// symbol table. `arms` is not a recognition register — it gates no transition; it is MATERIALIZATION
// being built, exactly like `out` (the ENGINE that decided which handlers a state has is the
// resolver, upstream and already shipped; this walk only reads that frozen decision). Nothing is
// glossed. The payoff claimed is DOGFOOD UNIFORMITY, differential-gated byte-for-byte against the
// preserved `state_dispatch_hand`.
//
// framec owns the WALK (both cursors, both bounds, the per-state accumulator reset, the
// descents/ascents, the halt). The un-Frame-able work is per-item NATIVE LEAVES: `handler_count`
// (the inner bound), `clear_arms` (the reset), `stamp_handler` (the symbol-table read Frame cannot
// do), and `dispatch_state`, which hands `(state, arms)` to `be.dispatch` — the SPELLING is the
// target's, so a target whose router calls `(state, event)` methods directly (Java, Rust, C)
// overrides nothing and this walk emits nothing for it.
//
// Regen: framec-ng -l rust --emit state_dispatch_walk.frs | grep -v '^#!\[allow' > state_dispatch_walk.gen.rs

#[derive(Clone)]
enum StateDispatchWalkVars {
    State {  },
    Handler {  },
    Done {  },
}
#[derive(Clone)]
enum StateDispatchWalkArgs {
    State {  },
    Handler {  },
    Done {  },
}
#[derive(Clone)]
struct StateDispatchWalkComp {
    state: String,
    vars: StateDispatchWalkVars,
    args: StateDispatchWalkArgs,
}

pub struct StateDispatchWalk<'a> {
    compartment: StateDispatchWalkComp,
    stack: Vec<StateDispatchWalkComp>,
    pub sym: &'a SystemSym,
    pub be: &'a dyn Backend,
    pub ns: usize,
    pub arms: EventVec,
    pub out: Sink,
    pub nh: usize,
    pub si: usize,
    pub hi: usize,
}

impl<'a> StateDispatchWalk<'a> {
    pub fn new(sym: &'a SystemSym, be: &'a dyn Backend, ns: usize, arms: EventVec, out: Sink) -> StateDispatchWalk<'a> {
        let compartment = StateDispatchWalkComp { state: "State".to_string(), vars: StateDispatchWalkVars::State {  }, args: StateDispatchWalkArgs::State {  } };
        StateDispatchWalk { compartment, stack: Vec::new(), sym: sym, be: be, ns: ns, arms: arms, out: out, nh: 0, si: 0, hi: 0 }
    }

    pub fn step(&mut self) {
        match self.compartment.state.as_str() {
            "State" => self.State_step(),
            "Handler" => self.Handler_step(),
            _ => {}
        }
    }

    fn State_step(&mut self) {
        if self.si >= self.ns {
            let mut __next = StateDispatchWalkComp { state: "Done".to_string(), vars: StateDispatchWalkVars::Done {  }, args: StateDispatchWalkArgs::Done { } };
            self.compartment = __next;
            return Default::default();
        }
        self.nh = handler_count(self.sym, self.si);
        self.hi = 0;
        clear_arms(&mut self.arms);
        let mut __next = StateDispatchWalkComp { state: "Handler".to_string(), vars: StateDispatchWalkVars::Handler {  }, args: StateDispatchWalkArgs::Handler { } };
        self.compartment = __next;
        return Default::default();
    }

    fn Handler_step(&mut self) {
        if self.hi >= self.nh {
            dispatch_state(self.sym, self.be, self.si, &self.arms, &mut self.out);
            self.si = self.si + 1;
            let mut __next = StateDispatchWalkComp { state: "State".to_string(), vars: StateDispatchWalkVars::State {  }, args: StateDispatchWalkArgs::State { } };
            self.compartment = __next;
            return Default::default();
        }
        stamp_handler(self.sym, self.si, self.hi, &mut self.arms);
        self.hi = self.hi + 1;
        let mut __next = StateDispatchWalkComp { state: "Handler".to_string(), vars: StateDispatchWalkVars::Handler {  }, args: StateDispatchWalkArgs::Handler { } };
        self.compartment = __next;
        return Default::default();
    }

}

