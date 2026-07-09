//! Persistence type manifest (RFC-0054 Phase A).
//!
//! One structured, framec-derived description of *what type occupies each
//! persisted compartment slot* — built ONCE from the machine AST and consumed by
//! every backend's save/restore emission, replacing the ~28 near-identical
//! per-backend AST-walks that independently re-derived the same per-state type
//! lists (and drifted, producing the 2026-07 state-var fidelity bugs).
//!
//! The manifest carries the **raw, opaque Frame type string** (`Type::Custom`
//! verbatim, `""` for `Unknown`) — the type-ignorant boundary (RFC-0053) holds:
//! framec never inspects a user type's fields. Each backend keeps its own type
//! mapping (`java_box`, `cpp_map_type`, …) and emission at the consumption site,
//! so output stays byte-identical; only the derivation is unified.

use crate::frame_c::compiler::frame_ast::{SystemAst, Type};

/// The typed compartment slots of a single state, in declaration order.
pub(in crate::frame_c::compiler::codegen::interface_gen) struct StateManifest {
    pub name: String,
    /// `(var_name, raw_type)` — keyed by name (`compartment.state_vars`).
    pub state_vars: Vec<(String, String)>,
    /// Raw types by index (`compartment.state_args`).
    pub state_args: Vec<String>,
    /// Raw types by index (`compartment.enter_args`).
    pub enter_args: Vec<String>,
    /// Raw types by index (`compartment.exit_args`).
    pub exit_args: Vec<String>,
}

/// The persist type manifest for one system: every state's typed compartment
/// slots. Domain fields are (currently) still derived per-backend — Phase A
/// step 1 unifies the compartment slots, where the duplication and drift lived.
pub(in crate::frame_c::compiler::codegen::interface_gen) struct PersistManifest {
    pub states: Vec<StateManifest>,
}

impl PersistManifest {
    /// The state vars of `state`, or `&[]` if the state has none / is unknown.
    pub fn state_vars_of(&self, state: &str) -> &[(String, String)] {
        self.states
            .iter()
            .find(|s| s.name == state)
            .map(|s| s.state_vars.as_slice())
            .unwrap_or(&[])
    }
}

/// The raw Frame type string for a declared type — verbatim for a user/native
/// type, empty for `Unknown`. The single place the manifest touches `Type`.
fn raw_type(t: &Type) -> String {
    match t {
        Type::Custom(s) => s.clone(),
        Type::Unknown => String::new(),
    }
}

/// Build the manifest from a system's machine AST. Mirrors, once, the AST-walk
/// every schema backend used to hand-write four times (state_args / enter_args /
/// exit_args by index, state_vars by name).
pub(in crate::frame_c::compiler::codegen::interface_gen) fn build_persist_manifest(
    system: &SystemAst,
) -> PersistManifest {
    let states = system
        .machine
        .as_ref()
        .map(|m| {
            m.states
                .iter()
                .map(|s| StateManifest {
                    name: s.name.clone(),
                    state_vars: s
                        .state_vars
                        .iter()
                        .map(|sv| (sv.name.clone(), raw_type(&sv.var_type)))
                        .collect(),
                    state_args: s.params.iter().map(|p| raw_type(&p.param_type)).collect(),
                    enter_args: s
                        .enter
                        .as_ref()
                        .map(|e| e.params.iter().map(|p| raw_type(&p.param_type)).collect())
                        .unwrap_or_default(),
                    exit_args: s
                        .exit
                        .as_ref()
                        .map(|e| e.params.iter().map(|p| raw_type(&p.param_type)).collect())
                        .unwrap_or_default(),
                })
                .collect()
        })
        .unwrap_or_default();
    PersistManifest { states }
}
