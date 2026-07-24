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
mod base_column;
pub mod c;
mod domain_init_walk;
pub mod driver;
mod emit_actions;
mod emit_file;
mod emit_handlers;
mod emit_interface;
mod emit_system;
mod hsm_chain_walk;
pub mod java;
pub mod persist;
pub mod python;
mod router_walk;
pub mod rust;
pub mod reindent;
mod state_dispatch_walk;
mod stmt_walk;

use super::NativeText;

/// The argument-list text of a `@@Sys(...)` field initializer — everything between the first
/// `(` and the last `)`. `@@Inner(42)` → `42`; `@@Inner()` / `@@Inner` → "". These are the
/// verbatim args the user wrote; the domain-init and state-var-seed emitters had been
/// discarding them and hardcoding an empty argument list, dropping e.g. `@@Inner(42)` to
/// `Inner()`. `init_system` gives the name; this recovers the args that sit beside it in
/// `init_text`. Whole-arg-string (not comma-split): each backend's `system_ctor_call` joins a
/// single-element list to itself, so `Inner(1, 2)` round-trips without re-parsing commas.
pub fn ctor_init_args(init_text: Option<&str>) -> String {
    let Some(t) = init_text else {
        return String::new();
    };
    let (Some(open), Some(close)) = (t.find('('), t.rfind(')')) else {
        return String::new();
    };
    if close <= open + 1 {
        return String::new();
    }
    t[open + 1..close].trim().to_string()
}

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
