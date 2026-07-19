use std::collections::HashMap;
use std::any::Any;


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

#[derive(Clone)]
enum StringScanVars {
    Start {  },
    Body {  },
    Accept {  },
    Reject {  },
}
#[derive(Clone)]
enum StringScanArgs {
    Start {  },
    Body {  },
    Accept {  },
    Reject {  },
}
#[derive(Clone)]
struct StringScanComp {
    state: String,
    vars: StringScanVars,
    args: StringScanArgs,
}

pub struct StringScan<'a> {
    src: &'a [u8],
    pub cursor: usize,
    compartment: StringScanComp,
    stack: Vec<StringScanComp>,
}

impl<'a> StringScan<'a> {
    pub fn over(src: &'a [u8]) -> Self {
        let compartment = StringScanComp { state: "Start".to_string(), vars: StringScanVars::Start {  }, args: StringScanArgs::Start {  } };
        StringScan { src, cursor: 0, compartment, stack: Vec::new() }
    }

    pub fn scan_at(&mut self, start: usize) -> bool {
        self.cursor = start;
        self.compartment = StringScanComp { state: "Start".to_string(), vars: StringScanVars::Start {  }, args: StringScanArgs::Start {  } };
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
            let mut __next = StringScanComp { state: "Reject".to_string(), vars: StringScanVars::Reject {  }, args: StringScanArgs::Reject { } };
            self.compartment = __next;
            return Default::default();
        }
        if self.src.fsm_get(self.cursor) != 34 {
            let mut __next = StringScanComp { state: "Reject".to_string(), vars: StringScanVars::Reject {  }, args: StringScanArgs::Reject { } };
            self.compartment = __next;
            return Default::default();
        }
        self.cursor = self.cursor + 1;
        let mut __next = StringScanComp { state: "Body".to_string(), vars: StringScanVars::Body {  }, args: StringScanArgs::Body { } };
        self.compartment = __next;
        return Default::default();
    }

    fn Body_step(&mut self) {
        if self.cursor >= self.src.fsm_len() {
            let mut __next = StringScanComp { state: "Reject".to_string(), vars: StringScanVars::Reject {  }, args: StringScanArgs::Reject { } };
            self.compartment = __next;
            return Default::default();
        }
        let b = self.src.fsm_get(self.cursor);
        if b == 92 {
            self.cursor = self.cursor + 2;
            let mut __next = StringScanComp { state: "Body".to_string(), vars: StringScanVars::Body {  }, args: StringScanArgs::Body { } };
            self.compartment = __next;
            return Default::default();
        }
        if b == 10 {
            let mut __next = StringScanComp { state: "Reject".to_string(), vars: StringScanVars::Reject {  }, args: StringScanArgs::Reject { } };
            self.compartment = __next;
            return Default::default();
        }
        if b == 34 {
            self.cursor = self.cursor + 1;
            let mut __next = StringScanComp { state: "Accept".to_string(), vars: StringScanVars::Accept {  }, args: StringScanArgs::Accept { } };
            self.compartment = __next;
            return Default::default();
        }
        self.cursor = self.cursor + 1;
        let mut __next = StringScanComp { state: "Body".to_string(), vars: StringScanVars::Body {  }, args: StringScanArgs::Body { } };
        self.compartment = __next;
        return Default::default();
    }

}

