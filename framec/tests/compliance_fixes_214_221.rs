//! Regression guards for four shipped bugs fixed together with the removal of the
//! Lua brace-form transform (#232). Each one compiled clean, exited 0, and emitted
//! wrong code. #213 and #220 are guarded by `frame_ref_atom_invariant.rs`.

mod common;
use common::compile_source;

/// #214: a UTF-8 BOM made the segmenter classify the whole `@@system` as native
/// text, so no class was generated at all.
#[test]
fn bom_prefixed_source_still_generates_the_system() {
    let src = "\u{FEFF}@@system B {\n    interface:\n        go()\n    machine:\n        $S {\n            go() {}\n        }\n}\n";
    let code = compile_source(src, "python_3");
    assert!(
        code.contains("class B"),
        "[#214] a BOM-prefixed source must still generate its system\n{code}"
    );
    assert!(
        !code.contains("@@system"),
        "[#214] the @@system block leaked through as native text\n{code}"
    );
}

/// #216: `@@` inside a string interpolation had its two sigil bytes deleted, so
/// `f"x is {@@:self.factor}"` emitted `f"x is {:self.factor}"`.
#[test]
fn context_ref_inside_interpolation_is_expanded() {
    let src = r#"
@@system I {
    interface:
        show(): str
    machine:
        $S {
            show(): str {
                @@:(f"x is {@@:self.factor}")
            }
        }
    domain:
        factor = 3
}
"#;
    let code = compile_source(src, "python_3");
    assert!(
        code.contains(r#"f"x is {self.factor}""#),
        "[#216] `@@:self.factor` inside an f-string must expand to `self.factor`\n{code}"
    );
    assert!(
        !code.contains("{:self.factor}"),
        "[#216] the `@@` sigil was deleted instead of expanded\n{code}"
    );
}

/// #217: the validator counted args with a string-blind splitter, so a string
/// literal containing a comma was rejected (E405) — while codegen, splitting
/// correctly, would have emitted it fine.
#[test]
fn string_arg_with_a_comma_counts_as_one_arg() {
    let src = r#"
@@system A {
    interface:
        go()
    machine:
        $Idle {
            go() { -> $S("hello, world", 9) }
        }
        $S(a: str, b: int) {
        }
}
"#;
    let code = compile_source(src, "python_3");
    assert!(
        code.contains(r#"__prepareEnter("S", ["hello, world", 9], [])"#),
        "[#217] both state args must be passed, the string intact\n{code}"
    );
}

/// #221: inside a string interpolation, `$.n`'s dict key was quote-swapped to
/// `'n'` on every target. In C# (and Java, Kotlin, Swift, C, C++, Go, Rust) that
/// is a char literal, so the output did not compile (CS1503). It shows in a native
/// statement; `@@:(…)` goes through a different expander that was already gated.
#[test]
fn csharp_interpolated_state_var_key_stays_a_string() {
    let src = r#"
@@system Q {
    interface:
        show()
    machine:
        $S {
            $.n: int = 5
            show() {
                var s = $"n={$.n}";
            }
        }
}
"#;
    let code = compile_source(src, "csharp");
    assert!(
        code.contains(r#"state_vars["n"]"#),
        "[#221] the C# state-var key must stay a string literal\n{code}"
    );
    assert!(
        !code.contains("state_vars['n']"),
        "[#221] `'n'` is a char literal in C#\n{code}"
    );
}
