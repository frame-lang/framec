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

/// **Item Zero: a `@@[scan(u8)]` system is a positioned, borrowed-input scanner** (RFC-0042.1
/// / #209) — the `@@system` analogue of an `@@fsm` recognizer, and the capability that lets
/// the compiler's own scanners be systems instead of hand-rolled byte-loops. `over(&bytes)`
/// borrows with zero copy; `scan_at(i)` scans a prefix from position `i`; the drive is
/// ITERATIVE (a 50 000-byte input does not blow the stack — the linearity #209 is about);
/// it accepts iff it ends in `$Accept`, leaving the match extent in `cursor`.
#[test]
fn a_scan_system_is_a_positioned_borrowed_input_scanner() {
    let frm = r#"@@[scan(u8)]
@@system StrScan {
    interface:
        step()
    machine:
        $Start {
            step() {
                if self.cursor >= self.src.fsm_len() || self.src.fsm_get(self.cursor) != 34 {
                    -> $Reject
                }
                self.cursor = self.cursor + 1;
                -> $Body
            }
        }
        $Body {
            step() {
                if self.cursor >= self.src.fsm_len() {
                    -> $Reject
                }
                let b = self.src.fsm_get(self.cursor);
                if b == 92 {
                    self.cursor = self.cursor + 2;
                    -> $Body
                }
                if b == 34 {
                    self.cursor = self.cursor + 1;
                    -> $Accept
                }
                self.cursor = self.cursor + 1;
                -> $Body
            }
        }
        $Accept { }
        $Reject { }
}
"#;
    let out = run(
        frm,
        r#"fn main() {
    let s: &[u8] = b"\"he\\\"llo\" x";
    let mut m = StrScan::over(s);
    let ok = m.scan_at(0);
    let n: &[u8] = b"nope";
    let mut r = StrScan::over(n);
    let mut big = vec![34u8]; for _ in 0..50000 { big.push(b'x'); } big.push(34);
    let mut b = StrScan::over(&big[..]);
    println!("{} {} {} {}", ok, m.cursor, r.scan_at(0), b.scan_at(0));
}"#,
        "rust_scan",
    );
    if out == "SKIP" {
        return;
    }
    assert_eq!(
        out.trim(),
        "true 9 false true",
        "borrowed zero-copy scan: accepts \"he\\\"llo\" (cursor 9), rejects `nope`, and scans 50k iteratively"
    );
}

/// **Two systems in one file compile** — the shared `Compartment` scaffold is emitted once
/// at file scope, not per system. Re-emitting a top-level `struct Compartment` per system
/// was an E0428 ("defined multiple times"); C shares it, the nesting targets scope it, and
/// Rust now emits it once too.
#[test]
fn two_systems_in_one_file_compile_and_run() {
    let frm = r#"@@system Alpha {
    interface: a(): i32
    machine: $S { a(): i32 { @@:(1) } }
}
@@system Beta {
    interface: b(): i32
    machine: $S { b(): i32 { @@:(2) } }
}
"#;
    let out = run(
        frm,
        r#"fn main() { let mut a = Alpha::new(); let mut b = Beta::new(); println!("{} {}", a.a(), b.b()); }"#,
        "rust_multisys",
    );
    if out == "SKIP" {
        return;
    }
    assert_eq!(out.trim(), "1 2", "both systems construct and dispatch; one shared Compartment");
}

/// **A domain param is a constructor arg, in scope for the domain field init** (spec §88),
/// and the call site fills defaults / routes args (§1155). `@@Counter(100)` seeds the
/// field from the param; `@@Counter()` falls back to the param's declared default.
#[test]
fn a_system_param_is_load_bearing() {
    let frm = r#"@@system Counter(start: i32 = 7) {
    interface:
        get(): i32
    machine:
        $C { get(): i32 { @@:(@@:self.count) } }
    domain:
        count: i32 = start
}

fn main() {
    let mut a = @@Counter(100);
    let mut b = @@Counter();
    println!("{} {}", a.get(), b.get());
}
"#;
    let out = run(frm, "", "rust_param");
    if out == "SKIP" {
        return;
    }
    assert_eq!(
        out.trim(),
        "100 7",
        "@@Counter(100) seeds count from the param; @@Counter() uses the default 7"
    );
}

/// **`@@Machine()` instantiation lowers to the target constructor** (spec §1103) in
/// top-level native water — Frame's own syntax, spelled `Counter::new()` on Rust. Before
/// this, `@@Counter()` was emitted verbatim and rustc rejected it. (Single system: Rust
/// does not yet namespace the per-system `Compartment` helper, so two systems in one file
/// collide — a separate gap, tracked apart from instantiation.)
#[test]
fn system_instantiation_lowers_to_the_constructor() {
    let frm = r#"@@system Counter {
    interface:
        inc()
        get(): i32
    machine:
        $C {
            $.n: i32 = 5
            inc() { $.n = $.n + 1 }
            get(): i32 { @@:($.n) }
        }
}
"#;
    // The `main` must live INSIDE the frm as top-level water so framec processes it;
    // text appended by the harness never passes through the compiler.
    let frm = format!(
        "{frm}\nfn main() {{ let mut c = @@Counter(); c.inc(); println!(\"{{}}\", c.get()); }}\n"
    );
    let out = run(&frm, "", "rust_instantiate");
    if out == "SKIP" {
        return;
    }
    assert_eq!(out.trim(), "6", "@@Counter() in water lowers to Counter::new(); 5 + 1 = 6");
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
