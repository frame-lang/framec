//! Per-target type mappers — Frame-generated state machines used
//! by codegen.
//!
//! Part of RFC-0035 round 2: take framec's per-target type
//! conversion helpers (`csharp_map_type`, `java_map_type`,
//! `kotlin_map_type`, `swift_map_type`, `go_map_type`,
//! `cpp_map_type`, plus the Rust `frame_type_to_rust_type`,
//! `rust_dispatch_convert`, and `rust_owned_promotion`) and
//! express each one as a Frame single-state system. The public
//! Rust function exposed here delegates to the generated FSM
//! and preserves the original signature so existing call sites
//! don't change.
//!
//! Why this round: type-map functions are uniformly shaped
//! (`Type → String`), small, side-effect-free, and have
//! exhaustive matching at the body. They are an excellent fit
//! for Frame's function-definition shape — the body is native
//! Rust passthrough (no Frame statements), so the Frame system
//! becomes a declarative wrapper that names the function and
//! its signature. Showing this fit is a goal in itself.
//!
//! `swift_map_type` is the most interesting case: its body
//! recurses (for `T | nil` and `T[]` suffixes). In Frame's
//! single-state shape recursion has to go through a fresh
//! `__create()` instance or through the public glue function.
//! We use the glue-function form (recursive call back through
//! this module's public API). That is awkward and worth
//! recording as a Frame ergonomics observation — the natural
//! Rust idiom (`fn swift_map_type(t: &str) -> String { ... }`
//! recursing on itself) is cleaner.
//!
//! To regenerate after editing a `.frs` source:
//!   ./target/release/framec compile -l rust \
//!     framec/src/frame_c/compiler/type_map/<name>.frs \
//!     > framec/src/frame_c/compiler/type_map/<name>.gen.rs

use crate::frame_c::compiler::frame_ast::Type;

mod csharp_map_type_fsm {
    #![allow(unreachable_patterns)]
    #![allow(unused_mut)]
    #![allow(dead_code)]
    #![allow(non_snake_case)]
    #![allow(unused_variables)]
    include!("csharp_map_type.gen.rs");
}

mod java_map_type_fsm {
    #![allow(unreachable_patterns)]
    #![allow(unused_mut)]
    #![allow(dead_code)]
    #![allow(non_snake_case)]
    #![allow(unused_variables)]
    include!("java_map_type.gen.rs");
}

mod kotlin_map_type_fsm {
    #![allow(unreachable_patterns)]
    #![allow(unused_mut)]
    #![allow(dead_code)]
    #![allow(non_snake_case)]
    #![allow(unused_variables)]
    include!("kotlin_map_type.gen.rs");
}

mod go_map_type_fsm {
    #![allow(unreachable_patterns)]
    #![allow(unused_mut)]
    #![allow(dead_code)]
    #![allow(non_snake_case)]
    #![allow(unused_variables)]
    include!("go_map_type.gen.rs");
}

mod cpp_map_type_fsm {
    #![allow(unreachable_patterns)]
    #![allow(unused_mut)]
    #![allow(dead_code)]
    #![allow(non_snake_case)]
    #![allow(unused_variables)]
    include!("cpp_map_type.gen.rs");
}

mod swift_map_type_fsm {
    #![allow(unreachable_patterns)]
    #![allow(unused_mut)]
    #![allow(dead_code)]
    #![allow(non_snake_case)]
    #![allow(unused_variables)]
    include!("swift_map_type.gen.rs");
}

mod rust_map_type_fsm {
    #![allow(unreachable_patterns)]
    #![allow(unused_mut)]
    #![allow(dead_code)]
    #![allow(non_snake_case)]
    #![allow(unused_variables)]
    include!("rust_map_type.gen.rs");
}

mod rust_dispatch_convert_fsm {
    #![allow(unreachable_patterns)]
    #![allow(unused_mut)]
    #![allow(dead_code)]
    #![allow(non_snake_case)]
    #![allow(unused_variables)]
    include!("rust_dispatch_convert.gen.rs");
}

mod rust_owned_promotion_fsm {
    #![allow(unreachable_patterns)]
    #![allow(unused_mut)]
    #![allow(dead_code)]
    #![allow(non_snake_case)]
    #![allow(unused_variables)]
    include!("rust_owned_promotion.gen.rs");
}

/// Map a Frame type string to C# type for `(Type)` cast contexts.
pub(crate) fn csharp_map_type(t: &str) -> String {
    csharp_map_type_fsm::CsharpMapType::__create().map(t.to_string())
}

/// Map a Frame type string to Java type for `(Type)` cast contexts.
pub(crate) fn java_map_type(t: &str) -> String {
    java_map_type_fsm::JavaMapType::__create().map(t.to_string())
}

/// Map a Frame type string to Kotlin type for cast contexts.
pub(crate) fn kotlin_map_type(t: &str) -> String {
    kotlin_map_type_fsm::KotlinMapType::__create().map(t.to_string())
}

/// Map a Frame type string to Go type for type assertions.
pub(crate) fn go_map_type(t: &str) -> String {
    go_map_type_fsm::GoMapType::__create().map(t.to_string())
}

/// Map a Frame type string to C++ type for `std::any_cast<T>` and
/// related contexts.
pub(crate) fn cpp_map_type(t: &str) -> String {
    cpp_map_type_fsm::CppMapType::__create().map(t.to_string())
}

/// Map a Frame type string to Swift type. Handles `T | nil` →
/// `T?` and `T[]` → `[T]` recursively. Recursion routes through
/// this public function (the FSM body calls back into here).
pub(crate) fn swift_map_type(t: &str) -> String {
    swift_map_type_fsm::SwiftMapType::__create().map(t.to_string())
}

/// Map a Frame `Type` to a Rust owned-type spelling. Used inside
/// generated structs and event variants. `Type::Unknown` collapses
/// to `String`.
pub(crate) fn frame_type_to_rust_type(t: &Type) -> String {
    let s = match t {
        Type::Custom(name) => name.clone(),
        Type::Unknown => String::new(),
    };
    rust_map_type_fsm::RustMapType::__create().map(s)
}

/// RFC-0033 helper: returns the `.to_string()` / `.to_vec()` /
/// `.clone()` suffix for a dispatch-site conversion from a
/// method's borrowed-or-owned param to the owned form held by
/// the event variant.
pub(crate) fn rust_dispatch_convert(t: &Type) -> &'static str {
    let s = match t {
        Type::Custom(name) => name.clone(),
        Type::Unknown => String::new(),
    };
    let result = rust_dispatch_convert_fsm::RustDispatchConvert::__create().suffix(s);
    match result.as_str() {
        ".to_string()" => ".to_string()",
        ".to_vec()" => ".to_vec()",
        _ => ".clone()",
    }
}

/// RFC-0033 helper: returns the promoted owned-type spelling for
/// a Rust interface-parameter type string, or `None` if no
/// promotion applies.
///
/// - `&str` → `Some("String".to_string())`
/// - `&[T]` → `Some("Vec<T>".to_string())`
/// - other → `None`
#[allow(dead_code)]
pub(crate) fn rust_owned_promotion(t: &str) -> Option<String> {
    let result = rust_owned_promotion_fsm::RustOwnedPromotion::__create().promote(t.to_string());
    if result.is_empty() {
        None
    } else {
        Some(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // FRAMEC_BUGS #37: the type-name alias tables were exterminated. Frame
    // has no type system — type names pass through VERBATIM. Each backend
    // mapper now asserts (a) native types pass through unchanged, (b) the
    // old Frame-isms (`str`/`int`/`Any`) are NO LONGER translated, and
    // (c) the few genuinely-structural transforms that remain (void-spelling,
    // Swift nullable/array syntax, Rust borrow→owned widening).

    #[test]
    fn csharp_basic() {
        // Native types pass through.
        assert_eq!(csharp_map_type("object"), "object");
        assert_eq!(csharp_map_type("string"), "string");
        assert_eq!(csharp_map_type("int"), "int");
        assert_eq!(csharp_map_type("MyType"), "MyType");
        // Frame-isms are no longer translated — verbatim passthrough.
        assert_eq!(csharp_map_type("str"), "str");
        assert_eq!(csharp_map_type("Any"), "Any");
    }

    #[test]
    fn java_basic() {
        assert_eq!(java_map_type("String"), "String");
        assert_eq!(java_map_type("int"), "int");
        assert_eq!(java_map_type("void"), "void");
        assert_eq!(java_map_type("str"), "str"); // passthrough now
        assert_eq!(java_map_type("Any"), "Any");
    }

    #[test]
    fn kotlin_basic() {
        assert_eq!(kotlin_map_type("Int"), "Int");
        assert_eq!(kotlin_map_type("String"), "String");
        assert_eq!(kotlin_map_type("MyType"), "MyType");
        assert_eq!(kotlin_map_type("void"), "Unit"); // structural: kept
        assert_eq!(kotlin_map_type("str"), "str"); // passthrough now
        assert_eq!(kotlin_map_type("Any"), "Any");
    }

    #[test]
    fn go_basic() {
        assert_eq!(go_map_type("string"), "string");
        assert_eq!(go_map_type("int"), "int");
        assert_eq!(go_map_type("void"), ""); // structural: kept
        assert_eq!(go_map_type("None"), ""); // structural: kept
        assert_eq!(go_map_type("str"), "str"); // passthrough now
        assert_eq!(go_map_type("any"), "any");
    }

    #[test]
    fn cpp_basic() {
        assert_eq!(cpp_map_type("std::string"), "std::string");
        assert_eq!(cpp_map_type("std::vector<int>"), "std::vector<int>");
        assert_eq!(cpp_map_type("int"), "int");
        assert_eq!(cpp_map_type("str"), "str"); // passthrough now
        assert_eq!(cpp_map_type("Any"), "Any");
    }

    #[test]
    fn swift_basic() {
        assert_eq!(swift_map_type("String"), "String");
        assert_eq!(swift_map_type("Int"), "Int");
        assert_eq!(swift_map_type("void"), "Void"); // structural: kept
        assert_eq!(swift_map_type("str"), "str"); // passthrough now
        assert_eq!(swift_map_type("Any"), "Any");
    }

    #[test]
    fn swift_nullable() {
        // Nullable SYNTAX is kept (structural), inner type passes through.
        assert_eq!(swift_map_type("String | nil"), "String?");
        assert_eq!(swift_map_type("Int | None"), "Int?");
    }

    #[test]
    fn swift_array() {
        // Array SYNTAX is kept (structural), inner type passes through.
        assert_eq!(swift_map_type("String[]"), "[String]");
        assert_eq!(swift_map_type("Int[]"), "[Int]");
    }

    #[test]
    fn rust_basic() {
        // Native Rust types pass through.
        assert_eq!(frame_type_to_rust_type(&Type::Custom("i64".into())), "i64");
        assert_eq!(
            frame_type_to_rust_type(&Type::Custom("String".into())),
            "String"
        );
        assert_eq!(
            frame_type_to_rust_type(&Type::Custom("MyEnum".into())),
            "MyEnum"
        );
        // Frame-isms are no longer translated — verbatim passthrough.
        assert_eq!(frame_type_to_rust_type(&Type::Custom("int".into())), "int");
        assert_eq!(frame_type_to_rust_type(&Type::Custom("str".into())), "str");
        // Unknown (no annotation) still defaults to an owned String, where a
        // concrete type is structurally required.
        assert_eq!(frame_type_to_rust_type(&Type::Unknown), "String");
    }

    #[test]
    fn rust_borrowed_promotion() {
        assert_eq!(
            frame_type_to_rust_type(&Type::Custom("&str".into())),
            "String"
        );
        assert_eq!(
            frame_type_to_rust_type(&Type::Custom("&[i32]".into())),
            "Vec<i32>"
        );
    }

    #[test]
    fn rust_dispatch_convert_basic() {
        assert_eq!(
            rust_dispatch_convert(&Type::Custom("&str".into())),
            ".to_string()"
        );
        assert_eq!(
            rust_dispatch_convert(&Type::Custom("&[u8]".into())),
            ".to_vec()"
        );
        assert_eq!(
            rust_dispatch_convert(&Type::Custom("String".into())),
            ".clone()"
        );
        assert_eq!(rust_dispatch_convert(&Type::Unknown), ".clone()");
    }

    #[test]
    fn rust_owned_promotion_basic() {
        assert_eq!(rust_owned_promotion("&str"), Some("String".to_string()));
        assert_eq!(rust_owned_promotion("&[u8]"), Some("Vec<u8>".to_string()));
        assert_eq!(rust_owned_promotion("String"), None);
        assert_eq!(rust_owned_promotion("MyType"), None);
    }
}
