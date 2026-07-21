use std::collections::HashMap;
use std::any::Any;


// Declaration-site header param parser — the `@@system` sibling of ArgScan (call-site args),
// REPLACING the string-blind + sigil-blind hand split (`split_system_params` + retired ParamSplit).
// ONE left-to-right walk over the ALREADY-`(`-balanced interior: top-level commas end params;
// a `$(`/`$>(` sigil at a param start opens a group whose closer is the BALANCED `)` (kills the
// old `trim_end_matches(')')` truncation, scan/mod.rs); a dual counter forks angles (`Map<K,V>` vs
// operator `<`/`>`) and adjudicates by ANGLE SELF-CONSISTENCY (`g_viable`) — a declaration has no
// arity. Deltas vs ArgScan: (1) `"`-only StringScan opacity (NOT target-aware OpaqueScan) — agrees
// with ParenBalance's interior boundary AND dodges the Rust `'a`-lifetime hazard, so a `'…'` char
// default stays CARRIED (F5 #2); (2) NO eq-naming — the whole `name: type = default` body is the
// value, split natively by parse_one_param. `<`/`>` are NEVER bracket-counted here (that was the
// ParamSplit `$>(` `>` miscount, F5 #3); they only move `adepth`.
//
//   depth  — the merged `([{` counter (Dyck-1); the group closer is the BALANCED `)` found by the
//            walk (fixes F5 #4 — the hand `trim_end_matches(')')` ate the user's own `)`).
//   adepth — angle nesting under hypothesis G (angles-as-brackets), counted ONLY at bracket depth 0,
//            outside `"…"`, in $Value (never in $GroupValue — group interiors sit at depth >= 1).
//            `g_viable` is cleared by a counted `>` at adepth 0 (operator evidence) or by adepth != 0
//            at end-of-interior (unclosed angle). Each record's `g_end` bit ("adepth == 0 at this
//            boundary") marks the boundaries that hold under G too. The wrapper folds the G candidate
//            from the records (`merge_g`) and takes G iff self-consistent — no arity, no guess.
//
// framec owns the WALK (dispatch + the two counters + the value-span register); `skip_string` only
// runs StringScan, `record_part` only pushes the emitted extent — no walk lives in a leaf.
//
// Regen: framec-ng -l rust --emit param_scan.frs | grep -v '^#!\[allow' > param_scan.gen.rs

pub trait ParamScanInput { fn fsm_get(&self, i: usize) -> u8; fn fsm_len(&self) -> usize; }
impl ParamScanInput for &[u8] { fn fsm_get(&self, i: usize) -> u8 { self[i] } fn fsm_len(&self) -> usize { self.len() } }

#[derive(Clone)]
enum ParamScanVars {
    SegStart {  },
    Value {  },
    GroupValue {  },
    AfterGroup {  },
    VerbatimTail {  },
    Accept {  },
}
#[derive(Clone)]
enum ParamScanArgs {
    SegStart {  },
    Value {  },
    GroupValue {  },
    AfterGroup {  },
    VerbatimTail {  },
    Accept {  },
}
#[derive(Clone)]
struct ParamScanComp {
    state: String,
    vars: ParamScanVars,
    args: ParamScanArgs,
}

pub struct ParamScan<'a> {
    src: &'a [u8],
    pub cursor: usize,
    compartment: ParamScanComp,
    stack: Vec<ParamScanComp>,
    pub depth: i32,
    pub adepth: i32,
    pub g_viable: bool,
    pub angle_touched: bool,
    pub group: i32,
    pub vs: usize,
    pub ve: usize,
    pub has_val: bool,
    pub refusal: i32,
    pub dropped_empty: i32,
    pub parts: Vec<(i32, usize, usize, bool)>,
}

impl<'a> ParamScan<'a> {
    pub fn over(src: &'a [u8]) -> Self {
        let compartment = ParamScanComp { state: "SegStart".to_string(), vars: ParamScanVars::SegStart {  }, args: ParamScanArgs::SegStart {  } };
        ParamScan { src, cursor: 0, compartment, stack: Vec::new(), depth: 0, adepth: 0, g_viable: true, angle_touched: false, group: 0, vs: 0, ve: 0, has_val: false, refusal: 0, dropped_empty: 0, parts: Vec::new() }
    }

    pub fn scan_at(&mut self, start: usize) -> bool {
        self.cursor = start;
        self.depth = 0;
        self.adepth = 0;
        self.g_viable = true;
        self.angle_touched = false;
        self.group = 0;
        self.vs = 0;
        self.ve = 0;
        self.has_val = false;
        self.refusal = 0;
        self.dropped_empty = 0;
        self.parts = Vec::new();
        self.compartment = ParamScanComp { state: "SegStart".to_string(), vars: ParamScanVars::SegStart {  }, args: ParamScanArgs::SegStart {  } };
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
            "SegStart" => self.SegStart_step(),
            "Value" => self.Value_step(),
            "GroupValue" => self.GroupValue_step(),
            "AfterGroup" => self.AfterGroup_step(),
            "VerbatimTail" => self.VerbatimTail_step(),
            _ => {}
        }
    }

    fn SegStart_step(&mut self) {
        if self.cursor >= self.src.fsm_len() {
            if self.adepth != 0 {
                self.g_viable = false;
            }
            let mut __next = ParamScanComp { state: "Accept".to_string(), vars: ParamScanVars::Accept {  }, args: ParamScanArgs::Accept { } };
            self.compartment = __next;
            return Default::default();
        }
        let b = self.src.fsm_get(self.cursor);
        if b == 32 || b == 9 || b == 10 || b == 13 {
            self.cursor = self.cursor + 1;
            let mut __next = ParamScanComp { state: "SegStart".to_string(), vars: ParamScanVars::SegStart {  }, args: ParamScanArgs::SegStart { } };
            self.compartment = __next;
            return Default::default();
        }
        if b == 44 {
            // `,,` / leading `,`: the empty segment is dropped (counted, so observable).
            self.dropped_empty = self.dropped_empty + 1;
            self.cursor = self.cursor + 1;
            let mut __next = ParamScanComp { state: "SegStart".to_string(), vars: ParamScanVars::SegStart {  }, args: ParamScanArgs::SegStart { } };
            self.compartment = __next;
            return Default::default();
        }
        if is_sigil_enter(self.src, self.cursor, self.src.fsm_len()) {
            self.group = 2;
            self.cursor = self.cursor + 3;   // `$>(`
            self.depth = 1;
            let mut __next = ParamScanComp { state: "GroupValue".to_string(), vars: ParamScanVars::GroupValue {  }, args: ParamScanArgs::GroupValue { } };
            self.compartment = __next;
            return Default::default();
        }
        if is_sigil_state(self.src, self.cursor, self.src.fsm_len()) {
            self.group = 1;
            self.cursor = self.cursor + 2;   // `$(`
            self.depth = 1;
            let mut __next = ParamScanComp { state: "GroupValue".to_string(), vars: ParamScanVars::GroupValue {  }, args: ParamScanArgs::GroupValue { } };
            self.compartment = __next;
            return Default::default();
        }
        self.group = 0;
        self.depth = 0;
        let mut __next = ParamScanComp { state: "Value".to_string(), vars: ParamScanVars::Value {  }, args: ParamScanArgs::Value { } };
        self.compartment = __next;
        return Default::default();
    }

    fn Value_step(&mut self) {
        if self.cursor >= self.src.fsm_len() {
            if self.adepth != 0 {
                self.g_viable = false;
            }
            record_part(&mut self.parts, self.group, self.has_val, self.vs, self.ve, self.cursor, true);
            let mut __next = ParamScanComp { state: "Accept".to_string(), vars: ParamScanVars::Accept {  }, args: ParamScanArgs::Accept { } };
            self.compartment = __next;
            return Default::default();
        }
        // Opacity FIRST — a `,`/bracket/angle inside a `"…"`-string never takes part.
        // `"`-only (StringScan): declines on `'` and on an unterminated `"` (returns
        // `cursor` unchanged), matching ParenBalance/ParamSplit — the F5 #2 carry.
        let sk = skip_string(self.src, self.cursor);
        if sk > self.cursor {
            if self.has_val == false { self.vs = self.cursor; self.has_val = true; }
            self.cursor = sk;
            self.ve = sk;
            let mut __next = ParamScanComp { state: "Value".to_string(), vars: ParamScanVars::Value {  }, args: ParamScanArgs::Value { } };
            self.compartment = __next;
            return Default::default();
        }
        let b = self.src.fsm_get(self.cursor);
        // The dual-counter angle branch: a counted `<`/`>` at bracket depth 0 updates
        // hypothesis G's registers, then FALLS THROUGH as ordinary content. Digraphs
        // (`<=` `>=` `->` `=>`) are guard-excluded and never counted.
        if (b == 60 || b == 62) && self.depth == 0 {
            if angle_guard(self.src, self.cursor, 0, self.src.fsm_len()) == false {
                self.angle_touched = true;
                if b == 60 {
                    self.adepth = self.adepth + 1;
                }
                if b == 62 {
                    if self.adepth == 0 {
                        self.g_viable = false;   // operator evidence: `>` with no open `<`
                    } else {
                        self.adepth = self.adepth - 1;
                    }
                }
            }
            // fall through: the byte is ordinary content under both hypotheses
        }
        if b == 44 && self.depth == 0 {
            // Top-level comma: the O boundary. g_end = "holds under G too" (adepth 0).
            record_part(&mut self.parts, self.group, self.has_val, self.vs, self.ve, self.cursor, self.adepth == 0);
            self.group = 0;
            self.has_val = false;
            self.cursor = self.cursor + 1;
            let mut __next = ParamScanComp { state: "SegStart".to_string(), vars: ParamScanVars::SegStart {  }, args: ParamScanArgs::SegStart { } };
            self.compartment = __next;
            return Default::default();
        }
        if b == 40 || b == 91 || b == 123 {  // ( [ {  — merged counter
            self.depth = self.depth + 1;
            if self.has_val == false { self.vs = self.cursor; self.has_val = true; }
            self.cursor = self.cursor + 1;
            self.ve = self.cursor;
            let mut __next = ParamScanComp { state: "Value".to_string(), vars: ParamScanVars::Value {  }, args: ParamScanArgs::Value { } };
            self.compartment = __next;
            return Default::default();
        }
        if b == 41 || b == 93 || b == 125 {  // ) ] }
            if self.depth == 0 {
                self.refusal = 2;            // StrayCloser
                self.g_viable = false;
                let mut __next = ParamScanComp { state: "VerbatimTail".to_string(), vars: ParamScanVars::VerbatimTail {  }, args: ParamScanArgs::VerbatimTail { } };
                self.compartment = __next;
                return Default::default();
            }
            self.depth = self.depth - 1;
            if self.has_val == false { self.vs = self.cursor; self.has_val = true; }
            self.cursor = self.cursor + 1;
            self.ve = self.cursor;
            let mut __next = ParamScanComp { state: "Value".to_string(), vars: ParamScanVars::Value {  }, args: ParamScanArgs::Value { } };
            self.compartment = __next;
            return Default::default();
        }
        if b == 32 || b == 9 || b == 10 || b == 13 {
            self.cursor = self.cursor + 1;   // ws: cursor moves, ve does not (auto-trim)
            let mut __next = ParamScanComp { state: "Value".to_string(), vars: ParamScanVars::Value {  }, args: ParamScanArgs::Value { } };
            self.compartment = __next;
            return Default::default();
        }
        if self.has_val == false { self.vs = self.cursor; self.has_val = true; }
        self.cursor = self.cursor + 1;
        self.ve = self.cursor;
        let mut __next = ParamScanComp { state: "Value".to_string(), vars: ParamScanVars::Value {  }, args: ParamScanArgs::Value { } };
        self.compartment = __next;
        return Default::default();
    }

    fn GroupValue_step(&mut self) {
        if self.cursor >= self.src.fsm_len() {
            self.refusal = 4;                // UnclosedGroup
            self.g_viable = false;
            let mut __next = ParamScanComp { state: "VerbatimTail".to_string(), vars: ParamScanVars::VerbatimTail {  }, args: ParamScanArgs::VerbatimTail { } };
            self.compartment = __next;
            return Default::default();
        }
        let sk = skip_string(self.src, self.cursor);
        if sk > self.cursor {
            if self.has_val == false { self.vs = self.cursor; self.has_val = true; }
            self.cursor = sk;
            self.ve = sk;
            let mut __next = ParamScanComp { state: "GroupValue".to_string(), vars: ParamScanVars::GroupValue {  }, args: ParamScanArgs::GroupValue { } };
            self.compartment = __next;
            return Default::default();
        }
        let b = self.src.fsm_get(self.cursor);
        if b == 40 || b == 91 || b == 123 {
            self.depth = self.depth + 1;
            if self.has_val == false { self.vs = self.cursor; self.has_val = true; }
            self.cursor = self.cursor + 1;
            self.ve = self.cursor;
            let mut __next = ParamScanComp { state: "GroupValue".to_string(), vars: ParamScanVars::GroupValue {  }, args: ParamScanArgs::GroupValue { } };
            self.compartment = __next;
            return Default::default();
        }
        if b == 41 || b == 93 || b == 125 {
            if self.depth == 1 {
                if b == 41 {
                    // The group's own BALANCED closer. F5 #4 dies here: `$(g: int = f(1))`
                    // keeps `f(1)`; the hand suffix-trimmed every trailing `)`.
                    self.depth = 0;
                    self.cursor = self.cursor + 1;
                    let mut __next = ParamScanComp { state: "AfterGroup".to_string(), vars: ParamScanVars::AfterGroup {  }, args: ParamScanArgs::AfterGroup { } };
                    self.compartment = __next;
                    return Default::default();
                }
                self.refusal = 2;            // StrayCloser (`]`/`}` closing a `(` group)
                self.g_viable = false;
                let mut __next = ParamScanComp { state: "VerbatimTail".to_string(), vars: ParamScanVars::VerbatimTail {  }, args: ParamScanArgs::VerbatimTail { } };
                self.compartment = __next;
                return Default::default();
            }
            self.depth = self.depth - 1;
            if self.has_val == false { self.vs = self.cursor; self.has_val = true; }
            self.cursor = self.cursor + 1;
            self.ve = self.cursor;
            let mut __next = ParamScanComp { state: "GroupValue".to_string(), vars: ParamScanVars::GroupValue {  }, args: ParamScanArgs::GroupValue { } };
            self.compartment = __next;
            return Default::default();
        }
        if b == 32 || b == 9 || b == 10 || b == 13 {
            self.cursor = self.cursor + 1;
            let mut __next = ParamScanComp { state: "GroupValue".to_string(), vars: ParamScanVars::GroupValue {  }, args: ParamScanArgs::GroupValue { } };
            self.compartment = __next;
            return Default::default();
        }
        if self.has_val == false { self.vs = self.cursor; self.has_val = true; }
        self.cursor = self.cursor + 1;
        self.ve = self.cursor;
        let mut __next = ParamScanComp { state: "GroupValue".to_string(), vars: ParamScanVars::GroupValue {  }, args: ParamScanArgs::GroupValue { } };
        self.compartment = __next;
        return Default::default();
    }

    fn AfterGroup_step(&mut self) {
        if self.cursor >= self.src.fsm_len() {
            if self.adepth != 0 {
                self.g_viable = false;
            }
            record_part(&mut self.parts, self.group, self.has_val, self.vs, self.ve, self.cursor, true);
            let mut __next = ParamScanComp { state: "Accept".to_string(), vars: ParamScanVars::Accept {  }, args: ParamScanArgs::Accept { } };
            self.compartment = __next;
            return Default::default();
        }
        let b = self.src.fsm_get(self.cursor);
        if b == 32 || b == 9 || b == 10 || b == 13 {
            self.cursor = self.cursor + 1;
            let mut __next = ParamScanComp { state: "AfterGroup".to_string(), vars: ParamScanVars::AfterGroup {  }, args: ParamScanArgs::AfterGroup { } };
            self.compartment = __next;
            return Default::default();
        }
        if b == 44 {
            record_part(&mut self.parts, self.group, self.has_val, self.vs, self.ve, self.cursor, self.adepth == 0);
            self.group = 0;
            self.has_val = false;
            self.cursor = self.cursor + 1;
            let mut __next = ParamScanComp { state: "SegStart".to_string(), vars: ParamScanVars::SegStart {  }, args: ParamScanArgs::SegStart { } };
            self.compartment = __next;
            return Default::default();
        }
        self.refusal = 3;                    // TrailingAfterGroup
        self.g_viable = false;
        let mut __next = ParamScanComp { state: "VerbatimTail".to_string(), vars: ParamScanVars::VerbatimTail {  }, args: ParamScanArgs::VerbatimTail { } };
        self.compartment = __next;
        return Default::default();
    }

    fn VerbatimTail_step(&mut self) {
        if self.cursor >= self.src.fsm_len() {
            record_part(&mut self.parts, self.group, self.has_val, self.vs, self.ve, self.cursor, true);
            let mut __next = ParamScanComp { state: "Accept".to_string(), vars: ParamScanVars::Accept {  }, args: ParamScanArgs::Accept { } };
            self.compartment = __next;
            return Default::default();
        }
        let b = self.src.fsm_get(self.cursor);
        if b == 32 || b == 9 || b == 10 || b == 13 {
            self.cursor = self.cursor + 1;
            let mut __next = ParamScanComp { state: "VerbatimTail".to_string(), vars: ParamScanVars::VerbatimTail {  }, args: ParamScanArgs::VerbatimTail { } };
            self.compartment = __next;
            return Default::default();
        }
        if self.has_val == false { self.vs = self.cursor; self.has_val = true; }
        self.cursor = self.cursor + 1;
        self.ve = self.cursor;
        let mut __next = ParamScanComp { state: "VerbatimTail".to_string(), vars: ParamScanVars::VerbatimTail {  }, args: ParamScanArgs::VerbatimTail { } };
        self.compartment = __next;
        return Default::default();
    }

}

