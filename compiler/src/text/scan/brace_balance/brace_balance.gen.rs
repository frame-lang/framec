use std::collections::HashMap;
use std::any::Any;


// Balanced-`{}` extent recognizer, dogfooded as an `@@[scan(u8)]` system — a COUNTER automaton
// (framec's bracket scanners count openers against closers). From a `{` at the cursor it finds
// the matching `}`, leaving cursor one past it. Used by StringScan to skip a Python interpolation
// hole (`{…}`) whole, so a delimiter inside the hole does not end the string.
//
// Unlike ParenBalance this is NOT string-aware: it counts raw `{`/`}`, matching the hand
// `Lexer::hole_at`, which brace-balances the hole contents without skipping strings inside.
// Rejects (no accept) if the `{` is never closed — the caller then treats it as a non-hole.
//
// Regen: framec-ng -l rust --emit brace_balance.frs | grep -v '^#!\[allow' > brace_balance.gen.rs

pub trait BraceBalanceInput { fn fsm_get(&self, i: usize) -> u8; fn fsm_len(&self) -> usize; }
impl BraceBalanceInput for &[u8] { fn fsm_get(&self, i: usize) -> u8 { self[i] } fn fsm_len(&self) -> usize { self.len() } }

#[derive(Clone)]
enum BraceBalanceVars {
    Scan {  },
    Accept {  },
    Reject {  },
}
#[derive(Clone)]
enum BraceBalanceArgs {
    Scan {  },
    Accept {  },
    Reject {  },
}
#[derive(Clone)]
struct BraceBalanceComp {
    state: String,
    vars: BraceBalanceVars,
    args: BraceBalanceArgs,
}

pub struct BraceBalance<'a> {
    src: &'a [u8],
    pub cursor: usize,
    compartment: BraceBalanceComp,
    stack: Vec<BraceBalanceComp>,
    pub depth: i32,
}

impl<'a> BraceBalance<'a> {
    pub fn over(src: &'a [u8]) -> Self {
        let compartment = BraceBalanceComp { state: "Scan".to_string(), vars: BraceBalanceVars::Scan {  }, args: BraceBalanceArgs::Scan {  } };
        BraceBalance { src, cursor: 0, compartment, stack: Vec::new(), depth: 0 }
    }

    pub fn scan_at(&mut self, start: usize) -> bool {
        self.cursor = start;
        self.depth = 0;
        self.compartment = BraceBalanceComp { state: "Scan".to_string(), vars: BraceBalanceVars::Scan {  }, args: BraceBalanceArgs::Scan {  } };
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
        if self.cursor >= self.src.fsm_len() {
            let mut __next = BraceBalanceComp { state: "Reject".to_string(), vars: BraceBalanceVars::Reject {  }, args: BraceBalanceArgs::Reject { } };
            self.compartment = __next;
            return Default::default();
        }
        let b = self.src.fsm_get(self.cursor);
        self.cursor = self.cursor + 1;
        if b == 123 {
            self.depth = self.depth + 1;
        }
        if b == 125 {
            self.depth = self.depth - 1;
            if self.depth == 0 {
                let mut __next = BraceBalanceComp { state: "Accept".to_string(), vars: BraceBalanceVars::Accept {  }, args: BraceBalanceArgs::Accept { } };
                self.compartment = __next;
                return Default::default();
            }
        }
    }

}
