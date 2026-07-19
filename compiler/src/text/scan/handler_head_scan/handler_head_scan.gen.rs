use std::collections::HashMap;
use std::any::Any;


// The HANDLER-HEAD reader, dogfooded as an `@@[scan(u8)]` system — the other half of the
// head-grammar family (sibling: StateHeadScan). From a member-start candidate it reads
// `name(params) [: T] {` with the `$>` / `<$` event forms into named registers, or Rejects.
// The four not-a-handler refusals of the hand code (no name / no `(` / unbalanced params /
// no `{` on the head line) share ONE $Reject — they have identical futures (the walk
// advances one byte), so distinct states would be costume AND would violate the scan(u8)
// pump contract (only `Accept`/`Reject` halt it); the CAUSE is articulated in reject_reason.
// `machine::handler_end` (the StateWalk boundary leaf) and `handler_at` (the node driver)
// are both projections of ONE run, so boundary and node cannot drift.
//
// Regen: framec-ng -l rust --emit handler_head_scan.frs | grep -v '^#!\[allow' > handler_head_scan.gen.rs

pub trait HandlerHeadScanInput { fn fsm_get(&self, i: usize) -> u8; fn fsm_len(&self) -> usize; }
impl HandlerHeadScanInput for &[u8] { fn fsm_get(&self, i: usize) -> u8 { self[i] } fn fsm_len(&self) -> usize { self.len() } }

#[derive(Clone)]
enum HandlerHeadScanVars {
    Name {  },
    NameIdent {  },
    WsBeforeParen {  },
    Params {  },
    AfterParams {  },
    RetType {  },
    SeekBrace {  },
    Body {  },
    Accept {  },
    Reject {  },
}
#[derive(Clone)]
enum HandlerHeadScanArgs {
    Name {  },
    NameIdent {  },
    WsBeforeParen {  },
    Params {  },
    AfterParams {  },
    RetType {  },
    SeekBrace {  },
    Body {  },
    Accept {  },
    Reject {  },
}
#[derive(Clone)]
struct HandlerHeadScanComp {
    state: String,
    vars: HandlerHeadScanVars,
    args: HandlerHeadScanArgs,
}

pub struct HandlerHeadScan<'a> {
    src: &'a [u8],
    pub cursor: usize,
    compartment: HandlerHeadScanComp,
    stack: Vec<HandlerHeadScanComp>,
    pub target: Target,
    pub limit: usize,
    pub name_kind: i32,
    pub name_start: usize,
    pub name_end: usize,
    pub params_open: usize,
    pub params_close: usize,
    pub has_return: bool,
    pub ret_start: usize,
    pub ret_end: usize,
    pub open: usize,
    pub end: usize,
    pub body_clamped: bool,
    pub reject_reason: i32,
}

impl<'a> HandlerHeadScan<'a> {
    pub fn over(src: &'a [u8], target: Target, limit: usize) -> Self {
        let compartment = HandlerHeadScanComp { state: "Name".to_string(), vars: HandlerHeadScanVars::Name {  }, args: HandlerHeadScanArgs::Name {  } };
        HandlerHeadScan { src, cursor: 0, compartment, stack: Vec::new(), target: target, limit: limit, name_kind: 0, name_start: 0, name_end: 0, params_open: 0, params_close: 0, has_return: false, ret_start: 0, ret_end: 0, open: 0, end: 0, body_clamped: false, reject_reason: 0 }
    }

    pub fn scan_at(&mut self, start: usize) -> bool {
        self.cursor = start;
        self.name_kind = 0;
        self.name_start = 0;
        self.name_end = 0;
        self.params_open = 0;
        self.params_close = 0;
        self.has_return = false;
        self.ret_start = 0;
        self.ret_end = 0;
        self.open = 0;
        self.end = 0;
        self.body_clamped = false;
        self.reject_reason = 0;
        self.compartment = HandlerHeadScanComp { state: "Name".to_string(), vars: HandlerHeadScanVars::Name {  }, args: HandlerHeadScanArgs::Name {  } };
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
            "Name" => self.Name_step(),
            "NameIdent" => self.NameIdent_step(),
            "WsBeforeParen" => self.WsBeforeParen_step(),
            "Params" => self.Params_step(),
            "AfterParams" => self.AfterParams_step(),
            "RetType" => self.RetType_step(),
            "SeekBrace" => self.SeekBrace_step(),
            "Body" => self.Body_step(),
            _ => {}
        }
    }

    fn Name_step(&mut self) {
        if at_enter(self.src, self.cursor) {
            self.name_kind = 1;
            self.name_start = self.cursor;
            self.name_end = self.cursor + 2;
            self.cursor = self.cursor + 2;
            let mut __next = HandlerHeadScanComp { state: "WsBeforeParen".to_string(), vars: HandlerHeadScanVars::WsBeforeParen {  }, args: HandlerHeadScanArgs::WsBeforeParen { } };
            self.compartment = __next;
            return Default::default();
        }
        if at_exit(self.src, self.cursor) {
            self.name_kind = 2;
            self.name_start = self.cursor;
            self.name_end = self.cursor + 2;
            self.cursor = self.cursor + 2;
            let mut __next = HandlerHeadScanComp { state: "WsBeforeParen".to_string(), vars: HandlerHeadScanVars::WsBeforeParen {  }, args: HandlerHeadScanArgs::WsBeforeParen { } };
            self.compartment = __next;
            return Default::default();
        }
        if is_name_start_here(self.src, self.cursor) {
            self.name_start = self.cursor;
            self.cursor = self.cursor + 1;
            let mut __next = HandlerHeadScanComp { state: "NameIdent".to_string(), vars: HandlerHeadScanVars::NameIdent {  }, args: HandlerHeadScanArgs::NameIdent { } };
            self.compartment = __next;
            return Default::default();
        }
        self.reject_reason = 1;
        let mut __next = HandlerHeadScanComp { state: "Reject".to_string(), vars: HandlerHeadScanVars::Reject {  }, args: HandlerHeadScanArgs::Reject { } };
        self.compartment = __next;
        return Default::default();
    }

    fn NameIdent_step(&mut self) {
        if is_name_byte(self.src, self.cursor, self.limit) {
            self.cursor = self.cursor + 1;
        } else {
            self.name_end = self.cursor;
            let mut __next = HandlerHeadScanComp { state: "WsBeforeParen".to_string(), vars: HandlerHeadScanVars::WsBeforeParen {  }, args: HandlerHeadScanArgs::WsBeforeParen { } };
            self.compartment = __next;
            return Default::default();
        }
    }

    fn WsBeforeParen_step(&mut self) {
        if is_ws(self.src, self.cursor, self.limit) {
            self.cursor = self.cursor + 1;
        } else {
            if at_open_paren(self.src, self.cursor, self.limit) {
                let mut __next = HandlerHeadScanComp { state: "Params".to_string(), vars: HandlerHeadScanVars::Params {  }, args: HandlerHeadScanArgs::Params { } };
                self.compartment = __next;
                return Default::default();
            }
            self.reject_reason = 2;
            let mut __next = HandlerHeadScanComp { state: "Reject".to_string(), vars: HandlerHeadScanVars::Reject {  }, args: HandlerHeadScanArgs::Reject { } };
            self.compartment = __next;
            return Default::default();
        }
    }

    fn Params_step(&mut self) {
        let pe = paren_extent(self.src, self.cursor, self.limit, self.target);
        if pe > 0 {
            self.params_open = self.cursor;
            self.params_close = pe;
            self.cursor = pe;
            let mut __next = HandlerHeadScanComp { state: "AfterParams".to_string(), vars: HandlerHeadScanVars::AfterParams {  }, args: HandlerHeadScanArgs::AfterParams { } };
            self.compartment = __next;
            return Default::default();
        }
        self.reject_reason = 3;
        let mut __next = HandlerHeadScanComp { state: "Reject".to_string(), vars: HandlerHeadScanVars::Reject {  }, args: HandlerHeadScanArgs::Reject { } };
        self.compartment = __next;
        return Default::default();
    }

    fn AfterParams_step(&mut self) {
        if is_ws(self.src, self.cursor, self.limit) {
            self.cursor = self.cursor + 1;
        } else {
            if at_colon(self.src, self.cursor, self.limit) {
                self.has_return = true;
                self.cursor = self.cursor + 1;
                self.ret_start = self.cursor;
                let mut __next = HandlerHeadScanComp { state: "RetType".to_string(), vars: HandlerHeadScanVars::RetType {  }, args: HandlerHeadScanArgs::RetType { } };
                self.compartment = __next;
                return Default::default();
            }
            let mut __next = HandlerHeadScanComp { state: "SeekBrace".to_string(), vars: HandlerHeadScanVars::SeekBrace {  }, args: HandlerHeadScanArgs::SeekBrace { } };
            self.compartment = __next;
            return Default::default();
        }
    }

    fn RetType_step(&mut self) {
        if ret_byte(self.src, self.cursor, self.limit) {
            self.cursor = self.cursor + 1;
        } else {
            self.ret_end = self.cursor;
            let mut __next = HandlerHeadScanComp { state: "SeekBrace".to_string(), vars: HandlerHeadScanVars::SeekBrace {  }, args: HandlerHeadScanArgs::SeekBrace { } };
            self.compartment = __next;
            return Default::default();
        }
    }

    fn SeekBrace_step(&mut self) {
        if self.cursor >= self.limit {
            self.reject_reason = 4;
            let mut __next = HandlerHeadScanComp { state: "Reject".to_string(), vars: HandlerHeadScanVars::Reject {  }, args: HandlerHeadScanArgs::Reject { } };
            self.compartment = __next;
            return Default::default();
        }
        if at_open_brace(self.src, self.cursor, self.limit) {
            self.open = self.cursor;
            let mut __next = HandlerHeadScanComp { state: "Body".to_string(), vars: HandlerHeadScanVars::Body {  }, args: HandlerHeadScanArgs::Body { } };
            self.compartment = __next;
            return Default::default();
        }
        if at_newline(self.src, self.cursor, self.limit) {
            self.reject_reason = 4;
            let mut __next = HandlerHeadScanComp { state: "Reject".to_string(), vars: HandlerHeadScanVars::Reject {  }, args: HandlerHeadScanArgs::Reject { } };
            self.compartment = __next;
            return Default::default();
        }
        self.cursor = self.cursor + 1;
    }

    fn Body_step(&mut self) {
        let e = body_end(self.src, self.open, self.limit, self.target);
        if e > 0 {
            self.end = e;
        } else {
            self.end = self.limit;
            self.body_clamped = true;
        }
        let mut __next = HandlerHeadScanComp { state: "Accept".to_string(), vars: HandlerHeadScanVars::Accept {  }, args: HandlerHeadScanArgs::Accept { } };
        self.compartment = __next;
        return Default::default();
    }

}

