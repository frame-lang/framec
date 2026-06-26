//! Issue #127 / RFC-0052 §4 — attribute affinity for `@@[persist]`.
//!
//! Before this change `@@[persist]` had MODULE affinity: declaring it on
//! ONE system in a multi-system file force-stamped EVERY sibling, so an
//! independent sibling that declared no save/load failed E814. The syntax
//! said "this system" while the behavior said "the module".
//!
//! RFC-0052 §4 fixes this as a strict RELAXATION:
//!   1. Default `@@[persist(blob)]` applies to the NEXT single `@@system`
//!      (consistent with `@@[async]` / `@@[main]` / `@@[create]`). A
//!      system without `@@[persist]` is simply non-persistable.
//!   2. `@@[*persist(blob)]` (star prefix) broadcasts to ALL systems in
//!      the module; legal only at module position. Companions
//!      `@@[*save]` / `@@[*load]` broadcast likewise.
//!   3. E814 relaxes from "if any persists, all must" to "each opts in".
//!   4. New composition E-code **E828**: a persistable system that holds
//!      a non-persistable `@@system` as a domain field is an error (the
//!      parent's save/load can't recurse). E829 guards the `*` module
//!      position.

mod common;
use common::{compile_expect_error, compile_source};

// ─── (a) Per-system persist: A persists, sibling B has none → COMPILES ──
//
// Today this was E814 (B force-stamped persistable, lacks save/load).
// Under the new default it must compile: B is simply non-persistable.

const PARTIAL_PERSIST_SRC: &str = r#"
@@[target("python_3")]

@@[persist(str)]
@@[save(snapshot)]
@@[load(restore)]
@@[main]
@@system Persisted {
    interface:
        bump()

    machine:
        $S {
            bump() { @@:self.n = @@:self.n + 1 }
        }

    domain:
        n: int = 0
}

@@system Independent {
    interface:
        ping()

    machine:
        $T {
            ping() { @@:self.hits = @@:self.hits + 1 }
        }

    domain:
        hits: int = 0
}
"#;

#[test]
fn partial_persist_multi_system_compiles() {
    // Previously E814 on `Independent`; now a clean relaxation.
    let code = compile_source(PARTIAL_PERSIST_SRC, "python_3");
    assert!(
        !code.is_empty(),
        "partial-persist multi-system file must compile"
    );
    // Only the persisting system gets save/load methods.
    assert!(
        code.contains("snapshot"),
        "the persisting system must emit its save op; got:\n{code}"
    );
}

// ─── (b) `@@[*persist]` at top broadcasts to ALL systems ────────────────

const BROADCAST_PERSIST_SRC: &str = r#"
@@[target("python_3")]
@@[*persist(str)]
@@[*save(snapshot)]
@@[*load(restore)]

@@[main]
@@system Alpha {
    interface:
        a()

    machine:
        $A {
            a() { @@:self.x = @@:self.x + 1 }
        }

    domain:
        x: int = 0
}

@@system Beta {
    interface:
        b()

    machine:
        $B {
            b() { @@:self.y = @@:self.y + 1 }
        }

    domain:
        y: int = 0
}
"#;

#[test]
fn broadcast_persist_applies_to_all_systems() {
    // Neither system declares its own `@@[persist]`/save/load, yet both
    // are persistable via `@@[*persist]` — and neither trips E814.
    let code = compile_source(BROADCAST_PERSIST_SRC, "python_3");
    // The save op is named `snapshot` and must appear for BOTH systems —
    // i.e. at least twice.
    let occurrences = code.matches("snapshot").count();
    assert!(
        occurrences >= 2,
        "broadcast persist must emit save op on every system (>=2 `snapshot`); got {occurrences}:\n{code}"
    );
}

// ─── (c) `@@[*persist]` before a non-first system → error (E829) ────────

const MISPLACED_BROADCAST_SRC: &str = r#"
@@[target("python_3")]

@@[main]
@@system First {
    interface:
        a()

    machine:
        $A {
            a() { }
        }
}

@@[*persist(str)]
@@system Second {
    interface:
        b()

    machine:
        $B {
            b() { }
        }
}
"#;

#[test]
fn broadcast_persist_must_be_at_module_position() {
    let err = compile_expect_error(MISPLACED_BROADCAST_SRC, "python_3");
    assert!(
        err.contains("E829"),
        "misplaced `@@[*persist]` must be E829; got:\n{err}"
    );
}

// ─── (d) Composition E828: persistable Parent holds non-persistable Child

const COMPOSITION_GAP_SRC: &str = r#"
@@[target("python_3")]

@@[persist(str)]
@@[save(snapshot)]
@@[load(restore)]
@@[main]
@@system Parent {
    interface:
        go()

    machine:
        $P {
            go() { }
        }

    domain:
        child: Child = Child()
}

@@system Child {
    interface:
        tick()

    machine:
        $C {
            tick() { }
        }
}
"#;

#[test]
fn persistable_parent_holding_nonpersistable_child_is_e828() {
    let err = compile_expect_error(COMPOSITION_GAP_SRC, "python_3");
    assert!(
        err.contains("E828"),
        "persistable Parent holding non-persistable Child must be E828; got:\n{err}"
    );
    // The message must name both systems for actionability.
    assert!(
        err.contains("Parent") && err.contains("Child"),
        "E828 must name both Parent and Child; got:\n{err}"
    );
}

// ─── (e) Regression: Parent + Child both persist, Parent holds Child ────

const COMPOSITION_OK_SRC: &str = r#"
@@[target("python_3")]

@@[persist(str)]
@@[save(snapshot)]
@@[load(restore)]
@@[main]
@@system Parent {
    interface:
        go()

    machine:
        $P {
            go() { }
        }

    domain:
        child: Child = Child()
}

@@[persist(str)]
@@[save(snapshot)]
@@[load(restore)]
@@system Child {
    interface:
        tick()

    machine:
        $C {
            tick() { }
        }
}
"#;

#[test]
fn persistable_parent_and_child_compose_cleanly() {
    // Both persistable → no E828, no E814; the parent's save/load can
    // recurse into the child.
    let code = compile_source(COMPOSITION_OK_SRC, "python_3");
    assert!(
        !code.is_empty(),
        "Parent+Child both persistable must compile"
    );
}

// ─── Broadcast graph: `@@[*persist]` covers the composition too ─────────

const COMPOSITION_BROADCAST_SRC: &str = r#"
@@[target("python_3")]
@@[*persist(str)]
@@[*save(snapshot)]
@@[*load(restore)]

@@[main]
@@system Parent {
    interface:
        go()

    machine:
        $P {
            go() { }
        }

    domain:
        child: Child = Child()
}

@@system Child {
    interface:
        tick()

    machine:
        $C {
            tick() { }
        }
}
"#;

#[test]
fn broadcast_persist_covers_the_whole_graph() {
    // `@@[*persist]` is the one-liner the E828 message points at — it
    // makes the held Child persistable, so composition is clean.
    let code = compile_source(COMPOSITION_BROADCAST_SRC, "python_3");
    assert!(
        !code.is_empty(),
        "broadcast persist must persist the whole composition graph"
    );
}
