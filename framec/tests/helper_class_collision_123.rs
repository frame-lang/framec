//! Issue #123 — framework runtime-helper classes must be identified by a
//! structural flag, not a class-name suffix probe.
//!
//! Backends used to detect the `{Sys}FrameEvent` / `{Sys}FrameContext` /
//! `{Sys}Compartment` helper classes with
//! `class_name.ends_with("Compartment")` (etc.) in order to skip the system
//! `new`/`_create` constructor split. That collided with a user system whose
//! name ends in one of those suffixes (e.g. `TaskCompartment`): it was
//! misclassified as a helper and silently lost its factory. The classification
//! now rides an `is_framework_helper` flag on `CodegenNode::Class`, set only at
//! the three helper construction sites in `runtime.rs`.

mod common;
use common::compile_source;

const COLLIDING_SYSTEM: &str = r#"
@@[target("LANG")]
@@[main]
@@system TaskCompartment {
    interface: tick()
    machine: $S { tick() {} }
    domain: count: int = 0
}
"#;

/// Python: a system gets a `_create` factory classmethod; a framework helper
/// class does not. A user system named `TaskCompartment` must still get it.
#[test]
fn python_system_named_like_helper_keeps_factory() {
    let code = compile_source(&COLLIDING_SYSTEM.replace("LANG", "python"), "python");
    assert!(
        code.contains("def _create(cls"),
        "[#123] a `*Compartment`-named system was misclassified as a framework \
         helper and lost its factory:\n{code}"
    );
}

/// C: a system gets the `TaskCompartment_new` / `TaskCompartment_create` split;
/// a helper gets neither. Confirms the flag, not the suffix, drives the split.
#[test]
fn c_system_named_like_helper_keeps_factory() {
    let code = compile_source(&COLLIDING_SYSTEM.replace("LANG", "c"), "c");
    assert!(
        code.contains("TaskCompartment* TaskCompartment_create("),
        "[#123] a `*Compartment`-named system lost its C factory:\n{code}"
    );
}

/// The genuine framework helper (`{Sys}Compartment`) for a normally-named
/// system must still be recognized as a helper — it has no factory split.
#[test]
fn genuine_helper_compartment_has_no_factory() {
    let code = compile_source(
        r#"
@@[target("python")]
@@[main]
@@system Gizmo {
    interface: tick()
    machine: $S { tick() {} }
    domain: count: int = 0
}
"#,
        "python",
    );
    // The GizmoCompartment helper class must not carry a `_create` factory…
    let comp_start = code
        .find("class GizmoCompartment")
        .expect("compartment helper present");
    let comp_end = comp_start
        + code[comp_start..]
            .find("\nclass ")
            .unwrap_or(code.len() - comp_start);
    let helper = &code[comp_start..comp_end];
    assert!(
        !helper.contains("def _create("),
        "[#123] the framework compartment helper must not get a system factory:\n{helper}"
    );
    // …while the Gizmo system class does.
    assert!(
        code.contains("def _create(cls"),
        "[#123] system factory missing"
    );
}
