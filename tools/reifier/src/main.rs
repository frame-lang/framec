//! The framec reifier — increment 1.
//!
//! Reads a native Rust `fn` and emits its Frame `@@system` per the reification calculus
//! (see README.md). framec, reifying itself. The ONLY fn left native is the OS boundary
//! (`main`/FFI) — because it cannot be a machine, not because it is exempt.

use quote::ToTokens;
use std::env;
use std::fs;
use syn::{Block, Expr, FnArg, ImplItem, Item, Pat, Signature, Stmt};

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 3 {
        eprintln!("usage: reifier <file.rs> <fn_name>");
        std::process::exit(2);
    }
    let (path, fn_name) = (&args[1], &args[2]);
    let src = fs::read_to_string(path).unwrap_or_else(|e| {
        eprintln!("read {path}: {e}");
        std::process::exit(1);
    });
    let file = syn::parse_file(&src).unwrap_or_else(|e| {
        eprintln!("parse {path}: {e}");
        std::process::exit(1);
    });

    let Some((sig, block)) = find_fn(&file, fn_name) else {
        eprintln!("fn `{fn_name}` not found in {path}");
        std::process::exit(1);
    };

    // The one carve-out: the OS boundary. Left native because it CANNOT be a machine.
    if is_os_boundary(fn_name) {
        println!("// `{fn_name}` is the OS boundary — left native (cannot be a machine).");
        return;
    }

    let sys = pascal(fn_name);
    let mut m = Machine::new();
    m.walk(&block.stmts, "Done");

    let mut out = String::new();
    out.push_str("@@[target(\"rust\")]\n\n");
    out.push_str(&format!(
        "// Reified from `{fn_name}` ({path}) by the framec reifier (increment 1).\n"
    ));
    out.push_str(&format!("@@system {sys}({}out: Sink) {{\n", domain_params(sig)));
    out.push_str("    interface:\n        step()\n");
    out.push_str("    machine:\n");
    out.push_str(&m.states);
    out.push_str("        $Done { }\n");
    out.push_str("    domain:\n");
    out.push_str(&domain_decls(sig));
    out.push_str("        out: Sink = out\n");
    out.push_str("}\n");
    print!("{out}");

    eprintln!(
        "// {fn_name} -> @@system {sys}: {} states  ({} straight, {} forks, {} cycles)",
        m.n_states, m.n_straight, m.n_forks, m.n_cycles
    );
}

/// The calculus core: walk a statement list into a chain of states, decomposing every
/// decision (`if`/`match`) into forks and every loop (`for`/`while`/`loop`) into cycles.
struct Machine {
    states: String,
    counter: usize,
    n_states: usize,
    n_straight: usize,
    n_forks: usize,
    n_cycles: usize,
}

impl Machine {
    fn new() -> Self {
        Machine {
            states: String::new(),
            counter: 0,
            n_states: 0,
            n_straight: 0,
            n_forks: 0,
            n_cycles: 0,
        }
    }

    fn fresh(&mut self, kind: &str) -> String {
        self.counter += 1;
        self.n_states += 1;
        format!("${kind}{}", self.counter)
    }

    fn push_state(&mut self, name: &str, body: &str) {
        self.states
            .push_str(&format!("        {name} {{\n            step() {{\n{body}            }}\n        }}\n"));
    }

    /// Emit states for `stmts`, ending by transitioning to `next`. Straight-line statements
    /// accumulate into one state (their native action); each `if`/`match`/loop breaks the run
    /// and becomes its own state per the calculus.
    fn walk(&mut self, stmts: &[Stmt], next: &str) {
        let mut i = 0;
        let mut pending_next = next.to_string();
        // Process from the END backwards so each segment knows the entry of what follows.
        // Simpler forward pass: collect segments, then chain. Here: forward, emitting straight
        // runs and flagging control-flow (full recursion into branches is increment 2).
        let mut run: Vec<String> = Vec::new();
        let mut segments: Vec<(String, String)> = Vec::new(); // (state_name, body)
        while i < stmts.len() {
            match classify(&stmts[i]) {
                Kind::Straight(txt) => run.push(txt),
                Kind::Fork(cond) => {
                    self.flush_run(&mut run, &mut segments);
                    let st = self.fresh("Fork");
                    self.n_forks += 1;
                    segments.push((
                        st,
                        format!("                // FORK on: {cond}\n                // (branch bodies -> states: increment 2)\n"),
                    ));
                }
                Kind::Cycle(cond) => {
                    self.flush_run(&mut run, &mut segments);
                    let st = self.fresh("Cycle");
                    self.n_cycles += 1;
                    segments.push((
                        st,
                        format!("                // CYCLE on: {cond}\n                // (cursor register + self-edge + exit: increment 2)\n"),
                    ));
                }
            }
            i += 1;
        }
        self.flush_run(&mut run, &mut segments);
        // chain the segments: each -> the next's entry, last -> `next`
        for k in 0..segments.len() {
            let target = if k + 1 < segments.len() {
                segments[k + 1].0.clone()
            } else {
                format!("${pending_next}")
            };
            let (name, body) = &segments[k];
            let body = format!("{body}                -> {target}\n");
            self.push_state(name, &body);
        }
        if segments.is_empty() {
            // empty body: a bare passthrough
            let _ = &mut pending_next;
        }
    }

    fn flush_run(&mut self, run: &mut Vec<String>, segments: &mut Vec<(String, String)>) {
        if run.is_empty() {
            return;
        }
        let st = self.fresh("Step");
        self.n_straight += 1;
        let mut body = String::new();
        for line in run.drain(..) {
            body.push_str(&format!("                @@: {line}\n"));
        }
        segments.push((st, body));
    }
}

enum Kind {
    Straight(String),
    Fork(String),
    Cycle(String),
}

fn classify(stmt: &Stmt) -> Kind {
    match stmt {
        Stmt::Expr(Expr::If(e), _) => Kind::Fork(one_line(&e.cond.to_token_stream().to_string())),
        Stmt::Expr(Expr::Match(e), _) => {
            Kind::Fork(format!("match {}", one_line(&e.expr.to_token_stream().to_string())))
        }
        Stmt::Expr(Expr::ForLoop(e), _) => {
            Kind::Cycle(format!("for {} in ...", e.pat.to_token_stream()))
        }
        Stmt::Expr(Expr::While(e), _) => {
            Kind::Cycle(format!("while {}", one_line(&e.cond.to_token_stream().to_string())))
        }
        Stmt::Expr(Expr::Loop(_), _) => Kind::Cycle("loop".into()),
        other => Kind::Straight(one_line(&other.to_token_stream().to_string())),
    }
}

fn one_line(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn find_fn<'a>(file: &'a syn::File, name: &str) -> Option<(&'a Signature, &'a Block)> {
    for item in &file.items {
        match item {
            Item::Fn(f) if f.sig.ident == name => return Some((&f.sig, &f.block)),
            Item::Impl(imp) => {
                for it in &imp.items {
                    if let ImplItem::Fn(f) = it {
                        if f.sig.ident == name {
                            return Some((&f.sig, &f.block));
                        }
                    }
                }
            }
            _ => {}
        }
    }
    None
}

fn is_os_boundary(name: &str) -> bool {
    matches!(name, "main")
}

fn pascal(s: &str) -> String {
    s.split('_')
        .map(|w| {
            let mut c = w.chars();
            match c.next() {
                Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
                None => String::new(),
            }
        })
        .collect()
}

fn param_iter(sig: &Signature) -> impl Iterator<Item = (String, String)> + '_ {
    sig.inputs.iter().filter_map(|a| match a {
        FnArg::Typed(pt) => {
            let name = match &*pt.pat {
                Pat::Ident(pi) => pi.ident.to_string(),
                other => one_line(&other.to_token_stream().to_string()),
            };
            if name == "out" {
                return None; // out: Sink is appended explicitly
            }
            Some((name, one_line(&pt.ty.to_token_stream().to_string())))
        }
        FnArg::Receiver(_) => None, // &self drops away — the fn's state IS the system
    })
}

fn domain_params(sig: &Signature) -> String {
    let ps: Vec<String> = param_iter(sig).map(|(n, t)| format!("{n}: {t}")).collect();
    if ps.is_empty() {
        String::new()
    } else {
        format!("{}, ", ps.join(", "))
    }
}

fn domain_decls(sig: &Signature) -> String {
    param_iter(sig)
        .map(|(n, t)| format!("        {n}: {t} = {n}\n"))
        .collect()
}
