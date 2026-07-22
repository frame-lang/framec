//! **GATE-A — the driver's interface/router walk, as a `@@system`, is byte-for-byte the hand
//! walk.** SCAFFOLDING (differential vs the preserved
//! [`frame_compiler::text::emit::driver::emit_interface_hand`] oracle; conversion-internal, never
//! promoted).
//!
//! `emit`'s `(method, arm)` router pass — one PUBLIC method per interface event, dispatching to the
//! private handler methods — was reified as the `EmitInterface` plain-`@@system`
//! (`emit_interface.frs`): a FIXED depth-2 walk expressed as two NESTED CYCLE STATES (`$Method` →
//! `$Arm`, no push$/pop$), carrying the two walk cursors `mi`/`ai`, their bounds, and a per-method
//! arm accumulator. The per-method SPELLING (the `resolve_handler` HSM stamp + `be.route`) is an
//! unchanged native leaf; only the 2-level SEQUENCING moved into the machine.
//!
//! This test proves — by running — that for **every** system, the machine path
//! ([`super::emit_interface::walk`]) and the preserved hand oracle ([`emit_interface_hand`]) emit
//! the **identical String** for all of that system's public router methods, for **all four**
//! cleanroom targets (python/java/rust/c), over (a) a curated corpus that exercises multiple events
//! (multiple router methods), multiple states (multiple arms per method), events some states do not
//! handle (the `$Arm` `resolve_handler` → `None` skip), HSM inheritance (an arm whose owner is an
//! ancestor state), inherited vs explicit return types, and async vs non-async (the `is_async`
//! disjunction), and (b) a deterministic fuzz of random (event, state) shapes. Byte-parity IS the
//! gate: a single differing space fails. The library owns the emission and the `.finish()`; the
//! test only compares ([`frame_compiler::text::emit::driver::interface_parity_report`]).

use frame_compiler::resolve::resolve;
use frame_compiler::scan::{literals::Target, segment};
use frame_compiler::text::emit::c::C;
use frame_compiler::text::emit::driver::{self, Backend};
use frame_compiler::text::emit::{java::Java, python::Python, rust::Rust};
use frame_compiler::Source;

const TARGETS: [Target; 4] = [Target::Python3, Target::Java, Target::Rust, Target::C];

/// Run the interface-pass differential over one Frame source, for all four targets. Asserts, for
/// every system, `machine_text == hand_text` (byte-for-byte). Returns the total number of public
/// router methods emitted across all systems and targets — so the caller can prove the corpus was
/// not vacuous.
fn check(label: &str, frm: &str) -> usize {
    let mut routes_seen = 0usize;
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
        let report = driver::interface_parity_report(&ast, &syms, be);
        assert!(
            !report.is_empty(),
            "{label}: no systems parsed for {target:?}"
        );
        for p in &report {
            assert_eq!(
                p.machine_text, p.hand_text,
                "{label} [{}] {target:?}: EmitInterface text != emit_interface_hand text\n\
                 === machine (EmitInterface) ===\n{}\n=== hand (oracle) ===\n{}",
                p.label, p.machine_text, p.hand_text
            );
            routes_seen += p.route_count;
        }
    }
    routes_seen
}

/// Multiple states, multiple events; `report` is handled by BOTH states (two arms), while `open`
/// and `close` are each handled by only ONE state — so the OTHER state's `resolve_handler` returns
/// `None` and stamps no arm. An explicit `String` return on `report`.
const DOOR: &str = r#"@@system Door {
    interface:
        open()
        close()
        report(): String
    machine:
        $Closed {
            open() { -> $Open }
            report(): String { @@:(String::from("closed")) }
        }
        $Open {
            close() { -> $Closed }
            report(): String { @@:(String::from("open")) }
        }
}
"#;

/// Lifecycle handlers ($>/<$) whose events are NOT interface methods (so they never route), plus
/// push$/pop$ bodies (routing is unaffected by body shape). Every interface event is handled by the
/// single state.
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

/// HSM: `$Awake => $Live`. `ping` is declared by BOTH (each routes to itself); `buzz` is declared
/// only by `$Awake`, and `$Live` has no parent that declares it — so `resolve_handler($Live, buzz)`
/// is `None` (no arm) while `resolve_handler($Awake, buzz)` is `$Awake`. The `$Arm` `None`-skip and
/// the ancestor-owner cases both fire.
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

/// **Multiple non-machine sections** (`interface` + `machine` + `actions` + `domain`); an **empty
/// state** ($Empty, handles nothing — so every method's `resolve_handler($Empty, _)` is `None`);
/// and an **inherited return type** (`decide` has no `: String` on its handler; the router still
/// returns the interface type).
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

/// An **async** system: every router method is async (`is_async` from the system `@@[async]`), one
/// with an inherited `int` return.
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
    let mut total_routes = 0usize;
    for (label, frm) in [
        ("Door", DOOR),
        ("Vend", VEND),
        ("Fwd", FWD),
        ("Rich", RICH),
        ("Async", ASYNCS),
    ] {
        total_routes += check(label, frm);
    }
    // The corpus must have actually emitted a substantial number of router methods (4 targets ×
    // the 13 distinct interface events above). A vacuous corpus would pass byte-parity trivially;
    // this guards against that.
    assert!(
        total_routes >= 40,
        "curated corpus must exercise many router methods across targets; saw {total_routes}"
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

/// Build a random system: a random number of states, each declaring a random SUBSET of a shared
/// event pool (so some `(state, event)` pairs resolve to a handler and others do not — exercising
/// the `$Arm` stamp/skip fork). Some states inherit a parent (HSM), so an event a child does not
/// declare may still resolve to an ancestor. Every event is declared in the interface. Randomly
/// async. Every `-> $S0` targets a state that always exists.
fn fuzz_system(rng: &mut Rng, n: usize) -> String {
    let n_states = 1 + rng.upto(4); // 1..4 states; $S0 always present as a -> target
    let n_events = 1 + rng.upto(4); // 1..4 shared events
    let is_async = rng.upto(2) == 0;

    let mut iface = String::new();
    for e in 0..n_events {
        if is_async {
            iface.push_str(&format!("        async e{e}(): int\n"));
        } else {
            iface.push_str(&format!("        e{e}()\n"));
        }
    }

    let mut machine = String::new();
    for si in 0..n_states {
        // Half the non-root states inherit the previous state as a parent (HSM).
        if si > 0 && rng.upto(2) == 0 {
            machine.push_str(&format!("        $S{si} => $S{} {{\n", si - 1));
        } else {
            machine.push_str(&format!("        $S{si} {{\n"));
        }
        for e in 0..n_events {
            // Each state declares each event with ~50% probability -> a mix of handled/unhandled.
            if rng.upto(2) == 0 {
                if is_async {
                    machine.push_str(&format!("            e{e}() {{ @@:(mk()) }}\n"));
                } else {
                    machine.push_str(&format!("            e{e}() {{ work(); }}\n"));
                }
            }
        }
        machine.push_str("        }\n");
    }

    let prefix = if is_async { "@@[async]\n" } else { "" };
    format!(
        "{prefix}@@system Fuzz{n} {{\n    interface:\n{iface}    machine:\n{machine}}}\n"
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
            for p in driver::interface_parity_report(&ast, &syms, be) {
                assert_eq!(
                    p.machine_text, p.hand_text,
                    "FUZZ n={n} {target:?} [{}]: interface text differs\nsource:\n{frm}\n\
                     === machine ===\n{}\n=== hand ===\n{}",
                    p.label, p.machine_text, p.hand_text
                );
            }
            ran += 1;
        }
    }
    assert!(ran >= 300, "fuzz must actually run across targets; ran {ran}");
}
