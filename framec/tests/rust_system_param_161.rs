//! Issue #161 — a system instance as an interface parameter on rust.
//!
//! A live FSM is move-only (Clone would be semantically wrong) while the event
//! is Rc-shared (context stack + kernel), so a by-value system param can never
//! reach the handler: rejected with **E616**, whose message names the working
//! spelling — `Rc<RefCell<Sys>>`, the reference-semantics equivalent of passing
//! an instance on the OO targets. Supporting changes: the event enum derives
//! `Clone` only (Debug forced `Sys: Debug` through the RefCell and is used
//! nowhere), wrapper packing MOVES owned params into the variant instead of
//! cloning, and `Rc<`/`Arc<`-family payloads clone (refcount bump) at the
//! dispatch site.

mod common;
use common::{compile_expect_error, compile_source};

#[test]
fn by_value_system_param_is_e616() {
    let err = compile_expect_error(
        r#"
@@[target("rust")]
@@system Counter {
    interface: poke(k: i32)
    machine: $S { poke(k: i32) { @@:self.n = @@:self.n + k; } }
    domain: n: i32 = 0
}
@@[main]
@@system Hub {
    interface: add(c: Counter)
    machine: $S { add(c: Counter) { @@:self.counters.push(c); } }
    domain: counters: Vec<Counter> = Vec::new()
}
"#,
        "rust",
    );
    assert!(
        err.contains("E616") && err.contains("Rc<RefCell<Counter>>"),
        "[#161] by-value system param must be E616 with Rc<RefCell> guidance; got:\n{err}"
    );
}

#[test]
fn rc_refcell_system_param_compiles() {
    let code = compile_source(
        r#"
@@[target("rust")]
use std::rc::Rc;
use std::cell::RefCell;
@@system Counter {
    interface: poke(k: i32)
    machine: $S { poke(k: i32) { @@:self.n = @@:self.n + k; } }
    domain: n: i32 = 0
}
@@[main]
@@system Hub {
    interface: add(c: Rc<RefCell<Counter>>)
    machine: $S { add(c: Rc<RefCell<Counter>>) { @@:self.counters.push(c); } }
    domain: counters: Vec<Rc<RefCell<Counter>>> = Vec::new()
}
"#,
        "rust",
    );
    // Wrapper MOVES the handle into the variant (no needless clone)…
    assert!(
        code.contains("HubFrameEvent::Add { c: c }"),
        "[#161] wrapper must move the param into the variant:\n{code}"
    );
    // …and the dispatch site clones it (refcount bump) out of the shared event.
    assert!(
        code.contains("c.clone()"),
        "[#161] dispatch site must clone the Rc handle:\n{code}"
    );
    // Debug is no longer derived on the event enum (it forced Sys: Debug).
    assert!(
        !code.contains("#[derive(Clone, Debug)]\n    #[allow(dead_code, non_camel_case_types)]\n    enum HubFrameEvent"),
        "[#161] the event enum must not derive Debug:\n{code}"
    );
}

#[test]
fn event_enum_derives_clone_only() {
    // Clone is REQUIRED (the forward path `-> => $S` clones the in-flight
    // event from a `&Event`); Debug is dropped.
    let code = compile_source(
        r#"
@@[target("rust")]
@@[main]
@@system F {
    interface: go(msg: String)
    machine:
        $A { go(msg: String) { -> => $B } }
        $B { go(msg: String) { } }
}
"#,
        "rust",
    );
    assert!(
        code.contains("#[derive(Clone)]"),
        "[#161] event enum must still derive Clone (forwards need it):\n{code}"
    );
    // (FrameValue — the closed @@:data value enum — keeps Clone+Debug; only
    // the EVENT enum, whose payloads are open user types, drops Debug.)
    let evt_pos = code.find("enum FFrameEvent").expect("event enum present");
    let derive_region = &code[evt_pos.saturating_sub(200)..evt_pos];
    assert!(
        !derive_region.contains("Debug"),
        "[#161] Debug must be gone from the EVENT enum:\n{derive_region}"
    );
    // Owned String param moves into the variant — no clone at the wrapper.
    assert!(
        code.contains("FFrameEvent::Go { msg: msg }"),
        "[#161] owned String param must move, not clone:\n{code}"
    );
}
