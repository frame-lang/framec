use std::collections::HashMap;
use std::any::Any;


// The STATE-member dispatch walk, dogfooded as an `@@[scan(u8)]` system — the member-level
// analogue of `MachineWalk` (state level) and `Segmenter` (item level). It walks a state BODY
// span and ACCUMULATES the member-START offsets into `starts`: a `$.x` state variable or a
// handler head (`ev(...) {`, `$>() {`, `<$() {`). It skips opaque regions and skips each member's
// whole extent (a state var to end-of-line; a handler to past its body), so a `$.x` or a `}`
// inside a handler body is never a member start. `target`/`limit` are construction config (they
// survive `scan_at`); `starts` is the accumulated output.
//
// framec owns the WALK (the `$Scan` step loop); the leaves do only transformation, each a proven
// system or a shared helper: `skip` = the opaque skip (OpaqueScan), `member_end` = the member's
// extent (a state var via `machine::to_end_of_line`; a handler via `machine::handler_end` →
// `handler_head`, the SAME source `handler_at` builds the node from, via DelimBalance — so the
// found boundary and the built extent cannot drift), `record` is trivial. `state()`'s member loop
// is now a thin native driver over these positions.
//
// Regen: framec-ng -l rust --emit state_walk.frs | grep -v '^#!\[allow' > state_walk.gen.rs

pub trait StateWalkInput { fn fsm_get(&self, i: usize) -> u8; fn fsm_len(&self) -> usize; }
impl StateWalkInput for &[u8] { fn fsm_get(&self, i: usize) -> u8 { self[i] } fn fsm_len(&self) -> usize { self.len() } }

#[derive(Clone)]
enum StateWalkVars {
    Scan {  },
    Accept {  },
}
#[derive(Clone)]
enum StateWalkArgs {
    Scan {  },
    Accept {  },
}
#[derive(Clone)]
struct StateWalkComp {
    state: String,
    vars: StateWalkVars,
    args: StateWalkArgs,
}

pub struct StateWalk<'a> {
    src: &'a [u8],
    pub cursor: usize,
    compartment: StateWalkComp,
    stack: Vec<StateWalkComp>,
    pub target: Target,
    pub limit: usize,
    pub starts: Vec<usize>,
}

impl<'a> StateWalk<'a> {
    pub fn over(src: &'a [u8], target: Target, limit: usize) -> Self {
        let compartment = StateWalkComp { state: "Scan".to_string(), vars: StateWalkVars::Scan {  }, args: StateWalkArgs::Scan {  } };
        StateWalk { src, cursor: 0, compartment, stack: Vec::new(), target: target, limit: limit, starts: Vec::new() }
    }

    pub fn scan_at(&mut self, start: usize) -> bool {
        self.cursor = start;
        self.starts = Vec::new();
        self.compartment = StateWalkComp { state: "Scan".to_string(), vars: StateWalkVars::Scan {  }, args: StateWalkArgs::Scan {  } };
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
            let mut __next = StateWalkComp { state: "Accept".to_string(), vars: StateWalkVars::Accept {  }, args: StateWalkArgs::Accept { } };
            self.compartment = __next;
            return Default::default();
        }
        // Skip a whole opaque region (a `$.` or handler-looking text inside it is not a
        // member).
        let sk = skip(self.src, self.cursor, self.limit, self.target);
        if sk > self.cursor {
            self.cursor = sk;
        } else {
            // A member opens here iff `member_end` reports an extent past the cursor (a
            // state var → end-of-line; a handler → past its body). Record its start, then
            // jump past its extent so the next thing we see is the next member.
            let me = member_end(self.src, self.cursor, self.limit, self.target);
            if me > self.cursor {
                record(&mut self.starts, self.cursor);
                self.cursor = me;
            } else {
                self.cursor = self.cursor + 1;
            }
        }
    }

}

