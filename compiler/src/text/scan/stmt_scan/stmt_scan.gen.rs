use std::collections::HashMap;
use std::any::Any;


// The statement-level classifier, dogfooded as an `@@[scan(u8)]` system — the `@@system`
// analogue of the hand `frame_stmt`'s dispatch. At the start of a statement it classifies the
// Frame construct: `push$` (StackPush), `(exit) ->` / `->` (Transition or StackPop, with the
// $Target guard), `=>` (Forward), else native. framec owns the DISPATCH (a state per leading
// token); the leaves reuse the exact hand sub-logic (balanced parens, end-of-line, the
// $Target guard) so there is no drift.
//
// kind: 0=none 1=Transition 2=StackPush 3=StackPop 4=Forward 5=StackPopBare
//
// Regen: framec-ng -l rust --emit stmt_scan.frs | grep -v '^#!\[allow' > stmt_scan.gen.rs

pub trait StmtScanInput { fn fsm_get(&self, i: usize) -> u8; fn fsm_len(&self) -> usize; }
impl StmtScanInput for &[u8] { fn fsm_get(&self, i: usize) -> u8 { self[i] } fn fsm_len(&self) -> usize { self.len() } }

#[derive(Clone)]
enum StmtScanVars {
    Start {  },
    ExitParen {  },
    ArrowBare {  },
    Accept {  },
    Reject {  },
}
#[derive(Clone)]
enum StmtScanArgs {
    Start {  },
    ExitParen {  },
    ArrowBare {  },
    Accept {  },
    Reject {  },
}
#[derive(Clone)]
struct StmtScanComp {
    state: String,
    vars: StmtScanVars,
    args: StmtScanArgs,
}

pub struct StmtScan<'a> {
    src: &'a [u8],
    pub cursor: usize,
    compartment: StmtScanComp,
    stack: Vec<StmtScanComp>,
    pub kind: i32,
    pub end_out: usize,
}

impl<'a> StmtScan<'a> {
    pub fn over(src: &'a [u8]) -> Self {
        let compartment = StmtScanComp { state: "Start".to_string(), vars: StmtScanVars::Start {  }, args: StmtScanArgs::Start {  } };
        StmtScan { src, cursor: 0, compartment, stack: Vec::new(), kind: 0, end_out: 0 }
    }

    pub fn scan_at(&mut self, start: usize) -> bool {
        self.cursor = start;
        self.kind = 0;
        self.end_out = 0;
        self.compartment = StmtScanComp { state: "Start".to_string(), vars: StmtScanVars::Start {  }, args: StmtScanArgs::Start {  } };
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
            "ExitParen" => self.ExitParen_step(),
            "ArrowBare" => self.ArrowBare_step(),
            _ => {}
        }
    }

    fn Start_step(&mut self) {
        if starts_push(self.src, self.cursor) {
            self.kind = 2;
            self.end_out = eol(self.src, self.cursor);
            let mut __next = StmtScanComp { state: "Accept".to_string(), vars: StmtScanVars::Accept {  }, args: StmtScanArgs::Accept { } };
            self.compartment = __next;
            return Default::default();
        }
        if starts_pop(self.src, self.cursor) {
            self.kind = 5;
            self.end_out = eol(self.src, self.cursor);
            let mut __next = StmtScanComp { state: "Accept".to_string(), vars: StmtScanVars::Accept {  }, args: StmtScanArgs::Accept { } };
            self.compartment = __next;
            return Default::default();
        }
        if is_open_paren(self.src, self.cursor) {
            let mut __next = StmtScanComp { state: "ExitParen".to_string(), vars: StmtScanVars::ExitParen {  }, args: StmtScanArgs::ExitParen { } };
            self.compartment = __next;
            return Default::default();
        }
        if starts_arrow(self.src, self.cursor) {
            let mut __next = StmtScanComp { state: "ArrowBare".to_string(), vars: StmtScanVars::ArrowBare {  }, args: StmtScanArgs::ArrowBare { } };
            self.compartment = __next;
            return Default::default();
        }
        if starts_fatarrow(self.src, self.cursor) {
            self.kind = 4;
            self.end_out = eol(self.src, self.cursor);
            let mut __next = StmtScanComp { state: "Accept".to_string(), vars: StmtScanVars::Accept {  }, args: StmtScanArgs::Accept { } };
            self.compartment = __next;
            return Default::default();
        }
        let mut __next = StmtScanComp { state: "Reject".to_string(), vars: StmtScanVars::Reject {  }, args: StmtScanArgs::Reject { } };
        self.compartment = __next;
        return Default::default();
    }

    fn ExitParen_step(&mut self) {
        let close = balanced_close(self.src, self.cursor);
        if close > self.cursor {
            let j = skip_ws(self.src, close);
            if starts_arrow(self.src, j) {
                let e = eol(self.src, self.cursor);
                if has_pop(self.src, j, e) {
                    self.kind = 3;
                    self.end_out = e;
                    let mut __next = StmtScanComp { state: "Accept".to_string(), vars: StmtScanVars::Accept {  }, args: StmtScanArgs::Accept { } };
                    self.compartment = __next;
                    return Default::default();
                }
                if arrow_target(self.src, j + 2, e) {
                    self.kind = 1;
                    self.end_out = e;
                    let mut __next = StmtScanComp { state: "Accept".to_string(), vars: StmtScanVars::Accept {  }, args: StmtScanArgs::Accept { } };
                    self.compartment = __next;
                    return Default::default();
                }
                let mut __next = StmtScanComp { state: "Reject".to_string(), vars: StmtScanVars::Reject {  }, args: StmtScanArgs::Reject { } };
                self.compartment = __next;
                return Default::default();
            }
            let mut __next = StmtScanComp { state: "Reject".to_string(), vars: StmtScanVars::Reject {  }, args: StmtScanArgs::Reject { } };
            self.compartment = __next;
            return Default::default();
        }
        let mut __next = StmtScanComp { state: "Reject".to_string(), vars: StmtScanVars::Reject {  }, args: StmtScanArgs::Reject { } };
        self.compartment = __next;
        return Default::default();
    }

    fn ArrowBare_step(&mut self) {
        let e = eol(self.src, self.cursor);
        if has_pop(self.src, self.cursor, e) {
            self.kind = 3;
            self.end_out = e;
            let mut __next = StmtScanComp { state: "Accept".to_string(), vars: StmtScanVars::Accept {  }, args: StmtScanArgs::Accept { } };
            self.compartment = __next;
            return Default::default();
        }
        self.kind = 1;
        self.end_out = e;
        let mut __next = StmtScanComp { state: "Accept".to_string(), vars: StmtScanVars::Accept {  }, args: StmtScanArgs::Accept { } };
        self.compartment = __next;
        return Default::default();
    }

}

