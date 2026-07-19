use std::collections::HashMap;
use std::any::Any;


// The STATE-HEAD reader, dogfooded as an `@@[scan(u8)]` system — one half of the head-
// grammar family (its sibling: HandlerHeadScan). From the `$` of `$Name(params) => $Parent {`
// it reads the WHOLE head into named registers: name_end, the params group extent, the
// parent-name extent, the body `{`, and the body end. TOTAL — it always Accepts (the walk's
// `is_state_start` did the gating); every refusal of the old code is a named register, never
// a silent local. `machine::state_extent` (the MachineWalk boundary leaf) and `state()` (the
// node driver) are both projections of ONE run, so boundary and node cannot drift.
//
// framec owns the WALK (every seek is a per-byte state); the leaves are O(1) byte facts or
// runs of published systems: paren_extent/body_end = DelimBalance, (Phase 2) skip = OpaqueScan.
//
// Regen: framec-ng -l rust --emit state_head_scan.frs | grep -v '^#!\[allow' > state_head_scan.gen.rs

pub trait StateHeadScanInput { fn fsm_get(&self, i: usize) -> u8; fn fsm_len(&self) -> usize; }
impl StateHeadScanInput for &[u8] { fn fsm_get(&self, i: usize) -> u8 { self[i] } fn fsm_len(&self) -> usize { self.len() } }

#[derive(Clone)]
enum StateHeadScanVars {
    Dollar {  },
    Name {  },
    Params {  },
    ParentSeek {  },
    ParentName {  },
    ParentIdent {  },
    SeekOpen {  },
    Body {  },
    Accept {  },
}
#[derive(Clone)]
enum StateHeadScanArgs {
    Dollar {  },
    Name {  },
    Params {  },
    ParentSeek {  },
    ParentName {  },
    ParentIdent {  },
    SeekOpen {  },
    Body {  },
    Accept {  },
}
#[derive(Clone)]
struct StateHeadScanComp {
    state: String,
    vars: StateHeadScanVars,
    args: StateHeadScanArgs,
}

pub struct StateHeadScan<'a> {
    src: &'a [u8],
    pub cursor: usize,
    compartment: StateHeadScanComp,
    stack: Vec<StateHeadScanComp>,
    pub target: Target,
    pub limit: usize,
    pub name_end: usize,
    pub has_params: bool,
    pub params_open: usize,
    pub params_close: usize,
    pub params_unbalanced: bool,
    pub has_parent: bool,
    pub parent_start: usize,
    pub parent_end: usize,
    pub open: usize,
    pub open_found: bool,
    pub end: usize,
    pub body_clamped: bool,
}

impl<'a> StateHeadScan<'a> {
    pub fn over(src: &'a [u8], target: Target, limit: usize) -> Self {
        let compartment = StateHeadScanComp { state: "Dollar".to_string(), vars: StateHeadScanVars::Dollar {  }, args: StateHeadScanArgs::Dollar {  } };
        StateHeadScan { src, cursor: 0, compartment, stack: Vec::new(), target: target, limit: limit, name_end: 0, has_params: false, params_open: 0, params_close: 0, params_unbalanced: false, has_parent: false, parent_start: 0, parent_end: 0, open: 0, open_found: false, end: 0, body_clamped: false }
    }

    pub fn scan_at(&mut self, start: usize) -> bool {
        self.cursor = start;
        self.name_end = 0;
        self.has_params = false;
        self.params_open = 0;
        self.params_close = 0;
        self.params_unbalanced = false;
        self.has_parent = false;
        self.parent_start = 0;
        self.parent_end = 0;
        self.open = 0;
        self.open_found = false;
        self.end = 0;
        self.body_clamped = false;
        self.compartment = StateHeadScanComp { state: "Dollar".to_string(), vars: StateHeadScanVars::Dollar {  }, args: StateHeadScanArgs::Dollar {  } };
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
            "Dollar" => self.Dollar_step(),
            "Name" => self.Name_step(),
            "Params" => self.Params_step(),
            "ParentSeek" => self.ParentSeek_step(),
            "ParentName" => self.ParentName_step(),
            "ParentIdent" => self.ParentIdent_step(),
            "SeekOpen" => self.SeekOpen_step(),
            "Body" => self.Body_step(),
            _ => {}
        }
    }

    fn Dollar_step(&mut self) {
        self.cursor = self.cursor + 1;
        let mut __next = StateHeadScanComp { state: "Name".to_string(), vars: StateHeadScanVars::Name {  }, args: StateHeadScanArgs::Name { } };
        self.compartment = __next;
        return Default::default();
    }

    fn Name_step(&mut self) {
        if is_name_byte(self.src, self.cursor, self.limit) {
            self.cursor = self.cursor + 1;
        } else {
            self.name_end = self.cursor;
            let mut __next = StateHeadScanComp { state: "Params".to_string(), vars: StateHeadScanVars::Params {  }, args: StateHeadScanArgs::Params { } };
            self.compartment = __next;
            return Default::default();
        }
    }

    fn Params_step(&mut self) {
        if at_open_paren(self.src, self.cursor, self.limit) {
            let pe = paren_extent(self.src, self.cursor, self.limit, self.target);
            if pe > 0 {
                self.has_params = true;
                self.params_open = self.cursor;
                self.params_close = pe;
            } else {
                self.params_unbalanced = true;
            }
        }
        // Phase 1 (hand-exact): the parent hunt starts at name_end and scans
        // THROUGH the params group (T-S5, carried; Phase 2: start at params_close
        // when has_params).
        self.cursor = self.name_end;
        let mut __next = StateHeadScanComp { state: "ParentSeek".to_string(), vars: StateHeadScanVars::ParentSeek {  }, args: StateHeadScanArgs::ParentSeek { } };
        self.compartment = __next;
        return Default::default();
    }

    fn ParentSeek_step(&mut self) {
        if self.cursor >= self.limit {
            self.open = self.limit;
            self.end = self.limit;
            let mut __next = StateHeadScanComp { state: "Accept".to_string(), vars: StateHeadScanVars::Accept {  }, args: StateHeadScanArgs::Accept { } };
            self.compartment = __next;
            return Default::default();
        }
        let sk = skip(self.src, self.cursor, self.limit, self.target);
        if sk > self.cursor {
            self.cursor = sk;
        } else {
            if at_open_brace(self.src, self.cursor, self.limit) {
                self.open = self.cursor;
                self.open_found = true;
                let mut __next = StateHeadScanComp { state: "Body".to_string(), vars: StateHeadScanVars::Body {  }, args: StateHeadScanArgs::Body { } };
                self.compartment = __next;
                return Default::default();
            }
            if at_newline(self.src, self.cursor, self.limit) {
                let mut __next = StateHeadScanComp { state: "SeekOpen".to_string(), vars: StateHeadScanVars::SeekOpen {  }, args: StateHeadScanArgs::SeekOpen { } };
                self.compartment = __next;
                return Default::default();
            }
            if at_arrow(self.src, self.cursor, self.limit) {
                self.cursor = self.cursor + 2;
                let mut __next = StateHeadScanComp { state: "ParentName".to_string(), vars: StateHeadScanVars::ParentName {  }, args: StateHeadScanArgs::ParentName { } };
                self.compartment = __next;
                return Default::default();
            }
            self.cursor = self.cursor + 1;
        }
    }

    fn ParentName_step(&mut self) {
        if is_ws(self.src, self.cursor, self.limit) {
            self.cursor = self.cursor + 1;
        } else {
            if is_dollar_name(self.src, self.cursor, self.limit) {
                self.parent_start = self.cursor + 1;
                self.cursor = self.cursor + 1;
                let mut __next = StateHeadScanComp { state: "ParentIdent".to_string(), vars: StateHeadScanVars::ParentIdent {  }, args: StateHeadScanArgs::ParentIdent { } };
                self.compartment = __next;
                return Default::default();
            }
            let mut __next = StateHeadScanComp { state: "SeekOpen".to_string(), vars: StateHeadScanVars::SeekOpen {  }, args: StateHeadScanArgs::SeekOpen { } };
            self.compartment = __next;
            return Default::default();
        }
    }

    fn ParentIdent_step(&mut self) {
        if is_name_byte(self.src, self.cursor, self.limit) {
            self.cursor = self.cursor + 1;
        } else {
            self.parent_end = self.cursor;
            self.has_parent = true;
            let mut __next = StateHeadScanComp { state: "SeekOpen".to_string(), vars: StateHeadScanVars::SeekOpen {  }, args: StateHeadScanArgs::SeekOpen { } };
            self.compartment = __next;
            return Default::default();
        }
    }

    fn SeekOpen_step(&mut self) {
        if self.cursor >= self.limit {
            self.open = self.limit;
            self.end = self.limit;
            let mut __next = StateHeadScanComp { state: "Accept".to_string(), vars: StateHeadScanVars::Accept {  }, args: StateHeadScanArgs::Accept { } };
            self.compartment = __next;
            return Default::default();
        }
        let sk = skip(self.src, self.cursor, self.limit, self.target);
        if sk > self.cursor {
            self.cursor = sk;
        } else {
            if at_open_brace(self.src, self.cursor, self.limit) {
                self.open = self.cursor;
                self.open_found = true;
                let mut __next = StateHeadScanComp { state: "Body".to_string(), vars: StateHeadScanVars::Body {  }, args: StateHeadScanArgs::Body { } };
                self.compartment = __next;
                return Default::default();
            }
            self.cursor = self.cursor + 1;
        }
    }

    fn Body_step(&mut self) {
        let e = body_end(self.src, self.open, self.limit, self.target);
        if e > 0 {
            self.end = e;
        } else {
            self.end = self.limit;
            self.body_clamped = true;
        }
        let mut __next = StateHeadScanComp { state: "Accept".to_string(), vars: StateHeadScanVars::Accept {  }, args: StateHeadScanArgs::Accept { } };
        self.compartment = __next;
        return Default::default();
    }

}

