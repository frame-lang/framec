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

    // Rust, Java, AND C now all run the kernel model (M1): each emits a real `_state_<S>` message
    // dispatcher, so all three DRIVE the StateDispatchWalk exactly as python does. (Their parity is
    // gated by `state_dispatch_walk_is_byte_identical_to_the_hand_loops` above.) The obsolete
    // pre-kernel "takes the no-op dispatch default" assertions are gone now that every backend has
    // landed its kernel model.
    for (bname, be) in backends() {
        if bname == "python" {
            continue;
        }
        let t: usize = driver::state_dispatch_parity_report(&ast, &syms, be.as_ref())
            .iter()
            .map(|p| p.machine_text.len())
            .sum();
        if bname == "rust" || bname == "java" || bname == "c" {
            assert!(t > 0, "{bname} runs the kernel model — it must drive the StateDispatchWalk with a real `_state_X` dispatcher");
        } else {
            assert_eq!(t, 0, "{bname} must take the no-op dispatch default (its output must not move)");
        }
    }
}

/// GATE-A for the shared `DispatchBody` `@@system` (the per-state dispatcher body for the `if`-chain
/// targets, driven by the production `Backend::dispatch`): for each reified target it must emit
/// byte-for-byte what that target's preserved frozen hand oracle does, over the same real systems and
/// the same stamped arms. The hand side is the STANDALONE frozen body — it does not route through
/// `be.dispatch` — so a spelling bug in a `dispatch_*` leaf is visible here (the false-gate trap the
/// reification must avoid). Each language listed here is that language's landed milestone; a language
/// still on its hand-written dispatcher is absent (we do not test what we have not modified).
#[test]
fn dispatch_body_is_byte_identical_to_the_frozen_hand() {
    // A system with an explicit default-forward (`=> $^`) so the close leaf's forward path is
    // exercised — the shared corpus's `$Child => $Parent` nesting alone does not set `default_forward`.
    const FWD: &str = r#"@@system Fwd {
    interface:
        e()
    machine:
        $Parent {
            e() { pe(); }
        }
        $Child => $Parent {
            => $^
        }
}
"#;
    type ReportFn =
        fn(&frame_compiler::tree::FileAst, &frame_compiler::resolve::SymbolTable) -> Vec<driver::DispatchBodyParity>;
    // (language, its DispatchBody parity accessor) — one entry per reified `if`-chain target.
    let cases: [(&str, ReportFn); 3] = [
        ("python", driver::py_dispatch_body_parity),
        ("java", driver::java_dispatch_body_parity),
        ("c", driver::c_dispatch_body_parity),
    ];
    for (lang, report_fn) in cases {
        let mut arms = 0usize;
        let mut empties = 0usize;
        let mut params = 0usize;
        let mut forwards = 0usize;
        for (label, frm) in CORPUS.into_iter().chain([("Fwd", FWD)]) {
            let (_src, ast, syms) = parse(frm);
            let report = report_fn(&ast, &syms);
            assert!(!report.is_empty(), "{lang}/{label}: no systems resolved");
            for p in &report {
                assert_eq!(
                    p.machine_text, p.hand_text,
                    "{lang}/{label} [{}]: DispatchBody machine text != {lang}_dispatch_hand",
                    p.label
                );
                arms += p.arm_count;
                empties += p.empty_states;
                params += p.param_states;
                forwards += p.forward_states;
            }
        }
        // Non-vacuity — one for each branch/leaf of the walk: real arms (the message if-chain), an
        // empty dispatcher (the empty close), a state with params (the bind loop), and a
        // default-forward state (the close leaf's forward path).
        assert!(arms >= 10, "{lang}: corpus must stamp many dispatch arms; saw {arms}");
        assert!(empties >= 1, "{lang}: corpus must include a state that handles nothing; saw {empties}");
        assert!(params >= 1, "{lang}: corpus must include a state with params; saw {params}");
        assert!(forwards >= 1, "{lang}: corpus must include a default-forward state; saw {forwards}");
    }
}

/// GATE-A for the rust-only `RustDispatch` `@@system` (the per-state `_state_<S>` dispatcher, driven
/// by the production `Backend::dispatch` on rust): for each real system it must emit byte-for-byte
/// what rust's preserved frozen hand oracle (`rust_dispatch_hand`) does, over the same stamped arms.
/// Rust's dispatcher is a `match` over a typed event enum — a different control structure from the
/// `if`-chain targets' shared `DispatchBody` — so it gets its own system and its own parity gate.
/// The hand side is the STANDALONE frozen body (it does not route through `be.dispatch`), so a
/// spelling bug in a `rust_dispatch_*` leaf is visible here.
#[test]
fn rust_dispatch_is_byte_identical_to_the_frozen_hand() {
    let mut arms = 0usize;
    let mut empties = 0usize;
    for (label, frm) in CORPUS {
        let (_src, ast, syms) = parse(frm);
        let report = driver::rust_dispatch_parity_report(&ast, &syms);
        assert!(!report.is_empty(), "rust/{label}: no systems resolved");
        for p in &report {
            assert_eq!(
                p.machine_text, p.hand_text,
                "rust/{label} [{}]: RustDispatch machine text != rust_dispatch_hand",
                p.label
            );
            arms += p.arm_count;
            empties += p.empty_states;
        }
    }
    // Non-vacuity: real arms stamped (the message match), and at least one state that declares
    // nothing (the empty dispatcher — `_ => {}` with no user arms).
    assert!(arms >= 10, "corpus must stamp many dispatch arms; saw {arms}");
    assert!(empties >= 1, "corpus must include a state that handles nothing; saw {empties}");
}

/// GATE-A for the shared `HandlerOpen` `@@system` (the per-state handler-METHOD opener for the
/// header + binding-loop targets, driven by the production `Backend::open_handler`): for each
/// reified target it must emit byte-for-byte what that target's preserved frozen hand oracle does,
/// over the same real systems and the same `(state, event, params)` per handler. The hand side is
/// the STANDALONE frozen body — it does not route through `be.open_handler` — so a spelling bug in a
/// `handler_*` leaf is visible here (the false-gate trap the reification must avoid). Rust is absent:
/// its opener is a scan-branch + header-only kernel branch with no binding loops, a separate future
/// milestone (we do not test what we have not modified).
#[test]
fn handler_open_is_byte_identical_to_the_frozen_hand() {
    type ReportFn =
        fn(&frame_compiler::tree::FileAst, &frame_compiler::resolve::SymbolTable) -> Vec<driver::HandlerOpenParity>;
    // (language, its HandlerOpen parity accessor) — one entry per reified header + binding-loop target.
    let cases: [(&str, ReportFn); 3] = [
        ("python", driver::py_handler_open_parity),
        ("java", driver::java_handler_open_parity),
        ("c", driver::c_handler_open_parity),
    ];
    for (lang, report_fn) in cases {
        let mut handlers = 0usize;
        let mut state_params = 0usize;
        let mut event_params = 0usize;
        let mut lifecycles = 0usize;
        for (label, frm) in CORPUS {
            let (_src, ast, syms) = parse(frm);
            let report = report_fn(&ast, &syms);
            assert!(!report.is_empty(), "{lang}/{label}: no systems resolved");
            for p in &report {
                assert_eq!(
                    p.machine_text, p.hand_text,
                    "{lang}/{label} [{}]: HandlerOpen machine text != {lang}_open_handler_hand",
                    p.label
                );
                handlers += p.handler_count;
                state_params += p.state_param_handlers;
                event_params += p.event_param_handlers;
                lifecycles += p.lifecycle_handlers;
            }
        }
        // Non-vacuity — one for each branch/loop of the walk: real handlers opened (the header), a
        // handler in a state WITH params (the state-arg bind loop), a handler with event params (the
        // event-arg bind loop), and a lifecycle handler (`$>`/`<$`, the enter/exit-arg slot arms).
        assert!(handlers >= 5, "{lang}: corpus must open many handlers; saw {handlers}");
        assert!(state_params >= 1, "{lang}: corpus must include a handler in a state with params; saw {state_params}");
        assert!(event_params >= 1, "{lang}: corpus must include a handler with event params; saw {event_params}");
        assert!(lifecycles >= 1, "{lang}: corpus must include a lifecycle ($>/<$) handler; saw {lifecycles}");
    }
}
