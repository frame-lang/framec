use std::collections::HashMap;
use std::any::Any;


// TOP-LEVEL `=` FINDER, dogfooded as an `@@[scan(u8)]` counter automaton — the offset of the
// FIRST `=` that is the default/init SEPARATOR of a `name: type = default` (or `name: type = init`)
// body: a `=` at bracket-depth 0 (Dyck-1 over `()[]{}`) AND angle-depth 0 (digraph-guarded `<>`),
// outside `"…"`-strings, excluding the `== <= >= != =>` digraphs. Reports via the `found` + `eq_at`
// registers (the wrapper maps `!found` -> `to`).
//
// It is the shared correct-class primitive that RETIRES three byte-blind hand splits (#249):
//   * `parse_one_param` (scan/mod.rs) — `split_once('=')`, so a `=` inside `<Item = u8>` truncated
//     the type and invented a bogus default (B2);
//   * `decl_read`'s `eq_or_end` leaf — a byte-blind `while != b'='`, the same truncation on a
//     domain/state-var type (B9);
//   * and, by composition through ParamScan, the emit-side `params_split`/`param_names` (B1).
//
// It HALTS at the first top-level `=`, which is ALWAYS in TYPE position — where `<` is
// unambiguously a generic opener, never a `less-than` operator. So, unlike ParamScan (which must
// FORK the angle reading because a DEFAULT can carry `a < b`), a single angle counter is EXACT
// here: no fork, no `g_viable`, no adjudicator. `adepth` never goes negative (a stray `>` at
// adepth 0 is a guarded operator or content, clamped) — the separator we seek precedes any
// operator `>`.
//
// String-blindness is killed by COMPOSITION, not reimplementation: `"`-only StringScan via the
// `skip_string` leaf (matching ParamScan — it AGREES with ParenBalance's interior boundary and
// dodges the Rust `'a`-lifetime hazard, so the residual char/lifetime gap is the SAME #219 carry).
// `angle_guard` (shared with ArgScan) excludes `<= >= -> =>` from the angle count; `eq_is_sep`
// excludes `== <= >= != =>` at the candidate `=`. framec owns the WALK (dispatch + the two
// counters); the leaves answer only O(1) facts or run StringScan — no walk lives in a leaf (D3).
//
// Regen: framec-ng -l rust --emit top_level_eq.frs | grep -v '^#!\[allow' > top_level_eq.gen.rs

pub trait TopLevelEqInput { fn fsm_get(&self, i: usize) -> u8; fn fsm_len(&self) -> usize; }
impl TopLevelEqInput for &[u8] { fn fsm_get(&self, i: usize) -> u8 { self[i] } fn fsm_len(&self) -> usize { self.len() } }

#[derive(Clone)]
enum TopLevelEqVars {
    Scan {  },
    Accept {  },
}
#[derive(Clone)]
enum TopLevelEqArgs {
    Scan {  },
    Accept {  },
}
#[derive(Clone)]
struct TopLevelEqComp {
    state: String,
    vars: TopLevelEqVars,
    args: TopLevelEqArgs,
}

pub struct TopLevelEq<'a> {
    src: &'a [u8],
    pub cursor: usize,
    compartment: TopLevelEqComp,
    stack: Vec<TopLevelEqComp>,
    pub from: usize,
    pub to: usize,
    pub depth: i32,
    pub adepth: i32,
    pub found: bool,
    pub eq_at: usize,
}

impl<'a> TopLevelEq<'a> {
    pub fn over(src: &'a [u8], from: usize, to: usize) -> Self {
        let compartment = TopLevelEqComp { state: "Scan".to_string(), vars: TopLevelEqVars::Scan {  }, args: TopLevelEqArgs::Scan {  } };
        TopLevelEq { src, cursor: 0, compartment, stack: Vec::new(), from: from, to: to, depth: 0, adepth: 0, found: false, eq_at: 0 }
    }

    pub fn scan_at(&mut self, start: usize) -> bool {
        self.cursor = start;
        self.depth = 0;
        self.adepth = 0;
        self.found = false;
        self.eq_at = 0;
        self.compartment = TopLevelEqComp { state: "Scan".to_string(), vars: TopLevelEqVars::Scan {  }, args: TopLevelEqArgs::Scan {  } };
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
        if self.cursor >= self.to {
            let mut __next = TopLevelEqComp { state: "Accept".to_string(), vars: TopLevelEqVars::Accept {  }, args: TopLevelEqArgs::Accept { } };
            self.compartment = __next;
            return Default::default();
        }
        // Opacity FIRST — a `=`/bracket/angle inside a `"…"`-string never takes part.
        // `"`-only (StringScan): declines on `'` and on an unterminated `"` (returns
        // `cursor` unchanged), matching ParamScan/ParenBalance — the #219 carry.
        let sk = skip_string(self.src, self.cursor);
        if sk > self.cursor {
            self.cursor = sk;
            let mut __next = TopLevelEqComp { state: "Scan".to_string(), vars: TopLevelEqVars::Scan {  }, args: TopLevelEqArgs::Scan { } };
            self.compartment = __next;
            return Default::default();
        }
        let b = self.src.fsm_get(self.cursor);
        // Angle nesting at bracket depth 0, digraph-guarded. In type position `<` is
        // always a generic opener, so a bare counter (no fork) is exact.
        if (b == 60 || b == 62) && self.depth == 0 {
            if angle_guard(self.src, self.cursor, self.from, self.to) == false {
                if b == 60 {
                    self.adepth = self.adepth + 1;
                }
                if b == 62 {
                    if self.adepth > 0 {
                        self.adepth = self.adepth - 1;
                    }
                }
            }
            self.cursor = self.cursor + 1;
            let mut __next = TopLevelEqComp { state: "Scan".to_string(), vars: TopLevelEqVars::Scan {  }, args: TopLevelEqArgs::Scan { } };
            self.compartment = __next;
            return Default::default();
        }
        if b == 40 || b == 91 || b == 123 {  // ( [ {  — merged Dyck-1 counter
            self.depth = self.depth + 1;
            self.cursor = self.cursor + 1;
            let mut __next = TopLevelEqComp { state: "Scan".to_string(), vars: TopLevelEqVars::Scan {  }, args: TopLevelEqArgs::Scan { } };
            self.compartment = __next;
            return Default::default();
        }
        if b == 41 || b == 93 || b == 125 {  // ) ] }
            if self.depth > 0 {
                self.depth = self.depth - 1;
            }
            self.cursor = self.cursor + 1;
            let mut __next = TopLevelEqComp { state: "Scan".to_string(), vars: TopLevelEqVars::Scan {  }, args: TopLevelEqArgs::Scan { } };
            self.compartment = __next;
            return Default::default();
        }
        if b == 61 && self.depth == 0 && self.adepth == 0 {  // `=` at top level
            if eq_is_sep(self.src, self.cursor, self.from, self.to) {
                self.found = true;
                self.eq_at = self.cursor;
                let mut __next = TopLevelEqComp { state: "Accept".to_string(), vars: TopLevelEqVars::Accept {  }, args: TopLevelEqArgs::Accept { } };
                self.compartment = __next;
                return Default::default();
            }
        }
        self.cursor = self.cursor + 1;
        let mut __next = TopLevelEqComp { state: "Scan".to_string(), vars: TopLevelEqVars::Scan {  }, args: TopLevelEqArgs::Scan { } };
        self.compartment = __next;
        return Default::default();
    }

}

