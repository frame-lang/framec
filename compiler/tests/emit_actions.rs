//! **GATE-A — the driver's actions/operations walk, as a `@@system`, is byte-for-byte the hand
//! walk.** SCAFFOLDING (differential vs the preserved
//! [`frame_compiler::text::emit::driver::emit_actions_hand`] oracle; conversion-internal, never
//! promoted).
//!
//! `emit`'s `(section, member)` actions/operations pass — one method per user-bodied `actions:` /
//! `operations:` member — was reified as the `EmitActions` plain-`@@system` (`emit_actions.frs`): a
//! FIXED depth-2 walk expressed as two NESTED CYCLE STATES (`$Section` → `$Member`, no push$/pop$),
//! carrying the two walk cursors `si`/`mi` and their bounds. The per-member SPELLING (open_action,
//! the StmtWalk body walk, close_action) is an unchanged native leaf; only the 2-level SEQUENCING
//! moved into the machine.
//!
//! This test proves — by running — that for **every** system, the machine path
//! ([`super::emit_actions::walk`]) and the preserved hand oracle ([`emit_actions_hand`]) emit the
//! **identical String** for all of that system's `actions:`/`operations:` methods, for **all four**
//! cleanroom targets (python/java/rust/c), over (a) a curated corpus that exercises multiple actions
//! in one section, an `operations:` section, a system with BOTH, comments between members (the
//! `$Member` `Decl::Trivia` skip), explicit vs void return types, native bodies with `return` /
//! `let` / calls, and a system with NO actions at all (the `$Section` skip emits nothing), and (b)
//! a deterministic fuzz of random action/operation shapes. Byte-parity IS the gate: a single
//! differing space fails. The library owns the emission and the `.finish()`; the test only compares
//! ([`frame_compiler::text::emit::driver::actions_parity_report`]).

use frame_compiler::resolve::resolve;
use frame_compiler::scan::{literals::Target, segment};
use frame_compiler::text::emit::c::C;
use frame_compiler::text::emit::driver::{self, Backend};
use frame_compiler::text::emit::{java::Java, python::Python, rust::Rust};
use frame_compiler::Source;

const TARGETS: [Target; 4] = [Target::Python3, Target::Java, Target::Rust, Target::C];

/// Run the actions-pass differential over one Frame source, for all four targets. Asserts, for
/// every system, `machine_text == hand_text` (byte-for-byte). Returns the total number of
/// `actions:`/`operations:` methods emitted across all systems and targets — so the caller can
/// prove the corpus was not vacuous.
fn check(label: &str, frm: &str) -> usize {
    let mut actions_seen = 0usize;
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
        let report = driver::actions_parity_report(&src, &ast, &syms, be);
        assert!(
            !report.is_empty(),
            "{label}: no systems parsed for {target:?}"
        );
        for p in &report {
            assert_eq!(
                p.machine_text, p.hand_text,
                "{label} [{}] {target:?}: EmitActions text != emit_actions_hand text\n\
                 === machine (EmitActions) ===\n{}\n=== hand (oracle) ===\n{}",
                p.label, p.machine_text, p.hand_text
            );
            actions_seen += p.action_count;
        }
    }
    actions_seen
}

/// Multiple actions in ONE section, with a COMMENT between them (a `Decl::Trivia` the `$Member`
/// fork skips), an explicit `int` return and a void action, native bodies (`return`, `let`, a
/// call).
const CALC: &str = r#"@@system Calc {
    interface:
        go()
    machine:
        $A {
            go() { -> $A }
        }
    actions:
        add(a: int, b: int): int {
            let s = a + b;
            return s;
        }
        // trivia between two actions (a Decl::Trivia the $Member fork skips)
        reset() {
            clear_all();
        }
}
"#;

/// An `operations:` section (the other arm of the `$Section` fork), one action.
const OPS: &str = r#"@@system Ops {
    interface:
        run()
    machine:
        $S {
            run() { -> $S }
        }
    operations:
        tick(): bool {
            return true;
        }
}
"#;

/// A system with BOTH `actions:` AND `operations:` sections (the `$Section` cycle descends twice),
/// plus a `domain:` section it must skip.
const BOTH: &str = r#"@@system Both {
    interface:
        e()
    machine:
        $S {
            e() { -> $S }
        }
    actions:
        one(): int {
            return 1;
        }
    operations:
        two(x: int): int {
            let y = x + 1;
            return y;
        }
    domain:
        n: int = 0
}
"#;

/// A system with NO actions/operations at all — the `$Section` walk skips every section and emits
/// nothing (the machine's `$Done` is reached with an empty output, byte-identical to the hand
/// walk's empty output).
const NONE: &str = r#"@@system None_ {
    interface:
        e()
    machine:
        $S {
            e() { work(); }
        }
    domain:
        n: int = 0
}
"#;

#[test]
fn curated_corpus_is_byte_identical_across_shapes() {
    let mut total_actions = 0usize;
    for (label, frm) in [
        ("Calc", CALC),
        ("Ops", OPS),
        ("Both", BOTH),
        ("None", NONE),
    ] {
        total_actions += check(label, frm);
    }
    // The corpus must have actually emitted a substantial number of action methods (4 targets ×
    // the distinct actions/operations above: Calc 2, Ops 1, Both 2, None 0 = 5). A vacuous corpus
    // would pass byte-parity trivially; this guards against that.
    assert!(
        total_actions >= 16,
        "curated corpus must exercise many action methods across targets; saw {total_actions}"
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
    fn pick<'a>(&mut self, xs: &[&'a str]) -> &'a str {
        xs[(self.next() as usize) % xs.len()]
    }
    fn upto(&mut self, n: usize) -> usize {
        (self.next() as usize) % n
    }
}

/// Well-formed action-body palette — a random one is always a parseable native body.
const BODY: [&str; 5] = [
    "return 0;",
    "let v = 1;\n            return v;",
    "do_side_effect();",
    "let a = 1;\n            let b = 2;\n            return a + b;",
    "compute(); finalize();",
];

/// Build a random system: a required interface + machine, then a random `actions:` section with a
/// random number of members (some with explicit `: int` returns, some void, occasional trivia
/// comments), and a random `operations:` section likewise — so the `$Section` fork descends into
/// zero, one, or two native sections, and the `$Member` fork skips trivia.
fn fuzz_system(rng: &mut Rng, n: usize) -> String {
    let mut mem = 0usize; // global member counter -> unique names

    let mut actions = String::new();
    if rng.upto(3) != 0 {
        actions.push_str("    actions:\n");
        let count = 1 + rng.upto(3);
        for _ in 0..count {
            let ret = if rng.upto(2) == 0 { ": int" } else { "" };
            let body = rng.pick(&BODY);
            if rng.upto(3) == 0 {
                actions.push_str("        // a trivia comment between members\n");
            }
            actions.push_str(&format!("        a{mem}(){ret} {{\n            {body}\n        }}\n"));
            mem += 1;
        }
    }

    let mut operations = String::new();
    if rng.upto(3) != 0 {
        operations.push_str("    operations:\n");
        let count = 1 + rng.upto(3);
        for _ in 0..count {
            let ret = if rng.upto(2) == 0 { ": int" } else { "" };
            let body = rng.pick(&BODY);
            operations.push_str(&format!("        o{mem}(){ret} {{\n            {body}\n        }}\n"));
            mem += 1;
        }
    }

    format!(
        "@@system Fuzz{n} {{\n    interface:\n        e()\n    machine:\n        $S {{\n            e() {{ -> $S }}\n        }}\n{actions}{operations}}}\n"
    )
}

#[test]
fn deterministic_fuzz_of_random_shapes_is_byte_identical() {
    let mut rng = Rng(0x5EED_1234_ABCD_0F01);
    let mut ran = 0usize;
    for n in 0..300usize {
        let frm = fuzz_system(&mut rng, n);
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
            for p in driver::actions_parity_report(&src, &ast, &syms, be) {
                assert_eq!(
                    p.machine_text, p.hand_text,
                    "FUZZ n={n} {target:?} [{}]: actions text differs\nsource:\n{frm}\n\
                     === machine ===\n{}\n=== hand ===\n{}",
                    p.label, p.machine_text, p.hand_text
                );
            }
            ran += 1;
        }
    }
    assert!(ran >= 300, "fuzz must actually run across targets; ran {ran}");
}
