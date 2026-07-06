//! Issue #166 — Ruby `@@[persist]` must emit `require 'json'`.
//!
//! The Ruby persist codegen uses `JSON.generate`/`JSON.parse`, but Ruby does
//! not auto-load the stdlib json module, so the generated module raised
//! `NameError: uninitialized constant JSON` on the first `save_state` unless the
//! host had already required it. Python emits `import json` at each persist
//! site; Ruby now mirrors that with `require 'json'` so the module is
//! self-contained.

mod common;
use common::compile_source;

const RUBY_PERSIST: &str = r#"
@@[target("ruby")]
@@[main]
@@[persist(string)]
@@[save(save_state)]
@@[load(restore_state)]
@@system Probe {
    interface: bump() val()
    machine: $S { bump() { self.n = self.n + 1 } val() { @@:(self.n) } }
    domain: n = 0
}
"#;

#[test]
fn ruby_persist_requires_json() {
    let code = compile_source(RUBY_PERSIST, "ruby");
    // Both persist methods use JSON, so both must require it.
    assert_eq!(
        code.matches("require 'json'").count(),
        2,
        "[#166] both save_state and restore_state must `require 'json'`:\n{code}"
    );
    // Sanity: the require precedes the first JSON use in the file.
    let req = code.find("require 'json'").expect("require present");
    let json = code.find("JSON.").expect("JSON use present");
    assert!(
        req < json,
        "[#166] `require 'json'` must precede the first JSON call"
    );
}
