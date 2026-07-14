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


// Frame-reference recognizer, dogfooded as an `@@[scan(u8)]` system — the `@@system`
// analogue of the hand `frame_ref_at`. From the cursor it recognizes `$.name` (a state var)
// or `@@:word` (a context ref: self / data / params / return / event / system), classifies
// the kind, and leaves the name extent + end in domain fields. framec owns the SHAPE
// recognition (states + the ident-run loop); the leaves are trivial byte predicates and the
// kind lookup. A differential test proves it agrees with `frame_ref_at` at every position.
//
// kind: 0=none 1=StateVar 2=ContextSelf 3=ContextData 4=ContextParams 5=ContextReturn
//       6=ContextEvent 7=ContextSystemState
//
// Regen: framec-ng -l rust --emit ref_scan.frs | grep -v '^#!\[allow' > ref_scan.gen.rs

pub trait RefScanInput { fn fsm_get(&self, i: usize) -> u8; fn fsm_len(&self) -> usize; }
impl RefScanInput for &[u8] { fn fsm_get(&self, i: usize) -> u8 { self[i] } fn fsm_len(&self) -> usize { self.len() } }

pub struct RefScan<'a> {
    src: &'a [u8],
    pub cursor: usize,
    compartment: Compartment,
    stack: Vec<Compartment>,
    pub kind: i32,
    pub name_start: usize,
    pub word_start: usize,
    pub name_out: usize,
    pub name_end: usize,
}

impl<'a> RefScan<'a> {
    pub fn over(src: &'a [u8]) -> Self {
        let mut compartment = Compartment::new("Start");
        RefScan { src, cursor: 0, compartment, stack: Vec::new(), kind: 0, name_start: 0, word_start: 0, name_out: 0, name_end: 0 }
    }

    pub fn scan_at(&mut self, start: usize) -> bool {
        self.cursor = start;
        self.kind = 0;
        self.name_start = 0;
        self.word_start = 0;
        self.name_out = 0;
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
            "StateVarName" => self.StateVarName_step(),
            "ContextWord" => self.ContextWord_step(),
            _ => {}
        }
    }

    fn Start_step(&mut self) {
        if starts_statevar(self.src, self.cursor) {
                    self.cursor = self.cursor + 2;
                    self.name_start = self.cursor;
            let mut __next = Compartment::new("StateVarName");
            self.compartment = __next;
            return Default::default();
        }
                if starts_context(self.src, self.cursor) {
                    self.cursor = self.cursor + 3;
                    self.word_start = self.cursor;
            let mut __next = Compartment::new("ContextWord");
            self.compartment = __next;
            return Default::default();
        }
        let mut __next = Compartment::new("Reject");
        self.compartment = __next;
        return Default::default();
    }

    fn StateVarName_step(&mut self) {
        if is_ident_at(self.src, self.cursor) {
                    self.cursor = self.cursor + 1;
                } else {
                    if self.cursor == self.name_start {
                let mut __next = Compartment::new("Reject");
                self.compartment = __next;
                return Default::default();
            }
                    self.kind = 1;
                    self.name_out = self.name_start;
                    self.name_end = self.cursor;
            let mut __next = Compartment::new("Accept");
            self.compartment = __next;
            return Default::default();
        }
    }

    fn ContextWord_step(&mut self) {
        if is_ident_or_dot_at(self.src, self.cursor) {
                    self.cursor = self.cursor + 1;
                } else {
                    if self.cursor == self.word_start {
                let mut __next = Compartment::new("Reject");
                self.compartment = __next;
                return Default::default();
            }
                    self.kind = classify_context(self.src, self.word_start, self.cursor);
                    self.name_out = name_start_ctx(self.src, self.word_start, self.cursor);
                    self.name_end = self.cursor;
            let mut __next = Compartment::new("Accept");
            self.compartment = __next;
            return Default::default();
        }
    }

}

