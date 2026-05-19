//! Name converters — Frame-generated state machines used by codegen.
//!
//! Part of RFC-0035 round 1: use Frame to express as many of
//! framec's own utility functions as possible. Each name
//! converter lives in its own `.frs` spec and is exposed here as
//! a public function with the original `&str → String` signature
//! so existing call sites need no changes.
//!
//! These are deliberately single-state systems — Frame's syntax
//! used as function-definition syntax. They generate substantial
//! scaffolding (mod wrapper, event/return/value enums, etc.)
//! which is fine: the value is showing what kinds of code Frame
//! CAN express, not minimizing the Rust line count.
//!
//! To regenerate after editing a `.frs` source:
//!   ./target/release/framec compile -l rust \
//!     -o framec/src/frame_c/compiler/name/ \
//!     framec/src/frame_c/compiler/name/<name>.frs
//!   mv framec/src/frame_c/compiler/name/<name>.rs \
//!     framec/src/frame_c/compiler/name/<name>.gen.rs

mod to_snake_case_fsm {
    #![allow(unreachable_patterns)]
    #![allow(unused_mut)]
    #![allow(dead_code)]
    #![allow(non_snake_case)]
    #![allow(unused_variables)]
    include!("to_snake_case.gen.rs");
}

mod pascal_case_variant_fsm {
    #![allow(unreachable_patterns)]
    #![allow(unused_mut)]
    #![allow(dead_code)]
    #![allow(non_snake_case)]
    #![allow(unused_variables)]
    include!("pascal_case_variant.gen.rs");
}

/// CamelCase / PascalCase → snake_case. Used by Erlang naming,
/// state-name → method-name conversion, and other normalization
/// sites across framec. Implemented as a Frame system (single-
/// state). See `to_snake_case.frs` for the source.
pub fn to_snake_case(s: &str) -> String {
    to_snake_case_fsm::ToSnakeCase::__create().convert(s.to_string())
}

/// snake_case → PascalCase, used by the Rust target to emit enum
/// variant names from Frame event names. Implemented as a Frame
/// system (single-state). See `pascal_case_variant.frs` for the
/// source.
pub fn pascal_case_variant(s: &str) -> String {
    pascal_case_variant_fsm::PascalCaseVariant::__create().convert(s.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snake_case_basic() {
        assert_eq!(to_snake_case("HelloWorld"), "hello_world");
        assert_eq!(to_snake_case("getStatus"), "get_status");
        assert_eq!(to_snake_case("ABCFlag"), "a_b_c_flag");
        assert_eq!(to_snake_case("already_snk"), "already_snk");
        assert_eq!(to_snake_case(""), "");
        assert_eq!(to_snake_case("X"), "x");
    }

    #[test]
    fn pascal_case_basic() {
        assert_eq!(pascal_case_variant("get_status"), "GetStatus");
        assert_eq!(pascal_case_variant("tick"), "Tick");
        assert_eq!(pascal_case_variant("_leading"), "Leading");
        assert_eq!(pascal_case_variant("snake_to_pascal"), "SnakeToPascal");
        assert_eq!(pascal_case_variant(""), "");
    }
}
