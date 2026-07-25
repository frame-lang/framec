use std::collections::HashMap;
use std::any::Any;


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

#[derive(Clone)]
enum DomainInitWalkVars {
    Field {  },
    Done {  },
}
#[derive(Clone)]
enum DomainInitWalkArgs {
    Field {  },
    Done {  },
}
#[derive(Clone)]
struct DomainInitWalkComp {
    state: String,
    vars: DomainInitWalkVars,
    args: DomainInitWalkArgs,
}

pub struct DomainInitWalk<'a> {
    compartment: DomainInitWalkComp,
    stack: Vec<DomainInitWalkComp>,
    pub sym: &'a SystemSym,
    pub be: &'a dyn Backend,
    pub nd: usize,
    pub out: Sink,
    pub i: usize,
}

impl<'a> DomainInitWalk<'a> {
    pub fn new(sym: &'a SystemSym, be: &'a dyn Backend, nd: usize, out: Sink) -> DomainInitWalk<'a> {
        let compartment = DomainInitWalkComp { state: "Field".to_string(), vars: DomainInitWalkVars::Field {  }, args: DomainInitWalkArgs::Field {  } };
        DomainInitWalk { compartment, stack: Vec::new(), sym: sym, be: be, nd: nd, out: out, i: 0 }
    }

    pub fn step(&mut self) {
        match self.compartment.state.as_str() {
            "Field" => self.Field_step(),
            _ => {}
        }
    }

    fn Field_step(&mut self) {
        if self.i >= self.nd {
            let mut __next = DomainInitWalkComp { state: "Done".to_string(), vars: DomainInitWalkVars::Done {  }, args: DomainInitWalkArgs::Done { } };
            self.compartment = __next;
            return Default::default();
        }
        stamp_domain_init(self.sym, self.be, self.i, &mut self.out);
        self.i = self.i + 1;
        let mut __next = DomainInitWalkComp { state: "Field".to_string(), vars: DomainInitWalkVars::Field {  }, args: DomainInitWalkArgs::Field { } };
        self.compartment = __next;
        return Default::default();
    }

}
