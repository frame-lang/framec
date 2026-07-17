use std::collections::HashMap;
use std::any::Any;


// Rust raw-string EXTENT recognizer, dogfooded as an `@@[scan(u8)]` COUNTER automaton — the
// `@@system` that replaces the hand `Lexer::rust_raw`. Recognizes `r"…"`, `r#"…"#`,
// `r##"…"##`, and the `b`/`br` byte-string variants: an optional `b`, then `r`, then N `#`,
// then `"`; the close is `"` followed by EXACTLY N `#`. No escapes, no interpolation. Counting
// the openers and matching the same count of closers is the counter the journal blesses — the
// hand lexer's own comment says the hash count "cannot be expressed in a table". Leaves the
// extent one past the close in `cursor`; rejects if it is not a raw string here (e.g. `read`).
//
// A `"` inside the body that is NOT followed by N `#` is content: it is consumed and scanning
// continues — no backtrack, so it stays a forward counter machine.
//
// Regen: framec-ng -l rust --emit raw_string.frs | grep -v '^#!\[allow' > raw_string.gen.rs

pub trait RawStringInput { fn fsm_get(&self, i: usize) -> u8; fn fsm_len(&self) -> usize; }
impl RawStringInput for &[u8] { fn fsm_get(&self, i: usize) -> u8 { self[i] } fn fsm_len(&self) -> usize { self.len() } }

#[derive(Clone)]
enum RawStringVars {
    Start {  },
    ExpectR {  },
    CountOpen {  },
    Body {  },
    MaybeClose {  },
    Accept {  },
    Reject {  },
}
#[derive(Clone)]
enum RawStringArgs {
    Start {  },
    ExpectR {  },
    CountOpen {  },
    Body {  },
    MaybeClose {  },
    Accept {  },
    Reject {  },
}
#[derive(Clone)]
struct RawStringComp {
    state: String,
    vars: RawStringVars,
    args: RawStringArgs,
}

pub struct RawString<'a> {
    src: &'a [u8],
    pub cursor: usize,
    compartment: RawStringComp,
    stack: Vec<RawStringComp>,
    pub hashes: i32,
    pub seen: i32,
}

impl<'a> RawString<'a> {
    pub fn over(src: &'a [u8]) -> Self {
        let compartment = RawStringComp { state: "Start".to_string(), vars: RawStringVars::Start {  }, args: RawStringArgs::Start {  } };
        RawString { src, cursor: 0, compartment, stack: Vec::new(), hashes: 0, seen: 0 }
    }

    pub fn scan_at(&mut self, start: usize) -> bool {
        self.cursor = start;
        self.hashes = 0;
        self.seen = 0;
        self.compartment = RawStringComp { state: "Start".to_string(), vars: RawStringVars::Start {  }, args: RawStringArgs::Start {  } };
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
            "ExpectR" => self.ExpectR_step(),
            "CountOpen" => self.CountOpen_step(),
            "Body" => self.Body_step(),
            "MaybeClose" => self.MaybeClose_step(),
            _ => {}
        }
    }

    fn Start_step(&mut self) {
        if self.src.fsm_get(self.cursor) == 98 {
            self.cursor = self.cursor + 1;
        }
        let mut __next = RawStringComp { state: "ExpectR".to_string(), vars: RawStringVars::ExpectR {  }, args: RawStringArgs::ExpectR { } };
        self.compartment = __next;
        return Default::default();
    }

    fn ExpectR_step(&mut self) {
        if self.cursor >= self.src.fsm_len() {
            let mut __next = RawStringComp { state: "Reject".to_string(), vars: RawStringVars::Reject {  }, args: RawStringArgs::Reject { } };
            self.compartment = __next;
            return Default::default();
        }
        if self.src.fsm_get(self.cursor) != 114 {
            let mut __next = RawStringComp { state: "Reject".to_string(), vars: RawStringVars::Reject {  }, args: RawStringArgs::Reject { } };
            self.compartment = __next;
            return Default::default();
        }
        self.cursor = self.cursor + 1;
        let mut __next = RawStringComp { state: "CountOpen".to_string(), vars: RawStringVars::CountOpen {  }, args: RawStringArgs::CountOpen { } };
        self.compartment = __next;
        return Default::default();
    }

    fn CountOpen_step(&mut self) {
        if self.cursor >= self.src.fsm_len() {
            let mut __next = RawStringComp { state: "Reject".to_string(), vars: RawStringVars::Reject {  }, args: RawStringArgs::Reject { } };
            self.compartment = __next;
            return Default::default();
        }
        let b = self.src.fsm_get(self.cursor);
        if b == 35 {
            self.hashes = self.hashes + 1;
            self.cursor = self.cursor + 1;
            let mut __next = RawStringComp { state: "CountOpen".to_string(), vars: RawStringVars::CountOpen {  }, args: RawStringArgs::CountOpen { } };
            self.compartment = __next;
            return Default::default();
        }
        if b == 34 {
            self.cursor = self.cursor + 1;
            let mut __next = RawStringComp { state: "Body".to_string(), vars: RawStringVars::Body {  }, args: RawStringArgs::Body { } };
            self.compartment = __next;
            return Default::default();
        }
        let mut __next = RawStringComp { state: "Reject".to_string(), vars: RawStringVars::Reject {  }, args: RawStringArgs::Reject { } };
        self.compartment = __next;
        return Default::default();
    }

    fn Body_step(&mut self) {
        if self.cursor >= self.src.fsm_len() {
            let mut __next = RawStringComp { state: "Reject".to_string(), vars: RawStringVars::Reject {  }, args: RawStringArgs::Reject { } };
            self.compartment = __next;
            return Default::default();
        }
        if self.src.fsm_get(self.cursor) == 34 {
            self.seen = 0;
            self.cursor = self.cursor + 1;
            let mut __next = RawStringComp { state: "MaybeClose".to_string(), vars: RawStringVars::MaybeClose {  }, args: RawStringArgs::MaybeClose { } };
            self.compartment = __next;
            return Default::default();
        }
        self.cursor = self.cursor + 1;
    }

    fn MaybeClose_step(&mut self) {
        if self.seen == self.hashes {
            let mut __next = RawStringComp { state: "Accept".to_string(), vars: RawStringVars::Accept {  }, args: RawStringArgs::Accept { } };
            self.compartment = __next;
            return Default::default();
        }
        if self.cursor >= self.src.fsm_len() {
            let mut __next = RawStringComp { state: "Reject".to_string(), vars: RawStringVars::Reject {  }, args: RawStringArgs::Reject { } };
            self.compartment = __next;
            return Default::default();
        }
        if self.src.fsm_get(self.cursor) == 35 {
            self.seen = self.seen + 1;
            self.cursor = self.cursor + 1;
            let mut __next = RawStringComp { state: "MaybeClose".to_string(), vars: RawStringVars::MaybeClose {  }, args: RawStringArgs::MaybeClose { } };
            self.compartment = __next;
            return Default::default();
        }
        let mut __next = RawStringComp { state: "Body".to_string(), vars: RawStringVars::Body {  }, args: RawStringArgs::Body { } };
        self.compartment = __next;
        return Default::default();
    }

}

