use std::collections::HashMap;
use std::any::Any;


// The handler/action body BASE-COLUMN min-fold, dogfooded as a plain `@@system` — the emit-side
// twin of StmtWalk (same READ-ONLY BORROWED DOMAIN: the statement slice is a shared borrow
// threaded through one lifetime `'a`; the cursor, the running minimum, and the seen bit are the
// OWNED domain). It reifies the `base` computation `emit_body` fed to StmtWalk: the SHALLOWEST
// logical column across the body's statements — the reindent baseline everything else is measured
// against, so the user's nesting is reproduced without framec knowing what an `if` is.
//
// framec owns the WALK (the cursor `i`, the `min`/`seen` registers, the halt at `len`); the 8-way
// per-Stmt column extraction is a per-item function surfaced as the leaf `col_at`, which returns
// the statement's column or -1 for a Trivia (or an out-of-bounds index) — exactly the arms of the
// original `.filter_map(...)`. `$Scan` cycles: at end-of-slice (`i >= len`) it halts to `$Done`; a
// -1 column is skipped (the `filter_map` None); the first real column seeds `min`+`seen`; a later
// column shrinks `min` when smaller. The wrapper reads `min` (or 0 when nothing was recorded — the
// original `.unwrap_or(0)`).
//
// Regen: framec-ng -l rust --emit base_column.frs | grep -v '^#!\[allow' > base_column.gen.rs

#[derive(Clone)]
enum BaseColumnVars {
    Scan {  },
    Done {  },
}
#[derive(Clone)]
enum BaseColumnArgs {
    Scan {  },
    Done {  },
}
#[derive(Clone)]
struct BaseColumnComp {
    state: String,
    vars: BaseColumnVars,
    args: BaseColumnArgs,
}

pub struct BaseColumn<'a> {
    compartment: BaseColumnComp,
    stack: Vec<BaseColumnComp>,
    pub stmts: &'a [Stmt],
    pub len: usize,
    pub min: u32,
    pub seen: bool,
    pub i: usize,
}

impl<'a> BaseColumn<'a> {
    pub fn new(stmts: &'a [Stmt], len: usize) -> BaseColumn<'a> {
        let compartment = BaseColumnComp { state: "Scan".to_string(), vars: BaseColumnVars::Scan {  }, args: BaseColumnArgs::Scan {  } };
        BaseColumn { compartment, stack: Vec::new(), stmts: stmts, len: len, min: 0, seen: false, i: 0 }
    }

    pub fn step(&mut self) {
        match self.compartment.state.as_str() {
            "Scan" => self.Scan_step(),
            _ => {}
        }
    }

    fn Scan_step(&mut self) {
        if self.i >= self.len {
            let mut __next = BaseColumnComp { state: "Done".to_string(), vars: BaseColumnVars::Done {  }, args: BaseColumnArgs::Done { } };
            self.compartment = __next;
            return Default::default();
        }
        let c = col_at(self.stmts, self.i);
        if c < 0 {
            self.i = self.i + 1;
            let mut __next = BaseColumnComp { state: "Scan".to_string(), vars: BaseColumnVars::Scan {  }, args: BaseColumnArgs::Scan { } };
            self.compartment = __next;
            return Default::default();
        }
        let cu = c as u32;
        if self.seen == false {
            self.min = cu;
            self.seen = true;
            self.i = self.i + 1;
            let mut __next = BaseColumnComp { state: "Scan".to_string(), vars: BaseColumnVars::Scan {  }, args: BaseColumnArgs::Scan { } };
            self.compartment = __next;
            return Default::default();
        }
        if cu < self.min {
            self.min = cu;
        }
        self.i = self.i + 1;
        let mut __next = BaseColumnComp { state: "Scan".to_string(), vars: BaseColumnVars::Scan {  }, args: BaseColumnArgs::Scan { } };
        self.compartment = __next;
        return Default::default();
    }

}
