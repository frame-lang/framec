//! The Frame compiler — rebuild (RFC-0057).
//!
//! # The wall
//!
//! This crate has **no dependency on `framec`**. Not by convention — by `Cargo.toml`.
//! `use framec::…` is a link error. See `../REUSE.md`.
//!
//! # Does codegen only source from the AST and the symbol table?
//!
//! **Yes — and it is a link error, not a promise.**
//!
//! Every one of the twenty-five bugs in the old compiler has one shape: framec knew a
//! fact while generating, encoded it into **text**, threw the fact away, and then
//! re-read its own output (or the user's) to recover it. Text is what makes that
//! possible, so text is what we take away.
//!
//! Exactly **two** modules in this compiler may touch text, and both are descendants
//! of [`text`], which is the only reason the restriction is *sayable* in Rust:
//!
//! | module | privilege | why it is allowed |
//! |---|---|---|
//! | [`text::scan`] | may call `Source::open` -> `&[u8]` | it is the lexer; someone has to read the file |
//! | [`text::emit`] | may call `NativeText::finish` -> `String` | it is the sink; someone has to write the file |
//!
//! **Everything else** — [`tree`], and every semantic pass that will be built on it —
//! sits outside `crate::text` and therefore **cannot obtain bytes and cannot obtain a
//! `String` from a node**. A pass physically cannot run `.contains("co_await")` on
//! anything, because it cannot get anything to run it on.
//!
//! So a future backend has no way to ask text a question. Its only source of facts is
//! the tree. That is not discipline; it is the type system, and it is checked on every
//! build.

pub mod text;

pub mod resolve;
pub mod tree;
pub mod validate;

pub use text::{NativeText, Source, SourceError, Span};
pub use text::scan;

/// # The wall, as a test that runs on every build
///
/// These two doctests are the guarantee. If someone ever widens `Source::open` or
/// `NativeText::finish`, **these start compiling and the build fails.** The proof is
/// not a comment; it is checked.
///
/// A pass outside `crate::text` cannot read the user's source bytes:
///
/// ```compile_fail,E0624
/// use frame_compiler::Source;
/// fn codegen(s: &Source) -> usize {
///     s.open().len()          // error[E0624]: method `open` is private
/// }
/// ```
///
/// A pass outside `crate::text` cannot turn a node back into greppable text — this is
/// the single line that shipped on seven backends:
///
/// ```compile_fail,E0624
/// use frame_compiler::NativeText;
/// fn codegen(t: NativeText) -> bool {
///     t.finish().contains("co_await")   // error[E0624]: method `finish` is private
/// }
/// ```
#[cfg(doctest)]
pub struct TheWall;
