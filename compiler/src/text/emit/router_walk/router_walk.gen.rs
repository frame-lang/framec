use std::collections::HashMap;
use std::any::Any;


// The generated runtime's STATE ROUTER walk, dogfooded as a plain `@@system` — the emit-side
// sequencer that produces one arm per state of "if the live compartment is in this state, hand the
// event to that state's dispatcher". It rides the same READ-ONLY BORROWED DOMAIN as the landed emit
// machines: the system symbol and the `&dyn Backend` are SHARED BORROWS threaded through one
// lifetime `'a`; the OWNED domain is the accumulating output `out`, the cursor `si` with its bound
// `ns`, and the `first` bit.
//
// A ONE-LEVEL CYCLE: `$Arm` stamps one state's arm per iteration and advances; at `si >= ns` it
// halts to `$Done`.
//
// THE ONE BIT WORTH NAMING — and why it is NOT a recognition register. `first` distinguishes the
// leading arm (`if`) from every later one (`elif` / `else if`). It is a WRITE-ONCE latch: true at
// entry, cleared by the first stamp, never read back to change which transition fires. It is
// carried here for one reason — so the SPELLING never has to re-derive "have I written an arm yet?"
// by looking at what it already wrote, which is precisely the emitted-text oracle RFC-0056 P6
// forbids. The old compiler's answer to this question was a `.is_empty()` on the output buffer; the
// answer here is a bool the walk owns. §3 degenerate pole otherwise: a program-counter cursor over
// the already-resolved symbol table, gated on nothing the input says.
//
// framec owns the WALK (the cursor, the bound, the latch, the halt). The un-Frame-able work is the
// single per-item NATIVE LEAF `stamp_router_arm`, which hands `(state, first)` to
// `be.router_arm` — the SPELLING is the target's, so a target whose dispatch is direct (Java, Rust,
// C) overrides nothing and this walk emits nothing for it.
//
// Regen: framec-ng -l rust --emit router_walk.frs | grep -v '^#!\[allow' > router_walk.gen.rs

#[derive(Clone)]
enum RouterWalkVars {
    Arm {  },
    Done {  },
}
#[derive(Clone)]
enum RouterWalkArgs {
    Arm {  },
    Done {  },
}
#[derive(Clone)]
struct RouterWalkComp {
    state: String,
    vars: RouterWalkVars,
    args: RouterWalkArgs,
}

pub struct RouterWalk<'a> {
    compartment: RouterWalkComp,
    stack: Vec<RouterWalkComp>,
    pub sym: &'a SystemSym,
    pub be: &'a dyn Backend,
    pub ns: usize,
    pub out: Sink,
    pub si: usize,
    pub first: bool,
}

impl<'a> RouterWalk<'a> {
    pub fn new(sym: &'a SystemSym, be: &'a dyn Backend, ns: usize, out: Sink) -> RouterWalk<'a> {
        let compartment = RouterWalkComp { state: "Arm".to_string(), vars: RouterWalkVars::Arm {  }, args: RouterWalkArgs::Arm {  } };
        RouterWalk { compartment, stack: Vec::new(), sym: sym, be: be, ns: ns, out: out, si: 0, first: true }
    }

    pub fn step(&mut self) {
        match self.compartment.state.as_str() {
            "Arm" => self.Arm_step(),
            _ => {}
        }
    }

    fn Arm_step(&mut self) {
        if self.si >= self.ns {
            let mut __next = RouterWalkComp { state: "Done".to_string(), vars: RouterWalkVars::Done {  }, args: RouterWalkArgs::Done { } };
            self.compartment = __next;
            return Default::default();
        }
        stamp_router_arm(self.sym, self.be, self.si, self.first, &mut self.out);
        self.first = false;
        self.si = self.si + 1;
        let mut __next = RouterWalkComp { state: "Arm".to_string(), vars: RouterWalkVars::Arm {  }, args: RouterWalkArgs::Arm { } };
        self.compartment = __next;
        return Default::default();
    }

}
