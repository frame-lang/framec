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
