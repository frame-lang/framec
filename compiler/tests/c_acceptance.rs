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

/// **The state-var read is a typed union-field access** — `self->compartment->vars.A.n` —
/// a member-access chain that binds at the highest precedence, so splicing it into a larger
/// expression cannot re-associate. No `*` deref (the typed compartment retired the boxed
/// `*(int*)` form and its #220 hazard along with it).
#[test]
fn a_state_var_read_is_a_typed_field_access() {
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
    assert!(
        code.contains("self->compartment->vars.A.n"),
        "the state-var read must be a typed union-field access:\n{code}"
    );
    // And it computes correctly: `self->compartment->vars.A.n * 2` binds `.n` before `*`.
    let out = run(
        frm,
        "int main(){ S* s=S_new(); printf(\"%d\\n\", S_double_it(s)); return 0; }",
        "c_deref",
    );
    if out == "SKIP" {
        return;
    }
    assert_eq!(out.trim(), "42", "21 * 2 — the field read bound correctly inside the product");
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

/// **`(exit) -> (enter) pop$` delivers BOTH arg sets** (RFC-0048): the exit args to the
/// leaving state's `<$`, and the enter args to the RESTORED state's `$>` — the latter via
/// a runtime state dispatch, since the popped state is dynamic. Floats round-trip by being
/// passed to the typed handler directly (no `_Generic` box needed). Compiles AND runs.
#[test]
fn a_pop_delivers_both_exit_and_enter_args() {
    let frm = r#"@@system PopArgs {
    interface:
        go()
        finish()
        peek_enter(): float
        peek_exit(): float
    machine:
        $Idle {
            $>(x: float) { @@:self.enter_seen = x; }
            go() { push$ -> $Work }
            peek_enter(): float { @@:(@@:self.enter_seen) }
            peek_exit(): float { @@:(@@:self.exit_seen) }
        }
        $Work {
            <$(y: float) { @@:self.exit_seen = y; }
            finish() { (2.5) -> (3.25) pop$ }
        }
    domain:
        enter_seen: float = 0.0
        exit_seen: float = 0.0
}

#include <stdio.h>
int main() {
    PopArgs* p = @@PopArgs();
    PopArgs_go(p);
    PopArgs_finish(p);
    printf("%.2f %.2f\n", PopArgs_peek_enter(p), PopArgs_peek_exit(p));
    PopArgs_destroy(p);
    return 0;
}
"#;
    let out = run(frm, "", "c_popargs");
    if out == "SKIP" {
        return;
    }
    assert_eq!(
        out.trim(),
        "3.25 2.50",
        "enter arg 3.25 reached the restored $Idle.$>; exit arg 2.5 reached $Work.<$"
    );
}

/// **`@@[async]` (or an async member) on C is E722, not a silent sync miscompile.** C has
/// no coroutine/future runtime (RFC-0044), so framec rejects the async surface at
/// validation rather than emitting sync code that quietly drops the async contract —
/// matching the shipped compiler's E722.
#[test]
fn async_on_c_is_rejected_e722() {
    let frm = r#"@@[async]
@@system AsyncFetcher {
    interface:
        async fetch(): int
    machine:
        $Ready { fetch(): int { @@:(0) } }
}
"#;
    let src = Source::new("t.frm", frm.as_bytes().to_vec()).unwrap();
    let ast = segment(&src, Target::C).unwrap();
    let (syms, _) = resolve(&ast);
    let diags = driver::target_diagnostics(&ast, &syms, &C::new());
    assert!(
        diags.iter().any(|d| d.code == "E722"),
        "@@[async] on C must raise E722; got {diags:#?}"
    );
}

/// The same async system on an async-CAPABLE target raises no E722 — the gate is scoped
/// to targets without an async runtime.
#[test]
fn async_is_fine_on_an_async_capable_target() {
    use frame_compiler::text::emit::rust::Rust;
    let frm = r#"@@[async]
@@system AsyncFetcher {
    interface:
        async fetch(): i32
    machine:
        $Ready { fetch(): i32 { @@:(0) } }
}
"#;
    let src = Source::new("t.frm", frm.as_bytes().to_vec()).unwrap();
    let ast = segment(&src, Target::Rust).unwrap();
    let (syms, _) = resolve(&ast);
    let diags = driver::target_diagnostics(&ast, &syms, &Rust);
    assert!(diags.is_empty(), "async on Rust must NOT raise E722; got {diags:#?}");
}

/// **A `@@Sub()` initializer on a STATE var is lowered** (not emitted raw), same as a
/// domain field. On C the box carries a `Sub*` holding `Sub_new()`; reading it back and
/// calling the child's interface returns the child's value. Compiles AND runs.
#[test]
fn a_state_var_initialized_from_a_system_is_constructed() {
    let frm = r#"@@system Sub {
    interface:
        ping(): int
    machine:
        $A { ping(): int { @@:(9) } }
}

@@system Outer {
    interface:
        use_child(): int
    machine:
        $A {
            $.child: Sub* = @@Sub()
            use_child(): int { @@:(Sub_ping($.child)) }
        }
}

#include <stdio.h>
int main() {
    Outer* o = @@Outer();
    printf("%d\n", Outer_use_child(o));
    Outer_destroy(o);
    return 0;
}
"#;
    let out = run(frm, "", "c_statevar_sys");
    if out == "SKIP" {
        return;
    }
    assert_eq!(out.trim(), "9", "$.child = @@Sub() constructed the child; Sub_ping($.child) = 9");
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
