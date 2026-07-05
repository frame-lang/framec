//! Issue #123 — the constructor factory/bare split must route by node identity,
//! not by scanning rendered text for parameter names.
//!
//! The old per-backend `mentions_param` scan was string-blind: it matched a
//! parameter name anywhere in a rendered constructor line — inside a string
//! literal, or as the assignment *target* when a field name coincided with a
//! param name — and silently dropped that initializer from the bare
//! (`@@!Sys()`) constructor. `constructor.rs` now tags each statement
//! structurally (`FactoryOnlyBlock` / `BareCtorBlock`), so the split is exact.
//!
//! These tests pin the two failure shapes on backends with an unambiguous
//! bare-constructor marker.

mod common;
use common::compile_source;

// param `rate` collides with domain field `rate`; `note` is a plain literal
// init whose *string* contains the word "rate"; `label` is an unrelated literal.
const SRC: &str = r#"
@@[target("LANG")]
@@[main]
@@system Probe(rate: int = 3) {
    interface: tick()
    machine: $S { tick() { @@:self.count = @@:self.count + @@:self.rate; } }
    domain:
        rate: int = 3
        count: int = 0
        note: LANGSTR = "rate limited"
        label: int = 7
}
"#;

/// C: the bare allocator is `Probe_new(void)`. It must initialize every
/// literal-default field — including `note` (whose string mentions the param
/// `rate`) and `rate` itself (field name == param name) — none of which may be
/// dropped by a param-name text match.
#[test]
fn c_bare_new_keeps_literal_inits() {
    let code = compile_source(&SRC.replace("LANGSTR", "char*").replace("LANG", "c"), "c");
    let new_start = code
        .find("Probe* Probe_new(void)")
        .expect("bare Probe_new present");
    let new_end = new_start + code[new_start..].find("\n}").expect("Probe_new body");
    let bare = &code[new_start..new_end];
    assert!(
        bare.contains("self->note = \"rate limited\""),
        "[#123] bare Probe_new dropped a string-literal init mentioning a param:\n{bare}"
    );
    assert!(
        bare.contains("self->rate = 3"),
        "[#123] bare Probe_new dropped a field init whose name equals a param:\n{bare}"
    );
    assert!(
        bare.contains("self->label = 7"),
        "[#123] bare Probe_new dropped an unrelated literal init:\n{bare}"
    );
    // The param override belongs to the factory, never the bare allocator.
    assert!(
        !bare.contains("self->rate = rate"),
        "[#123] param override leaked into the bare allocator:\n{bare}"
    );
    // …and the factory does apply it.
    let create_start = code.find("Probe* Probe_create(").expect("factory present");
    assert!(
        code[create_start..].contains("self->rate = rate"),
        "[#123] factory must apply the param override"
    );
}

/// Python: the bare constructor is `def __init__(self):` (no params); the
/// factory is the `_create` classmethod.
#[test]
fn python_bare_init_keeps_literal_inits() {
    let code = compile_source(
        &SRC.replace("LANGSTR", "str").replace("LANG", "python"),
        "python",
    );
    let init_start = code
        .find("def __init__(self):")
        .expect("bare __init__ present");
    let init_end = init_start
        + code[init_start..]
            .find("\n\n")
            .unwrap_or(code.len() - init_start);
    let bare = &code[init_start..init_end];
    assert!(
        bare.contains("self.note = \"rate limited\""),
        "[#123] bare __init__ dropped a string-literal init mentioning a param:\n{bare}"
    );
    assert!(
        bare.contains("self.rate = 3") && bare.contains("self.label = 7"),
        "[#123] bare __init__ dropped a literal init:\n{bare}"
    );
    assert!(
        !bare.contains("self.rate = rate"),
        "[#123] param override leaked into bare __init__:\n{bare}"
    );
}

/// Java: literal-default domain fields are emitted as field declarations, but
/// the param override (`this.rate = rate`) must live only in the `__create`
/// factory, never the bare `public Probe()` constructor.
#[test]
fn java_param_override_is_factory_only() {
    let code = compile_source(
        &SRC.replace("LANGSTR", "String").replace("LANG", "java"),
        "java",
    );
    let ctor_start = code.find("public Probe()").expect("bare ctor present");
    let ctor_end = ctor_start + code[ctor_start..].find("\n    }").expect("bare ctor body");
    let bare = &code[ctor_start..ctor_end];
    assert!(
        !bare.contains("this.rate = rate"),
        "[#123] param override leaked into the bare Java ctor:\n{bare}"
    );
    let create = code.find("__create(").expect("factory present");
    assert!(
        code[create..].contains("rate = rate"),
        "[#123] Java factory must apply the param override"
    );
}
