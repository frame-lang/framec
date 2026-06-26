//! Issue #132 — Erlang/gen_statem backend mechanical/operation defects.
//!
//! Four root-cause regressions (the A/B/C/D batch):
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

mod common;

use common::compile_source;

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
