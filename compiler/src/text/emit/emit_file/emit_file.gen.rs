use std::collections::HashMap;
use std::any::Any;


// The driver's TOP-LEVEL ITEM WALK, dogfooded as a plain `@@system` — the OUTERMOST emit sequencer,
// the body of the public `emit` fn. It reifies `emit`'s file-item loop: the pass that walks
// `ast.items` and, for each, either passes the user's top-level native code through verbatim (the
// "water") or delegates a system to the `EmitSystem` phase spine. It rides the same READ-ONLY
// BORROWED DOMAIN as the six landed emit machines: the source, the file AST, and the symbol table
// are SHARED BORROWS threaded through one lifetime `'a`, alongside the `&dyn Backend`; the OWNED
// domain is the accumulating output `out` and the single walk cursor `i` (bound `n`).
//
// A SINGLE CYCLE STATE — the reachability-style top walk. `$Item` cycles `ast.items`, and on each
// item FORKS structurally:
//   Item::Native  -> render the water (verbatim, minus `@@Sys(...)` islands) via a native leaf; SELF-LOOP.
//   otherwise     -> delegate to the `EmitSystem` phase spine (a System resolves to its symbol and
//                    runs; a Bom/Pragma/Efsm item resolves to nothing and emits nothing, exactly as
//                    the hand loop's `else { continue }` did); back to `$Item`.
// At `i >= n` it halts `-> $Done`. The `file_header` preamble is a NATIVE bookend in the wrapper
// (`walk`), emitted once before the cycle — a backend spelling, not a sub-system, so it stays out of
// the cycle. There is no closer: the wrapper's `out.finish()` is the terminal.
//
// THE HONEST MACHINE CLASS. This is the §3 DEGENERATE POLE — a program-counter walk over the
// ALREADY-PARSED item list, whose only fork is a structural type-dispatch (`Item::Native`? — Frame
// cannot match a Rust enum), NOT input recognition. The cursor `i` carries no recognition register;
// the same item list always walks the same way. Nothing is glossed. Its reify payoff is not a hidden
// mode but DOGFOOD UNIFORMITY: with this machine landed, the ENTIRE emit driver — from the file, down
// through each system's phases, its handlers, its statements, to the base column — runs through
// @@systems, differential-gated byte-for-byte vs the preserved `emit_file_hand`.
//
// framec owns the WALK (the cursor, the bound, the self-loops, the halt). The un-Frame-able work is
// per-item NATIVE LEAVES: `is_native_item` (the structural fork), `emit_native_item` (the water
// render — shared with the oracle via `driver::render_native_item`), and `emit_system_item` (resolve
// the system's symbol and call the landed `EmitSystem` `walk`, unchanged). Every spelling stays
// native and byte-identical; the machine only sequences the walk.
//
// Regen: framec-ng -l rust --emit emit_file.frs | grep -v '^#!\[allow' > emit_file.gen.rs

#[derive(Clone)]
enum EmitFileVars {
    Item {  },
    Done {  },
}
#[derive(Clone)]
enum EmitFileArgs {
    Item {  },
    Done {  },
}
#[derive(Clone)]
struct EmitFileComp {
    state: String,
    vars: EmitFileVars,
    args: EmitFileArgs,
}

pub struct EmitFile<'a> {
    compartment: EmitFileComp,
    stack: Vec<EmitFileComp>,
    pub src: &'a Source,
    pub ast: &'a FileAst,
    pub syms: &'a SymbolTable,
    pub be: &'a dyn Backend,
    pub n: usize,
    pub out: Sink,
    pub i: usize,
}

impl<'a> EmitFile<'a> {
    pub fn new(src: &'a Source, ast: &'a FileAst, syms: &'a SymbolTable, be: &'a dyn Backend, n: usize, out: Sink) -> EmitFile<'a> {
        let compartment = EmitFileComp { state: "Item".to_string(), vars: EmitFileVars::Item {  }, args: EmitFileArgs::Item {  } };
        EmitFile { compartment, stack: Vec::new(), src: src, ast: ast, syms: syms, be: be, n: n, out: out, i: 0 }
    }

    pub fn step(&mut self) {
        match self.compartment.state.as_str() {
            "Item" => self.Item_step(),
            _ => {}
        }
    }

    fn Item_step(&mut self) {
        if self.i >= self.n {
            let mut __next = EmitFileComp { state: "Done".to_string(), vars: EmitFileVars::Done {  }, args: EmitFileArgs::Done { } };
            self.compartment = __next;
            return Default::default();
        }
        let isn = is_native_item(self.ast, self.i);
        if isn {
            emit_native_item(self.src, self.syms, self.be, self.ast, self.i, &mut self.out);
            self.i = self.i + 1;
            let mut __next = EmitFileComp { state: "Item".to_string(), vars: EmitFileVars::Item {  }, args: EmitFileArgs::Item { } };
            self.compartment = __next;
            return Default::default();
        }
        emit_system_item(self.src, self.syms, self.be, self.ast, self.i, &mut self.out);
        self.i = self.i + 1;
        let mut __next = EmitFileComp { state: "Item".to_string(), vars: EmitFileVars::Item {  }, args: EmitFileArgs::Item { } };
        self.compartment = __next;
        return Default::default();
    }

}

