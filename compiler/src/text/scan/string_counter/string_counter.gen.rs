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


// COMPOSITION PROOF: a scan system that walks the whole input and skips each `"`-string by
// calling a native leaf `skip_string`, which invokes the StringScan SYSTEM. framec owns the
// walk (states + the step loop); the leaf does the transformation (run the sub-scanner).
// This is the mechanism the Segmenter needs — skip opaque regions mid-walk by composing a
// sub-scanner over the SAME borrowed `&[u8]` (no buffer copy). `count` is the number of
// strings skipped.
//
// Regen: framec-ng -l rust --emit string_counter.frs | grep -v '^#!\[allow' > string_counter.gen.rs

pub trait StringCounterInput { fn fsm_get(&self, i: usize) -> u8; fn fsm_len(&self) -> usize; }
impl StringCounterInput for &[u8] { fn fsm_get(&self, i: usize) -> u8 { self[i] } fn fsm_len(&self) -> usize { self.len() } }

pub struct StringCounter<'a> {
    src: &'a [u8],
    pub cursor: usize,
    compartment: Compartment,
    stack: Vec<Compartment>,
    pub count: i32,
}

impl<'a> StringCounter<'a> {
    pub fn over(src: &'a [u8]) -> Self {
        let mut compartment = Compartment::new("Walk");
        StringCounter { src, cursor: 0, compartment, stack: Vec::new(), count: 0 }
    }

    pub fn scan_at(&mut self, start: usize) -> bool {
        self.cursor = start;
        self.count = 0;
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
                let b = self.src.fsm_get(self.cursor);
                if b == 34 {
                    self.cursor = skip_string(self.src, self.cursor);
                    self.count = self.count + 1;
                } else {
                    self.cursor = self.cursor + 1;
                }
    }

}

