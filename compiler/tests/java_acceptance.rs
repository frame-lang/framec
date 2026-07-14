//! **The acceptance gate: generate -> javac -> run -> verify.**
//!
//! Not "the output looks right." The old compiler's output looked right for two years.
//! These tests invoke the real toolchain and check the real answer.
//!
//! Skipped (loudly) if `javac` is absent — never silently passed. A test that quietly
//! does nothing reports the same green as a test that verified everything, and that is
//! how a whole class of bug hides.

use frame_compiler::resolve::resolve;
use frame_compiler::scan::{literals::Target, segment};
use frame_compiler::text::emit::{driver, java::Java};
use frame_compiler::Source;
use std::process::Command;

fn have_javac() -> bool {
    Command::new("javac")
        .arg("-version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Compile `frm` to Java, append `main`, run it, and return stdout.
fn run(frm: &str, main: &str, dir: &str) -> String {
    let src = Source::new("t.frm", frm.as_bytes().to_vec()).unwrap();
    let ast = segment(&src, Target::Java).expect("segment");
    let (syms, diags) = resolve(&ast);
    assert!(diags.is_empty(), "{diags:#?}");
    let code = driver::emit(&src, &ast, &syms, &Java::new());

    let d = std::env::temp_dir().join(dir);
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();

    let name = ast
        .items
        .iter()
        .find_map(|i| match i {
            frame_compiler::tree::Item::System(s) => Some(s.name.clone()),
            _ => None,
        })
        .expect("a system");

    let file = d.join(format!("{name}.java"));
    std::fs::write(&file, format!("{code}\n{main}\n")).unwrap();

    let out = Command::new("javac")
        .arg(&file)
        .current_dir(&d)
        .output()
        .expect("javac");
    assert!(
        out.status.success(),
        "javac REJECTED the generated code:\n{}\n\n--- generated ---\n{code}",
        String::from_utf8_lossy(&out.stderr)
    );

    let out = Command::new("java")
        .arg("Main")
        .current_dir(&d)
        .output()
        .expect("java");
    assert!(
        out.status.success(),
        "the generated program CRASHED:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).into_owned()
}

/// **A domain param is a constructor arg, in scope for the domain field init** (spec §88),
/// and the call site fills defaults (§1155). `@@Counter(100)` seeds the field from the
/// param; `@@Counter()` falls back to the declared default.
#[test]
fn a_system_param_is_load_bearing() {
    if !have_javac() {
        eprintln!("SKIPPED: javac not installed.");
        return;
    }
    let frm = r#"@@system Counter(start: int = 7) {
    interface:
        get(): int
    machine:
        $C { get(): int { @@:(((Number) @@:self.count).intValue()) } }
    domain:
        count: int = start
}

class Main {
    public static void main(String[] args) {
        Counter a = @@Counter(100);
        Counter b = @@Counter();
        System.out.println(a.get() + " " + b.get());
    }
}
"#;
    let out = run(frm, "", "java_param");
    assert_eq!(
        out.trim(),
        "100 7",
        "@@Counter(100) seeds count from the param; @@Counter() uses the default 7"
    );
}

/// **`@@Counter()` instantiation lowers to `new Counter()`** (spec §1103) inside
/// top-level native water — Frame's own syntax. Before this it was emitted verbatim and
/// javac rejected it. The `Main` driver is trailing water in the `.frm`, so framec
/// processes it (text appended by the harness never passes through the compiler).
#[test]
fn system_instantiation_lowers_to_the_constructor() {
    if !have_javac() {
        eprintln!("SKIPPED: javac not installed.");
        return;
    }
    let frm = r#"@@system Counter {
    interface:
        inc()
        get(): int
    machine:
        $C {
            $.n: int = 5
            inc() { $.n = ((Number) $.n).intValue() + 1 }
            get(): int { @@:(((Number) $.n).intValue()) }
        }
}

class Main {
    public static void main(String[] args) {
        Counter c = @@Counter();
        c.inc();
        System.out.println(c.get());
    }
}
"#;
    let out = run(frm, "", "java_instantiate");
    assert_eq!(out.trim(), "6", "@@Counter() in water lowers to new Counter(); 5 + 1 = 6");
}

/// The machine actually works: it transitions, and a handler in the wrong state is a
/// no-op.
#[test]
fn a_generated_state_machine_compiles_and_runs() {
    if !have_javac() {
        eprintln!("SKIPPED: javac not installed — this test verifies NOTHING here.");
        return;
    }
    let frm = r#"@@system Door {
    interface:
        open()
        close()
    machine:
        $Closed {
            open() {
                System.out.println("opening");
                -> $Open
            }
        }
        $Open {
            close() {
                System.out.println("closing");
                -> $Closed
            }
        }
}
"#;
    let main = r#"
class Main {
    public static void main(String[] a) {
        Door d = new Door();
        d.open();
        d.close();
        d.open();
        d.close();   // cycled correctly
        d.close();   // WRONG STATE -> must be a no-op
        System.out.println("done");
    }
}"#;
    let out = run(frm, main, "frame_java_1");
    assert_eq!(
        out.lines().collect::<Vec<_>>(),
        ["opening", "closing", "opening", "closing", "done"],
        "the machine must cycle, and a handler in the wrong state must do NOTHING"
    );
}

/// **#215 — the user's string literal keeps its value.**
///
/// The old compiler's `normalize_indentation` stripped the left margin off every emitted
/// line *including lines inside a string literal*, so the user's string silently changed
/// value at runtime. Here re-indentation is a fold over nodes and has no arm that could
/// touch literal content.
///
/// Java text blocks (`"""`) make this observable: the string's interior whitespace IS
/// its value.
#[test]
fn a_string_literal_keeps_its_value() {
    if !have_javac() {
        eprintln!("SKIPPED: javac not installed — this test verifies NOTHING here.");
        return;
    }
    // A plain string with deliberate interior spaces. If anything re-indents INSIDE the
    // quotes, the output changes.
    let frm = r#"@@system Echo {
    interface:
        go()
    machine:
        $A {
            go() {
                String s = "left:[    ]right";
                System.out.println(s);
            }
        }
}
"#;
    let main = r#"
class Main {
    public static void main(String[] a) {
        new Echo().go();
    }
}"#;
    let out = run(frm, main, "frame_java_2");
    assert_eq!(
        out.trim(),
        "left:[    ]right",
        "the four spaces INSIDE the literal are the user's DATA and must survive verbatim"
    );
}

/// **Java makes unreachable code a COMPILE ERROR.**
///
/// A transition emits an implicit `return`, so anything after it in the same block is
/// dead — and javac will refuse the file. The old compiler handled this with
/// `strip_java_unreachable`, a post-emission text pass that deleted statements out of
/// already-generated code (a Rule-2 violation: reading text framec had just produced).
///
/// Here the tree knows the order, so the emitter simply stops. If it did not, THIS TEST
/// WOULD NOT COMPILE — javac is the oracle, not an assertion of ours.
#[test]
fn code_after_a_transition_is_not_emitted() {
    if !have_javac() {
        eprintln!("SKIPPED: javac not installed — this test verifies NOTHING here.");
        return;
    }
    let frm = r#"@@system Dead {
    interface:
        go()
    machine:
        $A {
            go() {
                System.out.println("before");
                -> $B
                System.out.println("UNREACHABLE");
            }
        }
        $B {
            go() { }
        }
}
"#;
    let main = r#"
class Main {
    public static void main(String[] a) {
        new Dead().go();
        System.out.println("survived");
    }
}"#;
    // If the emitter kept the unreachable statement, javac would reject the file and
    // `run` would panic with its stderr. Getting here at all IS the assertion.
    let out = run(frm, main, "frame_java_3");
    assert_eq!(out.lines().collect::<Vec<_>>(), ["before", "survived"]);
    assert!(
        !out.contains("UNREACHABLE"),
        "the statement after the transition must not run"
    );
}

/// **The ATOM invariant, on the real toolchain.**
///
/// A state-var read expands to a cast. If the cast is BARE — `(Integer) map.get("n")` —
/// then `.intValue()` binds on `Object` and javac rejects it. The only way to build a
/// cast here is `Atom::cast`, which parenthesizes. So the bare form is unrepresentable,
/// and javac confirms it.
///
/// (In C# the same shape did not even fail to compile — an `object` overload existed, so
/// it compiled clean, exited 0, and printed the wrong answer. #213.)
#[test]
fn a_state_var_read_is_an_atom() {
    use frame_compiler::text::emit::java::state_var_read;
    use frame_compiler::text::emit::atom::Atom;

    let read = state_var_read("Integer", "n");
    assert_eq!(
        read.as_str(),
        "((Integer) compartment.stateVars.get(\"n\"))",
        "the cast MUST be parenthesized"
    );

    // And it survives a member access — which is the operation that broke it.
    let called = Atom::method(read, "intValue", "");
    assert_eq!(
        called.as_str(),
        "((Integer) compartment.stateVars.get(\"n\")).intValue()"
    );

    if !have_javac() {
        eprintln!("SKIPPED the toolchain half: javac not installed.");
        return;
    }

    // Prove javac agrees: the parenthesized form compiles, the bare form does NOT.
    //
    // The probe needs a real `compartment`, because that is what the expansion refers
    // to. (My first version declared a bare local `stateVars`, javac rightly rejected
    // the generated expression, and my own assertion fired. The probe was wrong, not
    // the emitter — which is exactly why the toolchain is the oracle and not me.)
    let d = std::env::temp_dir().join("frame_java_atom");
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();

    let harness = |cls: &str, expr: &str| {
        format!(
            "import java.util.*;\n\
             public class {cls} {{\n\
             \x20 static class C {{ Map<String,Object> stateVars = new HashMap<>(); }}\n\
             \x20 static C compartment = new C();\n\
             \x20 public static void main(String[] a) {{\n\
             \x20   compartment.stateVars.put(\"n\", 42);\n\
             \x20   int v = {expr};\n\
             \x20   System.out.println(v);\n\
             \x20 }}\n}}\n"
        )
    };

    // THE ATOM FORM — what this compiler emits. Must compile.
    let atom_expr = Atom::method(state_var_read("Integer", "n"), "intValue", "").to_string();
    std::fs::write(d.join("Ok.java"), harness("Ok", &atom_expr)).unwrap();
    let out = Command::new("javac").arg("Ok.java").current_dir(&d).output().unwrap();
    assert!(
        out.status.success(),
        "the ATOM form `{atom_expr}` must compile:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );

    // THE BARE FORM — what the old compiler emitted. Must be REJECTED.
    // `(Integer) map.get("n").intValue()` -> `.intValue()` binds on Object.
    let bare_expr = "(Integer) compartment.stateVars.get(\"n\").intValue()";
    std::fs::write(d.join("Bad.java"), harness("Bad", bare_expr)).unwrap();
    let out = Command::new("javac").arg("Bad.java").current_dir(&d).output().unwrap();
    assert!(
        !out.status.success(),
        "the BARE cast `{bare_expr}` must be REJECTED by javac. If it compiles, this \
         test proves nothing and the atom invariant is not load-bearing here."
    );

    // And the atom form must give the RIGHT ANSWER, not merely compile. (In C# the bare
    // form compiled clean AND returned the wrong number — #213.)
    let out = Command::new("java").arg("Ok").current_dir(&d).output().unwrap();
    assert_eq!(
        String::from_utf8_lossy(&out.stdout).trim(),
        "42",
        "the atom form must also be CORRECT, not just compilable"
    );
}

/// **framec does not split arguments — and therefore cannot split them wrongly.**
///
/// Three commas here, none of them separators: one inside a string literal, two inside
/// an array initializer. The old compiler's validator counted `(` and `)` only and was
/// blind to strings, chars, comments and `[]`/`{}` — so it either REJECTED this legal
/// code (E405: "declares 3 params but transition supplies 5") or, when the miscount
/// happened to match the arity, silently dropped a state argument. Exit 0. Wrong program.
///
/// And a smarter splitter is not the fix. In C++, `f(a < b, c > d)` (two comparisons)
/// and `f(std::map<int,int>())` (one generic) are the same token shape — separating them
/// needs name lookup over the user's types, which a lexer cannot do and which C++'s own
/// grammar cannot do either.
///
/// So framec hands the blob to a varargs helper. **javac splits it.** Correctly, and for
/// free, including the arity error.
#[test]
fn framec_never_splits_arguments() {
    if !have_javac() {
        eprintln!("SKIPPED: javac not installed — this test verifies NOTHING here.");
        return;
    }
    let frm = r#"@@system Args {
    interface:
        go()
        show()
    machine:
        $A {
            go() {
                -> $B("hello, world", 9, new int[]{1, 2})
            }
        }
        $B(msg: String, n: int, arr: int[]) {
            show() {
                System.out.println("msg=" + compartment.stateArgs.get("msg"));
                System.out.println("n=" + compartment.stateArgs.get("n"));
                System.out.println("len=" + ((int[]) compartment.stateArgs.get("arr")).length);
            }
        }
}
"#;
    let main = r#"
class Main {
    public static void main(String[] a) {
        Args x = new Args();
        x.go();
        x.show();
    }
}"#;
    let out = run(frm, main, "frame_java_args");
    assert_eq!(
        out.lines().collect::<Vec<_>>(),
        ["msg=hello, world", "n=9", "len=2"],
        "the comma INSIDE the string is the user's data, and the commas inside the array \
         initializer are Java's — framec must not treat any of them as an argument \
         separator, and the way to guarantee that is to never look"
    );
}

/// **push$ / pop$ is a genuine pushdown**, and the compartment's memory travels with it.
#[test]
fn the_stack_preserves_the_compartment() {
    if !have_javac() {
        eprintln!("SKIPPED: javac not installed — this test verifies NOTHING here.");
        return;
    }
    let frm = r#"@@system Vend {
    interface:
        coin()
        pick()
        refund()
    machine:
        $Idle {
            $.credit: int = 0
            coin() {
                System.out.println("credit=" + $.credit);
                push$ -> $Paid(5)
            }
        }
        $Paid(amount: int) {
            pick() {
                System.out.println("dispensing amount=" + compartment.stateArgs.get("amount"));
                -> pop$
            }
            refund() {
                System.out.println("refunding");
                -> pop$
            }
        }
}
"#;
    let main = r#"
class Main {
    public static void main(String[] a) {
        Vend v = new Vend();
        v.coin();
        v.pick();     // pops back to Idle
        v.coin();     // Idle still works -> the pop restored a real compartment
        v.refund();
        v.pick();     // WRONG STATE -> no-op
        System.out.println("done");
    }
}"#;
    let out = run(frm, main, "frame_java_stack");
    assert_eq!(
        out.lines().collect::<Vec<_>>(),
        [
            "credit=0",
            "dispensing amount=5",
            "credit=0",
            "refunding",
            "done"
        ],
        "push must carry the state arg in, pop must restore the caller's compartment"
    );
}

/// **Types are emitted VERBATIM.** No canonical-scalar alias table.
///
/// Frame has no type system: a declared type is the USER's target-language text and
/// passes through unchanged. The shipping compiler's own source records that the alias
/// table (`str->String`, …) "was exterminated — it contradicted the passthrough
/// contract." The cleanroom briefly re-introduced it (`java_param_type`); this test
/// guards against that ever coming back.
#[test]
fn declared_types_pass_through_verbatim() {
    let frm = r#"@@system T {
    interface:
        go()
    machine:
        $A { go() { } }
    domain:
        good: String = null
        weird: Rc<RefCell<Whatever>> = null
        wrong: str = null
}
"#;
    let src = Source::new("t.frm", frm.as_bytes().to_vec()).unwrap();
    let ast = segment(&src, Target::Java).unwrap();
    let (syms, _) = resolve(&ast);
    let code = driver::emit(&src, &ast, &syms, &Java::new());

    // The user's exact type text, unchanged — including a Java-invalid one. Domain fields
    // are DECLARED with their verbatim type and ASSIGNED their init in the constructor (so
    // a domain param would be in scope for the init).
    assert!(code.contains("public String good;"), "String type verbatim on the field decl");
    assert!(code.contains("this.good = null;"), "the init is assigned in the constructor");
    assert!(
        code.contains("public Rc<RefCell<Whatever>> weird;"),
        "an arbitrary user type passes through untouched:\n{code}"
    );
    assert!(
        code.contains("public str wrong;"),
        "`str` is the USER's (Java-invalid) text — framec must NOT rewrite it to String. \
         Emitting it verbatim (and letting javac reject it) is the contract:\n{code}"
    );
}

/// **Consecutive `@@:self.x = e` each keep their terminator.**
///
/// The shipping compiler drops the `;` between adjacent native assignment segments when a
/// following Frame construct closes the body (#173 family) — a compile-breaker. The
/// cleanroom is immune by construction: `@@:self.x = e` is a typed `Stmt::Assign` that
/// framec terminates unconditionally, not a native segment terminated by a forward-pass
/// oracle. This test proves the class of bug cannot recur here.
#[test]
fn consecutive_self_assignments_keep_their_terminators() {
    let frm = r#"@@system C {
    interface:
        go(amount: int, label: String)
    machine:
        $A {
            go(amount: int, label: String) {
                @@:self.counter = amount
                @@:self.tag = label
            }
        }
    domain:
        counter: int = 0
        tag: String = ""
}
"#;
    let out = run(
        frm,
        "class Main { public static void main(String[] a) { \
            C c = new C(); c.go(42, \"hi\"); \
            System.out.println(c.counter + \" \" + c.tag); } }",
        "frame_java_consec",
    );
    assert_eq!(out.trim(), "42 hi", "both assignments must run — neither drops its `;`");
}
