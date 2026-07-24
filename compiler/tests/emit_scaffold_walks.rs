//! **GATE-A — the four CONSTRUCTOR/ROUTER/DISPATCH scaffold walks, as `@@system`s, are
//! byte-for-byte their hand loops.** SCAFFOLDING (differential vs the preserved oracles in
//! [`frame_compiler::text::emit::driver`]; conversion-internal, never promoted).
//!
//! Milestone 1 of the faithful-emit rebuild moved four loops out of the Python backend's
//! `open_system` / router / dispatch spellings and into plain `@@system`s, so the whole emitter is
//! systems all the way down:
//!
//! | machine | `.frs` | what it sequences | its leaf spelling |
//! |---|---|---|---|
//! | `DomainInitWalk`    | `domain_init_walk.frs`    | `sym.domain`, one ctor seed per field | [`Backend::domain_init`] |
//! | `HsmChainWalk`      | `hsm_chain_walk.frs`      | `sym.states`, + the inner ancestor climb | [`Backend::hsm_chain_entry`] |
//! | `RouterWalk`        | `router_walk.frs`         | `sym.states`, carrying the `first`-arm bit | [`Backend::router_arm`] |
//! | `StateDispatchWalk` | `state_dispatch_walk.frs` | `sym.states`, + the inner handler stamp | [`Backend::dispatch`] |
//!
//! Every one is the §3 degenerate pole — a program-counter walk over the ALREADY-RESOLVED symbol
//! table, carrying no recognition register. The payoff claimed for them is not compression but
//! DOGFOOD UNIFORMITY, and this file is the price of that claim: for **every** system in the
//! corpus, and for **every** backend, the machine's text must equal the preserved hand loop's text
//! byte for byte. A divergence is the machine's bug, not the oracle's.
//!
//! The corpus is shared across the four gates on purpose — one set of systems that between them
//! have zero/one/many domain fields (including a `@@Sub()` instantiation init), flat and NESTED
//! (`$Child => $Parent`, two deep) states, states that handle nothing, and states that declare
//! lifecycle (`$>`/`<$`) as well as user events.

use frame_compiler::resolve::resolve;
use frame_compiler::scan::{literals::Target, segment};
use frame_compiler::text::emit::driver::{self, Backend};
use frame_compiler::text::emit::{c::C, java::Java, python::Python, rust::Rust};
use frame_compiler::Source;

/// A system with several domain fields (one of them a sub-system instantiation), flat states, a
/// value-returning event, and a state that handles nothing.
const HOLDER: &str = r#"@@system Inner {
    interface:
        ping()
    machine:
        $I {
            ping() { }
        }
}

@@system Holder {
    interface:
        go()
        val(): int
    machine:
        $A {
            go() {
                @@:self.x = 1
                -> $B
            }
            val(): int {
                @@:(5)
            }
        }
        $B {
        }
    domain:
        x: int = 0
        label: str = "hi"
        sub: Inner = @@Inner()
}
"#;

/// A NESTED machine, two levels deep, so the ancestor climb is a genuine climb and not a
/// one-element list: `$Deep => $Kid => $Root`.
const NESTED: &str = r#"@@system Nest {
    interface:
        a()
        b()
        c()
    machine:
        $Root {
            a() { root_a(); }
        }
        $Kid => $Root {
            b() { kid_b(); }
        }
        $Deep => $Kid {
            c() { deep_c(); }
        }
}
"#;

/// Lifecycle handlers ($>/<$) alongside user events, state params, and a state with NO domain at
/// all — so the zero-field arm of the domain walk is exercised too.
const LIFECYCLE: &str = r#"@@system Life {
    interface:
        go()
        back()
    machine:
        $A {
            $>(a: int) { seen(a); }
            <$(z: int) { left(z); }
            go() { (5) -> ("e1", 2) $B("sa", 9) }
        }
        $B(p: str, q: int) {
            $>(x: str, y: int) { arrived(x, y); }
            back() { push$ -> $A }
        }
}
"#;

const CORPUS: [(&str, &str); 3] = [("Holder", HOLDER), ("Nest", NESTED), ("Life", LIFECYCLE)];

/// The four real backends, so a gate that only held for the one target whose spellings are
/// overridden would be caught. (`Java`/`Rust`/`C` take the no-op defaults; the walks must be
/// byte-identical to their hand loops there too — which for them means "both emit nothing".)
fn backends() -> Vec<(&'static str, Box<dyn Backend>)> {
    vec![
        ("python", Box::new(Python) as Box<dyn Backend>),
        ("java", Box::new(Java::new())),
        ("rust", Box::new(Rust)),
        ("c", Box::new(C::new())),
    ]
}

fn parse(frm: &str) -> (Source, frame_compiler::tree::FileAst, frame_compiler::resolve::SymbolTable) {
    let src = Source::new("t.frm", frm.as_bytes().to_vec()).unwrap();
    let ast = segment(&src, Target::Python3).expect("segment");
    let (syms, _diags) = resolve(&ast);
    (src, ast, syms)
}

#[test]
fn domain_init_walk_is_byte_identical_to_the_hand_loop() {
    let mut fields_seen = 0usize;
    let mut systems = 0usize;
    for (label, frm) in CORPUS {
        let (_src, ast, syms) = parse(frm);
        for (bname, be) in backends() {
            let report = driver::domain_init_parity_report(&ast, &syms, be.as_ref());
            assert!(!report.is_empty(), "{label}/{bname}: no systems resolved");
            for p in &report {
                assert_eq!(
                    p.machine_text, p.hand_text,
                    "{label}/{bname} [{}]: DomainInitWalk machine text != domain_init_hand",
                    p.label
                );
                if bname == "python" {
                    fields_seen += p.field_count;
                    systems += 1;
                }
            }
        }
    }
    // Non-vacuity: the corpus must actually have carried fields through the walk, and must have
    // included at least one system with none (`Nest`, `Life`) so the empty arm is covered.
    assert!(fields_seen >= 3, "corpus must exercise multi-field domains; saw {fields_seen}");
    assert!(systems >= 4, "corpus must span several systems; saw {systems}");
}

#[test]
fn hsm_chain_walk_is_byte_identical_to_the_hand_loop() {
    let mut deepest = 0usize;
    let mut states_seen = 0usize;
    for (label, frm) in CORPUS {
        let (_src, ast, syms) = parse(frm);
        for (bname, be) in backends() {
            let report = driver::hsm_chain_parity_report(&ast, &syms, be.as_ref());
            assert!(!report.is_empty(), "{label}/{bname}: no systems resolved");
            for p in &report {
                assert_eq!(
                    p.machine_text, p.hand_text,
                    "{label}/{bname} [{}]: HsmChainWalk machine text != hsm_chain_hand",
                    p.label
                );
                if bname == "python" {
                    deepest = deepest.max(p.max_depth);
                    states_seen += p.state_count;
                }
            }
        }
    }
    // Non-vacuity: a corpus of only FLAT machines would leave the climb (the whole reason this walk
    // is two-level) unproven — `Nest` is three deep.
    assert!(deepest >= 3, "corpus must exercise a real ancestor climb; deepest chain was {deepest}");
    assert!(states_seen >= 8, "corpus must span many states; saw {states_seen}");
}

#[test]
fn router_walk_is_byte_identical_to_the_hand_loop() {
    let mut multi_arm = 0usize;
    for (label, frm) in CORPUS {
        let (_src, ast, syms) = parse(frm);
        for (bname, be) in backends() {
            let report = driver::router_parity_report(&ast, &syms, be.as_ref());
            assert!(!report.is_empty(), "{label}/{bname}: no systems resolved");
            for p in &report {
                assert_eq!(
                    p.machine_text, p.hand_text,
                    "{label}/{bname} [{}]: RouterWalk machine text != router_hand",
                    p.label
                );
                if bname == "python" && p.state_count > 1 {
                    multi_arm += 1;
                }
            }
        }
    }
    // Non-vacuity: with one arm per system the `first` latch is never observed being CLEARED, and a
    // walk that ignored it entirely would still pass.
    assert!(multi_arm >= 3, "corpus must exercise multi-arm routers; saw {multi_arm}");
}

#[test]
fn state_dispatch_walk_is_byte_identical_to_the_hand_loops() {
    let mut arms = 0usize;
    let mut empties = 0usize;
    for (label, frm) in CORPUS {
        let (_src, ast, syms) = parse(frm);
        for (bname, be) in backends() {
            let report = driver::state_dispatch_parity_report(&ast, &syms, be.as_ref());
            assert!(!report.is_empty(), "{label}/{bname}: no systems resolved");
            for p in &report {
                assert_eq!(
                    p.machine_text, p.hand_text,
                    "{label}/{bname} [{}]: StateDispatchWalk machine text != state_dispatch_hand",
                    p.label
                );
                if bname == "python" {
                    arms += p.arm_count;
                    empties += p.empty_states;
                }
            }
        }
    }
    // Non-vacuity: the corpus must have stamped real arms (multi-handler states, lifecycle AND user
    // events) and must have hit at least one state that declares nothing (the `pass` arm).
    assert!(arms >= 10, "corpus must stamp many dispatch arms; saw {arms}");
    assert!(empties >= 1, "corpus must include a state that handles nothing; saw {empties}");
}

/// **The four gates above are not vacuous.** Three of the four walks hand their per-item work to a
/// `Backend` method with a NO-OP default, so "machine == hand" would hold trivially on a target
/// that overrides nothing — both paths emit the empty string. This proves the one target that DOES
/// override them emitted real text through every walk, so the parities above are comparing
/// something.
#[test]
fn the_python_target_actually_drives_all_four_walks() {
    let (_src, ast, syms) = parse(HOLDER);
    let be = Python;
    let dom: usize = driver::domain_init_parity_report(&ast, &syms, &be)
        .iter()
        .map(|p| p.machine_text.len())
        .sum();
    let chain: usize = driver::hsm_chain_parity_report(&ast, &syms, &be)
        .iter()
        .map(|p| p.machine_text.len())
        .sum();
    let router: usize = driver::router_parity_report(&ast, &syms, &be)
        .iter()
        .map(|p| p.machine_text.len())
        .sum();
    let disp: usize = driver::state_dispatch_parity_report(&ast, &syms, &be)
        .iter()
        .map(|p| p.machine_text.len())
        .sum();
    assert!(dom > 0, "DomainInitWalk emitted nothing on python — the gate would be vacuous");
    assert!(chain > 0, "HsmChainWalk emitted nothing on python — the gate would be vacuous");
    assert!(router > 0, "RouterWalk emitted nothing on python — the gate would be vacuous");
    assert!(disp > 0, "StateDispatchWalk emitted nothing on python — the gate would be vacuous");

    // And the three brace targets take the defaults, which is exactly why their bytes did not move.
    for (bname, be) in backends() {
        if bname == "python" {
            continue;
        }
        let t: usize = driver::state_dispatch_parity_report(&ast, &syms, be.as_ref())
            .iter()
            .map(|p| p.machine_text.len())
            .sum();
        assert_eq!(t, 0, "{bname} must take the no-op dispatch default (its output must not move)");
    }
}
