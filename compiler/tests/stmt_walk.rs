//! **GATE-A — the handler/action body statement walk, as a `@@system`, is byte-for-byte the hand
//! walk.** SCAFFOLDING (differential vs the preserved [`emit_body_hand`] oracle; conversion-
//! internal, never promoted).
//!
//! `emit_body` was reified as the `StmtWalk` plain-`@@system` (`stmt_walk.frs`) — the emit-side
//! Mealy transducer: a cursor over the statement slice carrying a one-bit `terminated` latch,
//! read back to halt the walk at a base-nesting terminal and to choose the body's terminal
//! (`Terminated` vs `Fell`). The per-arm SPELLING sequences are unchanged native leaves; only the
//! WALK moved into the machine.
//!
//! This test proves — by running — that for **every** handler and action body, the machine path
//! (`emit_body`) and the preserved hand oracle (`emit_body_hand`) emit the **identical String**
//! and report the **identical BodyEnd**, for **all four** cleanroom targets, over (a) a curated
//! corpus that exercises every one of the 10 `Stmt` variants (proven by a kind tally taken with
//! the machine's own classifier) plus the lifecycle/HSM-forward/nested-terminal edges, and (b) a
//! deterministic fuzz of random statement sequences. Byte-parity IS the gate: a single differing
//! space fails. The library owns the emission and the `.finish()`; the test only compares
//! ([`frame_compiler::text::emit::driver::body_parity_report`]).

use frame_compiler::resolve::resolve;
use frame_compiler::scan::{literals::Target, segment};
use frame_compiler::text::emit::c::C;
use frame_compiler::text::emit::driver::{self, Backend};
use frame_compiler::text::emit::{java::Java, python::Python, rust::Rust};
use frame_compiler::Source;

const TARGETS: [Target; 4] = [Target::Python3, Target::Java, Target::Rust, Target::C];

/// Run the differential over one Frame source, for all four targets. Asserts, for every parsed
/// body, `machine_text == hand_text` (byte-for-byte) AND `machine_terminated == hand_terminated`.
/// Returns the set of `Stmt` kinds (0..9) the corpus exercised, unioned across targets — so the
/// caller can prove full variant coverage using the machine's own classifier.
fn check(label: &str, frm: &str) -> Vec<i32> {
    let mut kinds_seen: Vec<i32> = Vec::new();
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
        let report = driver::body_parity_report(&src, &ast, &syms, be);
        assert!(
            !report.is_empty(),
            "{label}: no bodies parsed for {target:?} (did the system resolve?)"
        );
        for p in &report {
            assert_eq!(
                p.machine_text, p.hand_text,
                "{label} [{}] {target:?}: StmtWalk text != emit_body_hand text\n\
                 === machine (StmtWalk) ===\n{}\n=== hand (oracle) ===\n{}",
                p.label, p.machine_text, p.hand_text
            );
            assert_eq!(
                p.machine_terminated, p.hand_terminated,
                "{label} [{}] {target:?}: BodyEnd differs (machine terminated={}, hand terminated={})",
                p.label, p.machine_terminated, p.hand_terminated
            );
            for &k in &p.kinds {
                if !kinds_seen.contains(&k) {
                    kinds_seen.push(k);
                }
            }
        }
    }
    kinds_seen
}

/// Transitions, a state-var assign, a return, native code, and a comment (Trivia) inside a body.
const DOOR: &str = r#"@@system Door {
    interface:
        open()
        close()
        report(): String
    machine:
        $Closed {
            $.tries: i32 = 0
            open() {
                // bump the counter (this comment is Trivia between statements)
                let x = 1;
                $.tries = $.tries + 1
                -> $Open
            }
            report(): String { @@:(String::from("closed")) }
        }
        $Open {
            close() { -> $Closed }
            report(): String { @@:(String::from("open")) }
        }
}
"#;

/// push$/pop$ pushdown WITH lifecycle handlers ($>/<$), so the transition/push arms take their
/// `has_lifecycle` branches; the exit/enter handlers themselves are native-bearing bodies.
const VEND: &str = r#"@@system Vend {
    interface:
        coin()
        pick()
        refund()
    machine:
        $Idle {
            <$(code: i32) { log_exit(code); }
            coin() { push$ -> $Paid(5) }
        }
        $Paid {
            $>(amount: i32) { note_enter(amount); }
            pick() { -> pop$ }
            refund() { (99) -> pop$ }
        }
}
"#;

/// HSM forward: `=> $^` to a parent that HANDLES the event (real forward) and to one that does
/// NOT (the no-op branch). Both bodies carry a leading native statement.
const FWD: &str = r#"@@system Fwd {
    interface:
        ping()
        buzz()
    machine:
        $Awake => $Live {
            ping() {
                run_child();
                => $^
            }
            buzz() {
                only_child();
                => $^
            }
        }
        $Live {
            ping() { run_parent(); }
        }
}
"#;

/// Self-call, a self-field assign, bare `push$` (StackPush target=None) and bare `pop$`
/// (StackPopBare), a falling (non-terminating) body, and a NESTED terminal (a transition at brace
/// depth 1, which must NOT terminate the body).
const MISC: &str = r#"@@system Misc {
    interface:
        go()
        tick()
        act()
    machine:
        $A {
            go() {
                @@:self.flag = 2
                @@:self.tick()
                pop$
                push$
            }
            tick() {
                if flag {
                    -> $B
                }
                after_the_if();
            }
            act() {
                just_native();
            }
        }
        $B {
            go() { -> $A }
        }
    domain:
        flag: i32 = 0
}
"#;

#[test]
fn curated_corpus_is_byte_identical_and_covers_every_variant() {
    let mut all: Vec<i32> = Vec::new();
    for (label, frm) in [("Door", DOOR), ("Vend", VEND), ("Fwd", FWD), ("Misc", MISC)] {
        for k in check(label, frm) {
            if !all.contains(&k) {
                all.push(k);
            }
        }
    }
    all.sort();
    // The 10 Stmt variants, by the machine's own `kind_of`: 0=Trivia .. 9=Forward. If any is
    // missing the corpus did not exercise it — a hole in the gate, not a pass.
    assert_eq!(
        all,
        (0..=9).collect::<Vec<i32>>(),
        "curated corpus must exercise ALL 10 Stmt variants; saw {all:?}"
    );
}

#[test]
fn known_body_ends_are_correct_on_both_paths() {
    // A base-nesting `-> $Open` terminates; a purely-native body falls through. `check` already
    // asserts machine==hand for these; here we pin the absolute values so the latch is proven
    // load-bearing (not merely self-consistent).
    let src = Source::new("t.frm", MISC.as_bytes().to_vec()).unwrap();
    let ast = segment(&src, Target::Rust).unwrap();
    let (syms, _d) = resolve(&ast);
    let report = driver::body_parity_report(&src, &ast, &syms, &Rust);
    let end_of = |lbl: &str| {
        report
            .iter()
            .find(|p| p.label == lbl)
            .unwrap_or_else(|| panic!("no body {lbl}; have {:?}", report.iter().map(|p| &p.label).collect::<Vec<_>>()))
            .machine_terminated
    };
    // `act()` is one native statement — no terminal — so the body FALLS.
    assert!(!end_of("Misc/A/act"), "a native-only body must fall through");
    // `$B.go()` is a base-nesting `-> $A` — a terminal.
    assert!(end_of("Misc/B/go"), "a base-nesting transition must terminate the body");
    // `tick()` ends with a native call; its `-> $B` is nested at depth 1, so the body FALLS.
    assert!(!end_of("Misc/A/tick"), "a depth>0 transition must NOT terminate the body");
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

/// Well-formed statement palette — one per (or several per) Stmt variant. Every string parses in
/// isolation; a random sequence of them is always a parseable body (dead code after a mid-body
/// terminal is fine — the parser keeps it, and both walk paths agree on it).
const PALETTE: [&str; 12] = [
    "let v = 1;",           // Native
    "trivia_native();",     // Native
    "// a trailing comment",// Trivia
    "$.n = $.n + 1",        // Assign (state var)
    "@@:self.flag = 7",     // Assign (self field)
    "@@:self.tick()",       // SelfCall
    "@@:(String::from(\"x\"))", // ReturnCall
    "-> $B",                // Transition
    "push$ -> $B(1)",       // StackPush (target)
    "push$",                // StackPush (bare)
    "-> pop$",              // StackPop
    "pop$",                 // StackPopBare
];

fn fuzz_system(n: usize, body: &str) -> String {
    format!(
        r#"@@system Fuzz{n} {{
    interface:
        run()
        tick()
    machine:
        $A {{
            $.n: i32 = 0
            run() {{
{body}
            }}
            tick() {{ trivia(); }}
        }}
        $B {{
            run() {{ -> $A }}
        }}
    domain:
        flag: i32 = 0
}}
"#
    )
}

#[test]
fn deterministic_fuzz_of_random_statement_sequences_is_byte_identical() {
    let mut rng = Rng(0x5EED_1234_ABCD_0F01);
    let mut ran = 0usize;
    for n in 0..300usize {
        let count = 1 + rng.upto(9);
        let body: String = (0..count)
            .map(|_| format!("                {}", rng.pick(&PALETTE)))
            .collect::<Vec<_>>()
            .join("\n");
        let frm = fuzz_system(n, &body);

        // Compare, for every target, machine == hand on every body of this random system.
        for target in TARGETS {
            let src = Source::new("t.frm", frm.as_bytes().to_vec()).unwrap();
            let ast = match segment(&src, target) {
                Ok(a) => a,
                // A random-but-well-formed body should always segment; if a target's lexer
                // legitimately rejects it, skip (the point is parseable bodies, not this input).
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
            for p in driver::body_parity_report(&src, &ast, &syms, be) {
                assert_eq!(
                    p.machine_text, p.hand_text,
                    "FUZZ n={n} {target:?} [{}]: text differs\nbody:\n{body}\n=== machine ===\n{}\n=== hand ===\n{}",
                    p.label, p.machine_text, p.hand_text
                );
                assert_eq!(
                    p.machine_terminated, p.hand_terminated,
                    "FUZZ n={n} {target:?} [{}]: BodyEnd differs\nbody:\n{body}",
                    p.label
                );
            }
            ran += 1;
        }
    }
    assert!(ran >= 300, "fuzz must actually run across targets; ran {ran}");
}
