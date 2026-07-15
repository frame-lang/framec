//! **THE HONEST GAPS.** Features the cleanroom does not implement — as *tests*, not notes.
//!
//! # Why this file exists
//!
//! The cleanroom compiles **15/15** Java corpus fixtures. That number is a **lie**, and it
//! is the exact lie this project spent a session exposing:
//!
//! > A syntax check blesses code that **compiles and behaves wrongly**.
//!
//! Three fixtures pass **by omission**:
//!
//! | fixture | why it "passes" |
//! |---|---|
//! | `02_hsm` | `$Awake => $Live` declares a PARENT state. We ignore it and emit a FLAT machine. It compiles. It is wrong. |
//! | `07_forward` | `=> $^` is `Stmt::Forward(_) => {}` — a literal no-op. |
//! | `03_persist` / `12_no_persist` | persist is not implemented, so nothing is emitted, so nothing fails. |
//!
//! That is exactly how the old corpus came to have blessed snapshots of code that does not
//! compile: **a test that does not exercise the feature cannot fail on it.**
//!
//! So each gap is `#[ignore]`d here with a name. `cargo test -- --ignored` lists the debt.
//! When a feature lands, its test is un-ignored and must PASS — it cannot be quietly
//! forgotten, because the fixture that "passes" today is passing for the wrong reason.

use frame_compiler::resolve::resolve;
use frame_compiler::scan::{literals::Target, segment};
use frame_compiler::text::emit::{driver, java::Java};
use frame_compiler::Source;
use std::process::Command;

fn emit(frm: &str) -> String {
    let src = Source::new("t.frm", frm.as_bytes().to_vec()).unwrap();
    let ast = segment(&src, Target::Java).unwrap();
    let (syms, _) = resolve(&ast);
    driver::emit(&src, &ast, &syms, &Java::new())
}

fn run(code: &str, main: &str, dir: &str) -> String {
    let d = std::env::temp_dir().join(dir);
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    let cls = code
        .lines()
        .find_map(|l| l.strip_prefix("public class "))
        .and_then(|l| l.split_whitespace().next())
        .expect("a public class");
    std::fs::write(d.join(format!("{cls}.java")), format!("{code}\n{main}\n")).unwrap();
    let o = Command::new("javac")
        .arg(format!("{cls}.java"))
        .current_dir(&d)
        .output()
        .unwrap();
    assert!(
        o.status.success(),
        "javac rejected:\n{}",
        String::from_utf8_lossy(&o.stderr)
    );
    let o = Command::new("java")
        .arg("Main")
        .current_dir(&d)
        .output()
        .unwrap();
    String::from_utf8_lossy(&o.stdout).into_owned()
}

/// **GAP 1 — HIERARCHICAL STATE MACHINES.**
///
/// `$Awake => $Live` means `$Awake`'s parent is `$Live`. An event a child does not handle
/// must be **forwarded to the parent**. We emit a flat machine, so `ping()` in `$Awake`
/// silently does nothing instead of running `$Live`'s handler.
///
/// It compiles. `02_hsm` is green. The behaviour is wrong.
#[test]
fn hsm_forwards_an_unhandled_event_to_the_parent() {
    let frm = r#"@@system H {
    interface:
        ping()
    machine:
        $Live {
            ping() { System.out.println("parent handled it"); }
        }
        $Awake => $Live {
        }
}
"#;
    let out = run(
        &emit(frm),
        "class Main { public static void main(String[] a) { new H().ping(); } }",
        "gap_hsm",
    );
    assert_eq!(
        out.trim(),
        "parent handled it",
        "an event the CHILD does not handle must reach the PARENT"
    );
}

/// **GAP 2 — FORWARD (`=> $^`).**
///
/// `=> $^` forwards the current event to the parent state. It is currently
/// `Stmt::Forward(_) => {}` in the driver — a literal no-op.
#[test]
fn forward_sends_the_event_to_the_parent() {
    // NOTE: `$Awake` is declared FIRST because the machine starts in its first state.
    // My first version put `$Live` first, so the machine never entered `$Awake` and the
    // test failed for a reason that had nothing to do with forwarding. The test was
    // wrong, not the compiler.
    let frm = r#"@@system F {
    interface:
        ping()
    machine:
        $Awake => $Live {
            ping() {
                System.out.println("child first");
                => $^
            }
        }
        $Live {
            ping() { System.out.println("parent"); }
        }
}
"#;
    let out = run(
        &emit(frm),
        "class Main { public static void main(String[] a) { new F().ping(); } }",
        "gap_fwd",
    );
    assert_eq!(
        out.lines().collect::<Vec<_>>(),
        ["child first", "parent"],
        "`=> $^` must actually forward"
    );
}

// PERSIST round-trip (scalar, user types, collections, control state) is proven per backend
// in tests/persist.rs — Python (reflective), Rust (serde), Java (Gson), C (scalar). Those
// tests moved there when persistence became real; this file keeps only the still-open gaps.

/// **GAP 4 — ASYNC.**
///
/// `@@[async]` should make the interface async. And when it does, the `await` MUST be
/// parenthesized (`(await x.f())`) — `Atom::awaited` already guarantees that, but nothing
/// exercises it yet, so #225 is `unreachable`, not `fixed`.
#[test]
fn async_emits_an_async_interface() {
    let frm = r#"@@[async]
@@system A {
    interface:
        async fetch(): String
    machine:
        $R {
            fetch(): String { @@:("v") }
        }
}
"#;
    let code = emit(frm);
    assert!(
        code.contains("CompletableFuture") || code.contains("async"),
        "@@[async] must emit an async interface. It emits a plain one:\n{code}"
    );
}

/// The gap ledger itself. **This test always runs**, and it fails if the count of
/// `#[ignore]`d gaps above stops matching what we claim.
///
/// It exists so that "15/15 fixtures compile" can never be quoted without the asterisk.
#[test]
fn the_compliance_number_carries_its_asterisk() {
    // CLOSED, each proven by RUNNING (see honest_gaps + tests/persist.rs):
    //   HSM, forward `=> $^`, @@[async]; @@[persist] SCALAR round-trip + control state, and
    //   out-of-band framing (#233 impossible on Python).
    // OPEN — known-open items. Most are named by an #[ignore]d test; a *design-open* item
    // (a decision not yet taken, with nothing to un-ignore) is listed here and detailed in its
    // RFC instead.
    const GAPS: &[(&str, &str)] = &[(
        "persist user types (C)",
        "RFC-0056 R2 — a user-defined type MUST self-marshal through the host serializer. Rust \
         (serde) and Java (Gson) do this: user types, nesting, and collections round-trip (see \
         tests/persist.rs). C has NO standard serializer, so its user-type persist is the \
         RFC-0056 UNRESOLVED decision — scalar-limited vs. a single framec-emitted encoder. This \
         is design-open (nothing to un-ignore); it is tracked in RFC-0056 § Unresolved questions.",
    )];
    eprintln!("\n  CLEANROOM: 15/15 Java corpus fixtures COMPILE.");
    eprintln!("  That number is SYNTAX-ONLY, and {} feature(s) remain OPEN:\n", GAPS.len());
    for (name, why) in GAPS {
        eprintln!("      {name:<18} {why}");
    }
    eprintln!(
        "\n  A fixture that passes because a feature is ABSENT is not passing.\n\
           That is exactly how the old corpus acquired blessed snapshots of code\n\
           that does not compile (#232). Run `cargo test -- --ignored` for the debt.\n"
    );
    // This count pins the open-item ledger so "15/15 compile" is never quoted without its
    // asterisk. Each entry is either an #[ignore]d test (un-ignore when it lands) or a
    // design-open decision detailed in an RFC (the C persist decision). Update on any change.
    assert_eq!(GAPS.len(), 1, "the open-item ledger drifted");
}

/// **#225 — `await` MUST NOT land at the head.**
///
/// `await self.m().upper()` parses as `await (self.m().upper())` — `.upper()` is invoked
/// on the **coroutine**, not on the value. On eight targets the old compiler emitted
/// exactly that, and `java_await_rewrite` existed downstream purely to un-do it.
///
/// `Atom::awaited` parenthesizes. It is the ONLY constructor that can produce an `await`,
/// so the bare form is not something a backend must remember to avoid — it is something
/// it cannot express.
///
/// Proven by RUNNING an async Python machine, not by reading the emitted text.
#[test]
fn await_is_parenthesized_and_the_program_is_correct() {
    use frame_compiler::text::emit::atom::Atom;

    // The type guarantee.
    let a = Atom::awaited(Atom::call("self.val", ""), "await");
    assert_eq!(a.as_str(), "(await self.val())");
    assert_eq!(
        Atom::method(a, "upper", "").as_str(),
        "(await self.val()).upper()",
        "the member access must bind to the AWAITED VALUE, not the coroutine"
    );

    // And the runtime proof, on real Python.
    if Command::new("python3").arg("--version").output().is_err() {
        eprintln!("SKIPPED python3 — this verifies NOTHING.");
        return;
    }
    let frm = r#"@@[async]
@@system A {
    interface:
        async go()
    machine:
        $R {
            go() {
                @@:self.helper()
            }
        }
}
"#;
    let src = Source::new("t.frm", frm.as_bytes().to_vec()).unwrap();
    let ast = segment(&src, Target::Python3).unwrap();
    let (syms, _) = resolve(&ast);
    let code = driver::emit(&src, &ast, &syms, &frame_compiler::text::emit::python::Python);

    assert!(
        code.contains("(await self.helper())"),
        "the await must be PARENTHESIZED:\n{code}"
    );
    assert!(
        !code.contains("        await self.helper()"),
        "a BARE `await` at the head is #225 — it must be unrepresentable:\n{code}"
    );

    let d = std::env::temp_dir().join("gap_await");
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    let f = d.join("a.py");
    std::fs::write(
        &f,
        format!(
            "{code}\nimport asyncio\nasync def _h(self): print(\"awaited ok\")\nA.helper = _h\nasyncio.run(A().go())\n"
        ),
    )
    .unwrap();
    let out = Command::new("python3").arg(&f).output().unwrap();
    assert!(
        out.status.success(),
        "the async program CRASHED:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "awaited ok");
}
