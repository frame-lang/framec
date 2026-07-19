use std::collections::HashMap;
use std::any::Any;


// Balanced-`()` extent recognizer, dogfooded as an `@@[scan(u8)]` system. A COUNTER
// automaton (the journal's point: framec's bracket scanners count openers against closers;
// they are not kind-matched pushdowns) that is STRING-AWARE: it composes StringScan (via the
// `skip_string` leaf) to skip over a `"`-string so a `)` inside it does not count. From a
// `(` at the cursor it finds the matching `)`, leaving cursor one past it. `depth` is a
// domain counter; `scan_at` resets it per scan.
//
// Regen: framec-ng -l rust --emit paren_balance.frs | grep -v '^#!\[allow' > paren_balance.gen.rs

pub trait ParenBalanceInput { fn fsm_get(&self, i: usize) -> u8; fn fsm_len(&self) -> usize; }
impl ParenBalanceInput for &[u8] { fn fsm_get(&self, i: usize) -> u8 { self[i] } fn fsm_len(&self) -> usize { self.len() } }

#[derive(Clone)]
enum ParenBalanceVars {
    Scan {  },
    Accept {  },
    Reject {  },
}
#[derive(Clone)]
enum ParenBalanceArgs {
    Scan {  },
    Accept {  },
    Reject {  },
}
#[derive(Clone)]
struct ParenBalanceComp {
    state: String,
    vars: ParenBalanceVars,
    args: ParenBalanceArgs,
}

pub struct ParenBalance<'a> {
    src: &'a [u8],
    pub cursor: usize,
    compartment: ParenBalanceComp,
    stack: Vec<ParenBalanceComp>,
    pub depth: i32,
}

impl<'a> ParenBalance<'a> {
    pub fn over(src: &'a [u8]) -> Self {
        let compartment = ParenBalanceComp { state: "Scan".to_string(), vars: ParenBalanceVars::Scan {  }, args: ParenBalanceArgs::Scan {  } };
        ParenBalance { src, cursor: 0, compartment, stack: Vec::new(), depth: 0 }
    }

    pub fn scan_at(&mut self, start: usize) -> bool {
        self.cursor = start;
        self.depth = 0;
        self.compartment = ParenBalanceComp { state: "Scan".to_string(), vars: ParenBalanceVars::Scan {  }, args: ParenBalanceArgs::Scan {  } };
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
            let mut __next = ParenBalanceComp { state: "Reject".to_string(), vars: ParenBalanceVars::Reject {  }, args: ParenBalanceArgs::Reject { } };
            self.compartment = __next;
            return Default::default();
        }
        let sk = skip_string(self.src, self.cursor);
        if sk > self.cursor {
            self.cursor = sk;
        } else {
            let b = self.src.fsm_get(self.cursor);
            self.cursor = self.cursor + 1;
            if b == 40 {
                self.depth = self.depth + 1;
            }
            if b == 41 {
                self.depth = self.depth - 1;
                if self.depth == 0 {
                    let mut __next = ParenBalanceComp { state: "Accept".to_string(), vars: ParenBalanceVars::Accept {  }, args: ParenBalanceArgs::Accept { } };
                    self.compartment = __next;
                    return Default::default();
                }
            }
        }
    }

}

