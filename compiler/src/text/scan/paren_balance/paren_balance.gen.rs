use std::collections::HashMap;
use std::any::Any;

struct Compartment {
    state: String,
    state_vars: HashMap<String, Box<dyn Any>>,
    state_args: HashMap<String, Box<dyn Any>>,
}
impl Compartment {
    fn new(state: &str) -> Compartment {
        Compartment { state: state.to_string(), state_vars: HashMap::new(), state_args: HashMap::new() }
    }
}


// Balanced-`()` extent recognizer, dogfooded as an `@@[scan(u8)]` system. A COUNTER
// automaton (the journal's point: framec's bracket scanners count openers against closers;
// they are not kind-matched pushdowns). Matches `Lexer`-less `balanced(bytes, i, '(', ')')`
// for the string-free case: from a `(` at the cursor, find the matching `)`, leaving cursor
// one past it. `depth` is a domain counter; `scan_at` resets it per scan.
//
// Regen: framec-ng -l rust --emit paren_balance.frs | grep -v '^#!\[allow' > paren_balance.gen.rs

pub trait ParenBalanceInput { fn fsm_get(&self, i: usize) -> u8; fn fsm_len(&self) -> usize; }
impl ParenBalanceInput for &[u8] { fn fsm_get(&self, i: usize) -> u8 { self[i] } fn fsm_len(&self) -> usize { self.len() } }

pub struct ParenBalance<'a> {
    src: &'a [u8],
    pub cursor: usize,
    compartment: Compartment,
    stack: Vec<Compartment>,
    pub depth: i32,
}

impl<'a> ParenBalance<'a> {
    pub fn over(src: &'a [u8]) -> Self {
        let mut compartment = Compartment::new("Scan");
        ParenBalance { src, cursor: 0, compartment, stack: Vec::new(), depth: 0 }
    }

    pub fn scan_at(&mut self, start: usize) -> bool {
        self.cursor = start;
        self.depth = 0;
        let mut compartment = Compartment::new("Scan");
        self.compartment = compartment;
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
            let mut __next = Compartment::new("Reject");
            self.compartment = __next;
            return Default::default();
        }
                let b = self.src.fsm_get(self.cursor);
                self.cursor = self.cursor + 1;
                if b == 40 {
                    self.depth = self.depth + 1;
                }
                if b == 41 {
                    self.depth = self.depth - 1;
                    if self.depth == 0 {
                let mut __next = Compartment::new("Accept");
                self.compartment = __next;
                return Default::default();
            }
                }
    }

}

