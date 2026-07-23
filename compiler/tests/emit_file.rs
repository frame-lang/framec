//! **GATE-A — the driver's top-level item walk, as a `@@system`, is byte-for-byte the hand item
//! loop.** SCAFFOLDING (differential vs the preserved
//! [`frame_compiler::text::emit::driver::emit_file_hand`] oracle; conversion-internal, never
//! promoted).
//!
//! `emit`'s file-item loop — the `file_header` preamble, then per item either the "water" (top-level
//! native code, verbatim) or a system delegated to the `EmitSystem` phase spine — was reified as the
//! OUTERMOST `EmitFile` plain-`@@system` (`emit_file.frs`): a single `$Item` CYCLE STATE over one
//! walk cursor, forking structurally on each item (`Item::Native` → water leaf; otherwise → the
//! landed [`super::emit_system::walk`]). The per-item SPELLING (the water render, the system spine)
//! is shared with the oracle; only the item-loop SEQUENCING moved into the machine. This closes the
//! traversal composition — the entire emit driver, file → system → handler → statement, is now
//! `@@system`s.
//!
//! This test proves — by running — that for the WHOLE FILE, the machine path
//! ([`super::emit_file::walk`], which IS the production [`emit`]) and the preserved hand oracle
//! ([`emit_file_hand`]) emit the **identical String**, for **all four** cleanroom targets
//! (python/java/rust/c), over (a) a curated corpus that exercises files with LEADING, TRAILING, and
//! BETWEEN-systems native water, multiple systems, persist on/off, and a water-only file (zero
//! systems), and (b) a deterministic fuzz. Byte-parity IS the gate: a single differing byte fails.
//! The library owns the emission and the `.finish()`
//! ([`frame_compiler::text::emit::driver::file_parity_report`]).

use frame_compiler::resolve::resolve;
use frame_compiler::scan::{literals::Target, segment};
use frame_compiler::text::emit::c::C;
use frame_compiler::text::emit::driver::{self, Backend};
use frame_compiler::text::emit::{java::Java, python::Python, rust::Rust};
use frame_compiler::Source;

const TARGETS: [Target; 4] = [Target::Python3, Target::Java, Target::Rust, Target::C];

/// What a curated run observed — total items compared and how many were water — so the caller can
/// prove the `$Item` fork's water arm was actually taken.
#[derive(Default)]
struct Seen {
    items: usize,
    native: usize,
    files: usize,
}

/// Run the whole-file differential over one Frame source, for all four targets. Asserts, for the
/// whole file, `machine_text == hand_text` (byte-for-byte).
fn check(label: &str, frm: &str, seen: &mut Seen) {
    for target in TARGETS {
        let src = Source::new("t.frm", frm.as_bytes().to_vec()).unwrap();
        let ast = match segment(&src, target) {
            Ok(a) => a,
            Err(e) => panic!("{label}: segment failed for {target:?}: {e:?}"),
        };
        let (syms, _diags) = resolve(&ast);
        let jb = Java::new();
        let cb = C::new();
        let be: &dyn Backend = match target {
            Target::Python3 => &Python,
            Target::Java => &jb,
            Target::Rust => &Rust,
            Target::C => &cb,
            _ => unreachable!("targets are gated to the supported four"),
        };
        let p = driver::file_parity_report(&src, &ast, &syms, be);
        assert_eq!(
            p.machine_text, p.hand_text,
            "{label} {target:?}: EmitFile text != emit_file_hand text\n\
             === machine (EmitFile) ===\n{}\n=== hand (oracle) ===\n{}",
            p.machine_text, p.hand_text
        );
        seen.items += p.item_count;
        seen.native += p.native_count;
        seen.files += 1;
    }
}

/// Water LEADING, BETWEEN, and TRAILING two systems — every position of the `$Item` fork's Native
/// arm, interleaved with the delegate arm.
const SANDWICH: &str = r#"LEADING = 0

@@system One {
    interface:
        go()
    machine:
        $A { go() { -> $B } }
        $B { }
}

MIDDLE = 1

@@system Two {
    interface:
        tick()
    machine:
        $S { tick() { work(); } }
}

TRAILING = 2
"#;

/// A file with a persist-ENABLED system (the full three-attribute contract; bare `@@[persist]` is
/// E814) next to a plain one, no water — proves the delegate arm runs the full `EmitSystem` spine
/// (persist included) through the file walk.
const TWO_SYSTEMS: &str = r#"@@[persist(str)]
@@[save(snapshot)]
@@[load(restore)]
@@system Saver {
    interface:
        step()
    machine:
        $A { step() { -> $B } }
        $B { }
    domain:
        count: int = 0
}

@@system Plain {
    interface:
        go()
    machine:
        $X { go() { -> $X } }
}
"#;

/// A file with NO systems — only water. The `$Item` cycle takes the Native arm every time and never
/// delegates; the machine must still reproduce the header + verbatim water exactly.
const WATER_ONLY: &str = r#"ONLY_A = 1
ONLY_B = 2
"#;

/// A single system, no water — the minimal delegate-only file (one `$Item` step to the spine, then
/// halt).
const BARE: &str = r#"@@system Bare {
    interface:
        e()
    machine:
        $S { e() { -> $S } }
}
"#;

#[test]
fn curated_corpus_is_byte_identical_across_shapes() {
    let mut seen = Seen::default();
    for (label, frm) in [
        ("Sandwich", SANDWICH),
        ("TwoSystems", TWO_SYSTEMS),
        ("WaterOnly", WATER_ONLY),
        ("Bare", BARE),
    ] {
        check(label, frm, &mut seen);
    }
    // The corpus must have compared many whole files across targets, AND actually taken the water
    // arm of the `$Item` fork (a corpus of bare systems would leave it unproven).
    assert!(
        seen.files >= 12,
        "corpus must compare many whole files across targets; saw {}",
        seen.files
    );
    assert!(
        seen.native > 0,
        "corpus must exercise the top-level native (water) fork; saw {} native items",
        seen.native
    );
}

// ----------------------------------------------------------------- deterministic fuzz

/// xorshift64* — deterministic, seed-stable across runs and machines.
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545F4914F6CDD1D)
    }
    fn upto(&mut self, n: usize) -> usize {
        (self.next() as usize) % n
    }
}

/// Build a random FILE: a random interleaving of top-level water lines and systems (each a small
/// machine, randomly persisted), so the `$Item` fork alternates its Native and delegate arms in
/// random order and count.
fn fuzz_file(rng: &mut Rng, n: usize) -> String {
    let n_items = 1 + rng.upto(5);
    let mut file = String::new();
    let mut sys = 0usize;
    for _ in 0..n_items {
        if rng.upto(2) == 0 {
            // water
            file.push_str(&format!("W{n}_{sys} = {}\n\n", rng.upto(100)));
        } else {
            if rng.upto(2) == 0 {
                file.push_str("@@[persist(str)]\n@@[save(snapshot)]\n@@[load(restore)]\n");
            }
            file.push_str(&format!("@@system S{n}_{sys} {{\n    interface:\n        e()\n    machine:\n"));
            let n_states = 1 + rng.upto(3);
            for si in 0..n_states {
                if rng.upto(2) == 0 {
                    file.push_str(&format!("        $S{si} {{ e() {{ -> $S0 }} }}\n"));
                } else {
                    file.push_str(&format!("        $S{si} {{ }}\n"));
                }
            }
            file.push_str("    domain:\n        d: int = 0\n}\n\n");
            sys += 1;
        }
    }
    // Guarantee at least one system so `syms` is non-trivial for at least some files.
    if sys == 0 {
        file.push_str("@@system Fallback {\n    interface:\n        e()\n    machine:\n        $S { e() { -> $S } }\n}\n");
    }
    file
}

#[test]
fn deterministic_fuzz_of_random_files_is_byte_identical() {
    let mut rng = Rng(0x1234_5EED_0F01_ABCD);
    let mut ran = 0usize;
    for n in 0..300usize {
        let frm = fuzz_file(&mut rng, n);
        for target in TARGETS {
            let src = Source::new("t.frm", frm.as_bytes().to_vec()).unwrap();
            let ast = match segment(&src, target) {
                Ok(a) => a,
                Err(_) => continue,
            };
            let (syms, _d) = resolve(&ast);
            let jb = Java::new();
            let cb = C::new();
            let be: &dyn Backend = match target {
                Target::Python3 => &Python,
                Target::Java => &jb,
                Target::Rust => &Rust,
                Target::C => &cb,
                _ => unreachable!(),
            };
            let p = driver::file_parity_report(&src, &ast, &syms, be);
            assert_eq!(
                p.machine_text, p.hand_text,
                "FUZZ n={n} {target:?}: EmitFile text differs\nsource:\n{frm}\n\
                 === machine ===\n{}\n=== hand ===\n{}",
                p.machine_text, p.hand_text
            );
            ran += 1;
        }
    }
    assert!(ran >= 300, "fuzz must actually run across targets; ran {ran}");
}
