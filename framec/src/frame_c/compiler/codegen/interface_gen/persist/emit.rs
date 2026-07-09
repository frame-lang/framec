//! Shared persist emission scaffold (RFC-0054 Phase A / A2).
//!
//! A1 unified the *derivation* (one [`PersistManifest`](super::manifest)). A2
//! unifies the one piece of *emission* the "if-chain" schema backends (go, c#, c)
//! genuinely share: the per-state guarded-block control flow — "for each state,
//! build a decode branch; when it is non-empty, wrap it in a `state == "X"`
//! guard." Only the guard's surface text (compartment receiver, brace style)
//! varies per backend, so it is passed as a formatter.
//!
//! What is deliberately NOT abstracted here (would be a leaky trait, not a
//! primitive): the per-slot decode string (each backend's own `*_typed_conv`),
//! and the save-vs-restore asymmetry — cpp emits guarded blocks on *both* save
//! and restore with inline `any_cast`, and the switch-family backends (java,
//! kotlin, dart) emit a single `switch`/`default`, not an if-chain. Those stay
//! bespoke by decision (see rfc-0054 §"A2 (emission)").

use super::manifest::StateManifest;

/// Emit one per-state guarded block per state that has a non-empty decode branch.
///
/// - `branch(state)` builds the decode body for a state (empty ⇒ no block).
/// - `guard(state_name, branch)` wraps a non-empty body in the backend's guard.
///   It is `FnMut` so a backend can carry a first-block side-effect (e.g. C#
///   emits a one-time `// D10` header before its first non-empty block); pass
///   `&mut the_guard` to reuse one such guard across several category calls.
///
/// Byte-identical to the hand-written `for s in &manifest.states { … if
/// !branch.is_empty() { push guard } }` loops it replaces.
pub(in crate::frame_c::compiler::codegen::interface_gen) fn emit_per_state_blocks(
    out: &mut String,
    states: &[StateManifest],
    mut branch: impl FnMut(&StateManifest) -> String,
    mut guard: impl FnMut(&str, &str) -> String,
) {
    for s in states {
        let body = branch(s);
        if !body.is_empty() {
            out.push_str(&guard(&s.name, &body));
        }
    }
}

/// Build a decode branch for an **indexed** arg category (`state_args`,
/// `enter_args`, `exit_args`): concatenate `conv(type, index)` over the slot
/// types, skipping empties. The backend supplies `conv` (its `*_typed_conv`).
pub(in crate::frame_c::compiler::codegen::interface_gen) fn indexed_branch(
    types: &[String],
    mut conv: impl FnMut(&str, usize) -> String,
) -> String {
    let mut branch = String::new();
    for (i, t) in types.iter().enumerate() {
        let c = conv(t, i);
        if !c.is_empty() {
            branch.push_str(&c);
        }
    }
    branch
}

/// Build a decode branch for the **named** `state_vars` category: concatenate
/// `conv(name, type)` over the declared vars, skipping empties.
pub(in crate::frame_c::compiler::codegen::interface_gen) fn named_branch(
    vars: &[(String, String)],
    mut conv: impl FnMut(&str, &str) -> String,
) -> String {
    let mut branch = String::new();
    for (name, t) in vars {
        let c = conv(name, t);
        if !c.is_empty() {
            branch.push_str(&c);
        }
    }
    branch
}
