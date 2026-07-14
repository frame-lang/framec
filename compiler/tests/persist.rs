//! **`@@[persist]` — faithful round-trip, and #233 made impossible.**
//!
//! RFC-0053 ("Faithful persistence", ACCEPTED) requires a save/restore that reproduces
//! the system's state — domain values AND live control state — indistinguishably.
//!
//! Its *foundational* disambiguation requirement is the one the OLD compiler violated
//! (#233): a user dict carrying the reserved type marker as ordinary data was silently
//! mis-restored as a typed instance, because the old design stored its tag as an INLINE
//! key in the user's own namespace (serde "internally tagged").
//!
//! The rebuild uses **out-of-band framing**: `{"@f:t": "Point", "@f:v": {...}}`, user
//! payload confined to `@f:v`. The reviver reads the type ONLY from the envelope's tag
//! slot, never from a user key — so the collision is not made unlikely, it is made
//! **structurally impossible**. Every claim below is proven by RUNNING Python.

use frame_compiler::resolve::resolve;
use frame_compiler::scan::{literals::Target, segment};
use frame_compiler::text::emit::{driver, python::Python};
use frame_compiler::Source;
use std::process::Command;

const SPEC: &str = r#"class Point:
    def __init__(self, x=0, y=0):
        self.x = x
        self.y = y

@@[persist]
@@system Holder {
    interface:
        go()
    machine:
        $A { go() { } }
    domain:
        pt: Point = None
        n: int = 0
        data: dict = None
}
"#;

fn emit_python() -> String {
    let src = Source::new("t.frm", SPEC.as_bytes().to_vec()).unwrap();
    let ast = segment(&src, Target::Python3).unwrap();
    let (syms, _) = resolve(&ast);
    driver::emit(&src, &ast, &syms, &Python)
}

/// Run a Python driver appended to the generated code; return stdout.
fn run(driver_py: &str, dir: &str) -> String {
    if Command::new("python3").arg("--version").output().is_err() {
        return "SKIP".into();
    }
    let d = std::env::temp_dir().join(dir);
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    let f = d.join("m.py");
    std::fs::write(&f, format!("{}\n{driver_py}", emit_python())).unwrap();
    let o = Command::new("python3").arg(&f).output().unwrap();
    assert!(
        o.status.success(),
        "python crashed:\n{}",
        String::from_utf8_lossy(&o.stderr)
    );
    String::from_utf8_lossy(&o.stdout).into_owned()
}

/// A user type and a scalar round-trip faithfully — the base RFC-0053 contract.
#[test]
fn a_user_type_round_trips_faithfully() {
    let out = run(
        r#"
h = Holder(); h.pt = Point(3, 4); h.n = 7
h2 = Holder(); h2.restore(h.snapshot())
print("pt", isinstance(h2.pt, Point) and h2.pt.x == 3 and h2.pt.y == 4)
print("n", h2.n == 7)
"#,
        "persist_faithful",
    );
    if out == "SKIP" {
        eprintln!("SKIPPED python3 — verifies nothing.");
        return;
    }
    assert_eq!(out.lines().collect::<Vec<_>>(), ["pt True", "n True"]);
}

/// **#233 — a user dict carrying the marker as data comes back a DICT.**
#[test]
fn a_user_dict_carrying_the_marker_is_not_reconstructed() {
    let out = run(
        r#"
h = Holder()
h.data = {"@f:t": "Point", "x": 99, "y": 99}   # the user's OWN data
h2 = Holder(); h2.restore(h.snapshot())
print("is_dict", isinstance(h2.data, dict))
print("preserved", h2.data == {"@f:t": "Point", "x": 99, "y": 99})
"#,
        "persist_233",
    );
    if out == "SKIP" {
        return;
    }
    assert_eq!(
        out.lines().collect::<Vec<_>>(),
        ["is_dict True", "preserved True"],
        "a user dict with the reserved key must round-trip as a dict, NOT a typed instance (#233)"
    );
}

/// **The adversarial case: a user dict whose keys are EXACTLY the envelope slots.**
/// This one caught a hole in the first design (the escaped dict was re-read as an
/// envelope one level deeper). It must come back a dict.
#[test]
fn a_user_dict_that_mimics_an_envelope_is_still_data() {
    let out = run(
        r#"
h = Holder()
h.data = {"@f:t": "Point", "@f:v": {"x": 0}}   # both slots, as user data
h2 = Holder(); h2.restore(h.snapshot())
print("ok", isinstance(h2.data, dict) and h2.data == {"@f:t": "Point", "@f:v": {"x": 0}})
"#,
        "persist_adversarial",
    );
    if out == "SKIP" {
        return;
    }
    assert_eq!(out.trim(), "ok True");
}

/// A genuine typed value NESTED inside a user dict survives, and so does the dict.
#[test]
fn a_typed_value_nested_in_a_dict_survives() {
    let out = run(
        r#"
h = Holder(); h.data = {"k": Point(1, 2), "plain": 5}
h2 = Holder(); h2.restore(h.snapshot())
print("ok", isinstance(h2.data, dict) and isinstance(h2.data["k"], Point) and h2.data["k"].x == 1 and h2.data["plain"] == 5)
"#,
        "persist_nested",
    );
    if out == "SKIP" {
        return;
    }
    assert_eq!(out.trim(), "ok True");
}

/// **The safety floor (non-deferrable): a blob naming a stdlib type must NOT resolve.**
#[test]
fn restore_will_not_resolve_a_type_the_program_does_not_define() {
    let out = run(
        r#"
import json
h = Holder(); h.n = 1
snap = json.loads(h.snapshot())
snap["data"] = {"@f:t": "OrderedDict", "@f:v": {}}   # forge an envelope naming a stdlib type
try:
    Holder().restore(json.dumps(snap)); print("LEAK")
except RuntimeError as e:
    print("refused", "E750" in str(e))
"#,
        "persist_floor",
    );
    if out == "SKIP" {
        return;
    }
    assert_eq!(out.trim(), "refused True", "must NOT resolve ambient/stdlib types");
}

/// Live control state round-trips too — RFC-0053 requires domain AND control state.
#[test]
fn control_state_round_trips() {
    let out = run(
        r#"
h = Holder()
h.__dict__["_Holder__compartment"].state = "A"
h2 = Holder(); h2.restore(h.snapshot())
print("ok", h2.__dict__["_Holder__compartment"].state == "A")
"#,
        "persist_control",
    );
    if out == "SKIP" {
        return;
    }
    assert_eq!(out.trim(), "ok True");
}
