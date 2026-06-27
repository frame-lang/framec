//! Issue #132 — Erlang/gen_statem backend mechanical/operation defects.
//!
//! The A/B/C/D batch (mechanical lowering):
//!
//! - **A** — a state named after an Erlang reserved word (`$Begin`)
//!   must emit a *quoted* atom (`'begin'`) everywhere it lands as an
//!   atom or state-function name; a bare `begin` is a syntax error.
//! - **B** — an uppercase / PascalCase domain field (`Big`) must emit
//!   a lowercase (or quoted) Erlang record field; `#data{ Big = … }`
//!   is invalid.
//! - **C** — a state-arg read inside a `$>` (frame_enter) handler body
//!   must be bound; the enter path historically emitted the read
//!   lowercase while binding it capitalized → unbound variable.
//! - **D** — a brace-`if` inside an *operation* body must be lowered to
//!   `case … of`, the same as a handler body; the raw `if v > hi { … }`
//!   is a syntax error.
//!
//! The E/F/G batch (gen_statem handler-body semantics):
//!
//! - **E** — `@@:data.k = v` / `@@:data.k` were no-ops (write emitted
//!   `ok`, read emitted the literal atom `undefined`). They now thread
//!   a call-scoped local map (`__DataMapN`) so writes persist and reads
//!   observe them within the same handler activation, while a reentrant
//!   `@@:self.m()` gets its own map.
//! - **F** — `@@:return(e)` (the short-circuiting return form) must drop
//!   the statements that follow it in the same clause. Erlang has no
//!   native `return`, so a sentinel truncates the remaining top-level
//!   statements.
//! - **G** — `-> pop$` must fire the current state's `<$` exit handler
//!   (via `frame_exit_dispatch__`) before restoring the popped
//!   compartment, matching a normal transition.

mod common;

use common::{compile_source, find_tool};
use std::process::Command;

/// Compile-check the emitted Erlang with `erlc` (syntax + bindings),
/// when the toolchain is present. Skips (returns) silently when `erlc`
/// is unavailable so `cargo test` never *requires* an Erlang install;
/// the cross-language matrix is the authoritative runtime gate.
fn erlc_check(code: &str, module: &str) {
    let Some(erlc) = find_tool("erlc") else {
        return;
    };
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join(format!("{}.erl", module));
    std::fs::write(&path, code).expect("write .erl");
    let out = Command::new(erlc)
        .arg("-o")
        .arg(dir.path())
        .arg(&path)
        .output()
        .expect("run erlc");
    assert!(
        out.status.success(),
        "erlc rejected the generated module `{}`:\n--- stderr ---\n{}\n--- source ---\n{}",
        module,
        String::from_utf8_lossy(&out.stderr),
        code
    );
}

const A_SRC: &str = r#"
@@system T {
    interface:
        go()
    machine:
        $Begin {
            go() { -> $Other }
        }
        $Other {}
}
"#;

const B_SRC: &str = r#"
@@system P {
    interface:
        u(): integer
    machine:
        $S {
            u(): integer { @@:(@@:self.Big) }
        }
    domain:
        Big: integer = 7
}
"#;

const C_SRC: &str = r#"
@@system C {
    interface:
        go(t: integer)
        peek(): integer
    machine:
        $Init {
            go(t: integer) { -> $Active(t) }
        }
        $Active(task: integer) {
            $>() {
                @@:self.acc = @@:self.acc + task;
            }
            peek(): integer { @@:(@@:self.acc) }
        }
    domain:
        acc: integer = 0
}
"#;

const D_SRC: &str = r#"
@@system D {
    operations:
        clamp(v: integer, hi: integer): integer {
            if v > hi { @@:(hi) }
            @@:(v)
        }
    interface:
        run(v: integer, hi: integer): integer
    machine:
        $S {
            run(v: integer, hi: integer): integer { @@:(clamp(v, hi)) }
        }
}
"#;

// ── A — reserved-word state name → quoted atom ──────────────────────
#[test]
fn a_reserved_word_state_name_is_quoted() {
    let code = compile_source(A_SRC, "erlang");
    // The state-function clause head must be the quoted atom, never a
    // bare `begin(` (which the Erlang parser reads as a `begin … end`
    // block opener).
    assert!(
        code.contains("'begin'("),
        "expected quoted state-function head 'begin'(, got:\n{}",
        code
    );
    assert!(
        !contains_bare_word(&code, "begin"),
        "found a bare (unquoted) `begin` token, which is an Erlang reserved word:\n{}",
        code
    );
}

// ── B — uppercase domain field → lowercase record field ─────────────
#[test]
fn b_uppercase_domain_field_is_lowercased() {
    let code = compile_source(B_SRC, "erlang");
    // Record field must be a lowercase atom both at the record
    // definition and at every read/update site.
    assert!(
        !contains_word(&code, "Big"),
        "found an uppercase record-field token `Big`; Erlang record fields must be lowercase atoms:\n{}",
        code
    );
    assert!(
        code.contains("big"),
        "expected a lowercased `big` record field, got:\n{}",
        code
    );
}

// ── C — state-arg read inside `$>` enter handler is bound ────────────
#[test]
fn c_state_arg_read_in_enter_handler_is_bound() {
    let code = compile_source(C_SRC, "erlang");
    // The enter-handler body that reads the state-arg must reference
    // it by its *bound* (capitalized) Erlang variable name, never the
    // raw lowercase Frame identifier `task`.
    assert!(
        !contains_word(&code, "task"),
        "enter-handler body references the unbound lowercase `task`; \
         it must use the capitalized bound variable:\n{}",
        code
    );
}

// ── D — brace-if in operation body lowered to case…of ───────────────
#[test]
fn d_operation_body_if_is_lowered_to_case() {
    let code = compile_source(D_SRC, "erlang");
    // A raw `if v > hi {` (Frame brace form) must never reach Erlang
    // output — it must be lowered to `case … of`.
    assert!(
        !code.contains(" { "),
        "operation body still contains an un-lowered Frame brace block:\n{}",
        code
    );
    assert!(
        code.to_lowercase().contains("case"),
        "expected a `case … of` lowering of the operation's brace-if, got:\n{}",
        code
    );
}

// ── E — @@:data set/read is call-scoped, not a no-op ────────────────
const E_SRC: &str = r#"
@@system T {
    interface:
        echo(x):str
    machine:
        $S {
            echo(x):str {
                @@:data.k = @@:params.x
                @@:("got:" ++ @@:data.k)
            }
        }
}
"#;

// Reentrant self-call: the OUTER handler writes `@@:data.k = "OUTER"`,
// then calls `@@:self.inner()` which writes `@@:data.k = "INNER"`, then
// reads `@@:data.k` back. Call-scoping requires the read to see
// "OUTER" — the inner write must not clobber the caller's map.
const E_SCOPE_SRC: &str = r#"
@@system TS {
    interface:
        outer(): str
        inner()
    machine:
        $S {
            outer(): str {
                @@:data.k = "OUTER"
                @@:self.inner()
                @@:("after:" ++ @@:data.k)
            }
            inner() {
                @@:data.k = "INNER"
            }
        }
}
"#;

#[test]
fn e_context_data_write_then_read_is_threaded() {
    let code = compile_source(E_SRC, "erlang");
    // Write threads a fresh map: __DataMapN = maps:put(<<"k">>, X, …).
    assert!(
        code.contains("maps:put(<<\"k\">>,"),
        "expected `@@:data.k = …` to lower to a maps:put on a threaded \
         map, got:\n{}",
        code
    );
    // Read goes through the generated helper against the live map var,
    // NOT the literal atom `undefined`.
    assert!(
        code.contains("frame_data_get__(<<\"k\">>, __DataMap"),
        "expected `@@:data.k` read to lower to frame_data_get__ against \
         the live threaded map, got:\n{}",
        code
    );
    assert!(
        !code.contains("\"got:\" ++ undefined"),
        "read still resolves to the literal `undefined` (no-op):\n{}",
        code
    );
    // The runtime helper must be defined.
    assert!(
        code.contains("frame_data_get__(Key, Map) -> maps:get(Key, Map, undefined)."),
        "missing frame_data_get__/2 runtime helper:\n{}",
        code
    );
    erlc_check(&code, "t");
}

#[test]
fn e_context_data_is_call_scoped() {
    let code = compile_source(E_SCOPE_SRC, "erlang");
    // Both clauses seed their OWN fresh map. Because each handler is a
    // distinct Erlang function activation, the `inner` write binds a
    // local `__DataMap1` that is invisible to `outer`'s `__DataMap1`.
    // The proof is structural: the `outer` clause's read references the
    // map var bound in `outer` (after its own write), and the reentrant
    // `frame_dispatch__(inner, …)` cannot rebind it.
    let outer = clause_body(&code, "__Event = outer");
    assert!(
        outer.contains("maps:put(<<\"k\">>, \"OUTER\""),
        "outer clause should write OUTER to its own map:\n{}",
        outer
    );
    assert!(
        outer.contains("frame_dispatch__(inner"),
        "outer clause should reentrantly dispatch inner:\n{}",
        outer
    );
    assert!(
        outer.contains("frame_data_get__(<<\"k\">>, __DataMap1)"),
        "outer clause must read its OWN map (__DataMap1 holding OUTER), \
         not a map mutated by the inner dispatch:\n{}",
        outer
    );
    // The inner clause writes INNER into its own (separate) map.
    let inner = clause_body(&code, "__Event = inner");
    assert!(
        inner.contains("__DataMap0 = #{}") && inner.contains("maps:put(<<\"k\">>, \"INNER\""),
        "inner clause must seed its own fresh map and write INNER:\n{}",
        inner
    );
    erlc_check(&code, "t_s");
}

// ── F — @@:return(e) short-circuits the rest of the clause ──────────
const F_SRC: &str = r#"
@@system F {
    interface:
        guard(): integer
        helper(s): integer
    machine:
        $S {
            guard(): integer {
                @@:return(@@:self.limit)
                @@:self.helper("LEAK")
            }
            helper(s): integer {
                @@:self.touched = true
                @@:(0)
            }
        }
    domain:
        limit: integer = 5
        touched: boolean = false
}
"#;

#[test]
fn f_return_call_short_circuits() {
    let code = compile_source(F_SRC, "erlang");
    let guard = clause_body(&code, "__Event = guard");
    // The return value is set …
    assert!(
        guard.contains("Data#data.limit"),
        "guard should set the return value to self.limit:\n{}",
        guard
    );
    // … and the trailing `@@:self.helper("LEAK")` MUST be dropped: no
    // dispatch of `helper` may appear in the guard clause.
    assert!(
        !guard.contains("frame_dispatch__(helper"),
        "guard still runs the post-@@:return helper call — short-circuit \
         failed:\n{}",
        guard
    );
    // No sentinel may leak into the emitted Erlang.
    assert!(
        !code.contains("__FRAME_RETURN_SHORTCIRCUIT__"),
        "short-circuit sentinel leaked into output:\n{}",
        code
    );
    erlc_check(&code, "f");
}

// ── G — -> pop$ fires the popped-from state's <$ exit handler ───────
const G_SRC: &str = r#"
@@system G {
    interface:
        start()
        resume()
    machine:
        $Idle {
            start() {
                push$
                -> $Working
            }
        }
        $Working {
            <$() { @@:self.cleaned = true }
            resume() { -> pop$ }
        }
    domain:
        cleaned: boolean = false
}
"#;

#[test]
fn g_pop_fires_current_state_exit_dispatch() {
    let code = compile_source(G_SRC, "erlang");
    let resume = clause_body(&code, "__Event = resume");
    // The pop must call frame_exit_dispatch__ BEFORE restoring the
    // popped compartment — same as a normal transition.
    assert!(
        resume.contains("frame_exit_dispatch__("),
        "`-> pop$` must fire frame_exit_dispatch__ for the current \
         state's <$ exit handler:\n{}",
        resume
    );
    // Ordering: the exit dispatch precedes the stack-restore unpack.
    let exit_pos = resume
        .find("frame_exit_dispatch__(")
        .expect("exit dispatch present");
    let pop_pos = resume.find("frame_stack").expect("stack restore present");
    assert!(
        exit_pos < pop_pos,
        "exit dispatch must run before the compartment is restored:\n{}",
        resume
    );
    // The Working state's exit handler must be wired into the dispatch
    // table so the pop actually reaches it.
    assert!(
        code.contains("working -> frame_exit__working(Data)"),
        "frame_exit_dispatch__ must route the Working state to its exit \
         helper:\n{}",
        code
    );
    erlc_check(&code, "g");
}

/// Extract the body text of the gen_statem clause whose head contains
/// `needle` (e.g. `"__Event = guard"`), up to the clause terminator
/// `;` / `.` that ends it. Used by the E/F/G assertions to scope a
/// check to a single handler clause.
fn clause_body(code: &str, needle: &str) -> String {
    let start = code
        .find(needle)
        .unwrap_or_else(|| panic!("clause head `{}` not found in:\n{}", needle, code));
    // Walk to the end of this clause: the next line that is exactly `;`
    // or the next state-function clause head. A pragmatic bound is the
    // next line beginning a new top-level clause (`<state>(` at column
    // 0) or a lone `;`.
    let rest = &code[start..];
    let mut end = rest.len();
    for (i, line) in rest.match_indices('\n') {
        let after = &rest[i + 1..];
        let trimmed = after.trim_start();
        // A new clause head at column 0 or a bare `;`/`.` line ends it.
        if (i > 0)
            && (trimmed.starts_with(';')
                || (after
                    .chars()
                    .next()
                    .map(|c| c.is_ascii_lowercase() || c == '\'')
                    .unwrap_or(false)
                    && after.contains("({call, From}")))
        {
            // Include up through this newline so a trailing `;` on the
            // following line is captured for ordering checks.
            end = i + 1;
            break;
        }
        let _ = line;
    }
    rest[..end].to_string()
}

/// Whole-word containment: true iff `word` appears in `s` surrounded
/// by non-`[A-Za-z0-9_]` boundaries. Avoids false positives where the
/// token is a substring of a longer identifier (e.g. `begin` inside
/// `frame_begin__`, or `Big` inside `Bigger`).
fn contains_word(s: &str, word: &str) -> bool {
    let bytes = s.as_bytes();
    let w = word.as_bytes();
    let is_word = |b: u8| b.is_ascii_alphanumeric() || b == b'_';
    let mut i = 0;
    while i + w.len() <= bytes.len() {
        if &bytes[i..i + w.len()] == w
            && (i == 0 || !is_word(bytes[i - 1]))
            && (i + w.len() == bytes.len() || !is_word(bytes[i + w.len()]))
        {
            return true;
        }
        i += 1;
    }
    false
}

/// Like `contains_word`, but treats `'` as a word boundary so a *quoted*
/// atom (`'begin'`) does NOT count — only a genuinely bare reserved-word
/// token (e.g. `begin(` or `, begin,`) trips this. Used by defect A,
/// where the fix is precisely to quote the atom.
fn contains_bare_word(s: &str, word: &str) -> bool {
    let bytes = s.as_bytes();
    let w = word.as_bytes();
    let is_boundary_word = |b: u8| b.is_ascii_alphanumeric() || b == b'_' || b == b'\'';
    let mut i = 0;
    while i + w.len() <= bytes.len() {
        if &bytes[i..i + w.len()] == w
            && (i == 0 || !is_boundary_word(bytes[i - 1]))
            && (i + w.len() == bytes.len() || !is_boundary_word(bytes[i + w.len()]))
        {
            return true;
        }
        i += 1;
    }
    false
}
