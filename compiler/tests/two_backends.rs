//! **One driver, two structurally opposite languages.**
//!
//! Java: braces, `;`, static types, casts, unreachable code is a compile error.
//! Python: indentation, no terminator, dynamic, no casts, name mangling.
//!
//! If the shared driver had needed an escape hatch for either, the structure would be
//! wrong — and better to find that out at backend two than at backend eleven.
//!
//! # The rule, and how it is enforced
//!
//! > **`driver::emit` does not have the target language.**
//!
//! Its signature is `emit(src, ast, syms, &dyn Backend)`. There is no `Target`
//! parameter, so `match lang { … }` will not compile in there. It is the same trick as
//! the text wall: make the wrong thing *unrepresentable* rather than forbidden.
//!
//! That matters because the old compiler had **seventeen hand-written arms** for nearly
//! every decision, and they drifted systematically. Porting ONE feature to sixteen
//! backends produced **six identical mistakes** — the same error made independently in
//! Rust, Java, Go and C++. Sixteen arms are sixteen chances to be wrong; a reviewer who
//! checks fifteen of them has still shipped a bug.
//!
//! NOTE: the Java fixture uses Java's type names (`String`), the Python fixture uses
//! Python's (`str`). framec emits declared types VERBATIM — there is no Frame scalar
//! vocabulary it translates — so a probe must write each target's own type names, exactly
//! as a real user would. (This test once used `str` in the Java fixture and only passed
//! because a `str->String` alias masked it; that alias was a contract violation and is
//! gone.)
//!
//! FIXTURE MIGRATED (M1, faithful emit). `$Paid`'s handler used to reach into the compartment by
//! hand — `compartment.state_args["item"]` — which was native code written against the OLD
//! cleanroom compartment, where `state_args` was a name-keyed dict. In the faithful runtime a
//! state's params are POSITIONAL (`state_args` is a list) and are bound as locals for the handler,
//! so the fixture now says `show(item, amount)` — which is exactly what the JAVA half of this same
//! test already said (`"item=" + item + " amount=" + amount`). The two halves are symmetric again,
//! and the assertion — the four lines of runtime output — is unchanged.

use frame_compiler::resolve::resolve;
use frame_compiler::scan::{literals::Target, segment};
use frame_compiler::text::emit::{driver, java::Java, python::Python};
use frame_compiler::Source;
use std::process::Command;

fn tool(bin: &str) -> bool {
    Command::new(bin)
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn build(frm: &str, target: Target) -> (String, frame_compiler::resolve::SymbolTable, Source) {
    let src = Source::new("t.frm", frm.as_bytes().to_vec()).unwrap();
    let ast = segment(&src, target).expect("segment");
    let (syms, diags) = resolve(&ast);
    assert!(diags.is_empty(), "{diags:#?}");
    let be: &dyn driver::Backend = match target {
        Target::Java => &Java::new(),
        Target::Python3 => &Python,
        _ => unreachable!(),
    };
    let code = driver::emit(&src, &ast, &syms, be);
    (code, syms, src)
}

/// The SAME machine, expressed once per target's native syntax, through the SAME driver.
/// Both must produce the same behaviour.
#[test]
fn the_same_machine_runs_on_both_targets() {
    let py_frm = r#"@@system Vend {
    interface:
        coin()
        pick()
    machine:
        $Idle {
            $.credit: int = 0
            coin() {
                print_credit($.credit)
                push$ -> $Paid("cola, diet", 5)
            }
        }
        $Paid(item: str, amount: int) {
            pick() {
                show(item, amount)
                -> pop$
            }
        }
}
"#;

    let java_frm = r#"@@system Vend {
    interface:
        coin()
        pick()
    machine:
        $Idle {
            $.credit: int = 0
            coin() {
                System.out.println("credit=" + $.credit);
                push$ -> $Paid("cola, diet", 5)
            }
        }
        $Paid(item: String, amount: int) {
            pick() {
                System.out.println("item=" + item + " amount=" + amount);
                -> pop$
            }
        }
}
"#;

    let expected = ["credit=0", "item=cola, diet amount=5", "credit=0", "done"];

    // ---- PYTHON ----
    if tool("python3") {
        let (code, _, _) = build(py_frm, Target::Python3);
        let d = std::env::temp_dir().join("frame_two_py");
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        let main = "\ndef print_credit(c): print(\"credit=\" + str(c))\n\
                    def show(i, a): print(\"item=\" + i + \" amount=\" + str(a))\n\
                    v = Vend()\nv.coin()\nv.pick()\nv.coin()\nprint(\"done\")\n";
        let f = d.join("vend.py");
        // The prelude is now part of the backend's `file_header` — emitted once, at the
        // top of the file, before any item. (It has to be: the user's native code may
        // precede the system, and Java requires imports before any class.)
        std::fs::write(&f, format!("{code}{main}")).unwrap();
        let out = Command::new("python3").arg(&f).output().unwrap();
        assert!(
            out.status.success(),
            "python REJECTED or CRASHED:\n{}",
            String::from_utf8_lossy(&out.stderr)
        );
        let got = String::from_utf8_lossy(&out.stdout);
        assert_eq!(got.lines().collect::<Vec<_>>(), expected, "python");
    } else {
        eprintln!("SKIPPED python3 — this half verifies NOTHING.");
    }

    // ---- JAVA ----
    if tool("javac") || Command::new("javac").arg("-version").output().is_ok() {
        let (code, _, _) = build(java_frm, Target::Java);
        let d = std::env::temp_dir().join("frame_two_java");
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        let main = "\nclass Main { public static void main(String[] a) { \
                    Vend v = new Vend(); v.coin(); v.pick(); v.coin(); \
                    System.out.println(\"done\"); } }\n";
        std::fs::write(d.join("Vend.java"), format!("{code}{main}")).unwrap();
        let out = Command::new("javac")
            .arg("Vend.java")
            .current_dir(&d)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "javac REJECTED:\n{}",
            String::from_utf8_lossy(&out.stderr)
        );
        let out = Command::new("java")
            .arg("Main")
            .current_dir(&d)
            .output()
            .unwrap();
        let got = String::from_utf8_lossy(&out.stdout);
        assert_eq!(got.lines().collect::<Vec<_>>(), expected, "java");
    } else {
        eprintln!("SKIPPED javac — this half verifies NOTHING.");
    }
}

/// **The same node lowers to different shapes, and NEITHER backend knows the atom rule.**
///
/// Java's state-var read is a CAST and must be parenthesized. Python's is a postfix
/// chain and must not be. Both are atoms — because `Atom::cast` parenthesizes and
/// `Atom::index` builds a chain, not because either author remembered.
#[test]
fn the_atom_rule_is_in_the_type_not_in_the_backends() {
    use frame_compiler::text::emit::atom::Atom;

    // Java: a cast. Parenthesized, by construction.
    let j = Atom::cast(
        "Integer",
        Atom::method(
            Atom::field(Atom::ident("compartment"), "stateVars"),
            "get",
            "\"n\"",
        ),
    );
    assert_eq!(j.as_str(), "((Integer) compartment.stateVars.get(\"n\"))");

    // Python: a postfix chain. No parens, and none needed.
    let p = Atom::index(
        Atom::field(Atom::ident("compartment"), "state_vars"),
        "\"n\"",
    );
    assert_eq!(p.as_str(), "compartment.state_vars[\"n\"]");

    // BOTH survive a member access — which is the operation that broke the old
    // compiler's C# expansion (#213: compiled clean, exit 0, printed -1 instead of 84).
    assert_eq!(
        Atom::method(j, "intValue", "").as_str(),
        "((Integer) compartment.stateVars.get(\"n\")).intValue()"
    );
    assert_eq!(
        Atom::method(p, "bit_length", "").as_str(),
        "compartment.state_vars[\"n\"].bit_length()"
    );
}

/// **Lifecycle handlers RUN, on every backend, and enter/exit args are delivered.**
///
/// Before this, `$>`/`<$` were emitted but never CALLED — `-> $B` did not run B's enter
/// handler — and exit/enter args (`(x) -> (y) $T`) were silently dropped. A green compile
/// hid it completely. These tests execute the machines.
#[test]
fn lifecycle_handlers_run_with_args_on_both_backends() {
    // Java: enter + exit + both arg positions.
    let java = r#"@@system L {
    interface:
        go()
        back()
    machine:
        $A {
            go() { (7) -> ("hi") $B }
        }
        $B {
            $>(msg: String) { System.out.println("enter " + msg); }
            <$(code: int) { System.out.println("exit " + code); }
            back() { (99) -> $A }
        }
}
"#;
    if tool("javac") {
        let (code, _, _) = build(java, Target::Java);
        let d = std::env::temp_dir().join("frame_lc_java");
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        let main = "\nclass Main { public static void main(String[] a) { L l = new L(); \
                    l.go(); l.back(); System.out.println(\"done\"); } }\n";
        std::fs::write(d.join("L.java"), format!("{code}{main}")).unwrap();
        assert!(Command::new("javac").arg("L.java").current_dir(&d).output().unwrap().status.success());
        let out = Command::new("java").arg("Main").current_dir(&d).output().unwrap();
        assert_eq!(
            String::from_utf8_lossy(&out.stdout).lines().collect::<Vec<_>>(),
            ["enter hi", "exit 99", "done"],
            "$>(hi) runs on entering B; <$(99) runs on leaving B via back()'s exit arg"
        );
    } else {
        eprintln!("SKIPPED javac");
    }

    // Python: the same machine, its own syntax — enter handler must run.
    let py = r#"@@system L {
    interface:
        go()
    machine:
        $A { go() { -> ("hi") $B } }
        $B { $>(msg: str) { greet(msg) } }
}
"#;
    if tool("python3") {
        let (code, _, _) = build(py, Target::Python3);
        let d = std::env::temp_dir().join("frame_lc_py");
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        let main = "\ndef greet(m): print('enter ' + m)\nL().go()\n";
        std::fs::write(d.join("m.py"), format!("{}{code}{main}", frame_compiler::text::emit::python::PRELUDE)).unwrap();
        let out = Command::new("python3").arg(d.join("m.py")).output().unwrap();
        assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "enter hi", "$>(hi) must run on Python too");
    } else {
        eprintln!("SKIPPED python3");
    }
}
