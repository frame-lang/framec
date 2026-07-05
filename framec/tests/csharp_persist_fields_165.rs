//! Issue #165 — C# `@@[persist]` silently dropped field-based user types.
//!
//! System.Text.Json ignores public *fields* unless `IncludeFields = true`, so a
//! field-based user struct (the idiom C#'s own geometry types use — `Vector2`,
//! `System.Numerics`) serialized as `{}` and, because every restore line is
//! wrapped in `catch {}`, silently came back as `default(T)` — silent data loss
//! behind a valid-looking blob. The fix adds `IncludeFields = true` to the
//! serialize options AND threads a matching options object into every
//! `Deserialize<T>` call (previously optionsless). The flag is strictly widening:
//! property-based types are unaffected.

mod common;
use common::compile_source;

const FIELD_STRUCT_PERSIST: &str = r#"
@@[target("csharp")]

using System;
using System.Collections.Generic;

public struct Vec { public double X, Y; public Vec(double x, double y) { X = x; Y = y; } }

@@[main]
@@[persist(string)]
@@[save(save_state)]
@@[load(restore_state)]
@@system Probe {
    interface:
        set(x: double, y: double)
    machine:
        $S { set(x: double, y: double) { @@:self.v = new Vec(x, y); @@:self.vs.Add(new Vec(x, y)); } }
    domain:
        v: Vec = new Vec(0.0, 0.0)
        vs: List<Vec> = new List<Vec>()
        n: double = 7.5
}
"#;

#[test]
fn serialize_options_include_fields() {
    let code = compile_source(FIELD_STRUCT_PERSIST, "csharp");
    // The single serialize call must use options carrying IncludeFields.
    assert!(
        code.contains("IncludeFields = true")
            && code.contains("JsonSerializer.Serialize(__j, __opts)"),
        "[#165] serialize side must set IncludeFields on __opts:\n{code}"
    );
}

#[test]
fn every_deserialize_passes_include_fields_options() {
    let code = compile_source(FIELD_STRUCT_PERSIST, "csharp");
    // Every line that calls JsonSerializer.Deserialize must pass __opts — an
    // optionsless call is exactly the restore-side leak this fixes.
    let deser_lines: Vec<&str> = code
        .lines()
        .filter(|l| l.contains("JsonSerializer.Deserialize<"))
        .collect();
    assert!(
        deser_lines.len() >= 3,
        "[#165] expected the Vec/List<Vec>/double deserialize lines:\n{code}"
    );
    for l in &deser_lines {
        assert!(
            l.contains(".GetRawText(), __opts)"),
            "[#165] optionsless Deserialize (missing __opts):\n{l}"
        );
    }
}

#[test]
fn restore_opts_omitted_when_no_stj_fields() {
    // When no domain var takes the STJ deserialize path, the restore-side __opts
    // local must not be emitted (it would be an unused variable in the generated
    // C#). A `@@[no_persist]`-only domain is the deterministic case: the persist
    // machinery still emits save/restore for compartments + state stack, but no
    // field is deserialized. (The serialize-side __opts is always emitted and
    // always used by the single Serialize call, so it is unaffected.)
    let code = compile_source(
        r#"
@@[target("csharp")]
@@[main]
@@[persist(string)]
@@[save(save_state)]
@@[load(restore_state)]
@@system Solo {
    interface: ping()
    machine: $S { ping() {} }
    domain:
        @@[no_persist] cache: int = 0
}
"#,
        "csharp",
    );
    assert!(
        !code.contains("JsonSerializer.Deserialize<"),
        "[#165] no field should be STJ-deserialized here:\n{code}"
    );
    // Exactly one __opts (the serialize side); the restore side must omit it.
    let count = code.matches("var __opts =").count();
    assert_eq!(
        count, 1,
        "[#165] expected only the serialize-side __opts (restore must omit its unused one), found {count}:\n{code}"
    );
}
