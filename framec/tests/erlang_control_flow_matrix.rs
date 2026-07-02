//! Erlang handler-body control-flow regression net (#125 / #119).
//!
//! The Erlang backend currently reconstructs control-flow structure and `Data`
//! SSA threading from *emitted text* (~3,784 LOC of string processing). That is
//! fragile: edits ripple (the reverted #125 broke two unrelated shapes), and it
//! is formatting-sensitive. Before any change to that machinery — and before the
//! planned move to structured lowering — this matrix pins the *currently-working*
//! handler shapes so a regression is caught in-tree, fast, instead of only in the
//! Docker matrix.
//!
//! Each case compiles a handler to Erlang and asserts the emitted event clause is
//! **well-formed**: balanced `case … end`, no leaked C-style control-flow braces,
//! no invalid record-field assignment (`Data#data.f = …`), and no dropped
//! statements (every user statement's marker survives). When `erlc` is on PATH and
//! the module is self-contained, it is additionally compiled for real.
//!
//! The one known-broken shape (#125: mixed-terminal `else if` + trailing code) is a
//! `#[ignore]`d test carrying the exact assertion it must satisfy once fixed —
//! un-ignore it in the Phase 1 structured-lowering work.

mod common;
use common::compile_source;

/// Extract the user event-handler clause `s({call, From}, __Event = …) -> …` up to
/// the next `s(`/`done(` clause head. This is the region that carries the lowered
/// handler body.
fn handler_clause(code: &str) -> String {
    let mut out = String::new();
    let mut in_clause = false;
    for line in code.lines() {
        let t = line.trim_start();
        if t.starts_with("s({call, From}, __Event") {
            in_clause = true;
            out.push_str(line);
            out.push('\n');
            continue;
        }
        if in_clause {
            // A new top-level clause head ends the region.
            if t.starts_with("s(") || t.starts_with("done(") || t.starts_with("s_") {
                break;
            }
            out.push_str(line);
            out.push('\n');
        }
    }
    out
}

/// Assert the emitted Erlang handler clause is well-formed, and that each marker in
/// `must_contain` survives (no dropped statements).
fn assert_wellformed(src: &str, must_contain: &[&str]) {
    let code = compile_source(src, "erlang");
    let clause = handler_clause(&code);
    assert!(
        !clause.trim().is_empty(),
        "no event handler clause found in:\n{code}"
    );

    // 1. Balanced `case … of` / `end`.
    let cases = clause.matches("case ").count() + clause.matches("case(").count();
    let ends = clause
        .lines()
        .filter(|l| {
            let t = l.trim();
            t == "end" || t == "end," || t == "end;" || t.ends_with(" end") || t.ends_with(" end,")
        })
        .count();
    assert!(
        cases <= ends,
        "[wellformed] unbalanced case/end (case={cases} end={ends}) — unterminated case:\n{clause}"
    );

    // 2. No leaked C-style control-flow braces (Erlang uses `-> … end`, not `{ }`).
    for bad in ["} else", "else if", "} else if"] {
        assert!(
            !clause.contains(bad),
            "[wellformed] leaked C-style control flow `{bad}`:\n{clause}"
        );
    }
    // An `if <cond> {` opener is C-style; Erlang `if` guards never precede `{`.
    for line in clause.lines() {
        let t = line.trim();
        assert!(
            !(t.starts_with("if ") && t.ends_with('{')),
            "[wellformed] leaked C-style `if … {{`:\n{clause}"
        );
    }

    // 3. No invalid record-field assignment `Data#data.field = …`
    //    (a mutation must be `DataN = Data#data{field = …}`, never a field-access LHS).
    for line in clause.lines() {
        let t = line.trim();
        if let Some(pos) = t.find("#data.") {
            let after = &t[pos + "#data.".len()..];
            // ident then ` = ` (but not `==`) is an illegal assignment target.
            if let Some(eq) = after.find(" = ") {
                let ident = &after[..eq];
                let is_bare_ident = !ident.is_empty()
                    && ident.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
                assert!(
                    !is_bare_ident,
                    "[wellformed] invalid record-field assignment `#data.{ident} = …`:\n{clause}"
                );
            }
        }
    }

    // 4. No dropped statements — every marker must survive.
    for m in must_contain {
        assert!(
            clause.contains(m),
            "[wellformed] dropped statement — `{m}` missing from lowered handler:\n{clause}"
        );
    }
}

// ---- Currently-working shapes (regression protection) ----

#[test]
fn linear_mutation() {
    assert_wellformed(
        r#"@@[main]
@@system S { interface: e() machine: $S { e() { @@:self.x = 1 } } domain: x: int = 0 }"#,
        &["Data#data{x = 1}"],
    );
}

#[test]
fn read_modify_write() {
    assert_wellformed(
        r#"@@[main]
@@system S { interface: e(n: int) machine: $S { e(n: int) { @@:self.x = @@:self.x + n } } domain: x: int = 0 }"#,
        &["Data#data{x = Data#data.x + N}"],
    );
}

#[test]
fn single_if_transition() {
    assert_wellformed(
        r#"@@[main]
@@system S { interface: e(a: int) machine: $S { e(a: int) {
  if a == 1 {
    -> $D
  }
} } $D {} }"#,
        &["case (A == 1) of", "frame_transition__('d'"],
    );
}

#[test]
fn if_else_mutation() {
    assert_wellformed(
        r#"@@[main]
@@system S { interface: e(a: int) machine: $S { e(a: int) {
  if a == 1 {
    @@:self.x = 1
  } else {
    @@:self.x = 2
  }
} } domain: x: int = 0 }"#,
        &["case (A == 1) of", "Data#data{x = 1}", "Data#data{x = 2}"],
    );
}

#[test]
fn if_transition_then_trailing_mutation() {
    // Single if with a terminal true-arm + trailing mutation: the trailing code is
    // correctly hoisted into the synthesized `false ->` arm.
    assert_wellformed(
        r#"@@[main]
@@system S { interface: e(a: int) machine: $S { e(a: int) {
  if a == 1 {
    -> $D
  }
  @@:self.x = 1
} } $D {} domain: x: int = 0 }"#,
        &["frame_transition__('d'", "Data#data{x = 1}"],
    );
}

#[test]
fn native_call_passthrough() {
    assert_wellformed(
        r#"@@[main]
@@system S { interface: e() machine: $S { e() { do_thing() } } }"#,
        &["do_thing()"],
    );
}

#[test]
fn elif_all_terminal_transitions() {
    // An else-if chain where every arm transitions lowers to nested cases, both
    // closed. This works today and must keep working.
    assert_wellformed(
        r#"@@[main]
@@system S { interface: e(a: int) machine: $S { e(a: int) {
  if a == 1 {
    -> $D
  } else if a == 2 {
    -> $E
  } else {
    -> $F
  }
} } $D {} $E {} $F {} }"#,
        &[
            "frame_transition__('d'",
            "frame_transition__('e'",
            "frame_transition__('f'",
        ],
    );
}

// ---- The known-broken shape (#125) — un-ignore when structured lowering lands ----

#[test]
#[ignore = "#125: mixed-terminal `else if` + trailing code drops statements and leaves \
            the case unterminated. Fixed by the structured handler-body lowering; \
            un-ignore then."]
fn elif_mixed_terminal_plus_trailing() {
    // if a==1 { -> $D }  (terminal)
    // else if b==1 { do_b() }  (non-terminal)
    // do_trail()  (trailing)
    // Today: emits `case (A==1) of true -> frame_transition__(…) ;` — unterminated,
    // `do_b()` and `do_trail()` dropped.
    assert_wellformed(
        r#"@@[main]
@@system S { interface: e(a: int, b: int) machine: $S { e(a: int, b: int) {
  if a == 1 {
    -> $D
  } else if b == 1 {
    do_b()
  }
  do_trail()
} } $D {} }"#,
        &["frame_transition__('d'", "do_b()", "do_trail()"],
    );
}
