//! Issue #182 — the reflective JS/TS persist registry is seeded by walking the
//! fresh restore target's live object graph, which misses a persisted user type
//! not reachable from that graph (e.g. a field defaulting to `null` that held a
//! user object at save) → a false E750. The fix also seeds the registry from
//! DECLARED field / state-var / arg types (guarded by a runtime `typeof` so it
//! stays type-ignorant and skips primitives), restricted to PascalCase class-
//! convention identifiers. These tests pin that seed.

mod common;
use common::compile_source;

const SRC: &str = r#"
class Widget {
    constructor(v) { this.v = v; }
}
@@[persist(str)]
@@[save(save_state)]
@@[load(restore_state)]
@@system Holder {
    interface:
        setw(v)
    machine:
        $A { setw(v) {} }
    domain:
        w: Widget = null
}
"#;

#[test]
fn declared_user_type_seeds_the_registry() {
    // A PascalCase declared type is pre-seeded (guarded by runtime typeof), so it
    // resolves on restore even when absent from the fresh instance's live graph.
    for target in ["javascript", "typescript"] {
        let c = compile_source(SRC, target);
        assert!(
            c.contains(r#"if (typeof Widget === "function") _reg.set("Widget", Widget)"#),
            "[#182/{target}] declared user type Widget not seeded into the persist registry\n{c}"
        );
    }
}

#[test]
fn primitive_field_type_is_not_seeded() {
    // A lowercase primitive type must NOT emit a (dead) registry seed.
    let src = SRC.replace("w: Widget = null", "n: number = 0");
    let c = compile_source(&src, "javascript");
    assert!(
        !c.contains(r#"_reg.set("number""#),
        "[#182] primitive `number` must not be seeded\n{c}"
    );
}

/// Lua analogue (#182 twin). Lua has no name→metatable mapping for the separate-
/// metatable idiom, but the **class-is-metatable** convention (the declared name
/// resolves to a table carrying `__name`) can be seeded — keyed by `__name`,
/// guarded so a non-table name skips itself.
const LUA_SRC: &str = r#"
local Widget = {}
Widget.__index = Widget
Widget.__name = "Widget"
@@[persist(str)]
@@[save(save_state)]
@@[load(restore_state)]
@@system Holder {
    interface:
        setw(v)
    machine:
        $A { setw(v) {} }
    domain:
        w: Widget = nil
}
"#;

#[test]
fn lua_class_is_metatable_type_is_seeded() {
    let c = compile_source(LUA_SRC, "lua");
    assert!(
        c.contains(
            r#"if type(Widget) == "table" and Widget.__name then _reg[Widget.__name] = Widget end"#
        ),
        "[#182/lua] declared class-is-metatable type Widget not seeded\n{c}"
    );
}

#[test]
fn lua_primitive_field_type_is_not_seeded() {
    let src = LUA_SRC.replace("w: Widget = nil", "n: int = 0");
    let c = compile_source(&src, "lua");
    assert!(
        !c.contains("_reg[int."),
        "[#182/lua] primitive `int` must not be seeded\n{c}"
    );
}
