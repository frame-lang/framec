use std::collections::HashMap;
use std::any::Any;


// The MACHINE-section dispatch walk, dogfooded as an `@@[scan(u8)]` system — the state-level
// analogue of the item-level `Segmenter`. It walks a `machine:` section span and ACCUMULATES the
// `$Name` state-START offsets into `starts`, skipping opaque regions (a `$Name` inside a comment
// or string is not a state) and skipping each state's whole BODY (so a `$.x` or a nested `$Ref`
// inside a handler is never a top-level state start). `target`/`limit` are construction config
// (they survive `scan_at`); `starts` is the accumulated output.
//
// framec owns the WALK (the `$Scan` step loop); the leaves do only transformation, each already
// a proven system or a shared helper: `skip` = the opaque skip (OpaqueScan, kind-aware limit
// policy), `state_end` = `machine::state_extent` (the SAME source `state()` builds the node from,
// via DelimBalance — so the found boundary and the built extent cannot drift), `is_state_start`/
// `record` are trivial. `machine_section` is now a thin native driver over these positions.
//
// Regen: framec-ng -l rust --emit machine_walk.frs | grep -v '^#!\[allow' > machine_walk.gen.rs

pub trait MachineWalkInput { fn fsm_get(&self, i: usize) -> u8; fn fsm_len(&self) -> usize; }
impl MachineWalkInput for &[u8] { fn fsm_get(&self, i: usize) -> u8 { self[i] } fn fsm_len(&self) -> usize { self.len() } }

#[derive(Clone)]
enum MachineWalkVars {
    Scan {  },
    Accept {  },
}
#[derive(Clone)]
enum MachineWalkArgs {
    Scan {  },
    Accept {  },
}
#[derive(Clone)]
struct MachineWalkComp {
    state: String,
    vars: MachineWalkVars,
    args: MachineWalkArgs,
}

pub struct MachineWalk<'a> {
    src: &'a [u8],
    pub cursor: usize,
    compartment: MachineWalkComp,
    stack: Vec<MachineWalkComp>,
    pub target: Target,
    pub limit: usize,
    pub starts: Vec<usize>,
}

impl<'a> MachineWalk<'a> {
    pub fn over(src: &'a [u8], target: Target, limit: usize) -> Self {
        let compartment = MachineWalkComp { state: "Scan".to_string(), vars: MachineWalkVars::Scan {  }, args: MachineWalkArgs::Scan {  } };
        MachineWalk { src, cursor: 0, compartment, stack: Vec::new(), target: target, limit: limit, starts: Vec::new() }
    }

    pub fn scan_at(&mut self, start: usize) -> bool {
        self.cursor = start;
        self.starts = Vec::new();
        self.compartment = MachineWalkComp { state: "Scan".to_string(), vars: MachineWalkVars::Scan {  }, args: MachineWalkArgs::Scan {  } };
        let mut __steps: usize = 0;
        while self.compartment.state != "Accept" && self.compartment.state != "Reject" {
            self.step();
            __steps += 1;
            if __steps > self.src.fsm_len() * 4 + 64 { break; }
        }
        self.compartment.state == "Accept"
    }

    pub fn step(&mut self) {
        match self.compartment.state.as_str() {
            "Scan" => self.Scan_step(),
            _ => {}
        }
    }

    fn Scan_step(&mut self) {
        if self.cursor >= self.limit {
            let mut __next = MachineWalkComp { state: "Accept".to_string(), vars: MachineWalkVars::Accept {  }, args: MachineWalkArgs::Accept { } };
            self.compartment = __next;
            return Default::default();
        }
        // Skip a whole opaque region (a `$Name` inside it is not a state).
        let sk = skip(self.src, self.cursor, self.limit, self.target);
        if sk > self.cursor {
            self.cursor = sk;
        } else if is_state_start(self.src, self.cursor) {
            // A state opens here: record its start, then jump past its whole body so the
            // next `$Name` we see is the next top-level state.
            record(&mut self.starts, self.cursor);
            self.cursor = state_end(self.src, self.cursor, self.limit, self.target);
        } else {
            self.cursor = self.cursor + 1;
        }
    }

}
