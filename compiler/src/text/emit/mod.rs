//! EMIT — the sink. **The only module that may unwrap [`NativeText`].**
//!
//! # Emission is one-way, and it is a borrow-check error to make it otherwise
//!
//! RFC-0056 P6 says: once a node is rendered to text, no pass may run over that text.
//! In the old compiler that was a *policy*, and the policy lost — the assembler
//! grepped its own output for the `package` clause, `normalize_indentation`
//! post-processed emitted code (and silently changed the value of string literals),
//! `php_prefix_params` rewrote already-expanded output, and `java_await_rewrite`
//! existed solely to *un-do* a codegen bug by rewriting the text codegen had just
//! produced.
//!
//! Here it is not a policy:
//!
//! * [`NativeText::finish`](crate::text::NativeText::finish) is `pub(in crate::text)`
//!   — no module outside `crate::text` can call it. So the tree, the symbol table,
//!   and every semantic pass **cannot obtain a `String` from a node.** They cannot
//!   run `.contains()` on one because they cannot get one.
//! * `finish` **consumes `self`**. A pass that wanted to re-read what it emitted would
//!   have to hold a value it has already given away. rustc reports that, not a
//!   reviewer at the end of a long day.

pub mod atom;
pub mod c;
pub mod driver;
pub mod java;
pub mod persist;
pub mod python;
pub mod rust;
pub mod reindent;

use super::NativeText;

/// Accumulates the target file. Text goes IN; it never comes back out.
///
/// There is deliberately no `fn text(&self) -> &str`, no `Deref`, no `Display`. Once
/// a byte is in the sink it is gone, and a pass that wants to know something about
/// what was emitted must instead ask **the node it emitted from** — which still
/// exists, and which knows the answer, because framec put it there.
#[derive(Default)]
pub struct Sink {
    out: String,
}

impl Sink {
    pub fn new() -> Sink {
        Sink { out: String::new() }
    }

    /// Emit verbatim native text. This is the terminal move for a `NativeText`.
    pub fn native(&mut self, t: NativeText) {
        self.out.push_str(&t.finish());
    }

    /// Emit text framec authored. Distinct from `native` on purpose: framec MAY
    /// terminate, indent and format its own output, and MUST NOT touch the user's.
    pub fn frame(&mut self, s: &str) {
        self.out.push_str(s);
    }

    /// Hand the finished file to the caller. Consumes the sink.
    pub fn finish(self) -> String {
        self.out
    }
}
