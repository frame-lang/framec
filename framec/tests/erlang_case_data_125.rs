//! Issue #125 — an Erlang handler whose `if`/`else if`/`else` (or no-else `if`)
//! body MUTATES state but does NOT terminate, followed by trailing code that
//! reads/mutates state, must thread the case's resulting `Data` out to the
//! trailing statements.
//!
//! Root cause (`codegen/output_block_parser_erlang.frs`): the brace → `case …
//! of` lowering treated EVERY no-else `if` followed by trailing code as a
//! C-family "early exit" — folding the trailing code into the synthesized
//! `false` arm and deferring the `end`. That is only correct when the `if`
//! body actually returns/transitions. For a non-terminal body the trailing
//! code must run UNCONDITIONALLY on both paths (after the `end`), and its
//! reads of the if-body's `DataN` were left stale on the false path.
//!
//! Fix: gate the early-exit deferral on `body_is_terminal` — the body's last
//! statement begins with a transition / reply-tuple / `@@:return(expr)`
//! short-circuit sentinel. Non-terminal bodies close the case with `; false ->
//! ok` + immediate `end`, so trailing code follows the `end`. A companion fix
//! in `erlang_system.rs` stops wrapping such an intermediate case in a spurious
//! `__ReturnVal = case …` when the `@@:return` is a top-level statement AFTER
//! the case (it now flows through the linear path, replying with the SSA
//! `__ReturnVal_K`).
//!
//! Validation: each repro is asserted structurally on the generated Erlang AND
//! compiled with `erlc` + run under `escript` for its actual runtime values
//! when the toolchain is on PATH (skipped otherwise, mirroring the snapshot
//! suite).

mod common;
use common::{compile_source, find_tool};
use std::process::Command;

/// Extract the dispatch clause for `event` (`<state>({call, From}, … {event …`)
/// up to the next clause header (a line starting at column 0 ending in ` ->`).
fn handler_clause(code: &str, event: &str) -> String {
    let lines: Vec<&str> = code.lines().collect();
    let needle = format!("{{{event},", event = event);
    let start = lines
        .iter()
        .position(|l| l.contains("({call, From},") && l.contains(&needle))
        .unwrap_or_else(|| panic!("handler clause for {event} not found:\n{code}"));
    let mut out = vec![lines[start]];
    for l in &lines[start + 1..] {
        // A new top-level clause header starts in column 0 and ends in ` ->`.
        let is_new_clause = !l.starts_with(' ') && l.trim_end().ends_with(" ->");
        if is_new_clause {
            break;
        }
        out.push(l);
        if l.trim_end().ends_with(';') && !l.trim_start().starts_with('%') {
            break;
        }
    }
    out.join("\n")
}

/// The `Data` SSA var name bound just before the final `{keep_state, …}` reply,
/// and the var the reply tuple actually threads — they must match (the trailing
/// post-case mutation's `Data` reaches the reply, not a stale pre-case one).
fn reply_data_var(clause: &str) -> String {
    let reply = clause
        .lines()
        .rev()
        .find(|l| l.contains("{keep_state,"))
        .unwrap_or_else(|| panic!("no keep_state reply in clause:\n{clause}"));
    let after = reply.split("{keep_state,").nth(1).unwrap();
    after
        .split(',')
        .next()
        .unwrap()
        .trim()
        .trim_end_matches('}')
        .to_string()
}

/// Compile to Erlang, write the module + an escript driver, and run it under
/// `escript`. Returns `None` (treated as skip) when `escript`/`erlc` absent.
fn run_escript(src: &str, module: &str, driver_body: &str) -> Option<String> {
    let escript = find_tool("escript")?;
    let compiled = compile_source(src, "erlang");
    let dir = tempfile::tempdir().expect("tempdir");
    let mod_path = dir.path().join(format!("{module}.erl"));
    std::fs::write(&mod_path, &compiled).expect("write module");

    // Compile the module first with erlc so the escript can call it.
    if let Some(erlc) = find_tool("erlc") {
        let out = Command::new(&erlc)
            .arg("-o")
            .arg(dir.path())
            .arg(&mod_path)
            .output()
            .unwrap_or_else(|e| panic!("spawn erlc: {e}"));
        assert!(
            out.status.success(),
            "generated Erlang rejected by erlc:\n--- stderr ---\n{}\n--- source ---\n{}",
            String::from_utf8_lossy(&out.stderr),
            compiled
        );
    }

    let driver = format!(
        "#!/usr/bin/env escript\n%%! -pa {}\nmain(_) ->\n{}\n",
        dir.path().display(),
        driver_body
    );
    let driver_path = dir.path().join("drv.escript");
    std::fs::write(&driver_path, driver).expect("write driver");
    let out = Command::new(&escript)
        .arg(&driver_path)
        .output()
        .unwrap_or_else(|e| panic!("spawn escript: {e}"));
    assert!(
        out.status.success(),
        "escript run failed:\n--- stderr ---\n{}\n--- module ---\n{}",
        String::from_utf8_lossy(&out.stderr),
        compiled
    );
    Some(String::from_utf8_lossy(&out.stdout).to_string())
}

/// The prompt's exact repro: a symmetric `if / else if / else` chain whose arms
/// each mutate state, then trailing `self.total = self.hits + 1` + `@@:(...)`.
const ELSE_IF: &str = r#"
@@[target("erlang")]
@@[main]
@@system M {
    interface: classify(n: integer): integer
    machine: $S {
        classify(n: integer): integer {
            self.hits = self.hits + 1
            if n < 0 {
                self.neg = self.neg + 1
            } else if n == 0 {
                self.zero = self.zero + 1
            } else {
                self.pos = self.pos + 1
            }
            self.total = self.hits + 1
            @@:(self.total)
        }
    }
    domain:
        hits: integer = 0
        neg: integer = 0
        zero: integer = 0
        pos: integer = 0
        total: integer = 0
}
"#;

#[test]
fn else_if_threads_data_to_trailing_code() {
    let code = compile_source(ELSE_IF, "erlang");
    let clause = handler_clause(&code, "classify");

    // The case must NOT be wrapped as a value-producing expression — the
    // `@@:return` is a top-level statement AFTER the case, not the case value.
    assert!(
        !clause.contains("__ReturnVal = case"),
        "intermediate case wrongly hoisted as __ReturnVal value:\n{clause}",
    );
    // The trailing `self.total = …` mutation must appear AFTER the case `end`,
    // producing a fresh DataN, and the reply must thread THAT var.
    let end_pos = clause
        .find("\n    end")
        .expect("case must close with `end`");
    assert!(
        clause[end_pos..].contains("#data{total ="),
        "trailing `total` mutation must run after the case end:\n{clause}",
    );
    // The reply threads the post-case Data var (not a stale pre-case one). The
    // pre-case bind is Data1 (hits); the post-case total bind is a higher gen.
    let reply_var = reply_data_var(&clause);
    assert_ne!(
        reply_var, "Data1",
        "reply must use the post-case Data, not the pre-case Data1:\n{clause}",
    );
    assert!(
        reply_var.starts_with("Data"),
        "reply must thread a DataN var, got {reply_var}:\n{clause}",
    );
}

/// A no-else `if` whose body MUTATES (non-terminal), with trailing code that
/// reads the mutated state on BOTH paths. This is the sharpest #125 case: the
/// pre-fix lowering folded the trailing code into the false arm.
const NO_ELSE_MUTATE: &str = r#"
@@[target("erlang")]
@@[main]
@@system M2 {
    interface: bump(n: integer): integer
    machine: $S {
        bump(n: integer): integer {
            self.a = self.a + 1
            if n > 0 {
                self.b = self.b + 1
                self.c = self.c + 1
            }
            self.total = self.a + self.b + self.c
            @@:(self.total)
        }
    }
    domain:
        a: integer = 0
        b: integer = 0
        c: integer = 0
        total: integer = 0
}
"#;

#[test]
fn no_else_mutate_runs_trailing_unconditionally() {
    let code = compile_source(NO_ELSE_MUTATE, "erlang");
    let clause = handler_clause(&code, "bump");

    // Non-terminal no-else `if` closes with `; false -> ok` + immediate `end`.
    assert!(
        clause.contains("; false -> ok"),
        "non-terminal no-else `if` must get an `ok` false arm:\n{clause}",
    );
    // The trailing total mutation runs after the `end` (both paths).
    let end_pos = clause
        .find("\n    end")
        .expect("case must close with `end`");
    assert!(
        clause[end_pos..].contains("#data{total ="),
        "trailing total must run after the case end (both paths):\n{clause}",
    );
    // It must NOT be inside the false arm (between `; false ->` and `end`).
    let false_pos = clause.find("; false").expect("false arm");
    assert!(
        !clause[false_pos..end_pos].contains("#data{total ="),
        "trailing total must NOT be folded into the false arm:\n{clause}",
    );
}

/// Regression guard: a no-else `if` whose body SHORT-CIRCUITS via
/// `@@:return(expr)` is a genuine C-family early exit — trailing code is the
/// "didn't take the branch" continuation and DOES belong in the false arm.
/// The fix must preserve this (it's the `09_return_explicit` snapshot shape).
const NO_ELSE_RETURN: &str = r#"
@@[target("erlang")]
@@system M3 {
    interface: decide(score: integer): string
    machine: $S {
        decide(score: integer): string {
            if score >= 60 {
                @@:return("pass")
            }
            @@:return("fail")
        }
    }
}
"#;

#[test]
fn no_else_short_circuit_return_stays_early_exit() {
    let code = compile_source(NO_ELSE_RETURN, "erlang");
    let clause = handler_clause(&code, "decide");
    // Early-exit shape: `; false ->` carrying the trailing return, NOT
    // `; false -> ok` with the return after the end.
    assert!(
        clause.contains("; false ->") && !clause.contains("; false -> ok"),
        "short-circuit `@@:return` body must stay an early exit:\n{clause}",
    );
    // The trailing "fail" return must be inside the false arm (before `end`).
    let false_pos = clause.find("; false ->").expect("false arm");
    let end_pos = clause.find("\n    end").expect("end");
    assert!(
        clause[false_pos..end_pos].contains("\"fail\""),
        "trailing return must land in the false arm:\n{clause}",
    );
}

// ── Run-on-real-Erlang validations (skipped when escript/erlc absent) ──

#[test]
fn else_if_runs_correct_values() {
    let driver = r#"    P = m:create(),
    io:format("~p ~p ~p~n", [m:classify(P, -5), m:classify(P, 0), m:classify(P, 7)])."#;
    match run_escript(ELSE_IF, "m", driver) {
        Some(out) => {
            // hits increments each call (1,2,3); total = hits + 1 → 2,3,4.
            assert_eq!(
                out.trim(),
                "2 3 4",
                "classify must thread Data through the case to `total`: {out:?}",
            );
        }
        None => eprintln!("#125 else_if run skipped: escript not on PATH"),
    }
}

#[test]
fn no_else_mutate_runs_correct_values() {
    let driver = r#"    P = m2:create(),
    io:format("~p ~p~n", [m2:bump(P, 5), m2:bump(P, -1)])."#;
    match run_escript(NO_ELSE_MUTATE, "m2", driver) {
        Some(out) => {
            // call 1 (n=5): a=1,b=1,c=1 → total=3.
            // call 2 (n=-1): a=2 (b,c unchanged at 1) → total=4.
            assert_eq!(
                out.trim(),
                "3 4",
                "bump must run trailing total on both paths: {out:?}",
            );
        }
        None => eprintln!("#125 no_else run skipped: escript not on PATH"),
    }
}
