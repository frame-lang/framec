//! **Rust — the third backend, generate → rustc → run → verify.**
//!
//! Rust is the hardest stress-test of the "backends are only spellings" claim: no
//! `Object` (state vars live in `HashMap<String, Box<dyn Any>>`), no null, ownership,
//! and postfix `.await`. The shared driver flexed to it with no escape hatch — every
//! difference below is a spelling, and the proof is a program that RUNS.

use frame_compiler::resolve::resolve;
use frame_compiler::scan::{literals::Target, segment};
use frame_compiler::text::emit::{driver, rust::Rust};
use frame_compiler::Source;
use std::process::Command;

fn have_rustc() -> bool {
    Command::new("rustc")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn emit(frm: &str) -> String {
    let src = Source::new("t.frm", frm.as_bytes().to_vec()).unwrap();
    let ast = segment(&src, Target::Rust).unwrap();
    let (syms, diags) = resolve(&ast);
    assert!(diags.is_empty(), "{diags:#?}");
    driver::emit(&src, &ast, &syms, &Rust)
}

/// Compile `code` + a `main`, run it, return stdout.
fn run(frm: &str, main: &str, dir: &str) -> String {
    if !have_rustc() {
        return "SKIP".into();
    }
    let d = std::env::temp_dir().join(dir);
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    let src = d.join("m.rs");
    std::fs::write(&src, format!("{}\n{main}\n", emit(frm))).unwrap();

    let bin = d.join("m");
    let c = Command::new("rustc")
        .args(["--edition", "2021", "-o"])
        .arg(&bin)
        .arg(&src)
        .output()
        .unwrap();
    assert!(
        c.status.success(),
        "rustc REJECTED the generated code:\n{}",
        String::from_utf8_lossy(&c.stderr)
    );
    let o = Command::new(&bin).output().unwrap();
    assert!(
        o.status.success(),
        "the generated program CRASHED:\n{}",
        String::from_utf8_lossy(&o.stderr)
    );
    String::from_utf8_lossy(&o.stdout).into_owned()
}

/// A machine that transitions, reads a state var out of `Box<dyn Any>`, and returns a
/// value. Compiles and runs.
#[test]
fn a_generated_rust_machine_runs() {
    let frm = r#"@@system Door {
    interface:
        open()
        close()
        report(): String
    machine:
        $Closed {
            $.tries: i32 = 0
            open() {
                $.tries = $.tries + 1
                -> $Open
            }
            report(): String { @@:(format!("closed, tries={}", $.tries)) }
        }
        $Open {
            close() { -> $Closed }
            report(): String { @@:(String::from("open")) }
        }
}
"#;
    let out = run(
        frm,
        r#"fn main() {
    let mut d = Door::new();
    println!("{}", d.report());
    d.open();
    println!("{}", d.report());
    d.close();
    println!("{}", d.report());
}"#,
        "rust_door",
    );
    if out == "SKIP" {
        eprintln!("SKIPPED rustc — verifies nothing.");
        return;
    }
    assert_eq!(
        out.lines().collect::<Vec<_>>(),
        // tries=0: state vars are PER-COMPARTMENT and re-seeded on each entry to Closed.
        ["closed, tries=0", "open", "closed, tries=0"],
        "the machine must transition, and the state var reads correctly out of the Any box"
    );
}

/// **The domain initializer is emitted VERBATIM** — the user's native expression, not a
/// default framec invents. `Cache = Cache` must survive; a default (`Cache::default()`)
/// would not compile, because the user's type need not implement `Default`.
#[test]
fn a_domain_initializer_is_verbatim() {
    let frm = r#"struct Cache { hits: i32 }
impl Cache { fn new() -> Cache { Cache { hits: 7 } } }

@@system S {
    interface:
        peek(): i32
    machine:
        $A { peek(): i32 { @@:(@@:self.cache.hits) } }
    domain:
        cache: Cache = Cache::new()
}
"#;
    let out = run(
        frm,
        r#"fn main() { let mut s = S::new(); println!("{}", s.peek()); }"#,
        "rust_init",
    );
    if out == "SKIP" {
        return;
    }
    assert_eq!(out.trim(), "7", "the user's `= Cache::new()` init must be emitted verbatim");
}

/// **State vars live in framec's own `Box<dyn Any>` container, read via downcast** — and
/// the read is a postfix chain (an atom), so it needs no parenthesization and side-steps
/// the borrow checker via `.clone()`.
#[test]
fn a_state_var_reads_out_of_the_any_box() {
    let frm = r#"@@system Counter {
    interface:
        inc()
        get(): i32
    machine:
        $C {
            $.n: i32 = 10
            inc() { $.n = $.n + 5 }
            get(): i32 { @@:($.n) }
        }
}
"#;
    let out = run(
        frm,
        r#"fn main() { let mut c = Counter::new(); c.inc(); c.inc(); println!("{}", c.get()); }"#,
        "rust_statevar",
    );
    if out == "SKIP" {
        return;
    }
    assert_eq!(out.trim(), "20", "10 + 5 + 5, read back out of Box<dyn Any>");
}
