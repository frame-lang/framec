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
fn untyped_state_var_rejected_when_target_cannot_enumerate_module_classes() {
    // R1 is MUST wherever the target can't enumerate its own module's classes at
    // restore: static targets, Lua/GDScript, AND JS/TS (ES modules don't expose
    // top-level class decls — #182). State vars had NO type rule before, so this
    // is the core new coverage.
    for target in ["lua", "gdscript", "dart", "javascript", "typescript"] {
        let e = compile_expect_error(PERSISTED_UNTYPED_STATE_VAR, target);
        assert!(
            e.contains("E752"),
            "[E752/{target}] an untyped persisted state variable must be rejected\n{e}"
        );
    }
}

#[test]
fn untyped_state_var_allowed_on_module_enumerating_targets() {
    // Python/Ruby/PHP enumerate their module's own classes at restore (e.g. Python
    // walks `vars(_mod)`), so a declared type is genuinely optional — RECOMMENDED,
    // not required — and this compiles. (JS/TS are NOT in this set: see #182.)
    for target in ["python_3", "ruby", "php"] {
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
fn untyped_domain_field_rejected_on_js_ts_issue_182() {
    // #182: on JS/TS an untyped persisted field holding a user object is falsely
    // refused at *restore* (E750) because the closed-world registry can't be seeded
    // for a type framec never saw — ES modules can't be enumerated. R1 turns that
    // runtime surprise into a compile-time E752 that forces the (working) typed path.
    let src = r#"
@@[persist(str)]
@@[save(save_state)]
@@[load(restore_state)]
@@system Holder {
    interface:
        setw(v)
    machine:
        $A { setw(v) {} }
    domain:
        w = null
}
"#;
    // JavaScript has no prior domain-type rule (E605 skips it), so E752 is the new
    // gate that closes the #182 gap.
    let ejs = compile_expect_error(src, "javascript");
    assert!(
        ejs.contains("E752"),
        "[E752/javascript] #182: an untyped persisted domain field must be rejected\n{ejs}"
    );
    // TypeScript untyped domain fields were already rejected by E605, so the #182
    // repro could never compile there — the field kinds E752 newly adds for TS are
    // state vars and args, covered by the tests above.
    let ets = compile_expect_error(src, "typescript");
    assert!(
        ets.contains("E605") || ets.contains("E752"),
        "[TS] #182: an untyped persisted domain field must be rejected (E605 or E752)\n{ets}"
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
