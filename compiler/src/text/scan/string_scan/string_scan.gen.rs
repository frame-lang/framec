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


// The quoted-string EXTENT recognizer, dogfooded as an `@@[scan(u8)]` system — the
// `@@system` analogue of the shipping `string_scan_fsm`. It recognizes the same grammar
// as `Lexer::quoted(i, b'"', multiline=false, escapes=true)`: a `"`-delimited string with
// `\`-escapes, where a bare newline is unterminated. It finds the EXTENT (leaves `cursor`
// one past the closing quote); a differential test proves it agrees with the hand lexer at
// every position. This is where the string-blind `in_string: u8` native local used to live
// (#209 / P9); here the mode is a Frame STATE, not a byte that cannot tell `"` from `}`.
//
// Regen: edit this file, run `framec-ng -l rust --emit string_scan.frs > string_scan.gen.rs`.

pub trait StringScanInput { fn fsm_get(&self, i: usize) -> u8; fn fsm_len(&self) -> usize; }
impl StringScanInput for &[u8] { fn fsm_get(&self, i: usize) -> u8 { self[i] } fn fsm_len(&self) -> usize { self.len() } }
impl StringScanInput for Vec<u8> { fn fsm_get(&self, i: usize) -> u8 { self[i] } fn fsm_len(&self) -> usize { self.len() } }
pub struct StringScanFn<F: Fn(usize) -> u8>(pub F, pub usize);
impl<F: Fn(usize) -> u8> StringScanInput for StringScanFn<F> { fn fsm_get(&self, i: usize) -> u8 { (self.0)(i) } fn fsm_len(&self) -> usize { self.1 } }

pub struct StringScan<I: StringScanInput> {
    src: I,
    pub cursor: usize,
    compartment: Compartment,
    stack: Vec<Compartment>,
}

impl<I: StringScanInput> StringScan<I> {
    pub fn over(src: I) -> Self {
        let mut compartment = Compartment::new("Start");
        StringScan { src, cursor: 0, compartment, stack: Vec::new() }
    }

    pub fn scan_at(&mut self, start: usize) -> bool {
        self.cursor = start;
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
            "Body" => self.Body_step(),
            _ => {}
        }
    }

    fn Start_step(&mut self) {
        if self.cursor >= self.src.fsm_len() {
            let mut __next = Compartment::new("Reject");
            self.compartment = __next;
            return Default::default();
        }
                if self.src.fsm_get(self.cursor) != 34 {
            let mut __next = Compartment::new("Reject");
            self.compartment = __next;
            return Default::default();
        }
                self.cursor = self.cursor + 1;
        let mut __next = Compartment::new("Body");
        self.compartment = __next;
        return Default::default();
    }

    fn Body_step(&mut self) {
        if self.cursor >= self.src.fsm_len() {
            let mut __next = Compartment::new("Reject");
            self.compartment = __next;
            return Default::default();
        }
                let b = self.src.fsm_get(self.cursor);
                if b == 92 {
                    self.cursor = self.cursor + 2;
            let mut __next = Compartment::new("Body");
            self.compartment = __next;
            return Default::default();
        }
                if b == 10 {
            let mut __next = Compartment::new("Reject");
            self.compartment = __next;
            return Default::default();
        }
                if b == 34 {
                    self.cursor = self.cursor + 1;
            let mut __next = Compartment::new("Accept");
            self.compartment = __next;
            return Default::default();
        }
                self.cursor = self.cursor + 1;
        let mut __next = Compartment::new("Body");
        self.compartment = __next;
        return Default::default();
    }

}

