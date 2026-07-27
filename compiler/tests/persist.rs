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

const PY_STATEVAR: &str = r#"class Vec2:
    def __init__(self, x=0.0, y=0.0):
        self.x = x
        self.y = y
    def mag_sq(self):
        return self.x * self.x + self.y * self.y

@@[persist(str)]
@@[save(save_state)]
@@[load(restore_state)]
@@system Bag {
    interface:
        setv(x, y)
        getmag()
    machine:
        $S {
            $.sv = Vec2(0.0, 0.0)
            setv(x, y) { $.sv = Vec2(x, y) }
            getmag() { @@:($.sv.mag_sq()) }
        }
    domain:
        marker = 1
}
"#;

fn emit_python_of(spec: &str) -> String {
    let src = Source::new("t.frm", spec.as_bytes().to_vec()).unwrap();
    let ast = segment(&src, Target::Python3).unwrap();
    let (syms, _) = resolve(&ast);
    driver::emit(&src, &ast, &syms, &Python)
}

/// **Compartment fidelity (RFC-0056): a user-typed STATE VARIABLE round-trips.** The snapshot
/// must carry the whole compartment (state + state_vars), not just the state name — before the
/// compartment fix the `$.sv` was dropped and `getmag()` came back the default. Mirrors the
/// test-env `persist_fidelity_state_var` corpus fixture, in the cargo suite so a regression is
/// caught here (the exact blind spot the domain-only tests had).
#[test]
fn a_user_typed_state_var_round_trips_on_python() {
    if Command::new("python3").arg("--version").output().is_err() {
        return;
    }
    let d = std::env::temp_dir().join("persist_statevar_py");
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    let driver = "\ns1 = Bag(); s1.setv(3.0, 4.0)\nsnap = s1.save_state()\n\
                  s2 = Bag(); s2.restore_state(snap)\nprint(s2.getmag())\n";
    let f = d.join("m.py");
    std::fs::write(&f, format!("{}\n{driver}", emit_python_of(PY_STATEVAR))).unwrap();
    let o = Command::new("python3").arg(&f).output().unwrap();
    assert!(o.status.success(), "python crashed:\n{}", String::from_utf8_lossy(&o.stderr));
    assert_eq!(
        String::from_utf8_lossy(&o.stdout).trim(),
        "25.0",
        "a user-typed state variable must survive save/restore"
    );
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
         let s = c.snapshot(); let mut c2 = Counter::new(); c2.restore(s); \
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
         let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| c.restore(bad.to_string()))); \
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
         let s = t.snapshot(); let mut t2 = Toggle::new(); t2.restore(s); \
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
         let s = b.snapshot(); let mut b2 = Bag::new(); b2.restore(s); \
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
         let s = l.snapshot(); let mut l2 = Log::new(); l2.restore(s); \
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
         let s = n.snapshot(); let mut n2 = Named::new(); n2.restore(s); \
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

/// cJSON compile+link flags — RFC-0056 delegates C persistence to cJSON. Prefer pkg-config;
/// fall back to the common homebrew/usr-local prefixes. None → the C persist tests SKIP (they
/// cannot prove anything without the serializer). Returns e.g. ["-I/opt/homebrew/include",
/// "-L/opt/homebrew/lib", "-lcjson"].
fn cjson_flags() -> Option<Vec<String>> {
    // Filesystem prefixes FIRST: the running arch's homebrew prefix (/opt/homebrew on arm64,
    // /usr/local on x86_64) naturally holds a matching-arch lib. pkg-config is a last resort
    // because it can resolve to a stale WRONG-arch install (e.g. an x86_64 /usr/local cjson on
    // an arm64 host -> "Undefined symbols for architecture arm64" at link).
    for prefix in ["/opt/homebrew", "/usr/local", "/usr"] {
        let has_lib = ["dylib", "so", "a"]
            .iter()
            .any(|ext| std::path::Path::new(&format!("{prefix}/lib/libcjson.{ext}")).exists());
        if std::path::Path::new(&format!("{prefix}/include/cjson/cJSON.h")).exists() && has_lib {
            return Some(vec![
                format!("-I{prefix}/include"),
                format!("-L{prefix}/lib"),
                "-lcjson".to_string(),
            ]);
        }
    }
    if let Ok(o) = Command::new("pkg-config").args(["--cflags", "--libs", "libcjson"]).output() {
        if o.status.success() {
            let s = String::from_utf8_lossy(&o.stdout);
            let flags: Vec<String> = s.split_whitespace().map(str::to_string).collect();
            if !flags.is_empty() {
                return Some(flags);
            }
        }
    }
    None
}

/// Compile the generated C plus a `main` with cc (cJSON on the include/link path); return
/// stdout. SKIP if no cc or no cJSON.
fn run_c(frm: &str, main: &str, dir: &str) -> String {
    if Command::new("cc").arg("--version").output().is_err() {
        return "SKIP".into();
    }
    let flags = match cjson_flags() {
        Some(f) => f,
        None => return "SKIP".into(),
    };
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
        .args(&flags)
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
// FIXED-TYPE ROUTE (Java, Regime A per RFC-0056) via Jackson. Java delegates to Jackson: a
// user type self-marshals (Jackson reflects over its fields into the declared type), and the
// polymorphic control state round-trips from framec-generated @JsonTypeInfo/@JsonSubTypes
// annotations (keyed on framec's OWN states — type-ignorant, closed-world). framec writes no
// per-type code. Needs jackson-databind (+core +annotations) on the classpath, so these tests
// discover the jars in the local caches and SKIP if any is missing. Proven by RUNNING javac/java.
// ─────────────────────────────────────────────────────────────────────────────────────

use frame_compiler::text::emit::java::Java;

fn emit_java(frm: &str) -> String {
    let src = Source::new("t.frm", frm.as_bytes().to_vec()).unwrap();
    let ast = segment(&src, Target::Java).unwrap();
    let (syms, _) = resolve(&ast);
    driver::emit(&src, &ast, &syms, &Java::new())
}

/// Locate one jar matching `pattern` in the usual local caches (homebrew/maven/gradle).
fn find_jar(pattern: &str) -> Option<String> {
    let home = std::env::var("HOME").unwrap_or_default();
    for root in [
        "/opt/homebrew/Cellar".to_string(),
        format!("{home}/.gradle"),
        format!("{home}/.m2"),
    ] {
        if let Ok(o) = Command::new("find").arg(&root).args(["-name", pattern]).output() {
            if let Some(j) = String::from_utf8_lossy(&o.stdout).lines().find(|l| !l.is_empty()) {
                return Some(j.to_string());
            }
        }
    }
    None
}

/// The Jackson classpath: databind + core + annotations, `:`-joined. core and annotations are
/// taken from the SAME directory as databind so the three versions MATCH (a 2.8 databind with a
/// 2.18 core fails at runtime). None if a co-located matching set is not found → the Java persist
/// tests SKIP (they cannot prove anything without the serializer).
fn jackson_cp() -> Option<String> {
    let databind = find_jar("jackson-databind-*.jar")?;
    let dir = std::path::Path::new(&databind).parent()?;
    let sibling = |pat: &str| -> Option<String> {
        let o = Command::new("find").arg(dir).args(["-maxdepth", "1", "-name", pat]).output().ok()?;
        String::from_utf8_lossy(&o.stdout).lines().find(|l| !l.is_empty()).map(str::to_string)
    };
    let core = sibling("jackson-core-*.jar")?;
    let annotations = sibling("jackson-annotations-*.jar")?;
    Some(format!("{databind}:{core}:{annotations}"))
}

/// Compile the generated Java plus a `main` (Jackson on the classpath) and run `Main`; stdout.
fn run_java(frm: &str, main: &str, dir: &str) -> String {
    let cp = match jackson_cp() {
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
        .arg(&cp)
        .arg(format!("{cls}.java"))
        .current_dir(&d)
        .output()
        .unwrap();
    assert!(o.status.success(), "javac rejected:\n{}", String::from_utf8_lossy(&o.stderr));
    let o = Command::new("java")
        .arg("-cp")
        .arg(format!(".:{cp}"))
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
#[ignore = "pending Java M-persist: kernel-model persist not yet ported — java.rs persist() is a schema-guarded stub (save returns the schema, restore only checks it); a scalar must survive save->restore->observe"]
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
#[ignore = "pending Java M-persist: kernel-model persist not yet ported — java.rs persist() is a schema-guarded stub; a user type + collection must round-trip through Jackson"]
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
#[ignore = "pending Java M-persist: kernel-model persist not yet ported — java.rs persist() is a schema-guarded stub; the polymorphic control state must round-trip so read()==1 ($On) after restore"]
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

// ─── RFC-0056 Regime A2: C persists a user type through an AUTHOR HOOK (no E752 refusal) ───

const C_USERTYPE: &str = r#"typedef struct { int x; int y; } Point;
static Point zero(void) { Point p; p.x = 0; p.y = 0; return p; }

@@[persist(char*)]
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

/// RFC-0056 Option 2: C no longer REFUSES a user-typed persisted field (the old E752). It
/// emits an author-hook call + a forward `extern` for the marshaller, type-ignorantly. A
/// missing definition is a build-time link error, not a framec refusal. The scalar `n` is
/// marshalled directly; only `p: Point` routes through the hook.
#[test]
fn c_emits_an_author_hook_for_a_user_type() {
    let src = Source::new("t.frm", C_USERTYPE.as_bytes().to_vec()).unwrap();
    let ast = segment(&src, Target::C).unwrap();
    let (syms, _) = resolve(&ast);
    // No target refuses a persisted field any more — E752 is retired.
    let diags = driver::target_diagnostics(&ast, &syms, &CBackend::new());
    assert!(diags.iter().all(|d| d.code != "E752"), "E752 is retired: {diags:#?}");
    // The emitted C carries the extern hook decls + the pack/unpack calls for Point.
    let code = emit_c(C_USERTYPE);
    assert!(
        code.contains("extern cJSON* Bag_persist_pack_field_Point(void* v);"),
        "framec must emit the pack-hook extern with the shipping-compatible void* signature:\n{code}"
    );
    assert!(
        code.contains("Bag_persist_pack_field_Point(&(self->p))"),
        "framec must call the pack hook for the user field:\n{code}"
    );
    // The scalar `int` marshals directly (no hook) — as a lossless string, not a lossy double.
    assert!(
        code.contains("cJSON_AddItemToObject(__root, \"n\", Bag__pack_i64((long long)(self->n)))"),
        "the scalar int field marshals directly (lossless integer):\n{code}"
    );
}

/// The SAME spec on Rust raises no diagnostic — serde marshals the user type with no author
/// hook. Both targets now persist user types; the difference is only who supplies the
/// marshalling (serde derive vs. a C author hook).
#[test]
fn rust_accepts_the_same_user_type_persist_field() {
    let src = Source::new("t.frm", C_USERTYPE.as_bytes().to_vec()).unwrap();
    let ast = segment(&src, Target::Rust).unwrap();
    let (syms, _) = resolve(&ast);
    let diags = driver::target_diagnostics(&ast, &syms, &Rust);
    assert!(diags.iter().all(|d| d.code != "E752"), "serde marshals user types: {diags:#?}");
}

/// PROOF at runtime: a user-typed field round-trips on C when the author supplies the hook
/// pair. The Point struct + its pack/unpack hooks are defined in `main` (author code), framec
/// emits the calls, and cJSON does the JSON. save -> restore reproduces the value.
#[test]
fn c_user_type_round_trips_with_author_hook() {
    // `Point` + `zero()` are water in the fixture (above the system); the author's marshalling
    // hooks are defined here in `main` — after persist's `#include <cjson/cJSON.h>`, so cJSON
    // is in scope for them.
    let out = run_c(
        C_USERTYPE,
        r#"
cJSON* Bag_persist_pack_field_Point(void* p) {
    Point* v = (Point*)p;
    cJSON* o = cJSON_CreateObject();
    cJSON_AddNumberToObject(o, "x", v->x);
    cJSON_AddNumberToObject(o, "y", v->y);
    return o;
}
void Bag_persist_unpack_field_Point(cJSON* j, void* p) {
    Point* v = (Point*)p;
    v->x = (int)cJSON_GetObjectItem(j, "x")->valuedouble;
    v->y = (int)cJSON_GetObjectItem(j, "y")->valuedouble;
}
int main(void) {
    Bag* b = Bag_new();
    b->p.x = 3; b->p.y = 4; b->n = 9;
    char* s = Bag_snapshot(b);
    Bag* b2 = Bag_new();
    Bag_restore(b2, s);
    printf("%d %d %d\n", b2->p.x, b2->p.y, b2->n);
    return 0;
}"#,
        "c_persist_usertype_hook",
    );
    if out == "SKIP" {
        return;
    }
    assert_eq!(out.trim(), "3 4 9", "the user type round-trips through the author hook");
}
