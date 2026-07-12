//! Adversarial fixtures for the text-oracle bug family (#191–#209).
//!
//! **Every test in this file is expected to FAIL today.** Each one is a
//! confirmed, shipped bug, reduced to its smallest reproducing input. They are
//! `#[ignore]`d so the suite stays green, and they are the acceptance gate for
//! the rebuild: **as each oracle is deleted, the corresponding test is
//! un-ignored and must pass.**
//!
//! ```text
//! cargo test --test adversarial_oracles -- --ignored     # see the truth
//! ```
//!
//! ## Why this file has to exist BEFORE the rebuild starts
//!
//! The rebuild's safety model is byte-identity: under a behaviour-preserving
//! refactor, the diff against the baseline corpus must be *empty*. That model
//! is only sound if the baseline is **right**.
//!
//! It isn't. `tests/fixtures/_canonical/08_lifecycle.frm` **hand-writes the
//! semicolon that framec should be emitting** — the corpus adapted to the bug.
//! Freeze a byte-identity baseline against a corpus that encodes the workaround
//! and you freeze the *defect into the specification*.
//!
//! So these fixtures are a prerequisite, not a follow-up. They are the half of
//! the oracle that byte-identity cannot supply: byte-identity proves the new
//! compiler is *not different*; only these prove it is *right*.
//!
//! ## The one thing they all have in common
//!
//! Every one is a **hostile-but-entirely-legal string or comment in an ordinary
//! place**. That is not a coincidence — it is the shape of the whole bug class:
//! framec asks a question of text it promised not to understand, and the text
//! lies. Note how *boring* each input is. None of this is exotic.
//!
//! 6,141 fixtures across 17 real toolchains caught **zero** of these.

mod common;
use common::{compile_expect_error, compile_source};

// ---------------------------------------------------------------------------
// #191 — statement termination is position-dependent
// ---------------------------------------------------------------------------

/// Two ordinary consecutive assignments. The first gets no `;`.
///
/// framec emits `this.a = 1` (unterminated) then `this.b = 2;` — because a
/// statement gets its terminator only if it is *last*, or is followed by a
/// whitelisted segment kind. `javac`: `error: ';' expected`.
///
/// Fails to compile on Java, Rust, C++, C, C#, Dart, PHP.
#[test]
#[ignore = "RED until #191 — terminator is position-dependent"]
fn terminator_between_two_self_assignments() {
    let out = compile_source(
        r#"
@@[target("java")]
@@system V {
    interface:
        go()
    machine:
        $S {
            go() {
                @@:self.a = 1
                @@:self.b = 2
            }
        }
    domain:
        a: int = 0
        b: int = 0
}
"#,
        "java",
    );
    assert!(
        out.contains("this.a = 1;"),
        "first statement lost its terminator:\n{out}"
    );
    assert!(out.contains("this.b = 2;"), "second statement:\n{out}");
}

/// A trailing comment after a Frame statement swallows the semicolon.
///
/// The terminator is placed at the last non-whitespace byte of the *emitted
/// text* — which is inside the comment. Emitted: `this.a = 1 // set it;`
///
/// This is the purest demonstration of the whole bug class: framec is reading
/// text it promised not to understand, and the comment lies to it.
#[test]
#[ignore = "RED until #191 — the terminator probe is comment-blind"]
fn terminator_not_spliced_inside_a_trailing_comment() {
    let out = compile_source(
        r#"
@@[target("java")]
@@system C {
    interface:
        go()
    machine:
        $S {
            go() {
                @@:self.a = 1 // set the field
            }
        }
    domain:
        a: int = 0
}
"#,
        "java",
    );
    assert!(
        !out.contains("// set the field;"),
        "the ';' was spliced INSIDE the comment:\n{out}"
    );
    assert!(
        out.contains("this.a = 1;"),
        "statement is unterminated:\n{out}"
    );
}

// ---------------------------------------------------------------------------
// #192 — C++ async: a comment makes the generated program hang forever
// ---------------------------------------------------------------------------

/// A handler whose body merely *mentions* `co_return` in a comment.
///
/// `ensure_cpp_coroutine_terminator` asks `code.contains("co_await")` of the
/// handler's native text — which is user code, passed through verbatim. The
/// comment convinces it the body is already a coroutine, so the `co_return;` it
/// must append is never emitted, `FrameTask<void>` is returned with no backing
/// promise, and **the generated program hangs forever**. It still *compiles*
/// (one `-Wreturn-type` warning), so CI's build step stays green.
#[test]
#[ignore = "RED until #192 — the coroutine guard is fooled by a comment"]
fn cpp_coroutine_guard_not_fooled_by_a_comment() {
    let out = compile_source(
        r#"
@@[target("cpp")]
@@[async]
@@system Probe {
    interface:
        async ping(): int
    machine:
        $Active {
            $>() { @@:self.n = 0; }
            ping(): int {
                // bump the counter, then co_return happens implicitly
                @@:self.n = @@:self.n + 1;
                @@:(@@:self.n)
            }
        }
    domain:
        n: int = 0;
}
"#,
        "cpp",
    );
    // Anchor on the DEFINITION, not the name: the router *calls* the handler
    // before defining it, and the router's own `co_return;` sits right after the
    // call site. (The first draft of this test anchored on the bare name and so
    // read the router's terminator. Two traps, one test.)
    let handler = out
        .split("FrameTask<void> _s_Active_hdl_user_ping")
        .nth(1)
        .expect("handler definition not found");
    let body = &handler[..handler.find("\n    }").unwrap_or(handler.len())];

    // NOTE: a naive `body.contains("co_return")` PASSES here — because the
    // user's *comment* contains the text `co_return`. The first draft of this
    // test committed the exact bug it exists to catch, which is a rather good
    // illustration of the whole problem. Look for a real STATEMENT.
    let has_coroutine_stmt = body.lines().any(|l| {
        let t = l.trim_start();
        !t.starts_with("//") && (t.starts_with("co_return") || t.starts_with("co_await"))
    });
    assert!(
        has_coroutine_stmt,
        "handler has no co_return STATEMENT (only a comment mentioning one) — \
         FrameTask<void> has no backing promise and the generated program HANGS:\n{body}"
    );
}

// ---------------------------------------------------------------------------
// #193 — persist import injection is substring-blind
// ---------------------------------------------------------------------------

/// A Go prolog comment that merely *mentions* `"encoding/json"`.
///
/// The assembler decides whether the import is needed with
/// `output.contains("\"encoding/json\"")`. A comment satisfies it, the real
/// import is suppressed, and `go build` fails with `undefined: json`.
///
/// Rewording the comment makes it compile *and run*. The only delta is comment
/// text.
#[test]
#[ignore = "RED until #193 — import injection is substring-blind"]
fn go_persist_import_not_suppressed_by_a_comment() {
    let out = compile_source(
        r#"
@@[target("go")]
// we do NOT import "encoding/json" ourselves; framec must supply it
@@[persist(string)]
@@[save(save_state)]
@@[load(load_state)]
@@system Counter {
    interface:
        bump()
    machine:
        $S {
            bump() { @@:self.n = @@:self.n + 1 }
        }
    domain:
        n: int = 0
}
"#,
        "go",
    );
    // Whether by direct import or a framec-private alias, the emitted code must
    // actually be able to marshal. A mention in a comment must not satisfy it.
    let marshals = out.contains("json.Marshal") || out.contains("__framec_json.Marshal");
    let imports = out.lines().any(|l| {
        let t = l.trim_start();
        !t.starts_with("//") && t.contains("encoding/json")
    });
    assert!(
        !marshals || imports,
        "output calls json.Marshal but the import was suppressed by a COMMENT:\n{out}"
    );
}

// ---------------------------------------------------------------------------
// #194 — E407 rejects legal code
// ---------------------------------------------------------------------------

/// A Rust string literal that happens to contain `-> $A`, inside a closure.
///
/// The E407 check scans raw bytes with `windows(4)` for `-> $` — no string
/// awareness, no comment awareness — while the string-aware machine sits three
/// lines away. framec **rejects legal user code**.
///
/// Control: the identical string *outside* a closure compiles fine, which
/// proves the fault is the probe, not the scanner.
#[test]
#[ignore = "RED until #194 — the E407 probe is string-blind"]
fn e407_not_triggered_by_a_string_literal_in_a_closure() {
    let out = compile_source(
        r#"
@@[target("rust")]
@@system E {
    interface:
        go()
    machine:
        $S {
            go() {
                let f = || { let s = "-> $A"; s.len() };
                let _ = f();
            }
        }
    domain:
        n: i32 = 0
}
"#,
        "rust",
    );
    assert!(out.contains("struct E"), "should have compiled:\n{out}");
}

// ---------------------------------------------------------------------------
// #195 — a native string containing `@@persist` hard-fails the build
// ---------------------------------------------------------------------------

/// A Python docstring that documents the RFC-0013 migration.
///
/// The legacy-pragma check scans *every line of raw source* for
/// `starts_with("@@persist")`, including native regions that framec passes
/// through verbatim. Writing documentation *about* the migration is enough to
/// break the build (E803).
#[test]
#[ignore = "RED until #195 — the legacy-pragma check scans raw source"]
fn native_string_containing_at_persist_compiles() {
    let out = compile_source(
        r#"
@@[target("python_3")]
DOC = """
@@persist was removed in RFC-0013.
"""
@@system Foo {
    interface:
        go()
    machine:
        $A {
            go() { }
        }
}
"#,
        "python_3",
    );
    assert!(out.contains("class Foo"), "should have compiled:\n{out}");
}

// ---------------------------------------------------------------------------
// #196 — PHP: silent domain-field value corruption
// ---------------------------------------------------------------------------

/// A domain initializer whose *string literal* contains a system-param name.
///
/// `prefix_php_vars` prefixes `$` onto param references with a raw byte scan and
/// no skipper, so it matches **inside string literals**. PHP then interpolates:
/// `strtoupper("balance")` becomes `strtoupper("$balance")`, and the field
/// silently takes the value `"3"` instead of `"BALANCE"`.
///
/// No error, no warning — just a wrong value at runtime.
#[test]
#[ignore = "RED until #196 — prefix_php_vars is string-blind"]
fn php_param_name_inside_a_string_literal_is_not_prefixed() {
    let out = compile_source(
        r#"
@@[target("php")]
@@system W(balance: int = 3) {
    interface:
        go()
    machine:
        $A {
            go() { }
        }
    domain:
        balance: int = balance
        label: string = strtoupper("balance")
}
"#,
        "php",
    );
    assert!(
        out.contains(r#"strtoupper("balance")"#),
        "the string literal was corrupted into an interpolation:\n{out}"
    );
    assert!(
        !out.contains(r#"strtoupper("$balance")"#),
        "PHP will interpolate this and silently produce the wrong value:\n{out}"
    );
}

// ---------------------------------------------------------------------------
// #197 — a user system named *Compartment silently loses its factory
// ---------------------------------------------------------------------------

/// Framework helper classes are detected by `class_name.ends_with("Compartment")`.
///
/// So a *user* system named `LogCompartment` is misclassified as framec's own
/// runtime helper and silently loses its RFC-0017 factory. The assembler still
/// lowers `@@LogCompartment()` to `LogCompartment._create()` → the property does
/// not exist.
///
/// `EmitContext.is_framework_helper` exists **precisely to kill this probe** —
/// TypeScript and Kotlin were simply never migrated to read it.
#[test]
#[ignore = "RED until #197 — framework-helper detection is a name-suffix probe"]
fn user_system_named_compartment_keeps_its_factory() {
    let out = compile_source(
        r#"
@@[target("typescript")]
@@system LogCompartment {
    interface:
        go()
    machine:
        $A {
            go() { }
        }
}
"#,
        "typescript",
    );
    assert!(
        out.contains("_create"),
        "the factory was silently dropped because the NAME ends in 'Compartment':\n{out}"
    );
}

// ---------------------------------------------------------------------------
// #199 — the parser's initializer scans are string-blind
// ---------------------------------------------------------------------------

/// A state-var initializer whose string literal contains a `#`.
///
/// The parser drops out of the token stream into a hand-rolled byte scan that
/// treats `#` as a comment start — *inside a string*. It emits
/// `compartment.state_vars["tag"] = "` — **syntactically broken Python, with no
/// diagnostic at all.**
///
/// The tell: a `domain:` field with the *identical* text emits correctly,
/// because that path routes through the string-aware `DomainScannerFsm`. Same
/// text, two code paths, two answers. The #113 string-safety fix was applied to
/// the domain scanner and never propagated to the parser's four other scans.
#[test]
#[ignore = "RED until #199 — the parser's initializer scans are string-blind"]
fn state_var_initializer_string_containing_hash_survives() {
    let out = compile_source(
        r##"
@@[target("python_3")]
@@system H {
    interface:
        go()
    machine:
        $A {
            $.tag: str = "#hashtag"
            go() { }
        }
    domain:
        dtag: str = "#hashtag"
}
"##,
        "python_3",
    );
    // The domain path (string-aware) gets this right today.
    assert!(
        out.contains(r##"self.dtag = "#hashtag""##),
        "domain field (control):\n{out}"
    );
    // The state-var path (hand-rolled byte scan) truncates at the '#'.
    assert!(
        out.contains(r##""#hashtag""##) && !out.contains("state_vars[\"tag\"] = \"\n"),
        "state-var initializer was truncated at the '#' inside the string:\n{out}"
    );
}

// ---------------------------------------------------------------------------
// #200 — count_args is bracket- and string-blind
// ---------------------------------------------------------------------------

/// A single list argument in a transition.
///
/// `count_args` counts depth-0 commas tracking **only** `()` — it cannot see
/// `[`, `{`, or string literals. So one list argument reads as three, and valid
/// Frame is rejected with a **false** E405. Same helper produces false E417,
/// E419 and E602.
#[test]
#[ignore = "RED until #200 — count_args cannot see brackets or strings"]
fn transition_arg_containing_commas_in_brackets_is_one_arg() {
    let out = compile_source(
        r#"
@@[target("python_3")]
@@system L {
    interface:
        go()
    machine:
        $A {
            go() { -> $T([1, 2, 3]) }
        }
        $T(items: list) { }
}
"#,
        "python_3",
    );
    assert!(out.contains("class L"), "valid Frame was rejected:\n{out}");
}

/// A string argument containing a comma. Same root cause, same false E405.
#[test]
#[ignore = "RED until #200 — count_args cannot see strings"]
fn transition_arg_containing_a_comma_in_a_string_is_one_arg() {
    let err = compile_expect_error(
        r#"
@@[target("python_3")]
@@system M {
    interface:
        go()
    machine:
        $A {
            go() { -> $T("a, b") }
        }
        $T(label: str) { }
}
"#,
        "python_3",
    );
    assert!(
        !err.contains("E405"),
        "false E405 on a single string arg containing a comma: {err}"
    );
}
