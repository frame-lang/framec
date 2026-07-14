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


// The item-level SEGMENTER walk, dogfooded as an `@@[scan(u8)]` system — the first
// LOAD-BEARING compiler scanner authored as a Frame machine (docs/JOURNAL.md fubar). It
// finds the top-level `@@`-item START offsets, correctly skipping strings/comments (so a
// `@@` inside them is never an item) and skipping over each `@@system`/`@@fsm` BODY (so a
// `@@:self` inside a handler is not a top-level item). `target` is CONSTRUCTION CONFIG (the
// per-target lexical forms) — it survives `scan_at`; `starts` is the accumulated output.
//
// framec owns the WALK (the $Sol/$Mid states + the step loop); the leaves do the
// transformation: `skip_opaque_at`/`item_end_at` reuse the lexer, `at_pragma`/`record` are
// trivial. Target-specific opaque forms come from the config, so it is correct for any
// target — not the string-blind hand loop it replaces.
//
// Regen: framec-ng -l rust --emit segmenter.frs | grep -v '^#!\[allow' > segmenter.gen.rs

pub trait SegmenterInput { fn fsm_get(&self, i: usize) -> u8; fn fsm_len(&self) -> usize; }
impl SegmenterInput for &[u8] { fn fsm_get(&self, i: usize) -> u8 { self[i] } fn fsm_len(&self) -> usize { self.len() } }

pub struct Segmenter<'a> {
    src: &'a [u8],
    pub cursor: usize,
    compartment: Compartment,
    stack: Vec<Compartment>,
    pub target: Target,
    pub starts: Vec<usize>,
}

impl<'a> Segmenter<'a> {
    pub fn over(src: &'a [u8], target: Target) -> Self {
        let mut compartment = Compartment::new("Sol");
        Segmenter { src, cursor: 0, compartment, stack: Vec::new(), target: target, starts: Vec::new() }
    }

    pub fn scan_at(&mut self, start: usize) -> bool {
        self.cursor = start;
        self.starts = Vec::new();
        let mut compartment = Compartment::new("Sol");
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
            "Sol" => self.Sol_step(),
            "Mid" => self.Mid_step(),
            _ => {}
        }
    }

    fn Sol_step(&mut self) {
        if self.cursor >= self.src.fsm_len() {
            let mut __next = Compartment::new("Accept");
            self.compartment = __next;
            return Default::default();
        }
                let b = self.src.fsm_get(self.cursor);
                if b == 32 || b == 9 {
                    self.cursor = self.cursor + 1;
                } else if at_pragma(self.src, self.cursor) {
                    record(&mut self.starts, self.cursor);
                    self.cursor = item_end_at(self.src, self.cursor, self.target);
                } else if b == 10 {
                    self.cursor = self.cursor + 1;
                } else {
            let mut __next = Compartment::new("Mid");
            self.compartment = __next;
            return Default::default();
        }
    }

    fn Mid_step(&mut self) {
        if self.cursor >= self.src.fsm_len() {
            let mut __next = Compartment::new("Accept");
            self.compartment = __next;
            return Default::default();
        }
                let sk = skip_opaque_at(self.src, self.cursor, self.target);
                if sk > self.cursor {
                    self.cursor = sk;
                } else {
                    let b = self.src.fsm_get(self.cursor);
                    self.cursor = self.cursor + 1;
                    if b == 10 {
                let mut __next = Compartment::new("Sol");
                self.compartment = __next;
                return Default::default();
            }
                }
    }

}

