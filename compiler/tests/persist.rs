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
// FIXED-TYPE ROUTE (Rust, Regime A per RFC-0056). The Rust backend delegates value
// marshalling to serde: a user type self-marshals (derives Serialize/Deserialize) and serde
// handles nesting, collections, and string escaping. framec writes no per-type code. Because
// the generated code depends on serde + serde_json, these tests build a Cargo project (not
// bare rustc), sharing one target dir so serde compiles once. Proven by RUNNING the binary.
// ─────────────────────────────────────────────────────────────────────────────────────

use frame_compiler::text::emit::rust::Rust;

fn emit_rust(frm: &str) -> String {
    let src = Source::new("t.frm", frm.as_bytes().to_vec()).unwrap();
    let ast = segment(&src, Target::Rust).unwrap();
    let (syms, _) = resolve(&ast);
    driver::emit(&src, &ast, &syms, &Rust)
}

/// Build the generated code plus a `main` as a Cargo project (serde + serde_json deps) and
/// run it; return stdout. SKIP if no cargo. A shared target dir compiles serde once.
fn run_rust(frm: &str, main: &str, dir: &str) -> String {
    if Command::new("cargo").arg("--version").output().is_err() {
        return "SKIP".into();
    }
    let d = std::env::temp_dir().join(dir);
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    std::fs::write(
        d.join("Cargo.toml"),
        format!(
            "[package]\nname = \"{dir}\"\nversion = \"0.0.0\"\nedition = \"2021\"\n\n\
             [dependencies]\nserde = {{ version = \"1\", features = [\"derive\"] }}\n\
             serde_json = \"1\"\n\n[[bin]]\nname = \"{dir}\"\npath = \"main.rs\"\n\n[workspace]\n"
        ),
    )
    .unwrap();
    std::fs::write(d.join("main.rs"), format!("{}\n{main}\n", emit_rust(frm))).unwrap();
    let target = std::env::temp_dir().join("frame_persist_serde_target");
    let o = Command::new("cargo")
        .args(["build", "--offline", "--quiet", "--manifest-path"])
        .arg(d.join("Cargo.toml"))
        .env("CARGO_TARGET_DIR", &target)
        .output()
        .unwrap();
    assert!(
        o.status.success(),
        "cargo build rejected:\n{}",
        String::from_utf8_lossy(&o.stderr)
    );
    let o = Command::new(target.join("debug").join(dir)).output().unwrap();
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

/// **RFC-0056 R2: a user-defined type round-trips through serde.** `Point` derives
/// `Serialize`/`Deserialize` (the self-marshalling requirement); framec writes no code about
/// `Point` — serde marshals it. This was a `#[ignore]`d honest-gap under the flat format; the
/// serde route makes it pass.
#[test]
fn a_user_type_round_trips_on_the_fixed_type_rust_route() {
    let out = run_rust(
        RUST_USERTYPE,
        "#[derive(serde::Serialize, serde::Deserialize, Clone, Default)] struct Point { x: i32, y: i32 }\n\
         fn main() { let mut b = Bag::new(); b.p = Point { x: 7, y: 9 }; \
         let s = b.snapshot(); let mut b2 = Bag::new(); b2.restore(&s); \
         println!(\"{} {}\", b2.p.x, b2.p.y); }",
        "rust_persist_usertype",
    );
    if out == "SKIP" {
        return;
    }
    assert_eq!(out.trim(), "7 9", "a user-typed field must round-trip through serde");
}

const RUST_COLL: &str = r#"@@[persist(String)]
@@[save(snapshot)]
@@[load(restore)]
@@system Log {
    interface:
        go()
    machine:
        $A {
            go() { }
        }
    domain:
        entries: Vec<Point> = Vec::new()
        tags: Vec<String> = Vec::new()
}
"#;

/// **RFC-0056: collections and nesting round-trip.** A `Vec` of a user type and a `Vec` of
/// strings both survive — serde recurses; framec still writes no per-type code. This is the
/// dimension the scalar flat format could never reach.
#[test]
fn collections_and_nesting_round_trip_on_rust() {
    let out = run_rust(
        RUST_COLL,
        "#[derive(serde::Serialize, serde::Deserialize, Clone, Default)] struct Point { x: i32, y: i32 }\n\
         fn main() { let mut l = Log::new(); \
         l.entries = vec![Point{x:1,y:2}, Point{x:3,y:4}]; \
         l.tags = vec![String::from(\"a\"), String::from(\"b, c\")]; \
         let s = l.snapshot(); let mut l2 = Log::new(); l2.restore(&s); \
         println!(\"{} {} {} {}\", l2.entries.len(), l2.entries[1].y, l2.tags.len(), l2.tags[1]); }",
        "rust_persist_coll",
    );
    if out == "SKIP" {
        return;
    }
    // A collection of a user type (len 2, second element's y == 4) and a Vec<String> whose
    // second value contains the flat format's old delimiters (`, `) — serde escapes it.
    assert_eq!(out.trim(), "2 4 2 b, c", "Vec<UserType> and Vec<String> must round-trip");
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

/// A string field round-trips. Under serde (RFC-0056) this needs no special care — serde
/// escapes — and a delimiter-containing string is exercised by
/// `collections_and_nesting_round_trip_on_rust` (the `"b, c"` case). (The C/Java flat formats
/// still corrupt embedded delimiters; that limitation is theirs, not Rust's, now.)
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

// ─────────────────────────────────────────────────────────────────────────────────────
// FIXED-TYPE ROUTE (Java, Regime A per RFC-0056) via Gson. Java delegates to Gson: a user
// type self-marshals (Gson reflects over its fields into the declared type). framec writes
// no per-type code. Needs Gson on the classpath, so these tests discover a gson jar in the
// local caches and SKIP if none is found. Proven by RUNNING javac/java.
// ─────────────────────────────────────────────────────────────────────────────────────

use frame_compiler::text::emit::java::Java;

fn emit_java(frm: &str) -> String {
    let src = Source::new("t.frm", frm.as_bytes().to_vec()).unwrap();
    let ast = segment(&src, Target::Java).unwrap();
    let (syms, _) = resolve(&ast);
    driver::emit(&src, &ast, &syms, &Java::new())
}

/// Locate a gson jar in the usual local caches (maven/gradle). None → the Java tests SKIP.
fn gson_jar() -> Option<String> {
    let home = std::env::var("HOME").unwrap_or_default();
    for root in ["/opt/homebrew/Cellar".to_string(), format!("{home}/.gradle")] {
        if let Ok(o) = Command::new("find").arg(&root).args(["-name", "gson-*.jar"]).output() {
            if let Some(j) = String::from_utf8_lossy(&o.stdout).lines().find(|l| !l.is_empty()) {
                return Some(j.to_string());
            }
        }
    }
    None
}

/// Compile the generated Java plus a `main` (Gson on the classpath) and run `Main`; stdout.
fn run_java(frm: &str, main: &str, dir: &str) -> String {
    let gson = match gson_jar() {
        Some(j) => j,
        None => return "SKIP".into(),
    };
    if Command::new("javac").arg("--version").output().is_err() {
        return "SKIP".into();
    }
    let code = emit_java(frm);
    let cls = code
        .lines()
        .find_map(|l| l.strip_prefix("public class "))
        .and_then(|l| l.split_whitespace().next())
        .expect("a public class");
    let d = std::env::temp_dir().join(dir);
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    std::fs::write(d.join(format!("{cls}.java")), format!("{code}\n{main}\n")).unwrap();
    let o = Command::new("javac")
        .arg("-cp")
        .arg(&gson)
        .arg(format!("{cls}.java"))
        .current_dir(&d)
        .output()
        .unwrap();
    assert!(o.status.success(), "javac rejected:\n{}", String::from_utf8_lossy(&o.stderr));
    let o = Command::new("java")
        .arg("-cp")
        .arg(format!(".:{gson}"))
        .arg("Main")
        .current_dir(&d)
        .output()
        .unwrap();
    String::from_utf8_lossy(&o.stdout).into_owned()
}

const JAVA_COUNTER: &str = r#"@@[persist(String)]
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

/// A scalar field round-trips through Gson (save → restore → observe).
#[test]
fn a_scalar_round_trips_on_the_fixed_type_java_route() {
    let out = run_java(
        JAVA_COUNTER,
        "class Main { public static void main(String[] a) { \
            Counter c = new Counter(); c.bump(); c.bump(); c.bump(); \
            String s = c.snapshot(); Counter c2 = new Counter(); c2.restore(s); \
            System.out.println(c2.n); } }",
        "java_persist_roundtrip",
    );
    if out == "SKIP" {
        return;
    }
    assert_eq!(out.trim(), "3", "restore must reproduce n=3, not the default 0");
}

const JAVA_BAG: &str = r#"@@[persist(String)]
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
        p: Point = new Point()
        tags: java.util.List<String> = new java.util.ArrayList<>()
}
"#;

/// **RFC-0056 R2: a user type and a collection round-trip through Gson.** `Point` and a
/// `List<String>` (whose second value contains the old flat format's delimiters, `", "`) both
/// survive — Gson escapes and reconstructs into the declared field types. framec wrote no code
/// about `Point`. This was the fixed-type user-type honest-gap; Gson closes it for Java.
#[test]
fn a_user_type_and_collection_round_trip_on_java() {
    let out = run_java(
        JAVA_BAG,
        "class Point { public int x; public int y; } \
         class Main { public static void main(String[] a) { \
            Bag b = new Bag(); b.p.x = 7; b.p.y = 9; b.tags.add(\"a\"); b.tags.add(\"b, c\"); \
            String s = b.snapshot(); Bag b2 = new Bag(); b2.restore(s); \
            System.out.println(b2.p.x + \" \" + b2.p.y + \" \" + b2.tags.size() + \" \" + b2.tags.get(1)); } }",
        "java_persist_usertype",
    );
    if out == "SKIP" {
        return;
    }
    assert_eq!(out.trim(), "7 9 2 b, c", "user type + collection must round-trip via Gson");
}

const JAVA_TOGGLE: &str = r#"@@[persist(String)]
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

/// **Live control state round-trips on Java.** After a `flip` the machine is in `$On`; a
/// save → restore into a fresh instance lands back in `$On` (observable: `read()==1`).
#[test]
fn control_state_round_trips_on_java() {
    let out = run_java(
        JAVA_TOGGLE,
        "class Main { public static void main(String[] a) { \
            Toggle t = new Toggle(); t.flip(); \
            String s = t.snapshot(); Toggle t2 = new Toggle(); t2.restore(s); \
            System.out.println(t2.read()); } }",
        "java_persist_control",
    );
    if out == "SKIP" {
        return;
    }
    assert_eq!(out.trim(), "1", "after restore the machine must be in $On (read()==1), not $Off");
}

// ─── RFC-0056 Option 1: C persists scalars only; a user type is refused (E752) ───

const C_USERTYPE: &str = r#"@@[persist(char*)]
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
        p: Point = zero()
        n: int = 0
}
"#;

/// C has no serializer, so a user-typed persisted field is REFUSED at compile time (E752),
/// not silently miscompiled — RFC-0056's decided C contract (scalars + strings only). The
/// scalar `n` alongside it is fine; only `p: Point` is flagged.
#[test]
fn c_refuses_a_user_type_persist_field_e752() {
    let src = Source::new("t.frm", C_USERTYPE.as_bytes().to_vec()).unwrap();
    let ast = segment(&src, Target::C).unwrap();
    let (syms, _) = resolve(&ast);
    let diags = driver::target_diagnostics(&ast, &syms, &CBackend::new());
    let e752: Vec<_> = diags.iter().filter(|d| d.code == "E752").collect();
    assert_eq!(e752.len(), 1, "only the user-typed field is refused: {diags:#?}");
    assert!(e752[0].message.contains("Point"), "the message names the offending type");
}

/// The SAME spec on Rust raises no E752 — serde marshals the user type. The refusal is a C
/// fact (no serializer), not a persistence-wide rule.
#[test]
fn rust_accepts_the_same_user_type_persist_field() {
    let src = Source::new("t.frm", C_USERTYPE.as_bytes().to_vec()).unwrap();
    let ast = segment(&src, Target::Rust).unwrap();
    let (syms, _) = resolve(&ast);
    let diags = driver::target_diagnostics(&ast, &syms, &Rust);
    assert!(diags.iter().all(|d| d.code != "E752"), "serde marshals user types: {diags:#?}");
}
