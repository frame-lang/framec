//! Go `@@[persist]` codegen fixes.
//!
//! #171 — the persist methods use `json.Marshal`/`json.Unmarshal`, but Go has no
//! inline imports and requires `import "encoding/json"` after the `package`
//! clause. Framec now injects it (deduped against the user's own imports).
//!
//! #172 — a persisted parent that owns a persisted child recursed into the child
//! with the raw method names (`s.kid.save_state()`), but Go exports those with
//! leading-capital names (`Save_state`), so the parent didn't compile. The call
//! now uses the exported casing.

mod common;
use common::compile_source;

const GO_PERSIST: &str = r#"
@@[target("go")]
package main

@@[main]
@@[persist(string)]
@@[save(save_state)]
@@[load(restore_state)]
@@system Probe {
    interface: bump() val(): int
    machine: $S { bump() { @@:self.n = @@:self.n + 1 } val(): int { @@:(@@:self.n) } }
    domain: n: int = 0
}
"#;

/// #171: `import "encoding/json"` is injected after the `package` line.
#[test]
fn go_persist_injects_encoding_json() {
    let code = compile_source(GO_PERSIST, "go");
    assert!(
        code.contains("import \"encoding/json\""),
        "[#171] persist output must import encoding/json:\n{code}"
    );
    // Injected after the package clause (Go requires imports before other decls).
    let pkg = code.find("package main").expect("package clause present");
    let imp = code
        .find("import \"encoding/json\"")
        .expect("import present");
    assert!(imp > pkg, "[#171] import must follow the package clause");
    // And the import precedes the first json use.
    let use_json = code.find("json.").expect("json use present");
    assert!(imp < use_json, "[#171] import must precede json use");
}

/// #171 dedup: when the user already imports encoding/json, framec does not
/// duplicate it (a duplicate import is a Go compile error).
#[test]
fn go_persist_does_not_duplicate_user_json_import() {
    let code = compile_source(
        r#"
@@[target("go")]
package main

import "encoding/json"
var _ = json.Marshal

@@[main]
@@[persist(string)]
@@[save(save_state)]
@@[load(restore_state)]
@@system Probe {
    interface: bump()
    machine: $S { bump() { @@:self.n = @@:self.n + 1 } }
    domain: n: int = 0
}
"#,
        "go",
    );
    assert_eq!(
        code.matches("encoding/json").count(),
        1,
        "[#171] must not duplicate the user's encoding/json import:\n{code}"
    );
}

/// #172: a persisted parent calls its persisted child's save/restore with the
/// Go-exported (leading-capital) names.
#[test]
fn go_nested_persist_uses_exported_child_methods() {
    let code = compile_source(
        r#"
@@[target("go")]
package main

@@[persist(string)]
@@[save(save_state)]
@@[load(restore_state)]
@@system Child {
    interface: bump()
    machine: $S { bump() { @@:self.n = @@:self.n + 1 } }
    domain: n: int = 0
}
@@[main]
@@[persist(string)]
@@[save(save_state)]
@@[load(restore_state)]
@@system Parent {
    interface: bump()
    machine: $S { bump() { @@:self.kid.bump() } }
    domain: kid: Child = @@Child()
}
"#,
        "go",
    );
    // The child's methods are exported as Save_state / Restore_state; the parent
    // must call them with that casing, not the raw lowercase name.
    assert!(
        code.contains("s.kid.Save_state()"),
        "[#172] parent must call the child's exported Save_state:\n{code}"
    );
    assert!(
        code.contains("s.kid.Restore_state("),
        "[#172] parent must call the child's exported Restore_state:\n{code}"
    );
    assert!(
        !code.contains("s.kid.save_state(") && !code.contains("s.kid.restore_state("),
        "[#172] no raw-lowercase child persist calls may remain:\n{code}"
    );
}
