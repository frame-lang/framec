//! **C — the hardest backend, generate → gcc → run → verify.**
//!
//! C has no reflection, no generics, no `Object`/`Any`. State vars live in a hand-emitted
//! `void*`-keyed map, and reading one is `*(int*)get(...)` — a **prefix deref, a NON-atom**
//! (#220). `Atom::deref` parenthesizes it to `(*((int*) get(...)))`, so it survives being
//! spliced into `a * $.n + b` or `f($.n)`. This is the case that proves the Atom model is
//! load-bearing, not decorative — and the proof is a program that RUNS.

use frame_compiler::resolve::resolve;
use frame_compiler::scan::{literals::Target, segment};
use frame_compiler::text::emit::{c::C, driver};
use frame_compiler::Source;
use std::process::Command;

fn have_gcc() -> bool {
    Command::new("gcc")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn emit(frm: &str) -> String {
    let src = Source::new("t.frm", frm.as_bytes().to_vec()).unwrap();
    let ast = segment(&src, Target::C).unwrap();
    let (syms, diags) = resolve(&ast);
    assert!(diags.is_empty(), "{diags:#?}");
    driver::emit(&src, &ast, &syms, &C::new())
}

fn run(frm: &str, main: &str, dir: &str) -> String {
    if !have_gcc() {
        return "SKIP".into();
    }
    let d = std::env::temp_dir().join(dir);
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    let src = d.join("m.c");
    std::fs::write(&src, format!("{}\n#include <stdio.h>\n{main}\n", emit(frm))).unwrap();
    let bin = d.join("m");
    let c = Command::new("gcc")
        .args(["-std=c11", "-o"])
        .arg(&bin)
        .arg(&src)
        .output()
        .unwrap();
    assert!(
        c.status.success(),
        "gcc REJECTED the generated code:\n{}",
        String::from_utf8_lossy(&c.stderr)
    );
    let o = Command::new(&bin).output().unwrap();
    assert!(o.status.success(), "the program CRASHED");
    String::from_utf8_lossy(&o.stdout).into_owned()
}

/// A machine that boxes an `int` state var into `void*`, reads it back through the deref,
/// mutates it, transitions, and returns it. Compiles on gcc and runs.
#[test]
fn a_generated_c_machine_runs() {
    let frm = r#"@@system Counter {
    interface:
        inc()
        add(by: int)
        get(): int
    machine:
        $Counting {
            $.n: int = 0
            inc() { $.n = $.n + 1 }
            add(by: int) { $.n = $.n + by }
            get(): int { @@:($.n) }
        }
}
"#;
    let out = run(
        frm,
        "int main(){ Counter* c=Counter_new(); Counter_inc(c); Counter_inc(c); Counter_add(c,40); printf(\"%d\\n\", Counter_get(c)); return 0; }",
        "c_counter",
    );
    if out == "SKIP" {
        eprintln!("SKIPPED gcc — verifies nothing.");
        return;
    }
    assert_eq!(out.trim(), "42", "boxed into void*, read back through the deref, = 42");
}

/// **The state-var read is a PARENTHESIZED deref**, so splicing it into a larger
/// expression cannot re-associate. This is the #220 guarantee, at the C source level.
#[test]
fn a_state_var_read_is_a_parenthesized_deref() {
    let frm = r#"@@system S {
    interface:
        double_it(): int
    machine:
        $A {
            $.n: int = 21
            double_it(): int { @@:($.n * 2) }
        }
}
"#;
    let code = emit(frm);
    // The `*(int*)` deref must be wrapped: `(*((int*) ...))`, NOT a bare `*(int*)...`.
    // A bare deref spliced into `... * 2` would deref the product, not the value.
    assert!(
        code.contains("(*((int*)"),
        "the state-var read must be a PARENTHESIZED deref (#220):\n{code}"
    );
    // And it computes correctly.
    let out = run(
        frm,
        "int main(){ S* s=S_new(); printf(\"%d\\n\", S_double_it(s)); return 0; }",
        "c_deref",
    );
    if out == "SKIP" {
        return;
    }
    assert_eq!(out.trim(), "42", "21 * 2 — the deref bound correctly inside the product");
}

/// **A domain param is a constructor arg, in scope for the domain field init** (spec §88),
/// and the call site fills defaults / routes args (§1155). `@@Counter(100)` seeds the field
/// from the param; `@@Counter()` falls back to the declared default. The `main` is water in
/// the `.frm`, so framec lowers the `@@Counter(...)` calls (spelled `Counter_new(...)`).
#[test]
fn a_system_param_is_load_bearing() {
    let frm = r#"@@system Counter(start: int = 7) {
    interface:
        get(): int
    machine:
        $C { get(): int { @@:(@@:self.count) } }
    domain:
        count: int = start
}

#include <stdio.h>
int main() {
    Counter* a = @@Counter(100);
    Counter* b = @@Counter();
    printf("%d %d\n", Counter_get(a), Counter_get(b));
    Counter_destroy(a);
    Counter_destroy(b);
    return 0;
}
"#;
    let out = run(frm, "", "c_param");
    if out == "SKIP" {
        return;
    }
    assert_eq!(
        out.trim(),
        "100 7",
        "@@Counter(100) seeds count from the param; @@Counter() uses the default 7"
    );
}

/// **An embedded-system call `@@:self.child.method()` lowers to C's cross-system free
/// function** `Child_method(self->child, ...)` (RFC-0046), not `self->child.method()` —
/// which is invalid C (structs have no methods). Compiles AND runs.
#[test]
fn an_embedded_system_call_uses_the_free_function_form() {
    let frm = r#"@@system Inner {
    interface:
        ping(): int
    machine:
        $A { ping(): int { @@:(7) } }
}

@@system Outer {
    interface:
        relay(): int
    machine:
        $A { relay(): int { @@:(@@:self.inner.ping()) } }
    domain:
        inner: Inner* = @@Inner()
}

#include <stdio.h>
int main() {
    Outer* o = @@Outer();
    printf("%d\n", Outer_relay(o));
    Outer_destroy(o);
    return 0;
}
"#;
    let out = run(frm, "", "c_embed");
    if out == "SKIP" {
        return;
    }
    assert_eq!(out.trim(), "7", "@@:self.inner.ping() ran the child's dispatch via Inner_ping(self->inner)");
}

/// A `push$`/`pop$` pushdown, in C, with the compartment stack. Runs.
#[test]
fn the_c_stack_is_a_pushdown() {
    let frm = r#"@@system Vend {
    interface:
        coin()
        pick()
        where(): int
    machine:
        $Idle {
            coin() { push$ -> $Paid }
            where(): int { @@:(0) }
        }
        $Paid {
            pick() { -> pop$ }
            where(): int { @@:(1) }
        }
}
"#;
    let out = run(
        frm,
        "int main(){ Vend* v=Vend_new(); printf(\"%d\", Vend_where(v)); Vend_coin(v); printf(\"%d\", Vend_where(v)); Vend_pick(v); printf(\"%d\\n\", Vend_where(v)); return 0; }",
        "c_stack",
    );
    if out == "SKIP" {
        return;
    }
    assert_eq!(out.trim(), "010", "Idle -> push Paid -> pop Idle");
}
