use std::collections::HashMap;
use std::any::Any;


// Top-level comma-split of a `@@system Name(...)` param-list interior, dogfooded as an
// `@@[scan(u8)]` COUNTER automaton — the `@@system` replacement for the string-BLIND hand
// depth-0 comma loop in `text/scan/mod.rs::split_system_params`. This is C-final delegation 3:
// the last remaining production top-level-SPLIT recognition cycle (a Dyck/nesting DEPTH
// register). From the interior bytes it emits the top-level comma-split EXTENTS `(start, end)`;
// a `,` is a separator ONLY when the merged `()[]<>{}` nesting depth is 0 AND it is not inside
// a `"…"`-string default. The downstream per-part sigil parse (`$(`=state, `$>(`=enter, bare
// =domain; `name: type = default`) stays NATIVE — this machine produces only the split extents.
//
// It is a COUNTER (a single `depth`, merged Dyck-1 over five bracket kinds — deliberately NOT
// kind-matched, exactly the hand loop's alphabet), and it is STRING-AWARE by COMPOSITION: it
// runs StringScan via the `skip_string` leaf, so a `,`/`)`/bracket inside a `"…"` default (e.g.
// `b: T = "x,y"`) is skipped, never counted — the whole point of the fix. The alphabet is
// otherwise IDENTICAL to the hand loop (`( [ < {` open, `) ] > }` close, split at `,` when
// `depth == 0`), so on any input with no `"` the machine is byte-for-byte the hand loop; the
// SOLE intended divergence is string-awareness. It is TOTAL (always Accepts at end-of-input).
//
// STRING/ANGLE OPACITY (CARRIED — one root, sharpened 2026-07-21). ParamSplit replicates the
// retired hand loop's merged-Dyck alphabet EXACTLY: `$Scan` counts `<`/`>` as brackets alongside
// `()[]{}`, and skips `"`-strings ONLY (`skip_string` runs StringScan, double-quote only —
// matching ParenBalance, the `"`-only sibling that delimits this very interior via
// `paren_balance::scan`; the split extents must share ParenBalance's opacity model or disagree
// with the interior boundary). THREE known miscounts follow from that alphabet, ALL CARRIED:
//   (1) an operator/shift `<`/`>` in a default value MERGES params: `a: int = x < y, b: int`
//       inflates depth so the separator comma is swallowed; a lone `>` underflows depth. LOW
//       reachability (a top-level relational/shift as a default is unusual; `Map<K,V>` type
//       positions are the CARRIED-CORRECT case — the angle-as-bracket reading is right there).
//   (2) a `'…'` char default OVER-splits (`sep: char = ','`) or MIS-merges (`= ')'`) — the
//       `"`-only limit shared with ParenBalance/DelimBalance (#219). NOT separable: a `'`-aware
//       skip carries the Rust `'a`-lifetime hazard (literals.rs annotates the Rust `'` form
//       "char / lifetime" — it cannot tell `'a` from `'a'`), so it couples to a target-aware
//       char-vs-lifetime leaf across the whole `"`-only balance family. LOW–MODERATE reach.
//   (3) a `$(`/`$>(` header SIGIL is not recognized here (split_system_params does the sigil
//       parse natively, AFTER the split): the `>` of `$>(` is counted as a bracket-close, so a
//       header state/enter group with a TRAILING param drops it (`$(slot), $>(timeout), name`
//       silently loses `name` — pinned #[ignore]d in tests/paramsplit.rs), and
//       `trim_end_matches(')')` in split_system_params (scan/mod.rs) truncates a nested-paren
//       group default (`$(g: int = f(1))` → `f(1`). These are the declaration-site twins of the
//       Bug-A/Bug-B that ArgScan fixed at the CALL site. (3) is the REACHABLE one (a standard
//       multi-group header) and is currently UNTESTED apart from the ignore-pin.
// VOID CONDITION: the moment header state/enter-param correctness is in scope, this whole area is
// REPLACED by a declaration-site ArgScan — sigil recognition + the dual-counter fork adjudicated
// by ANGLE SELF-CONSISTENCY (`g_viable`), NOT declared arity (a declaration has none) — which
// subsumes ALL THREE. Do NOT patch the counter piecemeal: any change to its angle/opacity model
// forces a broad differential re-bless (the hand oracle counts angles and is sigil-blind).
//
// framec owns the WALK (dispatch + the depth counter + the part-start register); `skip_string`
// only runs StringScan, `record_part` only pushes the emitted extent — no walk lives in a leaf.
//
// Regen: framec-ng -l rust --emit paramsplit.frs | grep -v '^#!\[allow' > paramsplit.gen.rs

pub trait ParamSplitInput { fn fsm_get(&self, i: usize) -> u8; fn fsm_len(&self) -> usize; }
impl ParamSplitInput for &[u8] { fn fsm_get(&self, i: usize) -> u8 { self[i] } fn fsm_len(&self) -> usize { self.len() } }

#[derive(Clone)]
enum ParamSplitVars {
    Scan {  },
    Accept {  },
}
#[derive(Clone)]
enum ParamSplitArgs {
    Scan {  },
    Accept {  },
}
#[derive(Clone)]
struct ParamSplitComp {
    state: String,
    vars: ParamSplitVars,
    args: ParamSplitArgs,
}

pub struct ParamSplit<'a> {
    src: &'a [u8],
    pub cursor: usize,
    compartment: ParamSplitComp,
    stack: Vec<ParamSplitComp>,
    pub depth: i32,
    pub part_start: usize,
    pub parts: Vec<(usize, usize)>,
}

impl<'a> ParamSplit<'a> {
    pub fn over(src: &'a [u8]) -> Self {
        let compartment = ParamSplitComp { state: "Scan".to_string(), vars: ParamSplitVars::Scan {  }, args: ParamSplitArgs::Scan {  } };
        ParamSplit { src, cursor: 0, compartment, stack: Vec::new(), depth: 0, part_start: 0, parts: Vec::new() }
    }

    pub fn scan_at(&mut self, start: usize) -> bool {
        self.cursor = start;
        self.depth = 0;
        self.part_start = 0;
        self.parts = Vec::new();
        self.compartment = ParamSplitComp { state: "Scan".to_string(), vars: ParamSplitVars::Scan {  }, args: ParamSplitArgs::Scan {  } };
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
            "Scan" => self.Scan_step(),
            _ => {}
        }
    }

    fn Scan_step(&mut self) {
        if self.cursor >= self.src.fsm_len() {
            // Final tail: emit [part_start, len) iff nonempty — the hand loop's
            // `if start < inner.len() { push(inner[start..]) }`. An empty tail (a
            // trailing comma) is not emitted; an empty MIDDLE part is (native trims
            // + skips empties, exactly as the hand `if raw.is_empty() { continue }`).
            if self.part_start < self.cursor {
                record_part(&mut self.parts, self.part_start, self.cursor);
            }
            let mut __next = ParamSplitComp { state: "Accept".to_string(), vars: ParamSplitVars::Accept {  }, args: ParamSplitArgs::Accept { } };
            self.compartment = __next;
            return Default::default();
        }
        // Skip a whole "…"-string first, so a `,`/bracket inside a default value is
        // never counted (the fix). The leaf returns `cursor` unchanged when there is
        // no `"` here — then this is byte-for-byte the hand loop.
        let sk = skip_string(self.src, self.cursor);
        if sk > self.cursor {
            self.cursor = sk;
        } else {
            let b = self.src.fsm_get(self.cursor);
            if b == 44 {                                     // ,
                if self.depth == 0 {
                    // Top-level separator: emit [part_start, comma) and start the
                    // next part one past it. `cursor` IS the comma position here.
                    record_part(&mut self.parts, self.part_start, self.cursor);
                    self.part_start = self.cursor + 1;
                }
            }
            if b == 40 || b == 91 || b == 60 || b == 123 {   // ( [ < {
                self.depth = self.depth + 1;
            }
            if b == 41 || b == 93 || b == 62 || b == 125 {   // ) ] > }
                self.depth = self.depth - 1;
            }
            self.cursor = self.cursor + 1;
        }
    }

}

