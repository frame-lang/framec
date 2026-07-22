//! **GATE-A — the driver's handler-emission walk, as a `@@system`, is byte-for-byte the hand
//! walk.** SCAFFOLDING (differential vs the preserved
//! [`frame_compiler::text::emit::driver::emit_handlers_hand`] oracle; conversion-internal, never
//! promoted).
//!
//! `emit`'s `(section, state, handler)` nested pass — the private per-handler methods — was reified
//! as the `EmitHandlers` plain-`@@system` (`emit_handlers.frs`): a FIXED depth-3 walk expressed as
//! three NESTED CYCLE STATES (`$Section` → `$State` → `$Handler`, no push$/pop$), carrying the three
//! walk cursors `si`/`sti`/`hi` and their bounds. The per-handler SPELLING (open_handler, the
//! StmtWalk body walk, close_handler) is an unchanged native leaf; only the 3-level SEQUENCING moved
//! into the machine.
//!
//! This test proves — by running — that for **every** system, the machine path
//! ([`super::emit_handlers::walk`]) and the preserved hand oracle ([`emit_handlers_hand`]) emit the
//! **identical String** for all of that system's private methods, for **all four** cleanroom targets
//! (python/java/rust/c), over (a) a curated corpus that exercises multiple sections (the `$Section`
//! fork skipping interface/domain/actions), multiple states, multiple handlers per state, empty
//! states (the `$Handler` ascent on `nh == 0`), enter/exit lifecycle handlers, inherited vs explicit
//! return types, and async vs non-async, and (b) a deterministic fuzz of random
//! (section, state, handler) shapes. Byte-parity IS the gate: a single differing space fails. The
//! library owns the emission and the `.finish()`; the test only compares
//! ([`frame_compiler::text::emit::driver::handlers_parity_report`]).

use frame_compiler::resolve::resolve;
use frame_compiler::scan::{literals::Target, segment};
use frame_compiler::text::emit::c::C;
use frame_compiler::text::emit::driver::{self, Backend};
use frame_compiler::text::emit::{java::Java, python::Python, rust::Rust};
use frame_compiler::Source;

const TARGETS: [Target; 4] = [Target::Python3, Target::Java, Target::Rust, Target::C];

/// Run the handlers-pass differential over one Frame source, for all four targets. Asserts, for
/// every system, `machine_text == hand_text` (byte-for-byte). Returns the total number of
/// `(state, handler)` methods emitted across all systems and targets — so the caller can prove the
/// corpus was not vacuous.
fn check(label: &str, frm: &str) -> usize {
    let mut handlers_seen = 0usize;
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
        let report = driver::handlers_parity_report(&src, &ast, &syms, be);
        assert!(
            !report.is_empty(),
            "{label}: no systems parsed for {target:?}"
        );
        for p in &report {
            assert_eq!(
                p.machine_text, p.hand_text,
                "{label} [{}] {target:?}: EmitHandlers text != emit_handlers_hand text\n\
                 === machine (EmitHandlers) ===\n{}\n=== hand (oracle) ===\n{}",
                p.label, p.machine_text, p.hand_text
            );
            handlers_seen += p.handler_count;
        }
    }
    handlers_seen
}

/// Multiple states, multiple handlers per state, an explicit `String` return, Trivia, transitions,
/// a state-var assign.
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

/// Lifecycle handlers ($>/<$) whose events are NOT interface methods, plus push$/pop$.
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

/// HSM: a child state with a parent — the handler METHODS are still emitted per (declaring) state.
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

/// **Multiple non-machine sections** (`interface` + `machine` + `actions` + `domain`) so the
/// `$Section` fork must skip three section kinds; an **empty state** ($Empty, no handlers) so the
/// `$Handler` ascent on `nh == 0` fires; and an **inherited return type** (decide has no `: String`
/// on the handler; it inherits it from the interface).
const RICH: &str = r#"@@system Rich {
    interface:
        go()
        decide(): String
        idle()
    machine:
        $Empty {
        }
        $Work {
            go() { -> $Empty }
            decide() { @@:(String::from("x")) }
        }
        $Done {
            idle() { rest(); }
        }
    actions:
        helper(a: int): int {
            return a + 1;
        }
    domain:
        n: int = 0
}
"#;

/// An **async** system with an **inherited-return async handler** (fetch declares no type; it
/// inherits `int` from the async interface method) plus a plain handler.
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
    let mut total_handlers = 0usize;
    for (label, frm) in [
        ("Door", DOOR),
        ("Vend", VEND),
        ("Fwd", FWD),
        ("Rich", RICH),
        ("Async", ASYNCS),
    ] {
        total_handlers += check(label, frm);
    }
    // The corpus must have actually emitted a substantial number of handler methods (4 targets ×
    // the ~14 distinct handlers above). A vacuous corpus that skipped every handler would pass the
    // byte-parity asserts trivially; this guards against that.
    assert!(
        total_handlers >= 40,
        "curated corpus must exercise many handlers across targets; saw {total_handlers}"
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

/// Well-formed handler-body palette — a random one is always a parseable body.
const BODY: [&str; 6] = [
    "let v = 1;",
    "trivia_native();",
    "-> $S0",
    "@@:(mk())",
    "@@:self.tick()",
    "work(); more();",
];

/// Build a random system: a random number of states, each with a random number of handlers (0..3 —
/// so empty states occur), every handler event globally unique and declared in the interface (so
/// each has a matching interface method). Randomly append non-machine `actions:`/`domain:` sections
/// (so the `$Section` fork skips them). Every `-> $S0` targets a state that always exists.
fn fuzz_system(rng: &mut Rng, n: usize) -> String {
    let n_states = 1 + rng.upto(4); // 1..4 states; $S0 always present as a -> target
    let mut ev = 0usize; // global event counter -> unique names
    let mut iface = String::new();
    let mut machine = String::new();
    for si in 0..n_states {
        let n_handlers = rng.upto(3); // 0..2 -> some empty states
        machine.push_str(&format!("        $S{si} {{\n"));
        for _ in 0..n_handlers {
            let body = rng.pick(&BODY);
            iface.push_str(&format!("        e{ev}()\n"));
            machine.push_str(&format!("            e{ev}() {{ {body} }}\n"));
            ev += 1;
        }
        machine.push_str("        }\n");
    }
    // A no-handler interface method too, so the interface is never empty even with all-empty states.
    iface.push_str("        ping()\n");

    let mut extra = String::new();
    if rng.upto(2) == 0 {
        extra.push_str("    actions:\n        helper(a: int): int {\n            return a + 1;\n        }\n");
    }
    if rng.upto(2) == 0 {
        extra.push_str("    domain:\n        d: int = 0\n");
    }

    format!(
        "@@system Fuzz{n} {{\n    interface:\n{iface}    machine:\n{machine}{extra}}}\n"
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
            for p in driver::handlers_parity_report(&src, &ast, &syms, be) {
                assert_eq!(
                    p.machine_text, p.hand_text,
                    "FUZZ n={n} {target:?} [{}]: handlers text differs\nsource:\n{frm}\n\
                     === machine ===\n{}\n=== hand ===\n{}",
                    p.label, p.machine_text, p.hand_text
                );
            }
            ran += 1;
        }
    }
    assert!(ran >= 300, "fuzz must actually run across targets; ran {ran}");
}
