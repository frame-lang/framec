//! Target-language queries — Frame-generated predicates.
//!
//! Part of RFC-0035 round 2: tiny predicates (currently just
//! `is_dynamic_target`) expressed as Frame single-state systems.
//! The Frame interface is stringly-typed, so the predicate
//! round-trips the answer as `"true"` / `"false"` and the glue
//! function here parses it back to `bool`. This is a deliberate
//! ergonomic stress-test of Frame on a shape it does not fit
//! cleanly — booleans and enums are not first-class in Frame's
//! interface signatures yet, so the round-trip is awkward. The
//! awkwardness is the point: documenting where Frame fits well
//! and where it doesn't is RFC-0035's goal.
//!
//! To regenerate after editing the `.frs` source:
//!   ./target/release/framec compile -l rust \
//!     framec/src/frame_c/compiler/target_query/is_dynamic_target.frs \
//!     > framec/src/frame_c/compiler/target_query/is_dynamic_target.gen.rs

use crate::frame_c::visitors::TargetLanguage;

mod is_dynamic_target_fsm {
    #![allow(unreachable_patterns)]
    #![allow(unused_mut)]
    #![allow(dead_code)]
    #![allow(non_snake_case)]
    #![allow(unused_variables)]
    include!("is_dynamic_target.gen.rs");
}

/// True for dynamic / loosely-typed targets that don't need
/// explicit casts on `Any` values: Python, JavaScript, Ruby,
/// Lua, PHP, GDScript, Erlang.
pub(crate) fn is_dynamic_target(lang: TargetLanguage) -> bool {
    let name = match lang {
        TargetLanguage::Python3 => "python_3",
        TargetLanguage::JavaScript => "javascript",
        TargetLanguage::TypeScript => "typescript",
        TargetLanguage::Rust => "rust",
        TargetLanguage::C => "c",
        TargetLanguage::Cpp => "cpp",
        TargetLanguage::CSharp => "csharp",
        TargetLanguage::Java => "java",
        TargetLanguage::Go => "go",
        TargetLanguage::Php => "php",
        TargetLanguage::Kotlin => "kotlin",
        TargetLanguage::Swift => "swift",
        TargetLanguage::Ruby => "ruby",
        TargetLanguage::Lua => "lua",
        TargetLanguage::Dart => "dart",
        TargetLanguage::GDScript => "gdscript",
        TargetLanguage::Graphviz => "graphviz",
    };
    let result = is_dynamic_target_fsm::IsDynamicTarget::__create().check(name.to_string());
    result == "true"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dynamic_targets_match() {
        assert!(is_dynamic_target(TargetLanguage::Python3));
        assert!(is_dynamic_target(TargetLanguage::JavaScript));
        assert!(is_dynamic_target(TargetLanguage::Ruby));
        assert!(is_dynamic_target(TargetLanguage::Lua));
        assert!(is_dynamic_target(TargetLanguage::Php));
        assert!(is_dynamic_target(TargetLanguage::GDScript));
    }

    #[test]
    fn static_targets_excluded() {
        assert!(!is_dynamic_target(TargetLanguage::Rust));
        assert!(!is_dynamic_target(TargetLanguage::Java));
        assert!(!is_dynamic_target(TargetLanguage::Kotlin));
        assert!(!is_dynamic_target(TargetLanguage::Swift));
        assert!(!is_dynamic_target(TargetLanguage::Go));
        assert!(!is_dynamic_target(TargetLanguage::CSharp));
        assert!(!is_dynamic_target(TargetLanguage::Cpp));
        assert!(!is_dynamic_target(TargetLanguage::C));
        assert!(!is_dynamic_target(TargetLanguage::TypeScript));
        assert!(!is_dynamic_target(TargetLanguage::Dart));
    }
}
