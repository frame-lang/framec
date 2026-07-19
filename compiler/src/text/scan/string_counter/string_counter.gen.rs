use std::collections::HashMap;
use std::any::Any;


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

#[derive(Clone)]
enum StringCounterVars {
    Walk {  },
    Accept {  },
}
#[derive(Clone)]
enum StringCounterArgs {
    Walk {  },
    Accept {  },
}
#[derive(Clone)]
struct StringCounterComp {
    state: String,
    vars: StringCounterVars,
    args: StringCounterArgs,
}

pub struct StringCounter<'a> {
    src: &'a [u8],
    pub cursor: usize,
    compartment: StringCounterComp,
    stack: Vec<StringCounterComp>,
    pub count: i32,
}

impl<'a> StringCounter<'a> {
    pub fn over(src: &'a [u8]) -> Self {
        let compartment = StringCounterComp { state: "Walk".to_string(), vars: StringCounterVars::Walk {  }, args: StringCounterArgs::Walk {  } };
        StringCounter { src, cursor: 0, compartment, stack: Vec::new(), count: 0 }
    }

    pub fn scan_at(&mut self, start: usize) -> bool {
        self.cursor = start;
        self.count = 0;
        self.compartment = StringCounterComp { state: "Walk".to_string(), vars: StringCounterVars::Walk {  }, args: StringCounterArgs::Walk {  } };
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
            let mut __next = StringCounterComp { state: "Accept".to_string(), vars: StringCounterVars::Accept {  }, args: StringCounterArgs::Accept { } };
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

