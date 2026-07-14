//! `framec-ng` — the rebuilt compiler.
//!
//! Today it does exactly one thing, and that is deliberate: **`--dump-ast`**.
//!
//! This is not an output feature. It is the **test oracle for the front end**, and it
//! is built FIRST for a reason the old compiler taught us the hard way: the only way
//! to see what framec understood was to read the target code it emitted — which is
//! precisely why twenty-five bugs hid for so long. Building the tree before the tool
//! that lets you look at the tree would reproduce that exact condition inside the
//! rebuild.

use frame_compiler::scan::{literals::Target, segment};
use frame_compiler::Source;
use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    let mut path = None;
    let mut target = Target::Python3;
    let mut dump = false;
    let mut emit = false;

    let mut k = 1;
    while k < args.len() {
        match args[k].as_str() {
            "--dump-ast" => dump = true,
            "--emit" => emit = true,
            "-l" | "--language" => {
                k += 1;
                match args.get(k).map(String::as_str) {
                    Some(n) => match Target::ALL.iter().find(|t| t.name() == n) {
                        Some(t) => target = *t,
                        None => {
                            eprintln!("unknown target: {n}");
                            return ExitCode::from(2);
                        }
                    },
                    None => {
                        eprintln!("-l needs a target");
                        return ExitCode::from(2);
                    }
                }
            }
            other => path = Some(other.to_string()),
        }
        k += 1;
    }

    let Some(path) = path else {
        eprintln!("usage: framec-ng [-l <target>] --dump-ast <file>");
        return ExitCode::from(2);
    };

    let bytes = match std::fs::read(&path) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("{path}: {e}");
            return ExitCode::from(2);
        }
    };

    let src = match Source::new(&path, bytes) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("{e}");
            return ExitCode::from(65);
        }
    };

    let ast = match segment(&src, target) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("{path}: {e:?}");
            return ExitCode::from(65);
        }
    };

    // Both invariants. A tree that fails these is a COMPILER BUG, and it is reported
    // as one — loudly — rather than handed to a later pass that will quietly emit a
    // wrong program from it.
    if let Err(defect) = ast.check() {
        eprintln!("{path}: {defect}");
        return ExitCode::from(70); // EX_SOFTWARE: our fault, not the user's
    }

    if emit {
        let (syms, diags) = frame_compiler::resolve::resolve(&ast);
        let mut bad = false;
        for d in diags
            .iter()
            .chain(frame_compiler::validate::validate(&ast, &syms).iter())
        {
            let (l, c) = src.line_col(d.span.start);
            eprintln!("{path}:{l}:{c}: {}: {}", d.code, d.message);
            bad = true;
        }
        if bad {
            return ExitCode::from(65);
        }
        use frame_compiler::text::emit::{driver, java::Java, python::Python};
        use frame_compiler::text::emit::rust::Rust;
        let jb = Java::new();
        let be: &dyn driver::Backend = match target {
            Target::Java => &jb,
            Target::Python3 => &Python,
            Target::Rust => &Rust,
            Target::C => &frame_compiler::text::emit::c::C::new(),
            other => {
                eprintln!(
                    "no backend for `{}` yet. Built: java, python.",
                    other.name()
                );
                return ExitCode::from(2);
            }
        };
        print!("{}", driver::emit(&src, &ast, &syms, be));
        return ExitCode::SUCCESS;
    }

    if dump {
        for item in &ast.items {
            let s = item.span();
            let (line, col) = src.line_col(s.start);
            println!(
                "{:<8} {:>5}..{:<5} {}:{:<3} {}",
                kind_of(item),
                s.start,
                s.end,
                line,
                col,
                name_of(item)
            );
        }
    }
    ExitCode::SUCCESS
}

fn kind_of(i: &frame_compiler::tree::Item) -> &'static str {
    use frame_compiler::tree::Item::*;
    match i {
        Bom(_) => "BOM",
        Native(_) => "native",
        Pragma(_) => "pragma",
        System(_) => "SYSTEM",
        Efsm(_) => "EFSM",
    }
}

fn name_of(i: &frame_compiler::tree::Item) -> &str {
    use frame_compiler::tree::Item::*;
    match i {
        System(s) => &s.name,
        Efsm(e) => &e.name,
        _ => "",
    }
}
