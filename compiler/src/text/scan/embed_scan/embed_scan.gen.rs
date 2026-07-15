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


// Embedded-system-call recognizer, dogfooded as an `@@[scan(u8)]` system — the `@@system`
// analogue of the hand `embed_call_at`. From the cursor it recognizes
// `@@:self.<field>.<method>(args)`: the `@@:self.` head, a FIELD ident, a `.`, a METHOD
// ident, then a balanced `(...)`. framec owns the SHAPE ($Field/$Method run the ident loops
// and gate on `.`/`(`); the balanced arg extent is found by COMPOSING string-aware
// ParenBalance as a leaf. A bare `@@:self.a.b` (no parens) is a field read, not a call — so
// the `(` is required.
//
// Regen: framec-ng -l rust --emit embed_scan.frs | grep -v '^#!\[allow' > embed_scan.gen.rs

pub trait EmbedScanInput { fn fsm_get(&self, i: usize) -> u8; fn fsm_len(&self) -> usize; }
impl EmbedScanInput for &[u8] { fn fsm_get(&self, i: usize) -> u8 { self[i] } fn fsm_len(&self) -> usize { self.len() } }

pub struct EmbedScan<'a> {
    src: &'a [u8],
    pub cursor: usize,
    compartment: Compartment,
    stack: Vec<Compartment>,
    pub field_start: usize,
    pub field_end: usize,
    pub method_start: usize,
    pub method_end: usize,
    pub paren_open: usize,
}

impl<'a> EmbedScan<'a> {
    pub fn over(src: &'a [u8]) -> Self {
        let mut compartment = Compartment::new("Start");
        EmbedScan { src, cursor: 0, compartment, stack: Vec::new(), field_start: 0, field_end: 0, method_start: 0, method_end: 0, paren_open: 0 }
    }

    pub fn scan_at(&mut self, start: usize) -> bool {
        self.cursor = start;
        self.field_start = 0;
        self.field_end = 0;
        self.method_start = 0;
        self.method_end = 0;
        self.paren_open = 0;
        let mut compartment = Compartment::new("Start");
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
            "Start" => self.Start_step(),
            "Field" => self.Field_step(),
            "Method" => self.Method_step(),
            _ => {}
        }
    }

    fn Start_step(&mut self) {
        if starts_self_dot(self.src, self.cursor) {
                    self.cursor = self.cursor + 8;
                    self.field_start = self.cursor;
            let mut __next = Compartment::new("Field");
            self.compartment = __next;
            return Default::default();
        }
        let mut __next = Compartment::new("Reject");
        self.compartment = __next;
        return Default::default();
    }

    fn Field_step(&mut self) {
        if is_ident_at(self.src, self.cursor) {
                    self.cursor = self.cursor + 1;
                } else {
                    if self.cursor == self.field_start {
                let mut __next = Compartment::new("Reject");
                self.compartment = __next;
                return Default::default();
            }
                    if is_dot_at(self.src, self.cursor) {
                        self.field_end = self.cursor;
                        self.cursor = self.cursor + 1;
                        self.method_start = self.cursor;
                let mut __next = Compartment::new("Method");
                self.compartment = __next;
                return Default::default();
            }
            let mut __next = Compartment::new("Reject");
            self.compartment = __next;
            return Default::default();
        }
    }

    fn Method_step(&mut self) {
        if is_ident_at(self.src, self.cursor) {
                    self.cursor = self.cursor + 1;
                } else {
                    if self.cursor == self.method_start {
                let mut __next = Compartment::new("Reject");
                self.compartment = __next;
                return Default::default();
            }
                    self.method_end = self.cursor;
                    let p = skip_ws_at(self.src, self.cursor);
                    if is_open_paren_at(self.src, p) {
                        self.paren_open = p;
                        let e = paren_end(self.src, p);
                        if e > p {
                            self.cursor = e;
                    let mut __next = Compartment::new("Accept");
                    self.compartment = __next;
                    return Default::default();
                }
                let mut __next = Compartment::new("Reject");
                self.compartment = __next;
                return Default::default();
            }
            let mut __next = Compartment::new("Reject");
            self.compartment = __next;
            return Default::default();
        }
    }

}

