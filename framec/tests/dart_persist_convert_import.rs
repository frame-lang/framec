//! Dart `@@[persist]` must import `dart:convert`.
//!
//! The Dart persist codegen emits `jsonEncode`/`jsonDecode` (top-level functions
//! from `dart:convert`), but Dart has no inline imports and requires import
//! directives before all declarations. Framec now injects `import
//! 'dart:convert';` at the top of the file (deduped against the user's own),
//! mirroring the Go `encoding/json` fix (#171) and the Ruby `require 'json'`
//! fix (#166). Part of the persist-import hygiene sweep.

mod common;
use common::compile_source;

const DART_PERSIST: &str = r#"
@@[target("dart")]
@@[main]
@@[persist(string)]
@@[save(save_state)]
@@[load(restore_state)]
@@system Probe {
    interface: ping()
    machine: $S { ping() {} }
    domain: n: int = 0
}
"#;

#[test]
fn dart_persist_imports_convert() {
    let code = compile_source(DART_PERSIST, "dart");
    assert!(
        code.contains("import 'dart:convert';"),
        "[hygiene] dart persist must import dart:convert:\n{code}"
    );
    // Injected before the first declaration and before the first json use.
    let imp = code.find("import 'dart:convert';").expect("import present");
    let first_class = code.find("class ").expect("a class is emitted");
    assert!(
        imp < first_class,
        "[hygiene] import must precede declarations"
    );
    let use_json = code.find("jsonEncode").expect("jsonEncode used");
    assert!(imp < use_json, "[hygiene] import must precede json use");
}

#[test]
fn dart_persist_does_not_duplicate_user_convert_import() {
    let code = compile_source(
        r#"
@@[target("dart")]
import 'dart:convert';

@@[main]
@@[persist(string)]
@@[save(save_state)]
@@[load(restore_state)]
@@system Probe {
    interface: ping()
    machine: $S { ping() {} }
    domain: n: int = 0
}
"#,
        "dart",
    );
    assert_eq!(
        code.matches("dart:convert").count(),
        1,
        "[hygiene] must not duplicate the user's dart:convert import:\n{code}"
    );
}
