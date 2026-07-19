use std::collections::HashMap;
use std::any::Any;


// The full string+comment+literal EXTENT skipper (OpaqueScan), dogfooded as an `@@[scan(u8)]` system —
// the `@@system` analogue of the shipping `string_scan_fsm` + the per-target native-region
// skippers. It recognizes, at the cursor, exactly what `Lexer::comment_at`/`literal_at`
// recognize for the four cleanroom targets (python/java/rust/c), and leaves the EXTENT END in
// `cursor` (accept) or rejects (no string/comment opens here). This is where the string-blind
// `in_string: u8` native local used to live (#209 / P9): the mode is a Frame STATE, not a byte
// that cannot tell `"` from `}`.
//
// framec owns the WALK (dispatch + the per-body scan loops + the nesting depth counter); the
// native leaves answer only per-target FACTS ("does form X open at i", "is this the close") —
// no walk lives in a leaf. Every consumer of the hand lexer used only the extent (`.span.end`);
// a Python `{…}` is SKIPPED while scanning (it can hide a delim), so `hole_skip` reproduces
// that one effect on the extent.
//
// Registers for Item 4: `holes` accumulates each Python `{…}` content-span at the hole_skip
// sites (single-source with the skip — the walk that skips a hole is the walk that records
// it); `delim` is now also set on the raw-string edge (≡ `Lexer::rust_raw`'s `b'"'`). Read via
// `opaque_probe` (one run, all registers). String-blind hole delimitation (R6/T-N7) is FIXED
// (Δ1: `hole_skip` routes through the opaque-aware DelimBalance); the `{{` second-brace phantom
// (T-N8) is still a carried, pinned behavior — Δ2.
//
// Forms (Lexer form tables, 4 targets):
//   C/Java: `//`, `/*…*/`(no nest), `"…"`, `'…'` (escapes)
//   Rust  : `//`, `/*…*/`(nests), `r#*"…"#*`, `"…"`(multiline), `'…'`
//   Python: `#`,  `"""…"""`, `'''…'''`, `"…"`, `'…'`  (holes: `{…}`)
//
// Regen: framec-ng -l rust --emit opaque_scan.frs | grep -v '^#!\[allow' > opaque_scan.gen.rs

pub trait OpaqueScanInput { fn fsm_get(&self, i: usize) -> u8; fn fsm_len(&self) -> usize; }
impl OpaqueScanInput for &[u8] { fn fsm_get(&self, i: usize) -> u8 { self[i] } fn fsm_len(&self) -> usize { self.len() } }

#[derive(Clone)]
enum OpaqueScanVars {
    Start {  },
    LineBody {  },
    BlockBody {  },
    StrBody {  },
    TripleBody {  },
    Accept {  },
    Reject {  },
}
#[derive(Clone)]
enum OpaqueScanArgs {
    Start {  },
    LineBody {  },
    BlockBody {  },
    StrBody {  },
    TripleBody {  },
    Accept {  },
    Reject {  },
}
#[derive(Clone)]
struct OpaqueScanComp {
    state: String,
    vars: OpaqueScanVars,
    args: OpaqueScanArgs,
}

pub struct OpaqueScan<'a> {
    src: &'a [u8],
    pub cursor: usize,
    compartment: OpaqueScanComp,
    stack: Vec<OpaqueScanComp>,
    pub target: Target,
    pub delim: u8,
    pub multiline: bool,
    pub nests: bool,
    pub depth: i32,
    pub unterminated: bool,
    pub kind: i32,
    pub holes: Vec<(usize, usize)>,
}

impl<'a> OpaqueScan<'a> {
    pub fn over(src: &'a [u8], target: Target) -> Self {
        let compartment = OpaqueScanComp { state: "Start".to_string(), vars: OpaqueScanVars::Start {  }, args: OpaqueScanArgs::Start {  } };
        OpaqueScan { src, cursor: 0, compartment, stack: Vec::new(), target: target, delim: 0, multiline: false, nests: false, depth: 0, unterminated: false, kind: 0, holes: Vec::new() }
    }

    pub fn scan_at(&mut self, start: usize) -> bool {
        self.cursor = start;
        self.delim = 0;
        self.multiline = false;
        self.nests = false;
        self.depth = 0;
        self.unterminated = false;
        self.kind = 0;
        self.holes = Vec::new();
        self.compartment = OpaqueScanComp { state: "Start".to_string(), vars: OpaqueScanVars::Start {  }, args: OpaqueScanArgs::Start {  } };
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
            "LineBody" => self.LineBody_step(),
            "BlockBody" => self.BlockBody_step(),
            "StrBody" => self.StrBody_step(),
            "TripleBody" => self.TripleBody_step(),
            _ => {}
        }
    }

    fn Start_step(&mut self) {
        if self.cursor >= self.src.fsm_len() {
            let mut __next = OpaqueScanComp { state: "Reject".to_string(), vars: OpaqueScanVars::Reject {  }, args: OpaqueScanArgs::Reject { } };
            self.compartment = __next;
            return Default::default();
        }
        // `kind` (1=comment, 2=literal) is written at dispatch and read on Accept — the
        // register that lets a consumer apply a comment-vs-literal policy (Item 3).
        let lc = line_comment_len(self.src, self.cursor, self.target);
        if lc > 0 {
            self.kind = 1;
            self.cursor = self.cursor + lc;
            let mut __next = OpaqueScanComp { state: "LineBody".to_string(), vars: OpaqueScanVars::LineBody {  }, args: OpaqueScanArgs::LineBody { } };
            self.compartment = __next;
            return Default::default();
        }
        let bo = block_open_len(self.src, self.cursor, self.target);
        if bo > 0 {
            self.kind = 1;
            self.nests = block_nests(self.target);
            self.depth = 1;
            self.cursor = self.cursor + bo;
            let mut __next = OpaqueScanComp { state: "BlockBody".to_string(), vars: OpaqueScanVars::BlockBody {  }, args: OpaqueScanArgs::BlockBody { } };
            self.compartment = __next;
            return Default::default();
        }
        let rr = raw_scan(self.src, self.cursor, self.target);
        if rr > self.cursor {
            self.kind = 2;
            self.delim = 34;
            self.cursor = rr;
            let mut __next = OpaqueScanComp { state: "Accept".to_string(), vars: OpaqueScanVars::Accept {  }, args: OpaqueScanArgs::Accept { } };
            self.compartment = __next;
            return Default::default();
        }
        // A raw string OPENS here but never closes: RawString itself reports it (its own
        // register), so the `#`-counter stays in the sub-system. `unterminated` distinguishes
        // this Reject from a plain "nothing opened" Reject.
        if raw_unterminated(self.src, self.cursor, self.target) {
            self.unterminated = true;
            let mut __next = OpaqueScanComp { state: "Reject".to_string(), vars: OpaqueScanVars::Reject {  }, args: OpaqueScanArgs::Reject { } };
            self.compartment = __next;
            return Default::default();
        }
        let td = triple_delim(self.src, self.cursor, self.target);
        if td > 0 {
            self.kind = 2;
            self.delim = td;
            self.cursor = self.cursor + 3;
            let mut __next = OpaqueScanComp { state: "TripleBody".to_string(), vars: OpaqueScanVars::TripleBody {  }, args: OpaqueScanArgs::TripleBody { } };
            self.compartment = __next;
            return Default::default();
        }
        let sd = string_delim(self.src, self.cursor, self.target);
        if sd > 0 {
            self.kind = 2;
            self.delim = sd;
            self.multiline = string_multiline(self.target, sd);
            self.cursor = self.cursor + 1;
            let mut __next = OpaqueScanComp { state: "StrBody".to_string(), vars: OpaqueScanVars::StrBody {  }, args: OpaqueScanArgs::StrBody { } };
            self.compartment = __next;
            return Default::default();
        }
        let mut __next = OpaqueScanComp { state: "Reject".to_string(), vars: OpaqueScanVars::Reject {  }, args: OpaqueScanArgs::Reject { } };
        self.compartment = __next;
        return Default::default();
    }

    fn LineBody_step(&mut self) {
        if self.cursor >= self.src.fsm_len() {
            let mut __next = OpaqueScanComp { state: "Accept".to_string(), vars: OpaqueScanVars::Accept {  }, args: OpaqueScanArgs::Accept { } };
            self.compartment = __next;
            return Default::default();
        }
        if self.src.fsm_get(self.cursor) == 10 {
            let mut __next = OpaqueScanComp { state: "Accept".to_string(), vars: OpaqueScanVars::Accept {  }, args: OpaqueScanArgs::Accept { } };
            self.compartment = __next;
            return Default::default();
        }
        self.cursor = self.cursor + 1;
    }

    fn BlockBody_step(&mut self) {
        if self.cursor >= self.src.fsm_len() {
            self.unterminated = true;
            let mut __next = OpaqueScanComp { state: "Reject".to_string(), vars: OpaqueScanVars::Reject {  }, args: OpaqueScanArgs::Reject { } };
            self.compartment = __next;
            return Default::default();
        }
        if self.nests {
            let bo = block_open_len(self.src, self.cursor, self.target);
            if bo > 0 {
                self.depth = self.depth + 1;
                self.cursor = self.cursor + bo;
                let mut __next = OpaqueScanComp { state: "BlockBody".to_string(), vars: OpaqueScanVars::BlockBody {  }, args: OpaqueScanArgs::BlockBody { } };
                self.compartment = __next;
                return Default::default();
            }
        }
        let bc = block_close_len(self.src, self.cursor, self.target);
        if bc > 0 {
            self.depth = self.depth - 1;
            self.cursor = self.cursor + bc;
            if self.depth == 0 {
                let mut __next = OpaqueScanComp { state: "Accept".to_string(), vars: OpaqueScanVars::Accept {  }, args: OpaqueScanArgs::Accept { } };
                self.compartment = __next;
                return Default::default();
            }
            let mut __next = OpaqueScanComp { state: "BlockBody".to_string(), vars: OpaqueScanVars::BlockBody {  }, args: OpaqueScanArgs::BlockBody { } };
            self.compartment = __next;
            return Default::default();
        }
        self.cursor = self.cursor + 1;
    }

    fn StrBody_step(&mut self) {
        if self.cursor >= self.src.fsm_len() {
            self.unterminated = true;
            let mut __next = OpaqueScanComp { state: "Reject".to_string(), vars: OpaqueScanVars::Reject {  }, args: OpaqueScanArgs::Reject { } };
            self.compartment = __next;
            return Default::default();
        }
        let b = self.src.fsm_get(self.cursor);
        if b == 92 {
            self.cursor = self.cursor + 2;
            let mut __next = OpaqueScanComp { state: "StrBody".to_string(), vars: OpaqueScanVars::StrBody {  }, args: OpaqueScanArgs::StrBody { } };
            self.compartment = __next;
            return Default::default();
        }
        if b == 10 {
            if self.multiline {
                self.cursor = self.cursor + 1;
                let mut __next = OpaqueScanComp { state: "StrBody".to_string(), vars: OpaqueScanVars::StrBody {  }, args: OpaqueScanArgs::StrBody { } };
                self.compartment = __next;
                return Default::default();
            }
            self.unterminated = true;
            let mut __next = OpaqueScanComp { state: "Reject".to_string(), vars: OpaqueScanVars::Reject {  }, args: OpaqueScanArgs::Reject { } };
            self.compartment = __next;
            return Default::default();
        }
        let hs = hole_skip(self.src, self.cursor, self.target);
        if hs > self.cursor {
            record_hole(&mut self.holes, self.cursor + 1, hs - 1);
            self.cursor = hs;
            let mut __next = OpaqueScanComp { state: "StrBody".to_string(), vars: OpaqueScanVars::StrBody {  }, args: OpaqueScanArgs::StrBody { } };
            self.compartment = __next;
            return Default::default();
        }
        if b == self.delim {
            self.cursor = self.cursor + 1;
            let mut __next = OpaqueScanComp { state: "Accept".to_string(), vars: OpaqueScanVars::Accept {  }, args: OpaqueScanArgs::Accept { } };
            self.compartment = __next;
            return Default::default();
        }
        self.cursor = self.cursor + 1;
    }

    fn TripleBody_step(&mut self) {
        if self.cursor >= self.src.fsm_len() {
            self.unterminated = true;
            let mut __next = OpaqueScanComp { state: "Reject".to_string(), vars: OpaqueScanVars::Reject {  }, args: OpaqueScanArgs::Reject { } };
            self.compartment = __next;
            return Default::default();
        }
        if self.src.fsm_get(self.cursor) == 92 {
            self.cursor = self.cursor + 2;
            let mut __next = OpaqueScanComp { state: "TripleBody".to_string(), vars: OpaqueScanVars::TripleBody {  }, args: OpaqueScanArgs::TripleBody { } };
            self.compartment = __next;
            return Default::default();
        }
        let hs = hole_skip(self.src, self.cursor, self.target);
        if hs > self.cursor {
            record_hole(&mut self.holes, self.cursor + 1, hs - 1);
            self.cursor = hs;
            let mut __next = OpaqueScanComp { state: "TripleBody".to_string(), vars: OpaqueScanVars::TripleBody {  }, args: OpaqueScanArgs::TripleBody { } };
            self.compartment = __next;
            return Default::default();
        }
        if triple_close(self.src, self.cursor, self.delim) {
            self.cursor = self.cursor + 3;
            let mut __next = OpaqueScanComp { state: "Accept".to_string(), vars: OpaqueScanVars::Accept {  }, args: OpaqueScanArgs::Accept { } };
            self.compartment = __next;
            return Default::default();
        }
        self.cursor = self.cursor + 1;
    }

}

