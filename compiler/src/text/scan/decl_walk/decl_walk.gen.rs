use std::collections::HashMap;
use std::any::Any;


// The DECL-section dispatch walk, dogfooded as an `@@[scan(u8)]` system — the FOURTH section
// walk, completing the family (MachineWalk = states, StateWalk = members, BodyWalk = statements,
// Segmenter = items). It walks a decl-section span (`interface:` / `domain:` / `actions:` /
// `operations:`) and ACCUMULATES the declaration-START offsets into `starts`, skipping opaque
// regions (a decl-looking line inside a comment or string is not a decl), whitespace, and `@@[`
// attribute lines (the `public Object ;` guard), and jumping each recorded decl's whole extent —
// to end-of-line for a line decl, past the matching `}` for a body decl (`with_bodies`).
// `target`/`limit`/`with_bodies` are construction config (they survive `scan_at`); `starts` and
// the `unterminated_body` register are the accumulated output.
//
// `unterminated_body` (ledger T2): a body decl whose `{` never balances clamps its extent to
// `limit` — behavior carried verbatim from the hand `matching_brace(..).unwrap_or(limit)`, but the
// clamp is now a VALUE a future diagnostics pass can read, not an erased `unwrap_or`.
//
// framec owns the WALK (the `$Scan` step loop, dispatch order = the hand loop's byte for byte:
// opaque, ws, `@@[`, decl); the leaves do only transformation, each a proven system or a shared
// helper: `skip` = the opaque skip (OpaqueScan, kind-aware limit policy), `attr_end` =
// `machine::to_end_of_line`, `decl_end`/`decl_unterminated` = thin reads of the single-source
// `machine::decl_extent` head (the SAME source the driver will build the nodes from, via
// DelimBalance — so the found boundary and the built extent cannot drift), `is_ws`/`is_attr`/
// `record` are trivial.
//
// TOTAL-PROGRESS PROOF (ledger T6 — why the generated engine's `len*4+64` step budget is
// unreachable): every `$Scan` arm strictly advances the cursor. (1) `skip` is taken only when it
// returns `> cursor`. (2) The ws arm advances 1. (3) The attr arm advances >= 1: `@` != `\n`, so
// `to_end_of_line > cursor`. (4) A line decl's `eol > cursor`: the start byte is non-ws (the ws
// arm would have taken it), hence not `\n`. (5) A body decl's `end > cursor`: DelimBalance
// accepts only past the opener (`end >= open + 2 > cursor`), and the unbalanced clamp is `limit`,
// which is `> cursor` inside the loop guard. So the walk reaches `$Accept` in at most `limit`
// steps and the budget breaker never fires.
//
// Regen: framec-ng -l rust --emit decl_walk.frs | grep -v '^#!\[allow' > decl_walk.gen.rs

pub trait DeclWalkInput { fn fsm_get(&self, i: usize) -> u8; fn fsm_len(&self) -> usize; }
impl DeclWalkInput for &[u8] { fn fsm_get(&self, i: usize) -> u8 { self[i] } fn fsm_len(&self) -> usize { self.len() } }

#[derive(Clone)]
enum DeclWalkVars {
    Scan {  },
    Accept {  },
}
#[derive(Clone)]
enum DeclWalkArgs {
    Scan {  },
    Accept {  },
}
#[derive(Clone)]
struct DeclWalkComp {
    state: String,
    vars: DeclWalkVars,
    args: DeclWalkArgs,
}

pub struct DeclWalk<'a> {
    src: &'a [u8],
    pub cursor: usize,
    compartment: DeclWalkComp,
    stack: Vec<DeclWalkComp>,
    pub target: Target,
    pub limit: usize,
    pub with_bodies: bool,
    pub starts: Vec<usize>,
    pub unterminated_body: bool,
}

impl<'a> DeclWalk<'a> {
    pub fn over(src: &'a [u8], target: Target, limit: usize, with_bodies: bool) -> Self {
        let compartment = DeclWalkComp { state: "Scan".to_string(), vars: DeclWalkVars::Scan {  }, args: DeclWalkArgs::Scan {  } };
        DeclWalk { src, cursor: 0, compartment, stack: Vec::new(), target: target, limit: limit, with_bodies: with_bodies, starts: Vec::new(), unterminated_body: false }
    }

    pub fn scan_at(&mut self, start: usize) -> bool {
        self.cursor = start;
        self.starts = Vec::new();
        self.unterminated_body = false;
        self.compartment = DeclWalkComp { state: "Scan".to_string(), vars: DeclWalkVars::Scan {  }, args: DeclWalkArgs::Scan {  } };
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
            let mut __next = DeclWalkComp { state: "Accept".to_string(), vars: DeclWalkVars::Accept {  }, args: DeclWalkArgs::Accept { } };
            self.compartment = __next;
            return Default::default();
        }
        // Dispatch order = the hand loop's, byte for byte: opaque, ws, `@@[`, decl.
        let sk = skip(self.src, self.cursor, self.limit, self.target);
        if sk > self.cursor {
            self.cursor = sk;
        } else if is_ws(self.src, self.cursor) {
            self.cursor = self.cursor + 1;
        } else if is_attr(self.src, self.cursor, self.limit) {
            // `@@[attr]` — an attribute line, not a declaration (the `public Object ;` guard).
            self.cursor = attr_end(self.src, self.cursor, self.limit);
        } else {
            // A declaration starts here — record it, jump its whole extent.
            record(&mut self.starts, self.cursor);
            if decl_unterminated(self.src, self.cursor, self.limit, self.with_bodies, self.target) {
                self.unterminated_body = true;
            }
            self.cursor = decl_end(self.src, self.cursor, self.limit, self.with_bodies, self.target);
        }
    }

}
