use std::collections::HashMap;
use std::any::Any;


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

#[derive(Clone)]
enum InstScanVars {
    Start {  },
    Name {  },
    Accept {  },
    Reject {  },
}
#[derive(Clone)]
enum InstScanArgs {
    Start {  },
    Name {  },
    Accept {  },
    Reject {  },
}
#[derive(Clone)]
struct InstScanComp {
    state: String,
    vars: InstScanVars,
    args: InstScanArgs,
}

pub struct InstScan<'a> {
    src: &'a [u8],
    pub cursor: usize,
    compartment: InstScanComp,
    stack: Vec<InstScanComp>,
    pub name_start: usize,
    pub name_end: usize,
}

impl<'a> InstScan<'a> {
    pub fn over(src: &'a [u8]) -> Self {
        let compartment = InstScanComp { state: "Start".to_string(), vars: InstScanVars::Start {  }, args: InstScanArgs::Start {  } };
        InstScan { src, cursor: 0, compartment, stack: Vec::new(), name_start: 0, name_end: 0 }
    }

    pub fn scan_at(&mut self, start: usize) -> bool {
        self.cursor = start;
        self.name_start = 0;
        self.name_end = 0;
        self.compartment = InstScanComp { state: "Start".to_string(), vars: InstScanVars::Start {  }, args: InstScanArgs::Start {  } };
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
            let mut __next = InstScanComp { state: "Name".to_string(), vars: InstScanVars::Name {  }, args: InstScanArgs::Name { } };
            self.compartment = __next;
            return Default::default();
        }
        let mut __next = InstScanComp { state: "Reject".to_string(), vars: InstScanVars::Reject {  }, args: InstScanArgs::Reject { } };
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
                    let mut __next = InstScanComp { state: "Accept".to_string(), vars: InstScanVars::Accept {  }, args: InstScanArgs::Accept { } };
                    self.compartment = __next;
                    return Default::default();
                }
                let mut __next = InstScanComp { state: "Reject".to_string(), vars: InstScanVars::Reject {  }, args: InstScanArgs::Reject { } };
                self.compartment = __next;
                return Default::default();
            }
            let mut __next = InstScanComp { state: "Reject".to_string(), vars: InstScanVars::Reject {  }, args: InstScanArgs::Reject { } };
            self.compartment = __next;
            return Default::default();
        }
    }

}

