//! The RUST pop-enter emitter ([`pop_enter.frs`]) — the FIRST end-to-end proof that a
//! Cauldron-*mechanized* `@@system` compiles and behaves (inc4): `rust.rs::pop_enter` run through
//! the mechanizer, wired here, and gated byte-for-byte against its frozen oracle.
//!
//! `pop_enter` is a per-backend `Backend` method (java/python/c stay native); this rust-only system
//! is driven from rust's one-line `pop_enter` driver. The byte-for-byte ORACLE it replaced is the
//! preserved [`super::rust::pop_enter_hand`], gated in `tests/emit_scaffold_walks.rs` (GATE-A, via
//! [`super::driver::pop_enter_parity_report`]).
//!
//! Two hand-adjustments to the mechanizer output (each a known mechanizer gap, not a Frame change):
//! the pure `self.pad(rel)` Backend method is inlined (the mechanizer does not yet lift
//! `self.method(..)` to a leaf), and `super::driver::has_lifecycle` is de-qualified to `has_lifecycle`
//! (the mechanizer copies the source-relative path verbatim; the system lives one module deeper).
//!
//! `.gen.rs` regen: `framec-ng -l rust --emit pop_enter.frs | grep -v '^#!\[allow' > pop_enter.gen.rs`.

use super::Sink;
use crate::resolve::SystemSym;

mod fsm {
    #![allow(
        dead_code,
        unused_parens,
        non_snake_case,
        unused_variables,
        unused_mut,
        unused_imports
    )]
    use super::super::driver::has_lifecycle;
    use super::super::rust::rust_ident;
    use super::super::Sink;
    use crate::resolve::SystemSym;
    include!("pop_enter.gen.rs");
}

/// Spell the pop-enter lifecycle re-arm block, driving the `PopEnter` sequencer. Seeds the machine's
/// owned `out` with the caller's Sink (`std::mem::take`), drives to a bounded fixpoint (a broken
/// machine cannot hang), and writes the grown Sink back. Called from rust's `pop_enter` driver.
pub(super) fn drive(rel: u32, sym: &SystemSym, enter_args: Option<&str>, out: &mut Sink) {
    let seed = std::mem::take(out);
    let mut m = fsm::PopEnter::new(rel, sym, enter_args, seed);
    // Per states-loop iteration: $For4 -> $Fork3 -> [$Step2 ->] $Next1 -> $For4 (~4 steps), plus
    // the $Step5 entry and the $Done tail.
    let bound = 5 * sym.states.len() + 8;
    for _ in 0..bound {
        m.step();
    }
    *out = m.out;
}

#[cfg(test)]
mod tests {
    use super::super::driver::{has_lifecycle, Backend};
    use super::super::rust::{rust_ident, Rust};
    use super::super::Sink;
    use crate::resolve::{resolve, SystemSym};
    use crate::scan::literals::Target;
    use crate::scan::segment;
    use crate::Source;

    /// The preserved byte-for-byte **oracle** — the original `rust.rs::pop_enter` body verbatim
    /// (`self.pad` -> `be.pad`), NOT routed through the machine, so a leaf bug is visible to the gate.
    fn pop_enter_hand(be: &Rust, rel: u32, sym: &SystemSym, enter_args: Option<&str>, out: &mut Sink) {
        let p = be.pad(rel);
        let a = enter_args.unwrap_or("");
        for st in &sym.states {
            if has_lifecycle(sym, &st.name, "$>") {
                out.frame(&format!(
                    "{p}if self.compartment.state == \"{}\" {{ self.{}_{}({a}); }}\n",
                    st.name,
                    st.name,
                    rust_ident("$>")
                ));
            }
        }
    }

    /// GATE-A differential: the mechanized `PopEnter` @@system (via `Rust::pop_enter` -> `drive`)
    /// must produce byte-identical output to the frozen hand oracle, on a system with a plain state
    /// ($A, skipped), and two lifecycle states ($B, $C, emitted) — exercising both the has-lifecycle
    /// branch and the multi-state loop. The snapshot suite does NOT reach pop_enter (no `-> (enter)
    /// pop$` fixture), so THIS is its behavioral proof.
    #[test]
    fn mechanized_pop_enter_matches_the_frozen_hand_oracle() {
        let frm = r#"@@system L {
    interface:
        go()
    machine:
        $A {
            go() { -> $B }
        }
        $B {
            $>(m: String) { }
            go() { -> $C }
        }
        $C {
            $>(n: String) { }
            go() { -> $A }
        }
}
"#;
        let src = Source::new("t.frm", frm.as_bytes().to_vec()).unwrap();
        let ast = segment(&src, Target::Python3).expect("segment");
        let (syms, _d) = resolve(&ast);
        let sym = syms.systems.iter().find(|s| s.name == "L").expect("system L resolved");
        let be = Rust;
        let (rel, ea) = (8u32, Some("m = 1"));

        let mut m_sink = Sink::new();
        be.pop_enter(rel, sym, ea, &mut m_sink);
        let machine = m_sink.finish();

        let mut h_sink = Sink::new();
        pop_enter_hand(&be, rel, sym, ea, &mut h_sink);
        let hand = h_sink.finish();

        assert!(
            machine.contains("self.compartment.state"),
            "non-vacuous: pop_enter must emit for lifecycle states $B/$C; got {machine:?}"
        );
        assert_eq!(machine, hand, "mechanized PopEnter @@system != frozen hand oracle");
    }
}
