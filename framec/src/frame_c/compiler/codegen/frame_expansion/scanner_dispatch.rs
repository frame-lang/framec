//! Per-target scanner factory and the `@@:system.state` lowering.
//!
//! Three small entry points, all stateless:
//!
//! - `get_native_scanner(lang)` — returns a boxed
//!   `NativeRegionScanner` impl for the target language. Frame's
//!   scanner pipeline needs a per-target skipper so it can step
//!   past string literals, comments, raw strings, and other
//!   language-specific lexical regions while looking for Frame
//!   sigils.
//! - `expand_system_state(lang)` — emits the literal target-side
//!   expression for `@@:system.state` (the current state name
//!   accessor). One-liner per backend.
//! - `expand_system_state_in_code(code, lang)` — runs the two
//!   stateless rewrites on operation-body code:
//!     - `@@:system.state` → the per-backend compartment accessor
//!     - `@@:(<expr>)` → `return <expr>;` (or `<expr>` on Erlang)
//!
//! The pipeline calls `get_native_scanner` from three places
//! (state_dispatch, system_codegen, frame_validator) so it stays
//! `pub(crate)` and is re-exported from the parent module.

use crate::frame_c::compiler::native_region_scanner::{
    c::NativeRegionScannerC, cpp::NativeRegionScannerCpp, csharp::NativeRegionScannerCs,
    dart::NativeRegionScannerDart, gdscript::NativeRegionScannerGDScript,
    go::NativeRegionScannerGo, java::NativeRegionScannerJava, javascript::NativeRegionScannerJs,
    kotlin::NativeRegionScannerKotlin, lua::NativeRegionScannerLua, php::NativeRegionScannerPhp,
    python::NativeRegionScannerPy, ruby::NativeRegionScannerRuby, rust::NativeRegionScannerRust,
    swift::NativeRegionScannerSwift, typescript::NativeRegionScannerTs, NativeRegionScanner,
};
use crate::frame_c::visitors::TargetLanguage;

/// Get the native region scanner for the target language.
pub(crate) fn get_native_scanner(lang: TargetLanguage) -> Box<dyn NativeRegionScanner> {
    match lang {
        TargetLanguage::Python3 => Box::new(NativeRegionScannerPy),
        TargetLanguage::TypeScript => Box::new(NativeRegionScannerTs),
        TargetLanguage::JavaScript => Box::new(NativeRegionScannerJs),
        TargetLanguage::Rust => Box::new(NativeRegionScannerRust),
        TargetLanguage::CSharp => Box::new(NativeRegionScannerCs),
        TargetLanguage::C => Box::new(NativeRegionScannerC),
        TargetLanguage::Cpp => Box::new(NativeRegionScannerCpp),
        TargetLanguage::Java => Box::new(NativeRegionScannerJava),
        TargetLanguage::Kotlin => Box::new(NativeRegionScannerKotlin),
        TargetLanguage::Swift => Box::new(NativeRegionScannerSwift),
        TargetLanguage::Go => Box::new(NativeRegionScannerGo),
        TargetLanguage::Php => Box::new(NativeRegionScannerPhp),
        TargetLanguage::Ruby => Box::new(NativeRegionScannerRuby),
        TargetLanguage::Lua => Box::new(NativeRegionScannerLua),
        TargetLanguage::Dart => Box::new(NativeRegionScannerDart),
        TargetLanguage::GDScript => Box::new(NativeRegionScannerGDScript),
        // Graphviz is an output-only target (emitted from the SystemGraph IR,
        // not from native code). The validator still scans for Frame tokens
        // (e.g. @@:self.method()) during the graphviz compile path; those
        // tokens are target-language-agnostic, so any skipper works. Use the
        // Python scanner as a neutral default.
        TargetLanguage::Graphviz => Box::new(NativeRegionScannerPy),
    }
}

/// Expand `@@:system.state` to the target-language compartment state accessor.
/// Used by both handler body expansion and operation body expansion.
pub(crate) fn expand_system_state(lang: TargetLanguage) -> String {
    match lang {
        TargetLanguage::Python3 | TargetLanguage::GDScript => {
            "self.__compartment.state".to_string()
        }
        TargetLanguage::TypeScript | TargetLanguage::JavaScript | TargetLanguage::Dart => {
            "this.__compartment.state".to_string()
        }
        TargetLanguage::Rust => super::super::rust_system::rust_system_state(),
        TargetLanguage::C => "self->__compartment->state".to_string(),
        TargetLanguage::Cpp => "__compartment->state".to_string(),
        TargetLanguage::Java | TargetLanguage::Kotlin | TargetLanguage::CSharp => {
            "__compartment.state".to_string()
        }
        TargetLanguage::Swift => "__compartment.state".to_string(),
        TargetLanguage::Go => "s.__compartment.state".to_string(),
        TargetLanguage::Php => "$this->__compartment->state".to_string(),
        TargetLanguage::Ruby => "@__compartment.state".to_string(),
        TargetLanguage::Lua => "self.__compartment.state".to_string(),
        // Erlang: gen_statem keeps the current state as the
        // snake_case atom `frame_current_state`, but `@@:system.state`
        // is contractually a STRING with the user-facing name (matches
        // the other 16 backends). `frame_state_name__/1` is emitted by
        // `emit_runtime_helpers` and maps the atom back to the
        // original spelling.
        TargetLanguage::Graphviz => unreachable!(),
    }
}

/// Expand `@@:system.state` occurrences in operation body code.
/// Operations are native code but `@@:system.state` is a read-only accessor
/// that's safe in non-static operations.
pub(crate) fn expand_system_state_in_code(code: &str, lang: TargetLanguage) -> String {
    let mut result = code.to_string();

    // Expand @@:system.state.name → compartment accessor (the state-name
    // string). Bare `@@:system.state` is reserved (RFC-0045) and rejected by
    // E608 during validation, so only the `.name` form reaches codegen.
    // String/comment-safe (#155): a sigil inside a native literal or comment is
    // not a real reference and must not be rewritten.
    {
        let ssn = expand_system_state(lang);
        result =
            crate::frame_c::compiler::codegen::codegen_utils::replace_outside_strings_and_comments(
                &result,
                lang,
                &[("@@:system.state.name", ssn.as_str())],
            );
    }

    // #141: Expand @@:return(expr) → return expr  and  @@:return() → return.
    //
    // Action and operation bodies are plain native methods (no context stack),
    // lowered through this textual pass rather than the handler-body segment
    // scanner. The `@@:return(...)` call form was never handled here, so the
    // literal sigil leaked into the target output (the silent exit-0 defect).
    // The sibling `@@:(expr)` form below IS lowered, which is why a `@@:(0)` in
    // the same body works while `@@:return(1)` did not. In a method with a real
    // return slot the set-and-exit semantics collapse to a native `return`:
    //   @@:return(expr) → `return expr`   (set the return value and exit)
    //   @@:return()     → `return`        (exit, leaving the default value)
    // Processed BEFORE the `@@:(` loop: `@@:return(` does not start with the
    // literal `@@:(`, so the two scans are disjoint and order is not load-
    // bearing, but doing this first keeps the intent obvious.
    while let Some(start) =
        crate::frame_c::compiler::codegen::codegen_utils::find_outside_strings_and_comments(
            &result,
            lang,
            "@@:return(",
        )
    {
        let after = start + "@@:return(".len(); // position after the '('
        let bytes = result.as_bytes();
        let mut depth = 1i32;
        let mut j = after;
        while j < bytes.len() && depth > 0 {
            match bytes[j] {
                b'(' => depth += 1,
                b')' => depth -= 1,
                _ => {}
            }
            if depth > 0 {
                j += 1;
            }
        }
        if depth == 0 {
            let expr = result[after..j].trim();
            let expansion = if expr.is_empty() {
                // Void form: bare exit. Erlang has no return keyword (the last
                // expression is the value); emit the unit atom `ok` so the
                // enclosing clause body is never empty (an empty Erlang clause
                // body is a syntax error). Other targets emit a native return,
                // matching the per-language semicolon convention used by the
                // value form below.
                match lang {
                    TargetLanguage::Python3
                    | TargetLanguage::GDScript
                    | TargetLanguage::Ruby
                    | TargetLanguage::Kotlin
                    | TargetLanguage::Swift
                    | TargetLanguage::Lua
                    | TargetLanguage::Go => "return".to_string(),
                    _ => "return;".to_string(),
                }
            } else {
                match lang {
                    TargetLanguage::Python3
                    | TargetLanguage::GDScript
                    | TargetLanguage::Ruby
                    | TargetLanguage::Kotlin
                    | TargetLanguage::Swift
                    | TargetLanguage::Lua
                    | TargetLanguage::Go => format!("return {}", expr),
                    _ => format!("return {};", expr),
                }
            };
            result = format!("{}{}{}", &result[..start], expansion, &result[j + 1..]);
        } else {
            break; // unmatched paren — bail
        }
    }

    // Expand @@:(expr) → return expr
    // In operation bodies, @@:(expr) means "return this value" (no context stack).
    // This handles patterns like @@:(@@:system.state) where the inner was already expanded.
    while let Some(start) =
        crate::frame_c::compiler::codegen::codegen_utils::find_outside_strings_and_comments(
            &result, lang, "@@:(",
        )
    {
        let after = start + 4; // position after "@@:("
        let bytes = result.as_bytes();
        let mut depth = 1i32;
        let mut j = after;
        while j < bytes.len() && depth > 0 {
            match bytes[j] {
                b'(' => depth += 1,
                b')' => depth -= 1,
                _ => {}
            }
            if depth > 0 {
                j += 1;
            }
        }
        if depth == 0 {
            let expr = &result[after..j];
            let expansion = match lang {
                // Erlang: last expression IS the return value
                // No-semicolon languages
                TargetLanguage::Python3
                | TargetLanguage::GDScript
                | TargetLanguage::Ruby
                | TargetLanguage::Kotlin
                | TargetLanguage::Swift
                | TargetLanguage::Lua
                | TargetLanguage::Go => format!("return {}", expr),
                // Semicolon languages
                _ => format!("return {};", expr),
            };
            result = format!("{}{}{}", &result[..start], expansion, &result[j + 1..]);
        } else {
            break; // unmatched paren — bail
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    // #141: `@@:return(expr)` in an action/operation body must lower to a
    // native `return expr` on native-passthrough targets — previously the
    // literal sigil leaked through verbatim.
    #[test]
    fn return_call_in_action_body_lowers() {
        assert_eq!(
            expand_system_state_in_code("@@:return(1)", TargetLanguage::Python3),
            "return 1"
        );
        assert_eq!(
            expand_system_state_in_code("@@:return(1)", TargetLanguage::Ruby),
            "return 1"
        );
        assert_eq!(
            expand_system_state_in_code("@@:return(1)", TargetLanguage::TypeScript),
            "return 1;"
        );
    }

    // #141: void `@@:return()` in an action body → bare exit (no empty
    // assignment, no leaked sigil). `ok` on Erlang (empty clause body is a
    // syntax error there).
    #[test]
    fn void_return_call_in_action_body_lowers() {
        assert_eq!(
            expand_system_state_in_code("@@:return()", TargetLanguage::Python3),
            "return"
        );
        assert_eq!(
            expand_system_state_in_code("@@:return()", TargetLanguage::TypeScript),
            "return;"
        );
    }

    // Balanced-paren extraction: a nested call inside `@@:return(...)` must
    // not be truncated at the inner `)`.
    #[test]
    fn return_call_preserves_nested_parens() {
        assert_eq!(
            expand_system_state_in_code("@@:return(self.f(1, 2))", TargetLanguage::Python3),
            "return self.f(1, 2)"
        );
    }

    // The sibling `@@:(expr)` form is unaffected by the new pass.
    #[test]
    fn context_return_expr_still_lowers() {
        assert_eq!(
            expand_system_state_in_code("@@:(0)", TargetLanguage::Python3),
            "return 0"
        );
    }
}
