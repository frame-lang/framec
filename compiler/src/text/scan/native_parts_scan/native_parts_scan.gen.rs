use std::collections::HashMap;
use std::any::Any;


// The native-code ISLAND DISPATCH, dogfooded as an `@@[scan(u8)]` system — the PRODUCTION
// walk behind `parts::native_parts` (the construction driver). It walks `[from, limit)` of
// the FULL buffer and, at each position, tries the recognizers IN THE HAND ORDER (opaque ->
// instantiation -> embed-call -> ref); the first hit is flushed as a part (with the preceding
// text) and the cursor jumps past it. framec owns the WALK ($Walk + the step loop); the
// `try_island` leaf COMPOSES OpaqueScan / InstScan / EmbedScan / RefScan under the kind-aware
// `to`-policy (comment clamps to `limit`, a literal that overruns `limit` is rejected,
// unterminated falls through to water — ledger T-N1..T-N4). Composition four deep:
// NativePartsScan -> {OpaqueScan, InstScan, EmbedScan, RefScan} -> {RawString, BraceBalance,
// ParenBalance} -> StringScan.
//
// parts: (kind, start, end) — kind 0=Text 1=Literal 2=Ref 3=Instantiate 4=EmbedCall
// 5=Comment (the comment/literal split the driver's divergent policies and node shapes
// require — DP-2 keeps 1/2/3/4 stable).
//
// Bounds (DP-4): the walk runs over the FULL buffer with `limit` config, NEVER a truncated
// slice — a slice cannot distinguish a block comment that closes beyond `to` (hand: CLAMPED
// comment node) from one that never closes (hand: water); both hit slice-EOF. `from` is
// CONSTRUCTOR config (`text_start = from`) because `scan_at` resets every literal-initialized
// domain field — ctor-param-initialized fields survive it (gate amendment 2026-07-18).
//
// Honest class (D8): a regular TRANSDUCER — one steady mode + an output accumulator; the
// only counters live in the composed sub-systems. It does NOT descend into holes (per-level
// extent-independence; the driver recurses).
//
// Regen: framec-ng -l rust --emit native_parts_scan.frs | grep -v '^#!\[allow' > native_parts_scan.gen.rs

pub trait NativePartsScanInput { fn fsm_get(&self, i: usize) -> u8; fn fsm_len(&self) -> usize; }
impl NativePartsScanInput for &[u8] { fn fsm_get(&self, i: usize) -> u8 { self[i] } fn fsm_len(&self) -> usize { self.len() } }

#[derive(Clone)]
enum NativePartsScanVars {
    Walk {  },
    Accept {  },
}
#[derive(Clone)]
enum NativePartsScanArgs {
    Walk {  },
    Accept {  },
}
#[derive(Clone)]
struct NativePartsScanComp {
    state: String,
    vars: NativePartsScanVars,
    args: NativePartsScanArgs,
}

pub struct NativePartsScan<'a> {
    src: &'a [u8],
    pub cursor: usize,
    compartment: NativePartsScanComp,
    stack: Vec<NativePartsScanComp>,
    pub target: Target,
    pub limit: usize,
    pub parts: Vec<(i32, usize, usize)>,
    pub text_start: usize,
}

impl<'a> NativePartsScan<'a> {
    pub fn over(src: &'a [u8], target: Target, limit: usize, from: usize) -> Self {
        let compartment = NativePartsScanComp { state: "Walk".to_string(), vars: NativePartsScanVars::Walk {  }, args: NativePartsScanArgs::Walk {  } };
        NativePartsScan { src, cursor: 0, compartment, stack: Vec::new(), target: target, limit: limit, parts: Vec::new(), text_start: from }
    }

    pub fn scan_at(&mut self, start: usize) -> bool {
        self.cursor = start;
        self.parts = Vec::new();
        self.compartment = NativePartsScanComp { state: "Walk".to_string(), vars: NativePartsScanVars::Walk {  }, args: NativePartsScanArgs::Walk {  } };
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
            "Walk" => self.Walk_step(),
            _ => {}
        }
    }

    fn Walk_step(&mut self) {
        if self.cursor >= self.limit {
            flush_text(&mut self.parts, self.text_start, self.cursor);
            let mut __next = NativePartsScanComp { state: "Accept".to_string(), vars: NativePartsScanVars::Accept {  }, args: NativePartsScanArgs::Accept { } };
            self.compartment = __next;
            return Default::default();
        }
        // Δ3 (T-N1/T-N2, DP-1): an UNTERMINATED comment/literal opening here is not
        // scanned for islands — its rescued interior becomes ONE plain Text run to
        // `limit` (content, not code). No diagnostics channel (DP-1).
        if unterminated_at(self.src, self.cursor, self.target) {
            flush_text(&mut self.parts, self.text_start, self.limit);
            self.cursor = self.limit;
            let mut __next = NativePartsScanComp { state: "Accept".to_string(), vars: NativePartsScanVars::Accept {  }, args: NativePartsScanArgs::Accept { } };
            self.compartment = __next;
            return Default::default();
        }
        let hit = try_island(self.src, self.cursor, self.limit, self.target);
        if hit.0 != 0 {
            flush_text(&mut self.parts, self.text_start, self.cursor);
            record_part(&mut self.parts, hit.0, self.cursor, hit.1);
            self.cursor = hit.1;
            self.text_start = self.cursor;
        } else {
            self.cursor = self.cursor + 1;
        }
    }

}

