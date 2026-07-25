use std::collections::HashMap;
use std::any::Any;


// The instantiation ARG-LIST parser (spec §1103 call sites), dogfooded as an `@@[scan(u8)]`
// system — the `@@system` replacement for the hand `parse_inst_args` + `split_top_commas` +
// `split_top_eq` (parts.rs M6): the only production machinery in parts.rs that had no named
// system. ONE left-to-right walk segments the interior `[from,to)` at top-level commas,
// classifies each arg's group by its sigil (`$(`/`$>(`/bare), finds a qualifying top-level
// `=` with an IDENTIFIER left-hand side (named form), and records TRIMMED SPANS per arg.
// It is TOTAL (always Accepts; malformed input degrades to a VERBATIM tail with a named
// `refusal` reason — the hand's swallow-to-end, made observable).
//
// It is a TWO-COUNTER automaton (design record §11, Option C "fork and adjudicate"):
//   depth  — the merged `([{` counter (deliberately Dyck-1, kind-checked only at the group
//            sigil's own closer). The group closer is the BALANCED closer found by the walk —
//            the hand's `trim_end_matches(')')` (parts.rs:363,365) ate the user's own `)` in
//            `$(g(1))`.
//   adepth — angle nesting under hypothesis G (angles-as-brackets), counted ONLY at bracket
//            depth 0, outside opaque regions, in $Value (never in $GroupValue — group
//            interiors sit at depth >= 1 — and never in $VerbatimTail). Within one list either
//            EVERY counted angle is a bracket (hypothesis G — the hand comma splitter's
//            alphabet, disciplined) or NONE is (hypothesis O — the hand eq splitter's
//            alphabet): the two hand siblings WERE the two hypotheses, one per function.
//            One pass computes both: the records are the O segmentation; each record's
//            `g_end` bit ("adepth == 0 at this boundary") marks the boundaries that hold
//            under G too (Lemma 2: G boundaries are exactly the O boundaries at adepth 0).
//            `g_viable` is cleared by a counted `>` at adepth 0 (operator evidence), by
//            adepth != 0 at end-of-interior (unclosed angle), and by ANY refusal (refusal
//            supersedes fork). Operator digraphs `<=` `>=` `->` `=>` are guard-excluded
//            (`angle_guard`, O(1) byte compares) — never counted, ordinary content under
//            both hypotheses. The wrapper folds the G candidate from the records
//            (`merge_g`) and defers the choice to declared arity downstream — the machine
//            never guesses.
//
// String-blindness is killed by COMPOSITION, not reimplementation: opacity comes from
// OpaqueScan via the `opaque_skip`/`opaque_unterm` leaves (the same model the rest of the
// grammar uses — comments, chars, triples, raws; the hand code had three private
// `"`/`'`-only skippers). A `,`/`=`/bracket/angle inside an opaque region never takes part.
//
// Regen: framec-ng -l rust --emit arg_scan.frs | grep -v '^#!\[allow' > arg_scan.gen.rs

pub trait ArgScanInput { fn fsm_get(&self, i: usize) -> u8; fn fsm_len(&self) -> usize; }
impl ArgScanInput for &[u8] { fn fsm_get(&self, i: usize) -> u8 { self[i] } fn fsm_len(&self) -> usize { self.len() } }

#[derive(Clone)]
enum ArgScanVars {
    SegStart {  },
    Value {  },
    GroupValue {  },
    AfterGroup {  },
    VerbatimTail {  },
    Accept {  },
}
#[derive(Clone)]
enum ArgScanArgs {
    SegStart {  },
    Value {  },
    GroupValue {  },
    AfterGroup {  },
    VerbatimTail {  },
    Accept {  },
}
#[derive(Clone)]
struct ArgScanComp {
    state: String,
    vars: ArgScanVars,
    args: ArgScanArgs,
}

pub struct ArgScan<'a> {
    src: &'a [u8],
    pub cursor: usize,
    compartment: ArgScanComp,
    stack: Vec<ArgScanComp>,
    pub target: Target,
    pub from: usize,
    pub to: usize,
    pub depth: i32,
    pub adepth: i32,
    pub g_viable: bool,
    pub angle_touched: bool,
    pub group: i32,
    pub seg_start: usize,
    pub vs: usize,
    pub ve: usize,
    pub has_val: bool,
    pub ns: usize,
    pub ne: usize,
    pub has_name: bool,
    pub refusal: i32,
    pub dropped_empty: i32,
    pub args: Vec<(i32, bool, usize, usize, usize, usize, bool)>,
}

impl<'a> ArgScan<'a> {
    pub fn over(src: &'a [u8], target: Target, from: usize, to: usize) -> Self {
        let compartment = ArgScanComp { state: "SegStart".to_string(), vars: ArgScanVars::SegStart {  }, args: ArgScanArgs::SegStart {  } };
        ArgScan { src, cursor: 0, compartment, stack: Vec::new(), target: target, from: from, to: to, depth: 0, adepth: 0, g_viable: true, angle_touched: false, group: 0, seg_start: 0, vs: 0, ve: 0, has_val: false, ns: 0, ne: 0, has_name: false, refusal: 0, dropped_empty: 0, args: Vec::new() }
    }

    pub fn scan_at(&mut self, start: usize) -> bool {
        self.cursor = start;
        self.depth = 0;
        self.adepth = 0;
        self.g_viable = true;
        self.angle_touched = false;
        self.group = 0;
        self.seg_start = 0;
        self.vs = 0;
        self.ve = 0;
        self.has_val = false;
        self.ns = 0;
        self.ne = 0;
        self.has_name = false;
        self.refusal = 0;
        self.dropped_empty = 0;
        self.args = Vec::new();
        self.compartment = ArgScanComp { state: "SegStart".to_string(), vars: ArgScanVars::SegStart {  }, args: ArgScanArgs::SegStart {  } };
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
        if self.cursor >= self.to {
            // Empty interior / trailing comma: nothing pending. An unclosed angle
            // still kills hypothesis G (the tail after the last comma was ws-only,
            // so adepth is unchanged since that comma).
            if self.adepth != 0 {
                self.g_viable = false;
            }
            let mut __next = ArgScanComp { state: "Accept".to_string(), vars: ArgScanVars::Accept {  }, args: ArgScanArgs::Accept { } };
            self.compartment = __next;
            return Default::default();
        }
        let b = self.src.fsm_get(self.cursor);
        if b == 32 || b == 9 || b == 10 || b == 13 {
            self.cursor = self.cursor + 1;
            let mut __next = ArgScanComp { state: "SegStart".to_string(), vars: ArgScanVars::SegStart {  }, args: ArgScanArgs::SegStart { } };
            self.compartment = __next;
            return Default::default();
        }
        if b == 44 {
            // `,,` / leading `,`: the hand DROPS the empty segment silently
            // (parts.rs:358-360). Carried — but counted, so it is observable.
            // Comma-delimited empties ONLY: a ws tail after a trailing comma ends
            // at end-of-interior, not at a comma, and is NOT counted.
            self.dropped_empty = self.dropped_empty + 1;
            self.cursor = self.cursor + 1;
            let mut __next = ArgScanComp { state: "SegStart".to_string(), vars: ArgScanVars::SegStart {  }, args: ArgScanArgs::SegStart { } };
            self.compartment = __next;
            return Default::default();
        }
        self.seg_start = self.cursor;
        if is_sigil_enter(self.src, self.cursor, self.to) {
            self.group = 2;
            self.cursor = self.cursor + 3;   // `$>(`
            self.depth = 1;
            let mut __next = ArgScanComp { state: "GroupValue".to_string(), vars: ArgScanVars::GroupValue {  }, args: ArgScanArgs::GroupValue { } };
            self.compartment = __next;
            return Default::default();
        }
        if is_sigil_state(self.src, self.cursor, self.to) {
            self.group = 1;
            self.cursor = self.cursor + 2;   // `$(`
            self.depth = 1;
            let mut __next = ArgScanComp { state: "GroupValue".to_string(), vars: ArgScanVars::GroupValue {  }, args: ArgScanArgs::GroupValue { } };
            self.compartment = __next;
            return Default::default();
        }
        self.group = 0;
        self.depth = 0;
        let mut __next = ArgScanComp { state: "Value".to_string(), vars: ArgScanVars::Value {  }, args: ArgScanArgs::Value { } };
        self.compartment = __next;
        return Default::default();
    }

    fn Value_step(&mut self) {
        if self.cursor >= self.to {
            // End-of-interior emit: the boundary trivially holds under G, but an
            // unclosed angle (adepth != 0) kills G's viability.
            if self.adepth != 0 {
                self.g_viable = false;
            }
            record_arg(&mut self.args, self.group, self.has_name, self.ns, self.ne, self.has_val, self.vs, self.ve, self.cursor, true);
            let mut __next = ArgScanComp { state: "Accept".to_string(), vars: ArgScanVars::Accept {  }, args: ArgScanArgs::Accept { } };
            self.compartment = __next;
            return Default::default();
        }
        // Opacity FIRST — a `,`/`=`/bracket/angle inside a comment/string/char/
        // triple/raw never takes part in splitting. Same policy leaf as DelimBalance
        // (machine::skip_opaque). An opaque region that opens but never closes is
        // refusal 1: the hand's swallow-to-end, named.
        if opaque_unterm(self.src, self.cursor, self.target) {
            self.refusal = 1;                // UnterminatedOpaque
            self.g_viable = false;           // refusal supersedes fork
            let mut __next = ArgScanComp { state: "VerbatimTail".to_string(), vars: ArgScanVars::VerbatimTail {  }, args: ArgScanArgs::VerbatimTail { } };
            self.compartment = __next;
            return Default::default();
        }
        let sk = opaque_skip(self.src, self.cursor, self.to, self.target);
        if sk > self.cursor {
            if self.has_val == false { self.vs = self.cursor; self.has_val = true; }
            self.cursor = sk;
            self.ve = sk;
            let mut __next = ArgScanComp { state: "Value".to_string(), vars: ArgScanVars::Value {  }, args: ArgScanArgs::Value { } };
            self.compartment = __next;
            return Default::default();
        }
        let b = self.src.fsm_get(self.cursor);
        // The dual-counter angle branch (§11.1): a counted `<`/`>` at bracket depth 0
        // updates hypothesis G's registers, then FALLS THROUGH as ordinary content —
        // the byte is content under both hypotheses. Digraphs (`<=` `>=` `->` `=>`)
        // are guard-excluded and never counted.
        if (b == 60 || b == 62) && self.depth == 0 {
            if angle_guard(self.src, self.cursor, self.from, self.to) == false {
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
            record_arg(&mut self.args, self.group, self.has_name, self.ns, self.ne, self.has_val, self.vs, self.ve, self.cursor, self.adepth == 0);
            self.group = 0;
            self.has_val = false;
            self.has_name = false;
            self.cursor = self.cursor + 1;
            let mut __next = ArgScanComp { state: "SegStart".to_string(), vars: ArgScanVars::SegStart {  }, args: ArgScanArgs::SegStart { } };
            self.compartment = __next;
            return Default::default();
        }
        if b == 40 || b == 91 || b == 123 {  // ( [ {  — merged counter, deliberately
            self.depth = self.depth + 1;
            if self.has_val == false { self.vs = self.cursor; self.has_val = true; }
            self.cursor = self.cursor + 1;
            self.ve = self.cursor;
            let mut __next = ArgScanComp { state: "Value".to_string(), vars: ArgScanVars::Value {  }, args: ArgScanArgs::Value { } };
            self.compartment = __next;
            return Default::default();
        }
        if b == 41 || b == 93 || b == 125 {  // ) ] }
            if self.depth == 0 {
                // The hand drove depth negative here and silenced commas — or a later
                // `(` RESURRECTED depth 0 and split mid-group. Named refusal instead.
                self.refusal = 2;            // StrayCloser
                self.g_viable = false;
                let mut __next = ArgScanComp { state: "VerbatimTail".to_string(), vars: ArgScanVars::VerbatimTail {  }, args: ArgScanArgs::VerbatimTail { } };
                self.compartment = __next;
                return Default::default();
            }
            self.depth = self.depth - 1;
            if self.has_val == false { self.vs = self.cursor; self.has_val = true; }
            self.cursor = self.cursor + 1;
            self.ve = self.cursor;
            let mut __next = ArgScanComp { state: "Value".to_string(), vars: ArgScanVars::Value {  }, args: ArgScanArgs::Value { } };
            self.compartment = __next;
            return Default::default();
        }
        if b == 61 && self.depth == 0 && self.has_name == false {
            if eq_guard_ok(self.src, self.cursor, self.from, self.to) {
                if self.has_val {
                    if is_ident_span(self.src, self.vs, self.ve) {
                        // Named form: the value-so-far IS the (trimmed) name. The
                        // ident requirement (L27/L28) is load-bearing for Lemma 3(i):
                        // a run-initial name never contains a counted `<`, so naming
                        // never diverges between the hypotheses.
                        self.has_name = true;
                        self.ns = self.vs;
                        self.ne = self.ve;
                        self.has_val = false;
                        self.cursor = self.cursor + 1;
                        let mut __next = ArgScanComp { state: "Value".to_string(), vars: ArgScanVars::Value {  }, args: ArgScanArgs::Value { } };
                        self.compartment = __next;
                        return Default::default();
                    }
                }
            }
            // fall through: this `=` is an ordinary content byte
        }
        if b == 32 || b == 9 || b == 10 || b == 13 {
            self.cursor = self.cursor + 1;   // ws: cursor moves, ve does not (auto-trim)
            let mut __next = ArgScanComp { state: "Value".to_string(), vars: ArgScanVars::Value {  }, args: ArgScanArgs::Value { } };
            self.compartment = __next;
            return Default::default();
        }
        if self.has_val == false { self.vs = self.cursor; self.has_val = true; }
        self.cursor = self.cursor + 1;
        self.ve = self.cursor;
        let mut __next = ArgScanComp { state: "Value".to_string(), vars: ArgScanVars::Value {  }, args: ArgScanArgs::Value { } };
        self.compartment = __next;
        return Default::default();
    }

    fn GroupValue_step(&mut self) {
        if self.cursor >= self.to {
            self.refusal = 4;                // UnclosedGroup
            self.g_viable = false;
            let mut __next = ArgScanComp { state: "VerbatimTail".to_string(), vars: ArgScanVars::VerbatimTail {  }, args: ArgScanArgs::VerbatimTail { } };
            self.compartment = __next;
            return Default::default();
        }
        if opaque_unterm(self.src, self.cursor, self.target) {
            self.refusal = 1;                // UnterminatedOpaque
            self.g_viable = false;
            let mut __next = ArgScanComp { state: "VerbatimTail".to_string(), vars: ArgScanVars::VerbatimTail {  }, args: ArgScanArgs::VerbatimTail { } };
            self.compartment = __next;
            return Default::default();
        }
        let sk = opaque_skip(self.src, self.cursor, self.to, self.target);
        if sk > self.cursor {
            if self.has_val == false { self.vs = self.cursor; self.has_val = true; }
            self.cursor = sk;
            self.ve = sk;
            let mut __next = ArgScanComp { state: "GroupValue".to_string(), vars: ArgScanVars::GroupValue {  }, args: ArgScanArgs::GroupValue { } };
            self.compartment = __next;
            return Default::default();
        }
        let b = self.src.fsm_get(self.cursor);
        if b == 40 || b == 91 || b == 123 {
            self.depth = self.depth + 1;
            if self.has_val == false { self.vs = self.cursor; self.has_val = true; }
            self.cursor = self.cursor + 1;
            self.ve = self.cursor;
            let mut __next = ArgScanComp { state: "GroupValue".to_string(), vars: ArgScanVars::GroupValue {  }, args: ArgScanArgs::GroupValue { } };
            self.compartment = __next;
            return Default::default();
        }
        if b == 41 || b == 93 || b == 125 {
            if self.depth == 1 {
                if b == 41 {
                    // The group's own closer — the BALANCED closer found by the walk,
                    // kind-checked. Bug A dies here: `$(g(1))` keeps `g(1)`; the hand
                    // suffix-trimmed every trailing `)` (parts.rs:363,365).
                    self.depth = 0;
                    self.cursor = self.cursor + 1;
                    let mut __next = ArgScanComp { state: "AfterGroup".to_string(), vars: ArgScanVars::AfterGroup {  }, args: ArgScanArgs::AfterGroup { } };
                    self.compartment = __next;
                    return Default::default();
                }
                // A `]`/`}` closing the group's `(` — the one kind-check the merged
                // counter carries.
                self.refusal = 2;            // StrayCloser
                self.g_viable = false;
                let mut __next = ArgScanComp { state: "VerbatimTail".to_string(), vars: ArgScanVars::VerbatimTail {  }, args: ArgScanArgs::VerbatimTail { } };
                self.compartment = __next;
                return Default::default();
            }
            self.depth = self.depth - 1;
            if self.has_val == false { self.vs = self.cursor; self.has_val = true; }
            self.cursor = self.cursor + 1;
            self.ve = self.cursor;
            let mut __next = ArgScanComp { state: "GroupValue".to_string(), vars: ArgScanVars::GroupValue {  }, args: ArgScanArgs::GroupValue { } };
            self.compartment = __next;
            return Default::default();
        }
        if b == 61 && self.depth == 1 && self.has_name == false {
            if eq_guard_ok(self.src, self.cursor, self.from, self.to) {
                if self.has_val {
                    if is_ident_span(self.src, self.vs, self.ve) {
                        self.has_name = true;
                        self.ns = self.vs;
                        self.ne = self.ve;
                        self.has_val = false;
                        self.cursor = self.cursor + 1;
                        let mut __next = ArgScanComp { state: "GroupValue".to_string(), vars: ArgScanVars::GroupValue {  }, args: ArgScanArgs::GroupValue { } };
                        self.compartment = __next;
                        return Default::default();
                    }
                }
            }
            // fall through: this `=` is an ordinary content byte
        }
        if b == 32 || b == 9 || b == 10 || b == 13 {
            self.cursor = self.cursor + 1;
            let mut __next = ArgScanComp { state: "GroupValue".to_string(), vars: ArgScanVars::GroupValue {  }, args: ArgScanArgs::GroupValue { } };
            self.compartment = __next;
            return Default::default();
        }
        if self.has_val == false { self.vs = self.cursor; self.has_val = true; }
        self.cursor = self.cursor + 1;
        self.ve = self.cursor;
        let mut __next = ArgScanComp { state: "GroupValue".to_string(), vars: ArgScanVars::GroupValue {  }, args: ArgScanArgs::GroupValue { } };
        self.compartment = __next;
        return Default::default();
    }

    fn AfterGroup_step(&mut self) {
        if self.cursor >= self.to {
            if self.adepth != 0 {
                self.g_viable = false;
            }
            record_arg(&mut self.args, self.group, self.has_name, self.ns, self.ne, self.has_val, self.vs, self.ve, self.cursor, true);
            let mut __next = ArgScanComp { state: "Accept".to_string(), vars: ArgScanVars::Accept {  }, args: ArgScanArgs::Accept { } };
            self.compartment = __next;
            return Default::default();
        }
        let b = self.src.fsm_get(self.cursor);
        if b == 32 || b == 9 || b == 10 || b == 13 {
            self.cursor = self.cursor + 1;
            let mut __next = ArgScanComp { state: "AfterGroup".to_string(), vars: ArgScanVars::AfterGroup {  }, args: ArgScanArgs::AfterGroup { } };
            self.compartment = __next;
            return Default::default();
        }
        if b == 44 {
            record_arg(&mut self.args, self.group, self.has_name, self.ns, self.ne, self.has_val, self.vs, self.ve, self.cursor, self.adepth == 0);
            self.group = 0;
            self.has_val = false;
            self.has_name = false;
            self.cursor = self.cursor + 1;
            let mut __next = ArgScanComp { state: "SegStart".to_string(), vars: ArgScanVars::SegStart {  }, args: ArgScanArgs::SegStart { } };
            self.compartment = __next;
            return Default::default();
        }
        self.refusal = 3;                    // TrailingAfterGroup
        self.g_viable = false;
        let mut __next = ArgScanComp { state: "VerbatimTail".to_string(), vars: ArgScanVars::VerbatimTail {  }, args: ArgScanArgs::VerbatimTail { } };
        self.compartment = __next;
        return Default::default();
    }

    fn VerbatimTail_step(&mut self) {
        if self.cursor >= self.to {
            record_arg(&mut self.args, self.group, self.has_name, self.ns, self.ne, self.has_val, self.vs, self.ve, self.cursor, true);
            let mut __next = ArgScanComp { state: "Accept".to_string(), vars: ArgScanVars::Accept {  }, args: ArgScanArgs::Accept { } };
            self.compartment = __next;
            return Default::default();
        }
        let b = self.src.fsm_get(self.cursor);
        if b == 32 || b == 9 || b == 10 || b == 13 {
            self.cursor = self.cursor + 1;
            let mut __next = ArgScanComp { state: "VerbatimTail".to_string(), vars: ArgScanVars::VerbatimTail {  }, args: ArgScanArgs::VerbatimTail { } };
            self.compartment = __next;
            return Default::default();
        }
        if self.has_val == false { self.vs = self.cursor; self.has_val = true; }
        self.cursor = self.cursor + 1;
        self.ve = self.cursor;
        let mut __next = ArgScanComp { state: "VerbatimTail".to_string(), vars: ArgScanVars::VerbatimTail {  }, args: ArgScanArgs::VerbatimTail { } };
        self.compartment = __next;
        return Default::default();
    }

}
