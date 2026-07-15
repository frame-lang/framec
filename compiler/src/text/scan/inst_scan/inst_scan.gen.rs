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


// System-instantiation recognizer, dogfooded as an `@@[scan(u8)]` system — the `@@system`
// analogue of the hand `instantiation_at`. From the cursor it recognizes `@@Name(args)` (or
// `@@!Name(args)`): `@@` not followed by `:`, an optional `!`, an identifier NAME, then a
// balanced `(...)` arg list. framec owns the SHAPE (dispatch + the ident-run loop); the
// balanced arg extent is found by COMPOSING the (string-aware) ParenBalance system as a
// leaf. Arg PARSING (into groups) stays a native leaf in the wrapper.
//
// Regen: framec-ng -l rust --emit inst_scan.frs | grep -v '^#!\[allow' > inst_scan.gen.rs

pub trait InstScanInput { fn fsm_get(&self, i: usize) -> u8; fn fsm_len(&self) -> usize; }
impl InstScanInput for &[u8] { fn fsm_get(&self, i: usize) -> u8 { self[i] } fn fsm_len(&self) -> usize { self.len() } }

pub struct InstScan<'a> {
    src: &'a [u8],
    pub cursor: usize,
    compartment: Compartment,
    stack: Vec<Compartment>,
    pub name_start: usize,
    pub name_end: usize,
}

impl<'a> InstScan<'a> {
    pub fn over(src: &'a [u8]) -> Self {
        let mut compartment = Compartment::new("Start");
        InstScan { src, cursor: 0, compartment, stack: Vec::new(), name_start: 0, name_end: 0 }
    }

    pub fn scan_at(&mut self, start: usize) -> bool {
        self.cursor = start;
        self.name_start = 0;
        self.name_end = 0;
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
            "Name" => self.Name_step(),
            _ => {}
        }
    }

    fn Start_step(&mut self) {
        if is_inst_start(self.src, self.cursor) {
                    self.name_start = after_at_bang(self.src, self.cursor);
                    self.cursor = self.name_start;
            let mut __next = Compartment::new("Name");
            self.compartment = __next;
            return Default::default();
        }
        let mut __next = Compartment::new("Reject");
        self.compartment = __next;
        return Default::default();
    }

    fn Name_step(&mut self) {
        if is_ident_at(self.src, self.cursor) {
                    self.cursor = self.cursor + 1;
                } else {
                    self.name_end = self.cursor;
                    let p = skip_ws_at(self.src, self.cursor);
                    if is_open_paren_at(self.src, p) {
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

