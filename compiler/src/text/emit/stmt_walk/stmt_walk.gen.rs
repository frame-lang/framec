use std::collections::HashMap;
use std::any::Any;


// The handler/action BODY statement walk, dogfooded as a plain `@@system` TRANSDUCER — the
// emit-side analogue of the back-half graph walkers (Reachability, HsmCycle), and the first
// machine to ride the READ-ONLY BORROWED DOMAIN (the plain-`@@system` twin of a scanner's
// `&'a [u8]`): its input — the statement slice, the source, the symbol table, the current
// system/state, and the `&dyn Backend` — are all SHARED BORROWS threaded through one lifetime
// `'a`; its OWNED domain is the accumulating output `out`, the cursor `i`, and the one-bit
// `terminated` latch. It reifies `emit_body`.
//
// This is a genuine Mealy transducer: it consumes the statements in order, emits target text for
// each (through the backend's SPELLINGS, unchanged, in native leaves), and carries ONE bit of
// state — `terminated` — set when a BASE-NESTING terminal (`depth == 0 && rel == 0`
// transition / stack-push / pop / `@@:return`) fires. That bit is read back two ways, exactly as
// the hand walk read it: it HALTS the walk (`-> $Done`, so nothing after a base-nesting terminal
// is spelled — the dead code the old compiler stripped from text it had already emitted), and it
// selects the body's terminal (`Terminated` vs `Fell`) for the wrapper.
//
// WHICH KIND OF BODY this is rides the domain as `role: BodyRole` — Handler or Action. It is a
// TAG framec put on the tree (the body came out of a state's HandlerNode or out of an
// `actions:`/`operations:` Decl), not a sentinel decoded from `state == ""`, and it reaches
// exactly one arm: `@@:(expr)` parks its value on the live FrameContext in a HANDLER and spells
// the target's own `return` in an ACTION, which has no context because the user may call it
// directly.
//
// WHAT COUNTS AS TERMINAL is the backend's answer, not the walk's: a statement only ends the body
// if that target's SPELLING of it actually returns. `@@:(expr)` returns on Java/Rust/C and does
// NOT on Python (where it assigns the context's return slot and execution continues), so
// `emit_return_call` asks `Backend::return_call_terminates` before latching. Calling it terminal
// on a target that keeps running would DELETE LIVE CODE — the statements after it are reachable.
//
// framec owns the WALK (the cursor, the terminated latch, the halt); the 10-way Stmt DISPATCH is
// a per-item function surfaced here as a `kind`-keyed branch, and each arm's leaf holds the EXACT
// byte-for-byte spelling sequence of its `emit_body` match arm (Transition's exit->build->enter->
// return lifecycle via `has_lifecycle` guards, StackPush/StackPop/StackPopBare/Forward, the
// Lowering-backed Native/Assign/ReturnCall). `kind_at` returns -1 at end-of-slice (the loop
// bound), 0=Trivia, 1=Native, 2=Transition, 3=StackPush, 4=StackPopBare, 5=StackPop, 6=Assign,
// 7=ReturnCall, 8=SelfCall, 9=Forward — the hand match order. The wrapper drives `step()` a
// bounded number of times and reads `out` + `terminated`.
//
// Regen: framec-ng -l rust --emit stmt_walk.frs | grep -v '^#!\[allow' > stmt_walk.gen.rs

#[derive(Clone)]
enum StmtWalkVars {
    Walk {  },
    Done {  },
}
#[derive(Clone)]
enum StmtWalkArgs {
    Walk {  },
    Done {  },
}
#[derive(Clone)]
struct StmtWalkComp {
    state: String,
    vars: StmtWalkVars,
    args: StmtWalkArgs,
}

pub struct StmtWalk<'a> {
    compartment: StmtWalkComp,
    stack: Vec<StmtWalkComp>,
    pub src: &'a Source,
    pub syms: &'a SymbolTable,
    pub sym: &'a SystemSym,
    pub role: BodyRole,
    pub stmts: &'a [Stmt],
    pub state: &'a str,
    pub event: &'a str,
    pub is_async: bool,
    pub base: u32,
    pub be: &'a dyn Backend,
    pub out: Sink,
    pub terminated: bool,
    pub i: usize,
}

impl<'a> StmtWalk<'a> {
    pub fn new(src: &'a Source, syms: &'a SymbolTable, sym: &'a SystemSym, role: BodyRole, stmts: &'a [Stmt], state: &'a str, event: &'a str, is_async: bool, base: u32, be: &'a dyn Backend, out: Sink) -> StmtWalk<'a> {
        let compartment = StmtWalkComp { state: "Walk".to_string(), vars: StmtWalkVars::Walk {  }, args: StmtWalkArgs::Walk {  } };
        StmtWalk { compartment, stack: Vec::new(), src: src, syms: syms, sym: sym, role: role, stmts: stmts, state: state, event: event, is_async: is_async, base: base, be: be, out: out, terminated: false, i: 0 }
    }

    pub fn step(&mut self) {
        match self.compartment.state.as_str() {
            "Walk" => self.Walk_step(),
            _ => {}
        }
    }

    fn Walk_step(&mut self) {
        let k = kind_at(self.stmts, self.i);
        if k < 0 {
            let mut __next = StmtWalkComp { state: "Done".to_string(), vars: StmtWalkVars::Done {  }, args: StmtWalkArgs::Done { } };
            self.compartment = __next;
            return Default::default();
        }
        if k == 1 {
            emit_native(self.src, self.syms, self.sym, self.state, self.be, self.base, self.stmts, self.i, &mut self.out);
            self.i = self.i + 1;
            let mut __next = StmtWalkComp { state: "Walk".to_string(), vars: StmtWalkVars::Walk {  }, args: StmtWalkArgs::Walk { } };
            self.compartment = __next;
            return Default::default();
        }
        if k == 2 {
            let term = emit_transition(self.src, self.syms, self.sym, self.state, self.be, self.base, self.stmts, self.i, &mut self.out);
            if term {
                self.terminated = true;
                let mut __next = StmtWalkComp { state: "Done".to_string(), vars: StmtWalkVars::Done {  }, args: StmtWalkArgs::Done { } };
                self.compartment = __next;
                return Default::default();
            }
            self.i = self.i + 1;
            let mut __next = StmtWalkComp { state: "Walk".to_string(), vars: StmtWalkVars::Walk {  }, args: StmtWalkArgs::Walk { } };
            self.compartment = __next;
            return Default::default();
        }
        if k == 3 {
            let term = emit_stack_push(self.src, self.syms, self.sym, self.state, self.be, self.base, self.stmts, self.i, &mut self.out);
            if term {
                self.terminated = true;
                let mut __next = StmtWalkComp { state: "Done".to_string(), vars: StmtWalkVars::Done {  }, args: StmtWalkArgs::Done { } };
                self.compartment = __next;
                return Default::default();
            }
            self.i = self.i + 1;
            let mut __next = StmtWalkComp { state: "Walk".to_string(), vars: StmtWalkVars::Walk {  }, args: StmtWalkArgs::Walk { } };
            self.compartment = __next;
            return Default::default();
        }
        if k == 4 {
            emit_stack_pop_bare(self.be, self.base, self.stmts, self.i, &mut self.out);
            self.i = self.i + 1;
            let mut __next = StmtWalkComp { state: "Walk".to_string(), vars: StmtWalkVars::Walk {  }, args: StmtWalkArgs::Walk { } };
            self.compartment = __next;
            return Default::default();
        }
        if k == 5 {
            let term = emit_stack_pop(self.src, self.syms, self.sym, self.state, self.be, self.base, self.stmts, self.i, &mut self.out);
            if term {
                self.terminated = true;
                let mut __next = StmtWalkComp { state: "Done".to_string(), vars: StmtWalkVars::Done {  }, args: StmtWalkArgs::Done { } };
                self.compartment = __next;
                return Default::default();
            }
            self.i = self.i + 1;
            let mut __next = StmtWalkComp { state: "Walk".to_string(), vars: StmtWalkVars::Walk {  }, args: StmtWalkArgs::Walk { } };
            self.compartment = __next;
            return Default::default();
        }
        if k == 6 {
            emit_assign(self.src, self.syms, self.sym, self.state, self.be, self.base, self.stmts, self.i, &mut self.out);
            self.i = self.i + 1;
            let mut __next = StmtWalkComp { state: "Walk".to_string(), vars: StmtWalkVars::Walk {  }, args: StmtWalkArgs::Walk { } };
            self.compartment = __next;
            return Default::default();
        }
        if k == 7 {
            let term = emit_return_call(self.src, self.syms, self.sym, self.role, self.state, self.be, self.base, self.is_async, self.stmts, self.i, &mut self.out);
            if term {
                self.terminated = true;
                let mut __next = StmtWalkComp { state: "Done".to_string(), vars: StmtWalkVars::Done {  }, args: StmtWalkArgs::Done { } };
                self.compartment = __next;
                return Default::default();
            }
            self.i = self.i + 1;
            let mut __next = StmtWalkComp { state: "Walk".to_string(), vars: StmtWalkVars::Walk {  }, args: StmtWalkArgs::Walk { } };
            self.compartment = __next;
            return Default::default();
        }
        if k == 8 {
            emit_self_call(self.src, self.syms, self.sym, self.state, self.be, self.base, self.is_async, self.stmts, self.i, &mut self.out);
            self.i = self.i + 1;
            let mut __next = StmtWalkComp { state: "Walk".to_string(), vars: StmtWalkVars::Walk {  }, args: StmtWalkArgs::Walk { } };
            self.compartment = __next;
            return Default::default();
        }
        if k == 9 {
            emit_forward(self.sym, self.state, self.event, self.be, self.base, self.stmts, self.i, &mut self.out);
            self.i = self.i + 1;
            let mut __next = StmtWalkComp { state: "Walk".to_string(), vars: StmtWalkVars::Walk {  }, args: StmtWalkArgs::Walk { } };
            self.compartment = __next;
            return Default::default();
        }
        self.i = self.i + 1;
        let mut __next = StmtWalkComp { state: "Walk".to_string(), vars: StmtWalkVars::Walk {  }, args: StmtWalkArgs::Walk { } };
        self.compartment = __next;
        return Default::default();
    }

}
