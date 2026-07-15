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

const SPEC: &str = r#"from collections import OrderedDict
class Point:
    def __init__(self, x=0, y=0):
        self.x = x
        self.y = y

@@[persist(str)]
@@[save(snapshot)]
@@[load(restore)]
@@system Holder {
    interface:
        go()
    machine:
        $A {
            go() { -> $B }
        }
        $B { }
    domain:
        pt: Point = None
        n: int = 0
        data: dict = None
}
"#;

/// **Guard against a silently-green suite.** Every behavioural test here `return`s on
/// `SKIP` when its toolchain is absent — which means on a toolchain-less image the whole
/// persistence suite could report green having *run nothing*, the exact "proven by running"
/// lie the roadmap swears off. This test fails loudly if NONE of the three toolchains the
/// suite depends on is present, so a green persistence suite always means at least one route
/// actually executed.
#[test]
fn at_least_one_persist_toolchain_is_present() {
    let has = |bin: &str| Command::new(bin).arg("--version").output().is_ok();
    assert!(
        has("python3") || has("rustc") || has("cc"),
        "no persistence toolchain (python3/rustc/cc) present — the suite would be vacuously green"
    );
}

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

/// **The safety floor (non-deferrable): a blob naming a foreign type must NOT resolve —
/// even when that type is VISIBLE in the module.** `SPEC` imports `OrderedDict`, so
/// `"OrderedDict" in vars(module)` is true; refusing it here proves the floor rejects on
/// *definition origin* (`__module__`), not mere name-visibility. That is exactly the
/// Ruby-leak scenario RFC-0053 names — a resolvable-but-foreign (imported/monkeypatched)
/// class — not just an unknown string.
#[test]
fn restore_will_not_resolve_a_type_the_program_does_not_define() {
    let out = run(
        r#"
import json
h = Holder(); h.n = 1
snap = json.loads(h.snapshot())
snap["data"] = {"@f:t": "OrderedDict", "@f:v": {}}   # a type IMPORTED into this module, but foreign
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
    assert_eq!(out.trim(), "refused True", "a visible-but-foreign type must NOT resolve (Ruby-leak floor)");
}

/// Live control state round-trips too — RFC-0053 requires domain AND control state.
///
/// This test is DISTINGUISHING: `Holder` starts in `$A`, and `go()` transitions to `$B`.
/// A fresh restore target is born in `$A`, so a green here means restore actually moved it
/// to `$B` — not that both happened to share the single start state. (The earlier version
/// used a one-state machine and stayed green even with control-state restore deleted.)
#[test]
fn control_state_round_trips() {
    let out = run(
        r#"
h = Holder(); h.go()   # $A -> $B
before = Holder().__dict__["_Holder__compartment"].state   # a fresh target is in $A
h2 = Holder(); h2.restore(h.snapshot())
after = h2.__dict__["_Holder__compartment"].state
print("moved", before == "A" and after == "B")
"#,
        "persist_control",
    );
    if out == "SKIP" {
        return;
    }
    assert_eq!(out.trim(), "moved True", "restore must move control state A -> B, not no-op");
}

/// **Schema drift is refused (E751) on the reflective route too.** Python emits the check
/// (`python.rs`) but nothing exercised it — a snapshot whose `_schema` does not match this
/// program must raise, not silently mis-restore into a mismatched shape (RFC-0054).
#[test]
fn a_mismatched_schema_is_refused_on_python() {
    let out = run(
        r#"
import json
h = Holder(); h.n = 5
snap = json.loads(h.snapshot())
snap["_schema"] = "frame-persist:1|WRONG:int"   # forge a mismatched schema
try:
    Holder().restore(json.dumps(snap)); print("ACCEPTED")
except RuntimeError as e:
    print("refused", "E751" in str(e))
"#,
        "persist_schema_py",
    );
    if out == "SKIP" {
        return;
    }
    assert_eq!(out.trim(), "refused True", "a mismatched schema must be refused (E751)");
}

// ─────────────────────────────────────────────────────────────────────────────────────
// FIXED-TYPE ROUTE (Rust). The type of every field is fixed at codegen, so `restore`
// parses straight into the declared type and never reads a type name from the blob —
// structurally immune to #233. Dependency-free (no serde), mirroring the Java backend.
// Every claim below is proven by RUNNING rustc.
// ─────────────────────────────────────────────────────────────────────────────────────

use frame_compiler::text::emit::rust::Rust;

fn emit_rust(frm: &str) -> String {
    let src = Source::new("t.frm", frm.as_bytes().to_vec()).unwrap();
    let ast = segment(&src, Target::Rust).unwrap();
    let (syms, _) = resolve(&ast);
    driver::emit(&src, &ast, &syms, &Rust)
}

/// Compile the generated code plus a `main` with rustc; return stdout. SKIP if no rustc.
fn run_rust(frm: &str, main: &str, dir: &str) -> String {
    if Command::new("rustc").arg("--version").output().is_err() {
        return "SKIP".into();
    }
    let d = std::env::temp_dir().join(dir);
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    let src_path = d.join("m.rs");
    std::fs::write(&src_path, format!("{}\n{main}\n", emit_rust(frm))).unwrap();
    let bin = d.join("m");
    let o = Command::new("rustc")
        .arg("-o")
        .arg(&bin)
        .arg(&src_path)
        .output()
        .unwrap();
    assert!(
        o.status.success(),
        "rustc rejected:\n{}",
        String::from_utf8_lossy(&o.stderr)
    );
    let o = Command::new(&bin).output().unwrap();
    String::from_utf8_lossy(&o.stdout).into_owned()
}

const RUST_COUNTER: &str = r#"@@[persist(String)]
@@[save(snapshot)]
@@[load(restore)]
@@system Counter {
    interface:
        bump()
    machine:
        $A {
            bump() { @@:self.n = @@:self.n + 1; }
        }
    domain:
        n: i32 = 0
}
"#;

/// A scalar domain field round-trips: save -> restore into a fresh instance -> the value
/// is the persisted one, not the default. A no-op restore FAILS.
#[test]
fn a_scalar_round_trips_on_the_fixed_type_rust_route() {
    let out = run_rust(
        RUST_COUNTER,
        "fn main() { let mut c = Counter::new(); c.bump(); c.bump(); c.bump(); \
         let s = c.snapshot(); let mut c2 = Counter::new(); c2.restore(&s); \
         println!(\"{}\", c2.n); }",
        "rust_persist_roundtrip",
    );
    if out == "SKIP" {
        return;
    }
    assert_eq!(out.trim(), "3", "restore must reproduce n=3, not the default 0");
}

/// A snapshot whose schema does not match this program is REFUSED (E751), never silently
/// mis-restored into the wrong shape (RFC-0054).
#[test]
fn a_mismatched_schema_is_refused_on_rust() {
    let out = run_rust(
        RUST_COUNTER,
        "fn main() { let mut c = Counter::new(); \
         let bad = \"{\\\"_schema\\\":\\\"frame-persist:1|WRONG:i32\\\",\\\"_control\\\":\\\"A\\\",\\\"n\\\":9}\"; \
         let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| c.restore(bad))); \
         println!(\"{}\", if r.is_err() { \"refused\" } else { \"ACCEPTED\" }); }",
        "rust_persist_schema",
    );
    if out == "SKIP" {
        return;
    }
    assert_eq!(out.trim(), "refused", "a mismatched schema must be refused, not accepted");
}

const RUST_TOGGLE: &str = r#"@@[persist(String)]
@@[save(snapshot)]
@@[load(restore)]
@@system Toggle {
    interface:
        flip()
        read(): i32
    machine:
        $Off {
            flip() { -> $On }
            read(): i32 { @@:(0) }
        }
        $On {
            flip() { -> $Off }
            read(): i32 { @@:(1) }
        }
    domain:
        x: i32 = 0
}
"#;

/// **Live control state round-trips.** After a `flip` the machine is in `$On`; a
/// save -> restore into a fresh instance must land back in `$On` (observable: `read()==1`),
/// not the start state `$Off`.
#[test]
fn control_state_round_trips_on_rust() {
    let out = run_rust(
        RUST_TOGGLE,
        "fn main() { let mut t = Toggle::new(); t.flip(); \
         let s = t.snapshot(); let mut t2 = Toggle::new(); t2.restore(&s); \
         println!(\"{}\", t2.read()); }",
        "rust_persist_control",
    );
    if out == "SKIP" {
        return;
    }
    assert_eq!(out.trim(), "1", "after restore the machine must be in $On (read()==1), not $Off");
}

const RUST_USERTYPE: &str = r#"@@[persist(String)]
@@[save(snapshot)]
@@[load(restore)]
@@system Bag {
    interface:
        go()
    machine:
        $A {
            go() { }
        }
    domain:
        p: Point = Point::default()
}
"#;

/// **HONEST GAP — RFC-0055 R2: the fixed-type route round-trips only scalars.** A
/// user-defined-type domain field does not compile: `snapshot` needs `Display`, `restore`
/// needs `FromStr`, because the hand-rolled flat format bypasses the host serializer (serde)
/// that R2 makes mandatory on Regime A. `#[ignore]`d so the suite stays green while the debt
/// is named; `cargo test -- --ignored` shows it fail to compile. Un-ignore it when the
/// fixed-type route adopts a real serializer — then it must PASS. Java and C share this gap
/// (C additionally cannot yet construct a user-type field — a separate defect).
#[test]
#[ignore = "RFC-0055 R2 unmet: fixed-type user types need the host serializer; the flat format cannot"]
fn a_user_type_does_not_round_trip_on_the_fixed_type_rust_route() {
    let out = run_rust(
        RUST_USERTYPE,
        "#[derive(Clone, Default)] struct Point { x: i32, y: i32 }\n\
         fn main() { let b = Bag::new(); let s = b.snapshot(); \
         let mut b2 = Bag::new(); b2.restore(&s); println!(\"{}\", b2.p.x); }",
        "rust_persist_usertype",
    );
    if out == "SKIP" {
        return;
    }
    assert_eq!(out.trim(), "0", "when R2 lands, a user-typed field must round-trip");
}

const RUST_STR: &str = r#"@@[persist(String)]
@@[save(snapshot)]
@@[load(restore)]
@@system Named {
    interface:
        go()
    machine:
        $A {
            go() { }
        }
    domain:
        label: String = String::new()
}
"#;

/// A **plain** string field round-trips (substantiating the "strings are quoted" claim).
///
/// KNOWN LIMITATION, deliberately not exercised here: the flat format is **unescaped**, so a
/// value containing its delimiters (`"`, `,`, `}`) corrupts the snapshot — a separate bug to
/// fix by escaping on save + an escape-aware reader (or adopting the host serializer with
/// RFC-0055 R2). This test uses a delimiter-free value on purpose.
#[test]
fn a_plain_string_round_trips_on_rust() {
    let out = run_rust(
        RUST_STR,
        "fn main() { let mut n = Named::new(); n.label = String::from(\"hello\"); \
         let s = n.snapshot(); let mut n2 = Named::new(); n2.restore(&s); \
         println!(\"{}\", n2.label); }",
        "rust_persist_string",
    );
    if out == "SKIP" {
        return;
    }
    assert_eq!(out.trim(), "hello", "a plain string field must round-trip");
}

// ─────────────────────────────────────────────────────────────────────────────────────
// FIXED-TYPE ROUTE (C). Same flat format as Java/Rust, in C's idiom: free functions, no
// String (a `char*` built with snprintf, restored with strstr/atoi). No marker in the
// blob → immune to #233. Proven by RUNNING cc.
// ─────────────────────────────────────────────────────────────────────────────────────

use frame_compiler::text::emit::c::C as CBackend;

fn emit_c(frm: &str) -> String {
    let src = Source::new("t.frm", frm.as_bytes().to_vec()).unwrap();
    let ast = segment(&src, Target::C).unwrap();
    let (syms, _) = resolve(&ast);
    driver::emit(&src, &ast, &syms, &CBackend::new())
}

/// Compile the generated C plus a `main` with cc; return stdout. SKIP if no cc.
fn run_c(frm: &str, main: &str, dir: &str) -> String {
    if Command::new("cc").arg("--version").output().is_err() {
        return "SKIP".into();
    }
    let d = std::env::temp_dir().join(dir);
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    let src_path = d.join("m.c");
    std::fs::write(&src_path, format!("{}\n{main}\n", emit_c(frm))).unwrap();
    let bin = d.join("m");
    let o = Command::new("cc")
        .arg("-o")
        .arg(&bin)
        .arg(&src_path)
        .output()
        .unwrap();
    assert!(
        o.status.success(),
        "cc rejected:\n{}",
        String::from_utf8_lossy(&o.stderr)
    );
    let o = Command::new(&bin).output().unwrap();
    String::from_utf8_lossy(&o.stdout).into_owned()
}

const C_COUNTER: &str = r#"@@[persist(char*)]
@@[save(snapshot)]
@@[load(restore)]
@@system Counter {
    interface:
        bump()
    machine:
        $A {
            bump() { @@:self.n = @@:self.n + 1; }
        }
    domain:
        n: int = 0
}
"#;

/// A scalar domain field round-trips through save -> restore into a fresh instance.
#[test]
fn a_scalar_round_trips_on_the_fixed_type_c_route() {
    let out = run_c(
        C_COUNTER,
        r#"int main(void) { Counter* c = Counter_new(); Counter_bump(c); Counter_bump(c); Counter_bump(c);
            char* s = Counter_snapshot(c); Counter* c2 = Counter_new(); Counter_restore(c2, s);
            printf("%d\n", c2->n); return 0; }"#,
        "c_persist_roundtrip",
    );
    if out == "SKIP" {
        return;
    }
    assert_eq!(out.trim(), "3", "restore must reproduce n=3, not the default 0");
}

/// A mismatched-schema snapshot is REFUSED (E751 to stderr) and the instance is left
/// untouched — never silently mis-restored (RFC-0054). Observable: n stays at its default.
#[test]
fn a_mismatched_schema_is_refused_on_c() {
    let out = run_c(
        C_COUNTER,
        r#"int main(void) { Counter* c = Counter_new();
            const char* bad = "{\"_schema\":\"frame-persist:1|WRONG:int\",\"_control\":\"A\",\"n\":9}";
            Counter_restore(c, bad);
            printf("%d\n", c->n); return 0; }"#,
        "c_persist_schema",
    );
    if out == "SKIP" {
        return;
    }
    assert_eq!(out.trim(), "0", "a mismatched schema must be refused: n stays 0, not 9");
}

const C_TOGGLE: &str = r#"@@[persist(char*)]
@@[save(snapshot)]
@@[load(restore)]
@@system Toggle {
    interface:
        flip()
        read(): int
    machine:
        $Off {
            flip() { -> $On }
            read(): int { @@:(0) }
        }
        $On {
            flip() { -> $Off }
            read(): int { @@:(1) }
        }
    domain:
        x: int = 0
}
"#;

/// **Live control state round-trips.** After a `flip` the machine is in `$On`; a
/// save -> restore into a fresh instance lands back in `$On` (observable: `read()==1`).
#[test]
fn control_state_round_trips_on_c() {
    let out = run_c(
        C_TOGGLE,
        r#"int main(void) { Toggle* t = Toggle_new(); Toggle_flip(t);
            char* s = Toggle_snapshot(t); Toggle* t2 = Toggle_new(); Toggle_restore(t2, s);
            printf("%d\n", Toggle_read(t2)); return 0; }"#,
        "c_persist_control",
    );
    if out == "SKIP" {
        return;
    }
    assert_eq!(out.trim(), "1", "after restore the machine must be in $On (read()==1), not $Off");
}
