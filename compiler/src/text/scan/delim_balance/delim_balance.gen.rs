use std::collections::HashMap;
use std::any::Any;


// Opaque-aware balanced-delimiter extent recognizer, dogfooded as an `@@[scan(u8)]` COUNTER
// automaton — the `@@system` that replaces the hand `machine.rs::balanced`/`matching_brace` and
// (Item 2's deferral) `close_brace`'s `{}` counter. From an opener at the cursor it finds the
// matching closer, leaving cursor one past it (Accept), or Rejects if it never balances before
// `limit`.
//
// It is a COUNTER (a single integer `depth`, one delimiter pair at a time — Dyck-1, NOT a
// kind-matched pushdown), and it is OPAQUE-AWARE: it composes OpaqueScan via the `opaque_skip`
// leaf, so a delimiter inside a comment / string / char / raw / triple literal is skipped, never
// counted. This is STRONGER than ParenBalance (which skips only `"` via StringScan) — it is the
// full grammar-body skip. The open/close bytes and the `limit` are constructor config (a scan
// runs bounded by `limit`, not `fsm_len()`); `scan_at` resets only `cursor`/`depth`, so the
// config survives per scan (verified: _scratch/delim_probe.frs).
//
// framec owns the WALK (dispatch + the depth counter); the `opaque_skip` leaf only runs
// OpaqueScan and applies the grammar's kind-aware limit policy (comment clamps, literal rejects
// on overrun) — no walk lives in the leaf (D3).
//
// Regen: framec-ng -l rust --emit delim_balance.frs | grep -v '^#!\[allow' > delim_balance.gen.rs

pub trait DelimBalanceInput { fn fsm_get(&self, i: usize) -> u8; fn fsm_len(&self) -> usize; }
impl DelimBalanceInput for &[u8] { fn fsm_get(&self, i: usize) -> u8 { self[i] } fn fsm_len(&self) -> usize { self.len() } }

#[derive(Clone)]
enum DelimBalanceVars {
    Scan {  },
    Accept {  },
    Reject {  },
}
#[derive(Clone)]
enum DelimBalanceArgs {
    Scan {  },
    Accept {  },
    Reject {  },
}
#[derive(Clone)]
struct DelimBalanceComp {
    state: String,
    vars: DelimBalanceVars,
    args: DelimBalanceArgs,
}

pub struct DelimBalance<'a> {
    src: &'a [u8],
    pub cursor: usize,
    compartment: DelimBalanceComp,
    stack: Vec<DelimBalanceComp>,
    pub target: Target,
    pub open: u8,
    pub close: u8,
    pub limit: usize,
    pub depth: i32,
}

impl<'a> DelimBalance<'a> {
    pub fn over(src: &'a [u8], target: Target, open: u8, close: u8, limit: usize) -> Self {
        let compartment = DelimBalanceComp { state: "Scan".to_string(), vars: DelimBalanceVars::Scan {  }, args: DelimBalanceArgs::Scan {  } };
        DelimBalance { src, cursor: 0, compartment, stack: Vec::new(), target: target, open: open, close: close, limit: limit, depth: 0 }
    }

    pub fn scan_at(&mut self, start: usize) -> bool {
        self.cursor = start;
        self.depth = 0;
        self.compartment = DelimBalanceComp { state: "Scan".to_string(), vars: DelimBalanceVars::Scan {  }, args: DelimBalanceArgs::Scan {  } };
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
            let mut __next = DelimBalanceComp { state: "Reject".to_string(), vars: DelimBalanceVars::Reject {  }, args: DelimBalanceArgs::Reject { } };
            self.compartment = __next;
            return Default::default();
        }
        // Skip a whole opaque region (comment/literal) first, so a delimiter inside it
        // is not counted. The leaf returns `cursor` unchanged when nothing opaque opens.
        let sk = opaque_skip(self.src, self.cursor, self.limit, self.target);
        if sk > self.cursor {
            self.cursor = sk;
        } else {
            let b = self.src.fsm_get(self.cursor);
            self.cursor = self.cursor + 1;
            if b == self.open {
                self.depth = self.depth + 1;
            }
            if b == self.close {
                self.depth = self.depth - 1;
                if self.depth == 0 {
                    let mut __next = DelimBalanceComp { state: "Accept".to_string(), vars: DelimBalanceVars::Accept {  }, args: DelimBalanceArgs::Accept { } };
                    self.compartment = __next;
                    return Default::default();
                }
            }
        }
    }

}

