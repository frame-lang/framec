//! RFC-0055 R1 (E752): in a **persisted** system, every persisted field must
//! declare a type on the Regime A/C targets (static + Lua/GDScript), because the
//! declared type is the type-identity source for faithful restore and for a
//! complete drift fingerprint. Regime B (Python/JS/Ruby/PHP/TS) supplies the type
//! from a runtime tag, so a declared type is RECOMMENDED there, not required.
//! Scoped to not overlap the codegen rules E605 (domain) / E606 (args); the
//! genuinely-new coverage is state variables (every A/C target) and the Dart/Lua/
//! GDScript domain+arg gaps. Persist-gated and `@@[no_persist]`-exempt.

mod common;
use common::{compile_expect_error, compile_source};

const PERSISTED_UNTYPED_STATE_VAR: &str = r#"
@@[persist(string)]
@@[save(save_state)]
@@[load(restore_state)]
@@system S {
    interface:
        go()
    machine:
        $A {
            $.n = 0
            go() {}
        }
}
"#;

#[test]
fn untyped_state_var_rejected_on_regime_ac() {
    // State vars had NO type rule before; this is the core new coverage.
    for target in ["lua", "gdscript", "dart"] {
        let e = compile_expect_error(PERSISTED_UNTYPED_STATE_VAR, target);
        assert!(
            e.contains("E752"),
            "[E752/{target}] an untyped persisted state variable must be rejected\n{e}"
        );
    }
}

#[test]
fn untyped_state_var_allowed_on_regime_b() {
    // Regime B: the runtime tag supplies the type; a declared type is RECOMMENDED,
    // not required — so this compiles.
    for target in ["python_3", "javascript", "ruby"] {
        let _ = compile_source(PERSISTED_UNTYPED_STATE_VAR, target);
    }
}

#[test]
fn untyped_state_var_allowed_when_not_persisted() {
    // R1 governs *persisted* fields; a non-persisted system is unaffected (Lua has
    // no other state-var type rule).
    let src = r#"
@@system S {
    interface:
        go()
    machine:
        $A {
            $.n = 0
            go() {}
        }
}
"#;
    let _ = compile_source(src, "lua");
}

#[test]
fn untyped_domain_field_rejected_on_regime_c() {
    // Domain fields on Lua/GDScript are the E605 Regime-C gap this fills.
    let src = r#"
@@[persist(string)]
@@[save(save_state)]
@@[load(restore_state)]
@@system S {
    interface:
        go()
    machine:
        $A { go() {} }
    domain:
        marker = 1
}
"#;
    let e = compile_expect_error(src, "gdscript");
    assert!(
        e.contains("E752"),
        "[E752/gdscript] an untyped persisted domain field must be rejected\n{e}"
    );
}

#[test]
fn no_persist_domain_field_is_exempt() {
    // A @@[no_persist] field is not persisted, so it is exempt from R1.
    let src = r#"
@@[persist(string)]
@@[save(save_state)]
@@[load(restore_state)]
@@system S {
    interface:
        go()
    machine:
        $A {
            $.n: int = 0
            go() {}
        }
    domain:
        @@[no_persist]
        scratch = 1
}
"#;
    let _ = compile_source(src, "lua");
}
