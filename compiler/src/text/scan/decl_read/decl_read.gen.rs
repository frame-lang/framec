use std::collections::HashMap;
use std::any::Any;


// The DECL-line reader, dogfooded as an `@@[scan(u8)]` system — a per-decl register TRANSDUCER
// over the window `[from, to)` (the walk's answer: eol for a line decl, the body `{` for a
// body-decl signature), StmtScan-style: the entry slices the input to `to` and scans at `from`;
// offsets stay absolute. It replaces the hand `machine.rs::decl_of` chain.
//
// Its states are GRAMMAR POSITIONS that gate which fields remain admissible (after `:` no params
// can follow; after `=` nothing but init text can). That gating is load-bearing: the one recorded
// bug in this reader (`async` read as a name -> `def async(self):` SyntaxError) was a MODE error,
// and `$Async` finally names the mode. It has NO `$Reject` — deliberately: the reader is TOTAL
// (every window yields a decl shape, because the tree partition must own every byte);
// malformedness is REGISTERS, not refusal — `empty_name` (ledger T7), `params_clamped` (T8).
//
// The machine records GEOMETRY (offsets + flags); the native `member_decl_of` builder makes the
// VALUES (slice, trim, empty->None). framec owns the SEQUENCING; the leaves are per-target facts
// or shared heads: `indent_end`/`ident_end`/`at_byte`/`async_modifier_at`/`eq_or_end`/`sys_start`
// are O(window) byte scans reproducing the hand code exactly, and `params_close` is THE fix seam:
// Phase A = the hand bare `(`/`)` counter verbatim (a RECORDED guardrail-4 exception with a
// bounded lifetime — GATE-B does not close until Phase B routes it through
// `delim_balance::balanced`, retiring the string-blindness of ledger T9). `target` is
// construction config from day one so Phase B touches only the leaf.
//
// Regen: framec-ng -l rust --emit decl_read.frs | grep -v '^#!\[allow' > decl_read.gen.rs

pub trait DeclReadInput { fn fsm_get(&self, i: usize) -> u8; fn fsm_len(&self) -> usize; }
impl DeclReadInput for &[u8] { fn fsm_get(&self, i: usize) -> u8 { self[i] } fn fsm_len(&self) -> usize { self.len() } }

#[derive(Clone)]
enum DeclReadVars {
    Indent {  },
    Async {  },
    Name {  },
    Params {  },
    Type {  },
    Init {  },
    Accept {  },
}
#[derive(Clone)]
enum DeclReadArgs {
    Indent {  },
    Async {  },
    Name {  },
    Params {  },
    Type {  },
    Init {  },
    Accept {  },
}
#[derive(Clone)]
struct DeclReadComp {
    state: String,
    vars: DeclReadVars,
    args: DeclReadArgs,
}

pub struct DeclRead<'a> {
    src: &'a [u8],
    pub cursor: usize,
    compartment: DeclReadComp,
    stack: Vec<DeclReadComp>,
    pub target: Target,
    pub is_async: bool,
    pub empty_name: bool,
    pub name_s: usize,
    pub name_e: usize,
    pub has_params: bool,
    pub params_open: usize,
    pub params_close: usize,
    pub params_clamped: bool,
    pub has_type: bool,
    pub type_s: usize,
    pub type_e: usize,
    pub has_init: bool,
    pub init_s: usize,
    pub has_sys: bool,
    pub sys_s: usize,
    pub sys_e: usize,
}

impl<'a> DeclRead<'a> {
    pub fn over(src: &'a [u8], target: Target) -> Self {
        let compartment = DeclReadComp { state: "Indent".to_string(), vars: DeclReadVars::Indent {  }, args: DeclReadArgs::Indent {  } };
        DeclRead { src, cursor: 0, compartment, stack: Vec::new(), target: target, is_async: false, empty_name: false, name_s: 0, name_e: 0, has_params: false, params_open: 0, params_close: 0, params_clamped: false, has_type: false, type_s: 0, type_e: 0, has_init: false, init_s: 0, has_sys: false, sys_s: 0, sys_e: 0 }
    }

    pub fn scan_at(&mut self, start: usize) -> bool {
        self.cursor = start;
        self.is_async = false;
        self.empty_name = false;
        self.name_s = 0;
        self.name_e = 0;
        self.has_params = false;
        self.params_open = 0;
        self.params_close = 0;
        self.params_clamped = false;
        self.has_type = false;
        self.type_s = 0;
        self.type_e = 0;
        self.has_init = false;
        self.init_s = 0;
        self.has_sys = false;
        self.sys_s = 0;
        self.sys_e = 0;
        self.compartment = DeclReadComp { state: "Indent".to_string(), vars: DeclReadVars::Indent {  }, args: DeclReadArgs::Indent {  } };
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
            "Indent" => self.Indent_step(),
            "Async" => self.Async_step(),
            "Name" => self.Name_step(),
            "Params" => self.Params_step(),
            "Type" => self.Type_step(),
            "Init" => self.Init_step(),
            _ => {}
        }
    }

    fn Indent_step(&mut self) {
        self.cursor = indent_end(self.src, self.cursor);
        let mut __next = DeclReadComp { state: "Async".to_string(), vars: DeclReadVars::Async {  }, args: DeclReadArgs::Async { } };
        self.compartment = __next;
        return Default::default();
    }

    fn Async_step(&mut self) {
        if async_modifier_at(self.src, self.cursor) {
            self.is_async = true;
            self.cursor = indent_end(self.src, self.cursor + 5);
        }
        let mut __next = DeclReadComp { state: "Name".to_string(), vars: DeclReadVars::Name {  }, args: DeclReadArgs::Name { } };
        self.compartment = __next;
        return Default::default();
    }

    fn Name_step(&mut self) {
        self.name_s = self.cursor;
        self.cursor = ident_end(self.src, self.cursor);
        self.name_e = self.cursor;
        if self.name_s == self.name_e {
            self.empty_name = true;
        }
        let mut __next = DeclReadComp { state: "Params".to_string(), vars: DeclReadVars::Params {  }, args: DeclReadArgs::Params { } };
        self.compartment = __next;
        return Default::default();
    }

    fn Params_step(&mut self) {
        if at_byte(self.src, self.cursor, 40) {
            self.has_params = true;
            self.params_open = self.cursor;
            let c = params_close(self.src, self.cursor, self.target);
            if c > self.cursor {
                self.params_close = c;
                self.cursor = c;
            } else {
                self.params_clamped = true;
                self.params_close = self.src.fsm_len();
                self.cursor = self.src.fsm_len();
            }
        }
        let mut __next = DeclReadComp { state: "Type".to_string(), vars: DeclReadVars::Type {  }, args: DeclReadArgs::Type { } };
        self.compartment = __next;
        return Default::default();
    }

    fn Type_step(&mut self) {
        self.cursor = indent_end(self.src, self.cursor);
        if at_byte(self.src, self.cursor, 58) {
            self.cursor = self.cursor + 1;
            self.type_s = self.cursor;
            self.cursor = eq_or_end(self.src, self.cursor);
            self.type_e = self.cursor;
            self.has_type = true;
        }
        let mut __next = DeclReadComp { state: "Init".to_string(), vars: DeclReadVars::Init {  }, args: DeclReadArgs::Init { } };
        self.compartment = __next;
        return Default::default();
    }

    fn Init_step(&mut self) {
        if at_byte(self.src, self.cursor, 61) {
            self.has_init = true;
            self.init_s = self.cursor + 1;
            let s = sys_start(self.src, self.cursor + 1);
            let e = ident_end(self.src, s);
            if e > s {
                self.has_sys = true;
                self.sys_s = s;
                self.sys_e = e;
            }
        }
        let mut __next = DeclReadComp { state: "Accept".to_string(), vars: DeclReadVars::Accept {  }, args: DeclReadArgs::Accept { } };
        self.compartment = __next;
        return Default::default();
    }

}
