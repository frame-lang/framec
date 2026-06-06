//! WebAssembly bindings for the Frame transpiler (`framec`).
//!
//! A thin `#[wasm_bindgen]` shim over [`framec::run`]. Keeping wasm-bindgen
//! confined to this crate lets the `framec` library and CLI stay
//! dependency-clean. Built with `wasm-pack` and published to npm as
//! `@frame-lang/framec-wasm` for the web playground and Node consumers.

use wasm_bindgen::prelude::*;

/// Transpile Frame `source` to the named `target` language.
///
/// `target` is one of framec's target names (`"rust"`, `"python_3"`,
/// `"typescript"`, …); the caller selects it, so the source need not carry
/// an `@@[target(...)]` directive. Returns the generated code, or the
/// compiler's error text on failure.
#[wasm_bindgen]
pub fn run(source: &str, target: &str) -> String {
    framec::run(source, target)
}

/// RFC-0043 Phase 1 migration: insert `@@[async]` on every `@@system`
/// in `source` whose body declares async members but whose header lacks
/// the attribute. The WASM-callable equivalent of the CLI subcommand
/// `framec project add-async-attr`, intended for in-browser / npm
/// migrations that cannot reach the CLI. Idempotent.
#[wasm_bindgen]
pub fn migrate_async_attr(source: &str) -> String {
    framec::frame_c::codemod::add_async_attr_to_source(source)
}
