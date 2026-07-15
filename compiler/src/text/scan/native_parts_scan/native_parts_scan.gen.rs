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


// The native-code ISLAND DISPATCH, dogfooded as an `@@[scan(u8)]` system — the `@@system`
// analogue of the hand `native_parts`. It walks native code and, at each position, tries the
// recognizers IN ORDER (comment, literal, instantiation, embed-call, ref); the first hit is
// flushed as a part (with the preceding text) and the cursor jumps past it. framec owns the
// WALK ($Walk + the step loop); the `try_island` leaf COMPOSES the InstScan / EmbedScan /
// RefScan systems (and the lexer for comments/literals) — so this is composition four deep:
// NativePartsScan -> {InstScan, EmbedScan} -> ParenBalance -> StringScan. `target` is config.
//
// parts: (kind, start, end) — kind 0=Text 1=Literal 2=Ref 3=Instantiate 4=EmbedCall.
//
// Regen: framec-ng -l rust --emit native_parts_scan.frs | grep -v '^#!\[allow' > native_parts_scan.gen.rs

pub trait NativePartsScanInput { fn fsm_get(&self, i: usize) -> u8; fn fsm_len(&self) -> usize; }
impl NativePartsScanInput for &[u8] { fn fsm_get(&self, i: usize) -> u8 { self[i] } fn fsm_len(&self) -> usize { self.len() } }

pub struct NativePartsScan<'a> {
    src: &'a [u8],
    pub cursor: usize,
    compartment: Compartment,
    stack: Vec<Compartment>,
    pub target: Target,
    pub parts: Vec<(i32, usize, usize)>,
    pub text_start: usize,
}

impl<'a> NativePartsScan<'a> {
    pub fn over(src: &'a [u8], target: Target) -> Self {
        let mut compartment = Compartment::new("Walk");
        NativePartsScan { src, cursor: 0, compartment, stack: Vec::new(), target: target, parts: Vec::new(), text_start: 0 }
    }

    pub fn scan_at(&mut self, start: usize) -> bool {
        self.cursor = start;
        self.parts = Vec::new();
        self.text_start = 0;
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
                    flush_text(&mut self.parts, self.text_start, self.cursor);
            let mut __next = Compartment::new("Accept");
            self.compartment = __next;
            return Default::default();
        }
                let hit = try_island(self.src, self.cursor, self.target);
                if hit.0 != 0 {
                    flush_text(&mut self.parts, self.text_start, self.cursor);
                    record_part(&mut self.parts, hit.0, self.cursor, hit.1);
                    self.cursor = hit.1;
                    self.text_start = self.cursor;
                } else {
                    self.cursor = self.cursor + 1;
                }
    }

}

