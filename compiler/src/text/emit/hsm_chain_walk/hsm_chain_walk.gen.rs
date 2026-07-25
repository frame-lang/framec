use std::collections::HashMap;
use std::any::Any;


// The generated runtime's STATE-CHAIN TABLE walk, dogfooded as a plain `@@system` — the emit-side
// sequencer that produces, for every leaf state, the ROOT..LEAF path the target's compartment
// factory walks when it enters that state. It rides the same READ-ONLY BORROWED DOMAIN as the
// landed emit machines: the system symbol and the `&dyn Backend` are SHARED BORROWS threaded
// through one lifetime `'a`; the OWNED domain is the accumulating output `out`, the outer cursor
// `si` with its bound `ns`, the climb cursor `ci`, the climb depth `depth`, and the per-state path
// accumulator `chain`.
//
// THE 2-LEVEL NESTING, EXPRESSED WITHOUT push$/pop$ (the EmitInterface shape). The outer level is a
// cursor over `sym.states`; the inner level is an ANCESTOR CLIMB whose depth is bounded by the state
// count, so a stack is unnecessary (a stack buys UNBOUNDED depth; this depth is bounded and known).
// Three cycle states with explicit down/across/up edges:
//   $State  cycles over `sym.states` (`ns` states); on a state it CLEARS the path accumulator, seeds
//           the climb cursor (`ci = si`), and descends `-> $Climb`; at `si >= ns` it halts `-> $Done`.
//   $Climb  pushes the current node's NAME onto `chain` and looks up its parent's INDEX; a parent
//           (`p >= 0`) moves the cursor and loops; no parent (`p < 0`) — or a depth past `ns`, the
//           defensive cycle guard — crosses `-> $Emit`.
//   $Emit   asks the backend to spell ONE table entry from the (reversed, root-first) path, then
//           ASCENDS: `si += 1`, `depth = 0`, `-> $State`.
//
// THE HONEST MACHINE CLASS. §3 degenerate pole again, and the classification is worth stating
// because the climb *looks* like it carries something: `ci` is a MONOTONE CURSOR over an already-
// resolved parent link, not a recognition register — it is read out of the symbol table's frozen
// `parent` field, never advanced by input, and no later behaviour is gated on its value beyond the
// halt. `depth` is a bound, not a mode. Nothing is glossed; the payoff claimed is DOGFOOD UNIFORMITY,
// differential-gated byte-for-byte against the preserved `hsm_chain_hand`.
//
// framec owns the WALK (both cursors, both bounds, the clear, the descents/ascents, the halt). The
// un-Frame-able work is per-item NATIVE LEAVES: `clear_chain` (the per-state accumulator reset),
// `push_state_name` + `parent_index` (symbol-table reads Frame cannot do), and `stamp_chain`, which
// reverses the leaf-first path into root-first order and hands it to `be.hsm_chain_entry` — the
// SPELLING is the target's, so a target with no such table overrides nothing and this walk emits
// nothing for it.
//
// Regen: framec-ng -l rust --emit hsm_chain_walk.frs | grep -v '^#!\[allow' > hsm_chain_walk.gen.rs

#[derive(Clone)]
enum HsmChainWalkVars {
    State {  },
    Climb {  },
    Emit {  },
    Done {  },
}
#[derive(Clone)]
enum HsmChainWalkArgs {
    State {  },
    Climb {  },
    Emit {  },
    Done {  },
}
#[derive(Clone)]
struct HsmChainWalkComp {
    state: String,
    vars: HsmChainWalkVars,
    args: HsmChainWalkArgs,
}

pub struct HsmChainWalk<'a> {
    compartment: HsmChainWalkComp,
    stack: Vec<HsmChainWalkComp>,
    pub sym: &'a SystemSym,
    pub be: &'a dyn Backend,
    pub ns: usize,
    pub chain: ChainVec,
    pub out: Sink,
    pub si: usize,
    pub ci: usize,
    pub depth: usize,
}

impl<'a> HsmChainWalk<'a> {
    pub fn new(sym: &'a SystemSym, be: &'a dyn Backend, ns: usize, chain: ChainVec, out: Sink) -> HsmChainWalk<'a> {
        let compartment = HsmChainWalkComp { state: "State".to_string(), vars: HsmChainWalkVars::State {  }, args: HsmChainWalkArgs::State {  } };
        HsmChainWalk { compartment, stack: Vec::new(), sym: sym, be: be, ns: ns, chain: chain, out: out, si: 0, ci: 0, depth: 0 }
    }

    pub fn step(&mut self) {
        match self.compartment.state.as_str() {
            "State" => self.State_step(),
            "Climb" => self.Climb_step(),
            "Emit" => self.Emit_step(),
            _ => {}
        }
    }

    fn State_step(&mut self) {
        if self.si >= self.ns {
            let mut __next = HsmChainWalkComp { state: "Done".to_string(), vars: HsmChainWalkVars::Done {  }, args: HsmChainWalkArgs::Done { } };
            self.compartment = __next;
            return Default::default();
        }
        clear_chain(&mut self.chain);
        self.ci = self.si;
        self.depth = 0;
        let mut __next = HsmChainWalkComp { state: "Climb".to_string(), vars: HsmChainWalkVars::Climb {  }, args: HsmChainWalkArgs::Climb { } };
        self.compartment = __next;
        return Default::default();
    }

    fn Climb_step(&mut self) {
        if self.depth > self.ns {
            let mut __next = HsmChainWalkComp { state: "Emit".to_string(), vars: HsmChainWalkVars::Emit {  }, args: HsmChainWalkArgs::Emit { } };
            self.compartment = __next;
            return Default::default();
        }
        push_state_name(self.sym, self.ci, &mut self.chain);
        self.depth = self.depth + 1;
        let p = parent_index(self.sym, self.ci);
        if p < 0 {
            let mut __next = HsmChainWalkComp { state: "Emit".to_string(), vars: HsmChainWalkVars::Emit {  }, args: HsmChainWalkArgs::Emit { } };
            self.compartment = __next;
            return Default::default();
        }
        self.ci = p as usize;
        let mut __next = HsmChainWalkComp { state: "Climb".to_string(), vars: HsmChainWalkVars::Climb {  }, args: HsmChainWalkArgs::Climb { } };
        self.compartment = __next;
        return Default::default();
    }

    fn Emit_step(&mut self) {
        stamp_chain(self.sym, self.be, self.si, &mut self.chain, &mut self.out);
        self.si = self.si + 1;
        let mut __next = HsmChainWalkComp { state: "State".to_string(), vars: HsmChainWalkVars::State {  }, args: HsmChainWalkArgs::State { } };
        self.compartment = __next;
        return Default::default();
    }

}
