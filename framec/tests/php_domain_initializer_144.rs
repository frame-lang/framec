//! Issue #144 — a non-constant PHP domain-field initializer (`new X()`, a
//! call, or `@@<System>()` instantiation) must be lowered into the constructor
//! body, NOT emitted as a class-property default.
//!
//! PHP property defaults admit only *constant expressions*; `public $p = new
//! Pt(3);` is rejected at parse time ("New expressions are not supported in
//! this context") while framec itself exits 0. The field-emission path
//! (`system_codegen/fields.rs`) previously stripped only `@@`-tagged system
//! instantiations, so a native `new`/call slipped through as an invalid
//! property default. The fix uses one predicate — `php_init_needs_constructor`
//! — symmetrically in the strip decision (fields.rs) and the constructor-body
//! emission (constructor.rs). Constant scalars/strings/arrays stay inline.
//!
//! Scope note: PHP-only. Every OO backend already lowers param-referencing
//! initializers into the constructor; PHP additionally needs it for any
//! non-constant expression because of the property-default constraint.

mod common;
use common::{compile_source, find_tool};
use std::process::Command;

/// The issue repro plus a constant scalar (`n = 5`) that must stay a property
/// default, a native `new Pt(3)`, and an `@@Sensor()` composition.
const REPRO: &str = r#"
@@[target("php")]
class Pt { public $x; function __construct($x) { $this->x = $x; } }
@@[main]
@@system Sensor {
    interface:
        bump()
        read(): int
    machine:
        $S {
            bump() { $this->val = $this->val + 1; }
            read(): int { @@:($this->val) }
        }
    domain:
        val = 0
}
@@system G {
    interface:
        tick()
        total(): int
        pt(): int
    machine:
        $S {
            tick() { $this->seen = $this->seen + 1; $this->sensor->bump(); }
            total(): int { @@:($this->sensor->read()) }
            pt(): int { @@:($this->box->x) }
        }
    domain:
        n = 5
        seen = 0
        sensor = @@Sensor()
        box = new Pt(42)
}
"#;

/// Slice the generated `class G { … }` body (properties + `__construct`).
fn class_g(code: &str) -> String {
    let start = code
        .find("class G {")
        .expect("generated PHP must contain `class G {`");
    // Balance braces to find the class end.
    let bytes = &code.as_bytes()[start..];
    let mut depth = 0i32;
    let mut end = start;
    for (i, &b) in bytes.iter().enumerate() {
        match b {
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    end = start + i + 1;
                    break;
                }
            }
            _ => {}
        }
    }
    code[start..end].to_string()
}

#[test]
fn constant_scalar_stays_property_default() {
    let g = class_g(&compile_source(REPRO, "php"));
    assert!(
        g.contains("public $n = 5;"),
        "[#144] constant scalar `n = 5` must stay an inline property default:\n{g}"
    );
    assert!(
        g.contains("public $seen = 0;"),
        "[#144] constant scalar `seen = 0` must stay an inline property default:\n{g}"
    );
    // A constant default is NOT re-assigned in the constructor.
    assert!(
        !g.contains("$this->n = 5;"),
        "[#144] constant scalar must not be redundantly assigned in the ctor:\n{g}"
    );
}

#[test]
fn non_constant_initializer_moves_to_constructor() {
    let g = class_g(&compile_source(REPRO, "php"));
    // Property declarations carry NO default for the non-const fields.
    assert!(
        g.contains("public $box;"),
        "[#144] `new Pt(42)` field must be declared without a default:\n{g}"
    );
    assert!(
        g.contains("public $sensor;"),
        "[#144] `@@Sensor()` field must be declared without a default:\n{g}"
    );
    // Neither `new` nor the tagged instantiation may appear in a property default.
    assert!(
        !g.contains("public $box = new"),
        "[#144] `new` must never appear in a PHP property default:\n{g}"
    );
    // The assignments live in the constructor body instead.
    assert!(
        g.contains("$this->box = new Pt(42);"),
        "[#144] `new Pt(42)` must be assigned in the constructor:\n{g}"
    );
    assert!(
        g.contains("$this->sensor = Sensor::_create();"),
        "[#144] `@@Sensor()` must lower to a constructor-body factory call:\n{g}"
    );
}

/// `php -l` (lint) must accept the generated file. Skipped (not failed) when
/// `php` is absent, mirroring the snapshot suites.
#[test]
fn generated_php_lints_clean() {
    let bin = match find_tool("php") {
        Some(p) => p,
        None => {
            eprintln!("#144 lint skipped: `php` not on PATH");
            return;
        }
    };
    let code = compile_source(REPRO, "php");
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("repro144.php");
    std::fs::write(&path, &code).expect("write temp");
    let out = Command::new(&bin)
        .arg("-l")
        .arg(&path)
        .output()
        .unwrap_or_else(|e| panic!("spawn php: {e}"));
    assert!(
        out.status.success(),
        "[#144] generated PHP rejected by `php -l`:\n--- stderr ---\n{}\n--- stdout ---\n{}\n--- source ---\n{}",
        String::from_utf8_lossy(&out.stderr),
        String::from_utf8_lossy(&out.stdout),
        code
    );
}

/// Full runtime: construct `G`, drive it, and confirm the constructor-lowered
/// `new`/`@@` fields are live objects. Skipped when `php` is absent.
#[test]
fn runtime_uses_constructor_lowered_fields() {
    let bin = match find_tool("php") {
        Some(p) => p,
        None => {
            eprintln!("#144 run skipped: `php` not on PATH");
            return;
        }
    };
    let code = compile_source(REPRO, "php");
    let dir = tempfile::tempdir().expect("tempdir");
    let src = dir.path().join("repro144.php");
    std::fs::write(&src, &code).expect("write temp");
    let driver = dir.path().join("drive144.php");
    std::fs::write(
        &driver,
        format!(
            "<?php\nrequire '{}';\n$g = G::_create();\n$g->tick(); $g->tick();\necho $g->total() . ' ' . $g->pt();\n",
            src.display()
        ),
    )
    .expect("write driver");
    let out = Command::new(&bin)
        .arg(&driver)
        .output()
        .unwrap_or_else(|e| panic!("spawn php: {e}"));
    assert!(
        out.status.success(),
        "[#144] php run failed:\n--- stderr ---\n{}\n--- source ---\n{}",
        String::from_utf8_lossy(&out.stderr),
        code
    );
    let got = String::from_utf8_lossy(&out.stdout);
    assert_eq!(
        got.trim(),
        "2 42",
        "[#144] expected `total=2 pt=42` from constructor-lowered fields, got: {got:?}"
    );
}
