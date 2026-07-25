use std::collections::HashMap;
use std::any::Any;


// The handler-BODY statement dispatch walk, dogfooded as an `@@[scan(u8)]` system — the
// statement-level analogue of StateWalk/MachineWalk/Segmenter, and the first that FUSES the two
// shipped machine rungs: a segmenter-style ACCUMULATOR (`starts`) AND a DelimBalance-style running
// COUNTER (`depth`). It walks a handler body and records each Frame-statement START together with
// the brace `depth` at that point, skipping opaque regions and skipping each statement's extent.
//
// The brace-DEPTH counter is honest machine content: a running, saturating `{`/`}` level over the
// body's native water (opaque-skipped), sampled at each statement start. It is stateful across the
// whole traversal, so it MUST ride this walk — a native driver would need its own hand brace-loop
// to reproduce it (guardrail-4 forbidden). It is NOT DelimBalance (which balances-to-close); it is
// a running level never "closed".
//
// COUPLING (#249 B7 review): the walk now runs THROUGH each statement's extent (a `barrier`
// suppresses re-detection) instead of jumping it, so a `{` a frame-assign RHS absorbs (`$.x = {`)
// is counted. The consequence: the depth count now DEPENDS on OpaqueScan completeness for the RHS
// bytes — an in-string brace is only skipped if OpaqueScan models that string form. This is safe
// for the depth-CONSUMING targets (Python/GdScript reindent, Java unreachable-suppression), whose
// string/comment forms OpaqueScan models (guarded by `b7_instring_brace_in_frame_assign_rhs_not_
// counted` in parser_bug_corpus.rs). It is only LATENT for targets whose forms OpaqueScan
// under-models (e.g. Go backtick raw strings, JS/TS template literals) — harmless while those
// targets do not consume block_depth. Closing those OpaqueScan form gaps is the standing follow-up.
//
// framec owns the WALK (dispatch order frame_call → frame_assign → frame_stmt, then opaque-skip,
// then brace-count — matching the hand `body()`); the leaves do only transformation: `stmt_end` =
// the statement extent (the shared `frame_call_end`/`frame_assign_end`/`stmt_scan::classify`
// heads, `native_parts`-free — the SAME sources the driver builds the nodes from, so found
// boundary and built extent cannot drift), `skip` = OpaqueScan, `record` is trivial. `body()` is
// now a thin native driver over `(start, depth)`.
//
// Regen: framec-ng -l rust --emit body_walk.frs | grep -v '^#!\[allow' > body_walk.gen.rs

pub trait BodyWalkInput { fn fsm_get(&self, i: usize) -> u8; fn fsm_len(&self) -> usize; }
impl BodyWalkInput for &[u8] { fn fsm_get(&self, i: usize) -> u8 { self[i] } fn fsm_len(&self) -> usize { self.len() } }

#[derive(Clone)]
enum BodyWalkVars {
    Scan {  },
    Accept {  },
}
#[derive(Clone)]
enum BodyWalkArgs {
    Scan {  },
    Accept {  },
}
#[derive(Clone)]
struct BodyWalkComp {
    state: String,
    vars: BodyWalkVars,
    args: BodyWalkArgs,
}

pub struct BodyWalk<'a> {
    src: &'a [u8],
    pub cursor: usize,
    compartment: BodyWalkComp,
    stack: Vec<BodyWalkComp>,
    pub target: Target,
    pub limit: usize,
    pub depth: u32,
    pub barrier: usize,
    pub starts: Vec<(usize, u32)>,
}

impl<'a> BodyWalk<'a> {
    pub fn over(src: &'a [u8], target: Target, limit: usize) -> Self {
        let compartment = BodyWalkComp { state: "Scan".to_string(), vars: BodyWalkVars::Scan {  }, args: BodyWalkArgs::Scan {  } };
        BodyWalk { src, cursor: 0, compartment, stack: Vec::new(), target: target, limit: limit, depth: 0, barrier: 0, starts: Vec::new() }
    }

    pub fn scan_at(&mut self, start: usize) -> bool {
        self.cursor = start;
        self.depth = 0;
        self.barrier = 0;
        self.starts = Vec::new();
        self.compartment = BodyWalkComp { state: "Scan".to_string(), vars: BodyWalkVars::Scan {  }, args: BodyWalkArgs::Scan {  } };
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
            let mut __next = BodyWalkComp { state: "Accept".to_string(), vars: BodyWalkVars::Accept {  }, args: BodyWalkArgs::Accept { } };
            self.compartment = __next;
            return Default::default();
        }
        // A Frame statement opens here iff `stmt_end` reports an extent past the cursor —
        // but ONLY detect one outside an already-recorded statement's extent (`barrier`).
        // Record its start WITH the current brace depth, then set the barrier to its extent
        // end and KEEP WALKING (do not jump): the byte-walk below runs THROUGH the extent so
        // a `{` the statement absorbed (a frame-assign RHS opening a block, #249 B7) is still
        // counted into `depth`. The barrier suppresses re-detecting a nested `@@:`/`$.` in an
        // outer statement's interior as a spurious start.
        if self.cursor >= self.barrier {
            let se = stmt_end(self.src, self.cursor, self.limit, self.target);
            if se > self.cursor {
                record(&mut self.starts, self.cursor, self.depth);
                self.barrier = se;
            }
        }
        // Byte-walk (now runs through statement interiors too): skip a whole opaque region,
        // else count one brace of native water into `depth` and advance one byte. A `{`/`}`
        // inside a string/comment is opaque-skipped, so only real block braces count. Each
        // step advances the cursor by >= 1, so the scan's progress budget is unthreatened.
        let sk = skip(self.src, self.cursor, self.limit, self.target);
        if sk > self.cursor {
            self.cursor = sk;
        } else {
            let b = self.src.fsm_get(self.cursor);
            if b == 123 {
                self.depth = self.depth + 1;
            }
            if b == 125 {
                if self.depth > 0 {
                    self.depth = self.depth - 1;
                }
            }
            self.cursor = self.cursor + 1;
        }
    }

}
