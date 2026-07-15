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


// The section backbone, dogfooded as an `@@[scan(u8)]` system — the `@@system` analogue of
// the shipping SystemBackbone's section dispatch. It walks a system body and finds the
// section keywords (interface / machine / domain / actions / operations) at brace depth 0,
// skipping strings/comments and nested braces. framec owns the WALK ($Walk + step loop + the
// depth counter); the leaves skip opaque (via the one lexer, so a `machine:` in a string is
// not a section) and match/record the keyword. `target` is config; `depth` is the counter.
//
// Regen: framec-ng -l rust --emit section_scan.frs | grep -v '^#!\[allow' > section_scan.gen.rs

pub trait SectionScanInput { fn fsm_get(&self, i: usize) -> u8; fn fsm_len(&self) -> usize; }
impl SectionScanInput for &[u8] { fn fsm_get(&self, i: usize) -> u8 { self[i] } fn fsm_len(&self) -> usize { self.len() } }

pub struct SectionScan<'a> {
    src: &'a [u8],
    pub cursor: usize,
    compartment: Compartment,
    stack: Vec<Compartment>,
    pub target: Target,
    pub depth: i32,
    pub starts: Vec<(usize, usize, usize)>,
}

impl<'a> SectionScan<'a> {
    pub fn over(src: &'a [u8], target: Target) -> Self {
        let mut compartment = Compartment::new("Walk");
        SectionScan { src, cursor: 0, compartment, stack: Vec::new(), target: target, depth: 0, starts: Vec::new() }
    }

    pub fn scan_at(&mut self, start: usize) -> bool {
        self.cursor = start;
        self.depth = 0;
        self.starts = Vec::new();
        let mut compartment = Compartment::new("Walk");
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
            "Walk" => self.Walk_step(),
            _ => {}
        }
    }

    fn Walk_step(&mut self) {
        if self.cursor >= self.src.fsm_len() {
            let mut __next = Compartment::new("Accept");
            self.compartment = __next;
            return Default::default();
        }
                let sk = skip_opaque(self.src, self.cursor, self.target);
                if sk > self.cursor {
                    self.cursor = sk;
                } else {
                    let b = self.src.fsm_get(self.cursor);
                    if b == 123 {
                        self.depth = self.depth + 1;
                    }
                    if b == 125 {
                        self.depth = self.depth - 1;
                    }
                    if self.depth == 0 {
                        record_kw(&mut self.starts, self.src, self.cursor);
                    }
                    self.cursor = self.cursor + 1;
                }
    }

}

