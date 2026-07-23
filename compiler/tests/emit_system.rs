//! **GATE-A — the driver's per-system phase spine, as a `@@system`, is byte-for-byte the hand
//! phase run.** SCAFFOLDING (differential vs the preserved
//! [`frame_compiler::text::emit::driver::emit_system_hand`] oracle; conversion-internal, never
//! promoted).
//!
//! `emit`'s per-system run of passes — `open_system` → interface router → private handlers →
//! `actions:`/`operations:` → `@@[persist]` save/restore → `close_system` — was reified as the
//! `EmitSystem` plain-`@@system` (`emit_system.frs`): a LINEAR 4-STATE SPINE (`$Interface` →
//! `$Handlers` → `$Actions` → `$Persist`, no cursor, no cycle), each phase an unconditional advance
//! that calls one already-landed sub-system's `walk` as a leaf; `$Persist` is the one guarded state
//! (`manifest.enabled`). The `open_system`/`close_system` bookends stay native in the wrapper. Only
//! the four-phase SEQUENCING moved into the machine.
//!
//! This test proves — by running — that for **every** system, the machine path
//! ([`super::emit_system::walk`]) and the preserved hand oracle ([`emit_system_hand`]) emit the
//! **identical String** for the WHOLE system, for **all four** cleanroom targets (python/java/rust/c),
//! over (a) a curated corpus that exercises multiple systems in one file, top-level native water
//! between them, persist-ENABLED vs disabled (both `$Persist` arms), and all section kinds (interface,
//! machine with HSM states/handlers, `actions:`, `operations:`, `domain:`), and (b) a deterministic
//! fuzz. Byte-parity IS the gate: a single differing space fails. The library owns the emission and
//! the `.finish()` ([`frame_compiler::text::emit::driver::system_parity_report`]).

use frame_compiler::resolve::resolve;
use frame_compiler::scan::{literals::Target, segment};
use frame_compiler::text::emit::c::C;
use frame_compiler::text::emit::driver::{self, Backend};
use frame_compiler::text::emit::{java::Java, python::Python, rust::Rust};
use frame_compiler::Source;

const TARGETS: [Target; 4] = [Target::Python3, Target::Java, Target::Rust, Target::C];

/// The result of one differential run: how many systems were compared, and whether the corpus
/// actually exercised BOTH persist arms (enabled and disabled) — so a caller can prove non-vacuity.
#[derive(Default)]
struct Seen {
    systems: usize,
    persist_on: usize,
    persist_off: usize,
}

/// Run the per-system differential over one Frame source, for all four targets. Asserts, for every
/// system, `machine_text == hand_text` (byte-for-byte).
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
        let report = driver::system_parity_report(&src, &ast, &syms, be);
        assert!(
            !report.is_empty(),
            "{label}: no systems parsed for {target:?}"
        );
        for p in &report {
            assert_eq!(
                p.machine_text, p.hand_text,
                "{label} [{}] {target:?}: EmitSystem text != emit_system_hand text\n\
                 === machine (EmitSystem) ===\n{}\n=== hand (oracle) ===\n{}",
                p.label, p.machine_text, p.hand_text
            );
            seen.systems += 1;
            if p.persist_enabled {
                seen.persist_on += 1;
            } else {
                seen.persist_off += 1;
            }
        }
    }
}

/// TWO systems in one file with top-level native WATER between them (skipped by the per-system walk,
/// present to prove segmentation survives interleaving). `Alpha` transitions between states; `Beta`
/// has a single self-looping state.
const MULTI: &str = r#"GLOBAL_A = 1

@@system Alpha {
    interface:
        go()
    machine:
        $A { go() { -> $B } }
        $B { }
}

GLOBAL_B = 2

@@system Beta {
    interface:
        tick()
    machine:
        $S { tick() { do_it(); } }
}
"#;

/// **All section kinds in one system**, persist DISABLED: `interface`, `machine` with HSM
/// (`$Work => $Base`) states and handlers, `actions:` (a bodied method), `operations:` (another), and
/// `domain:` fields. Exercises the whole spine — `$Interface`, `$Handlers`, `$Actions` — with the
/// `$Persist` guard taking its SKIP arm.
const RICH: &str = r#"@@system Rich {
    interface:
        go()
        decide(): String
    machine:
        $Base {
            decide(): String { @@:(String::from("base")) }
        }
        $Work => $Base {
            go() { -> $Base }
        }
    actions:
        helper(a: int): int {
            return a + 1;
        }
    operations:
        util(): int {
            return 7;
        }
    domain:
        n: int = 0
}
"#;

/// **Persist ENABLED** — the full three-attribute contract (`@@[persist]` + `@@[save]` + `@@[load]`;
/// bare `@@[persist]` is E814), so `manifest.enabled` is true and `$Persist` takes its positive arm
/// (`be.persist(&manifest, out)`). A `domain` carrying a persisted field, a start/terminal machine.
const PERSISTED: &str = r#"@@[persist(str)]
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
"#;

/// An **async** system (every router async), persist disabled — a different `is_async` path through
/// the interface phase, still one whole-system spine.
const ASYNCS: &str = r#"@@[async]
@@system Async {
    interface:
        async fetch(): int
        tick()
    machine:
        $A {
            fetch() { @@:(go()) }
            tick() { work(); }
        }
}
"#;

#[test]
fn curated_corpus_is_byte_identical_across_shapes() {
    let mut seen = Seen::default();
    for (label, frm) in [
        ("Multi", MULTI),
        ("Rich", RICH),
        ("Persisted", PERSISTED),
        ("Async", ASYNCS),
    ] {
        check(label, frm, &mut seen);
    }
    // The corpus must have compared many whole systems across targets, AND exercised BOTH `$Persist`
    // arms — a corpus that never enabled persist would leave the positive arm unproven (a vacuous
    // pass on the one guarded state).
    assert!(
        seen.systems >= 20,
        "corpus must compare many systems across targets; saw {}",
        seen.systems
    );
    assert!(
        seen.persist_on > 0 && seen.persist_off > 0,
        "corpus must exercise BOTH persist arms; on={} off={}",
        seen.persist_on,
        seen.persist_off
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

/// Build a random FILE: a random number of systems, each with a random machine (states + handlers),
/// an optional `actions:` section, a random `@@[persist(str)]` toggle, and a random `@@[async]`
/// toggle — with occasional top-level water between systems. Exercises the whole spine across shapes.
fn fuzz_file(rng: &mut Rng, n: usize) -> String {
    let n_systems = 1 + rng.upto(3);
    let mut file = String::new();
    for s in 0..n_systems {
        if rng.upto(2) == 0 {
            file.push_str(&format!("GLOBAL_{s} = {s}\n\n"));
        }
        let is_async = rng.upto(2) == 0;
        let persisted = rng.upto(2) == 0;
        if persisted {
            // The FULL persist contract — bare `@@[persist]` is E814 (rejected, `enabled` stays
            // false); only the three-attribute form actually flips the `$Persist` guard.
            file.push_str("@@[persist(str)]\n@@[save(snapshot)]\n@@[load(restore)]\n");
        }
        if is_async {
            file.push_str("@@[async]\n");
        }
        file.push_str(&format!("@@system Sys{n}_{s} {{\n    interface:\n"));
        let n_events = 1 + rng.upto(3);
        for e in 0..n_events {
            if is_async {
                file.push_str(&format!("        async e{e}(): int\n"));
            } else {
                file.push_str(&format!("        e{e}()\n"));
            }
        }
        file.push_str("    machine:\n");
        let n_states = 1 + rng.upto(3);
        for si in 0..n_states {
            file.push_str(&format!("        $S{si} {{\n"));
            for e in 0..n_events {
                if rng.upto(2) == 0 {
                    if is_async {
                        file.push_str(&format!("            e{e}() {{ @@:(mk()) }}\n"));
                    } else {
                        file.push_str(&format!("            e{e}() {{ -> $S0 }}\n"));
                    }
                }
            }
            file.push_str("        }\n");
        }
        if rng.upto(2) == 0 {
            file.push_str("    actions:\n        act(): int {\n            return 0;\n        }\n");
        }
        file.push_str("    domain:\n        d: int = 0\n}\n\n");
    }
    file
}

#[test]
fn deterministic_fuzz_of_random_files_is_byte_identical() {
    let mut rng = Rng(0x5EED_1234_ABCD_0F01);
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
            for p in driver::system_parity_report(&src, &ast, &syms, be) {
                assert_eq!(
                    p.machine_text, p.hand_text,
                    "FUZZ n={n} {target:?} [{}]: EmitSystem text differs\nsource:\n{frm}\n\
                     === machine ===\n{}\n=== hand ===\n{}",
                    p.label, p.machine_text, p.hand_text
                );
            }
            ran += 1;
        }
    }
    assert!(ran >= 300, "fuzz must actually run across targets; ran {ran}");
}
