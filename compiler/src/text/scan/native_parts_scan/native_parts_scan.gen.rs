use std::collections::HashMap;
use std::any::Any;


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

#[derive(Clone)]
enum NativePartsScanVars {
    Walk {  },
    Accept {  },
}
#[derive(Clone)]
enum NativePartsScanArgs {
    Walk {  },
    Accept {  },
}
#[derive(Clone)]
struct NativePartsScanComp {
    state: String,
    vars: NativePartsScanVars,
    args: NativePartsScanArgs,
}

pub struct NativePartsScan<'a> {
    src: &'a [u8],
    pub cursor: usize,
    compartment: NativePartsScanComp,
    stack: Vec<NativePartsScanComp>,
    pub target: Target,
    pub parts: Vec<(i32, usize, usize)>,
    pub text_start: usize,
}

impl<'a> NativePartsScan<'a> {
    pub fn over(src: &'a [u8], target: Target) -> Self {
        let compartment = NativePartsScanComp { state: "Walk".to_string(), vars: NativePartsScanVars::Walk {  }, args: NativePartsScanArgs::Walk {  } };
        NativePartsScan { src, cursor: 0, compartment, stack: Vec::new(), target: target, parts: Vec::new(), text_start: 0 }
    }

    pub fn scan_at(&mut self, start: usize) -> bool {
        self.cursor = start;
        self.parts = Vec::new();
        self.text_start = 0;
        self.compartment = NativePartsScanComp { state: "Walk".to_string(), vars: NativePartsScanVars::Walk {  }, args: NativePartsScanArgs::Walk {  } };
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
            let mut __next = NativePartsScanComp { state: "Accept".to_string(), vars: NativePartsScanVars::Accept {  }, args: NativePartsScanArgs::Accept { } };
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

