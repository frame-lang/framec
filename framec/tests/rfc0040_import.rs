//! RFC-0040 — analysis-only `@@import` cross-file resolution.
//!
//! These exercise the cross-file path, so they write two Frame files to a
//! temp dir and compile the importer *with its path* (so `@@import`'s
//! relative resolution works) via `compile_module_with_path`.

use framec::frame_c::compiler::{compile_module_with_path, TargetLanguage};
use std::fs;
use std::path::PathBuf;

/// Fresh, unique temp dir for one test's two-file fixture.
fn scratch(tag: &str) -> PathBuf {
    let mut d = std::env::temp_dir();
    d.push(format!("framec_rfc0040_{}_{}", tag, std::process::id()));
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).unwrap();
    d
}

fn js() -> TargetLanguage {
    TargetLanguage::try_from("javascript").unwrap()
}

/// A composing parent that `@@import`s a child which renamed its persist ops
/// to snake_case (NON-default for JS, whose default is camelCase) must call the
/// child's DECLARED names — resolved by reading the imported source. This is
/// the cross-file half of the composed-child persist fix.
#[test]
fn cross_file_persist_names_resolve_through_import() {
    let dir = scratch("resolve");
    fs::write(
        dir.join("child.fjs"),
        r#"@@[persist(string)]
@@[save(save_state)]
@@[load(restore_state)]
@@system Child {
    interface:
        ping()
    machine:
        $Idle { ping() { self.hits = self.hits + 1 } }
    domain:
        hits: int = 0
}
"#,
    )
    .unwrap();
    let parent = r#"import { Child } from "./child.machine.js"
@@import "./child.fjs"
@@[main]
@@[persist(string)]
@@[save(save_state)]
@@[load(restore_state)]
@@system Parent {
    interface:
        poke()
    machine:
        $Run { poke() { self.child.ping() } }
    domain:
        child: Child = @@Child()
}
"#;
    let parent_path = dir.join("parent.fjs");
    fs::write(&parent_path, parent).unwrap();

    let out = compile_module_with_path(parent, js(), Some(parent_path))
        .expect("parent.fjs should compile");

    assert!(
        out.contains("this.child.save_state()"),
        "parent must call the imported child's DECLARED save name:\n{out}"
    );
    assert!(
        out.contains("this.child.restore_state("),
        "parent must call the imported child's DECLARED load name:\n{out}"
    );
    assert!(
        !out.contains("this.child.saveState()"),
        "parent must not fall back to the JS-default name on a renamed imported child:\n{out}"
    );
    // Emission-excluded: the imported system is analysis-visible but never
    // generated into the importer's output.
    assert!(
        !out.contains("class Child"),
        "imported system must not be emitted into the importer:\n{out}"
    );

    let _ = fs::remove_dir_all(&dir);
}

/// Regression for FRAMEC_BUGS #45: an imported file's own `@@[main]` must not
/// count toward the importer's single-`@@[main]` rule. Both files carry
/// `@@[main]` (each is its own module's primary); the importer must still
/// compile — `@@[main]` is a local/module-primary concern, not a cross-file one.
#[test]
fn imported_main_attr_does_not_trip_e806() {
    let dir = scratch("main");
    fs::write(
        dir.join("lib.fjs"),
        r#"@@[main]
@@system Lib {
    interface:
        go()
    machine:
        $A { go() { } }
}
"#,
    )
    .unwrap();
    let app = r#"import { Lib } from "./lib.machine.js"
@@import "./lib.fjs"
@@[main]
@@system App {
    interface:
        run()
    machine:
        $S { run() { } }
    domain:
        lib: Lib = @@Lib()
}
"#;
    let app_path = dir.join("app.fjs");
    fs::write(&app_path, app).unwrap();

    let out = compile_module_with_path(app, js(), Some(app_path))
        .expect("app.fjs should compile — imported @@[main] must not trip E806");
    // The imported system stays emission-excluded.
    assert!(
        !out.contains("class Lib"),
        "imported system must not be emitted into the importer:\n{out}"
    );

    let _ = fs::remove_dir_all(&dir);
}

/// Negative control: without the `@@import`, framec can't see the child's
/// renamed ops and falls back to the JS default — confirming it's the
/// `@@import` that supplies the cross-file knowledge.
#[test]
fn without_import_falls_back_to_default_name() {
    let dir = scratch("noimport");
    let parent = r#"import { Child } from "./child.machine.js"
@@[main]
@@[persist(string)]
@@[save(save_state)]
@@[load(restore_state)]
@@system Parent {
    interface:
        poke()
    machine:
        $Run { poke() { self.child.ping() } }
    domain:
        child: Child = @@Child()
}
"#;
    let parent_path = dir.join("parent.fjs");
    fs::write(&parent_path, parent).unwrap();

    let out = compile_module_with_path(parent, js(), Some(parent_path))
        .expect("parent.fjs should compile");
    assert!(
        out.contains("this.child.saveState()"),
        "without @@import, the cross-file child uses the JS default name:\n{out}"
    );

    let _ = fs::remove_dir_all(&dir);
}
