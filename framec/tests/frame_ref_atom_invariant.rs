//! THE ATOM INVARIANT — the law that #213 / #220 / #221 / #222 all broke.
//!
//! > Every Frame-reference expansion MUST be an ATOM in the target grammar:
//! > an identifier, a literal, a parenthesized expression, or an unbroken
//! > postfix chain (`a.b`, `a[i]`, `f(x)`, `x.(T)`) rooted at one of those.
//! >
//! > Any expansion whose HEAD is a prefix operator — a C-style cast, a `*`
//! > deref, `!`, `-` — MUST be wrapped in the target's grouping parens.
//!
//! Why this is a law and not four bug fixes:
//!
//! framec splices Frame references into native expressions it does not parse and
//! must never parse. That is only sound if the spliced text behaves as a **single
//! operand wherever it lands**. A bare cast is not: `(int) m["n"].Doubled()` binds
//! the cast to the result of `.Doubled()`, because a C# cast is *unary* precedence
//! while `.` is *primary*. framec does not need to understand the surrounding
//! expression — **it needs to make its own output structureless.**
//!
//! #213 was the punishment for breaking it: C# compiled clean, exited 0, and
//! printed -1 instead of 84.
//!
//! COVERAGE NOTE — this is *why* the bugs shipped. Before this file, **zero**
//! fixtures used `$.x` followed by a member access or an index, and **zero** C#
//! snapshots exercised the `$.x` read path at all. The suite was green because it
//! had never looked.

mod common;

/// Targets whose `$.x` read is a **typed** read — it must cast or unwrap out of a
/// reflective container, so its expansion is the one at risk of being a non-atom.
/// (Dynamic targets emit a bare index chain, which is already an atom.)
const TYPED_TARGETS: &[&str] = &[
    "csharp", "c", "cpp", "java", "kotlin", "swift", "dart", "go", "rust",
];

const SPEC: &str = r#"@@system W {
    interface:
        run()
    machine:
        $S {
            $.n: int = 0
            run() {
                use($.n)
            }
        }
}
"#;

/// Index of the `)` matching a `(` at byte 0. `None` if byte 0 is not `(`.
fn matching_close(e: &str) -> Option<usize> {
    let b = e.as_bytes();
    if b.first() != Some(&b'(') {
        return None;
    }
    let mut depth = 0i32;
    for (i, &c) in b.iter().enumerate() {
        match c {
            b'(' => depth += 1,
            b')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
    }
    None
}

/// True when `expr`'s head is a prefix operator, which makes it a NON-atom:
/// splicing it into any postfix (`X.m()`, `X[i]`) or binary (`a*X+b`) context
/// silently re-associates it.
fn head_is_prefix_operator(expr: &str) -> bool {
    let e = expr.trim();
    // A deref / logical-not / negation head — e.g. C's `CMarshal::Boxed` -> `*(T*)x`.
    if e.starts_with('*') || e.starts_with('!') {
        return true;
    }
    // A C-style cast head: the leading paren closes BEFORE the end of the
    // expression, i.e. the parens wrap only the *type*, not the whole read.
    // `((int) x)` is an atom; `(int) x` is not.
    match matching_close(e) {
        Some(close) => close != e.len() - 1,
        None => false,
    }
}

/// Pull the single argument out of the emitted `use(...)` call.
///
/// Strips comments first: framec echoes the original Frame spec into the
/// generated file as a comment, and grepping *that* instead of the code is
/// exactly how two earlier tests in this repo passed by accident while the bug
/// they existed to catch was still live.
fn emitted_read(out: &str) -> Option<String> {
    for line in out.lines() {
        let code = line.split("//").next().unwrap_or(line);
        let code = code.split('#').next().unwrap_or(code);
        if let Some(i) = code.find("use(") {
            let rest = &code[i + "use(".len()..];
            let close = matching_close(&format!("({rest}"))?;
            let arg = rest[..close.saturating_sub(1)].trim();
            if !arg.is_empty() {
                return Some(arg.to_string());
            }
        }
    }
    None
}

#[test]
fn frame_ref_expansion_is_an_atom_in_every_typed_target() {
    let mut violations = Vec::new();
    let mut checked = 0;

    for target in TYPED_TARGETS {
        let out = common::compile_source(SPEC, target);
        let Some(read) = emitted_read(&out) else {
            continue;
        };
        checked += 1;
        if head_is_prefix_operator(&read) {
            violations.push(format!(
                "  {target:<7} $.n  ->  {read}\n           \
                 NOT AN ATOM: prefix-operator head, so `X.m()` / `X[i]` / `a*X+b` mis-parse"
            ));
        }
    }

    assert!(
        checked >= 5,
        "expected to actually inspect the emitted read on most typed targets, \
         but only found it on {checked} — the probe stopped probing, which is \
         how this class of bug hides. Fix the extractor, don't relax the test."
    );

    assert!(
        violations.is_empty(),
        "Frame-ref expansions that are NOT atoms in the target grammar — they must \
         be wrapped in the target's grouping parens:\n\n{}\n\n\
         #213 (C# silently returned the WRONG ANSWER), #220 (C rejected), #222.",
        violations.join("\n")
    );
}

/// #221 — the interpolation quote-swap is a **Python-only** workaround (#47).
/// Applying it where `'` delimits a CHAR emits code that does not compile:
/// C# `CS1503: cannot convert char to string`; swiftc "single-quoted string
/// literal found"; kotlinc type-inference failure.
#[test]
fn interpolation_quote_swap_never_emits_a_char_literal_key() {
    const CHAR_LITERAL_LANGS: &[&str] =
        &["csharp", "java", "kotlin", "swift", "c", "cpp", "go", "rust"];

    let src = r#"@@system W {
    interface:
        run()
    machine:
        $S {
            $.n: int = 7
            run() {
                emit("n is {$.n}")
            }
        }
}
"#;

    for target in CHAR_LITERAL_LANGS {
        let out = common::compile_source(src, target);
        for line in out.lines() {
            let code = line.split("//").next().unwrap_or(line);
            assert!(
                !code.contains("state_vars['"),
                "{target}: emitted a single-quoted dict key, but `'...'` is a CHAR \
                 literal in {target}, not a string — this does not compile (#221).\n  {}",
                code.trim()
            );
        }
    }
}
