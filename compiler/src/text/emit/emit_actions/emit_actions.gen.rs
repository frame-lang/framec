use std::collections::HashMap;
use std::any::Any;


// The driver's ACTIONS/OPERATIONS walk, dogfooded as a plain `@@system` — the emit-side sequencer
// that reifies `emit`'s `actions:` / `operations:` pass (one method per user-bodied member; the
// signature is Frame's, the body is the user's). It rides the same READ-ONLY BORROWED DOMAIN as
// StmtWalk/BaseColumn/EmitHandlers: the section slice, the source, the symbol table, the system
// symbol, and the `&dyn Backend` are SHARED BORROWS threaded through one lifetime `'a`; the OWNED
// domain is the accumulating output `out`, the two walk cursors (`si`/`mi`) and their bounds
// (`nsec`/`nm`).
//
// THE 2-LEVEL NESTING, EXPRESSED WITHOUT push$/pop$. The pass is a FIXED depth-2 walk — sections,
// then, for each `actions:`/`operations:` section, its member decls — so a stack is unnecessary (a
// stack buys UNBOUNDED depth; this depth is 2 and known). It is expressed instead as two NESTED
// CYCLE STATES with explicit up/down edges, one owned cursor per level:
//   $Section  cycles over `sections` (fork: only the sections ADMITTED IN THIS PASS descend); on
//             such a section it sets the member bound `nm`, resets `mi`, and descends
//             `-> $Member`; at `si >= nsec` it advances the PASS (below) or halts `-> $Done`.
//   $Member   cycles over the current section's `members` (fork: only `Decl::WithBody` emits); on a
//             bodied member it opens one action, walks its body via the StmtWalk leaf, and closes
//             it; at `mi >= nm` it ASCENDS (`si += 1`, `-> $Section`).
// THE PASS CURSOR. Some targets emit `actions:` members BEFORE `operations:` members whatever
// order the two sections were declared in (the shipped compiler holds them in two collections and
// runs two passes; Frame's canonical block order actually puts `operations:` FIRST, so source order
// is not it). That is expressed as a third cursor, `phase`, over a FIXED, KNOWN pass count `nphase`
// (1 = one pass admitting both kinds in source order; 2 = actions pass, then operations pass): when
// `si` runs off the end and another pass remains, `phase` advances and `si` resets.
//
// `phase` is a CURSOR, not a mode worth naming as a state (Shadows §3 degenerate pole): like `si`
// and `mi` it is a program counter over ALREADY-PARSED tree data, it gates no recognition, and
// deleting it would change output only by reordering. It becomes a state to reify the day a pass
// carries something forward INTO the next one — a name table, a dedup set, an emitted-count read
// back to gate the second pass. Nothing does today.
//
// The "mode" is the walk DEPTH (which of the two cycle states is live); the cursors advance it.
// This is the §3 degenerate pole — a program-counter walk over ALREADY-PARSED tree data, whose
// forks are structural type-dispatch (`Section::Actions`? `Decl::WithBody`? — Frame cannot match a
// Rust enum), not input recognition. It carries no recognition register; nothing is glossed. Its
// reify payoff is not a hidden mode but DOGFOOD UNIFORMITY (the maximal-rebuild campaign: the
// cleanroom emits its own driver as an @@system, differential-gated byte-for-byte vs the preserved
// `emit_actions_hand`).
//
// framec owns the WALK (the two cursors, the bounds, the descents/ascents, the halt). The
// un-Frame-able work is per-item NATIVE LEAVES: the structural forks/bounds (`is_action_section`,
// `action_member_count`, `is_withbody_member`), and `emit_action`, which spells ONE method:
// `be.open_action(...)`, then the StmtWalk body walk (`emit_body`, unchanged, called as a leaf —
// NOT reinlined, its `BodyEnd` discarded exactly as the hand pass discarded it), then
// `be.close_action(...)`. Every materialization spelling stays native and byte-identical; the
// machine only sequences the walk.
//
// Regen: framec-ng -l rust --emit emit_actions.frs | grep -v '^#!\[allow' > emit_actions.gen.rs

#[derive(Clone)]
enum EmitActionsVars {
    Section {  },
    Member {  },
    Done {  },
}
#[derive(Clone)]
enum EmitActionsArgs {
    Section {  },
    Member {  },
    Done {  },
}
#[derive(Clone)]
struct EmitActionsComp {
    state: String,
    vars: EmitActionsVars,
    args: EmitActionsArgs,
}

pub struct EmitActions<'a> {
    compartment: EmitActionsComp,
    stack: Vec<EmitActionsComp>,
    pub src: &'a Source,
    pub syms: &'a SymbolTable,
    pub sym: &'a SystemSym,
    pub sections: &'a [Section],
    pub be: &'a dyn Backend,
    pub nsec: usize,
    pub nphase: usize,
    pub out: Sink,
    pub nm: usize,
    pub si: usize,
    pub phase: usize,
    pub mi: usize,
}

impl<'a> EmitActions<'a> {
    pub fn new(src: &'a Source, syms: &'a SymbolTable, sym: &'a SystemSym, sections: &'a [Section], be: &'a dyn Backend, nsec: usize, nphase: usize, out: Sink) -> EmitActions<'a> {
        let compartment = EmitActionsComp { state: "Section".to_string(), vars: EmitActionsVars::Section {  }, args: EmitActionsArgs::Section {  } };
        EmitActions { compartment, stack: Vec::new(), src: src, syms: syms, sym: sym, sections: sections, be: be, nsec: nsec, nphase: nphase, out: out, nm: 0, si: 0, phase: 0, mi: 0 }
    }

    pub fn step(&mut self) {
        match self.compartment.state.as_str() {
            "Section" => self.Section_step(),
            "Member" => self.Member_step(),
            _ => {}
        }
    }

    fn Section_step(&mut self) {
        if self.si >= self.nsec {
            if self.phase + 1 >= self.nphase {
                let mut __next = EmitActionsComp { state: "Done".to_string(), vars: EmitActionsVars::Done {  }, args: EmitActionsArgs::Done { } };
                self.compartment = __next;
                return Default::default();
            }
            self.phase = self.phase + 1;
            self.si = 0;
            let mut __next = EmitActionsComp { state: "Section".to_string(), vars: EmitActionsVars::Section {  }, args: EmitActionsArgs::Section { } };
            self.compartment = __next;
            return Default::default();
        }
        let isa = is_action_section(self.sections, self.si, self.phase, self.nphase);
        if isa == false {
            self.si = self.si + 1;
            let mut __next = EmitActionsComp { state: "Section".to_string(), vars: EmitActionsVars::Section {  }, args: EmitActionsArgs::Section { } };
            self.compartment = __next;
            return Default::default();
        }
        self.nm = action_member_count(self.sections, self.si, self.phase, self.nphase);
        self.mi = 0;
        let mut __next = EmitActionsComp { state: "Member".to_string(), vars: EmitActionsVars::Member {  }, args: EmitActionsArgs::Member { } };
        self.compartment = __next;
        return Default::default();
    }

    fn Member_step(&mut self) {
        if self.mi >= self.nm {
            self.si = self.si + 1;
            let mut __next = EmitActionsComp { state: "Section".to_string(), vars: EmitActionsVars::Section {  }, args: EmitActionsArgs::Section { } };
            self.compartment = __next;
            return Default::default();
        }
        let iswb = is_withbody_member(self.sections, self.si, self.mi, self.phase, self.nphase);
        if iswb == false {
            self.mi = self.mi + 1;
            let mut __next = EmitActionsComp { state: "Member".to_string(), vars: EmitActionsVars::Member {  }, args: EmitActionsArgs::Member { } };
            self.compartment = __next;
            return Default::default();
        }
        emit_action(self.src, self.syms, self.sym, self.be, self.sections, self.si, self.mi, self.phase, self.nphase, &mut self.out);
        self.mi = self.mi + 1;
        let mut __next = EmitActionsComp { state: "Member".to_string(), vars: EmitActionsVars::Member {  }, args: EmitActionsArgs::Member { } };
        self.compartment = __next;
        return Default::default();
    }

}

