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
    /// `(field_name, raw_type)` for each **persisted** domain field (i.e. minus
    /// `@@[no_persist]`), keyed by name. Fingerprinted for B1 drift detection —
    /// a domain field's type changing is drift too. (A1's schema-backend decode
    /// consumers read only `states`; `domain` exists for the fingerprint.)
    pub domain: Vec<(String, String)>,
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

    /// A deterministic, opaque fingerprint of the compartment type shape (RFC-0054
    /// Phase B1). framec bakes this string into the generated program and writes
    /// it into every snapshot's `_manifest` field; on restore a backend compares
    /// the snapshot's `_manifest` to its own baked fingerprint by **string
    /// equality** and refuses (E751) on any mismatch. The runtime never *parses*
    /// this — only compares it — so no type is ever resolved from the blob (no new
    /// security surface); the format below exists solely to be injective over
    /// distinct schemas and stable across builds.
    ///
    /// Encoding: length-prefixed (`<byte-len>:<bytes>`), quote-free ASCII framing,
    /// so it embeds as a plain string literal in every target with no escaping and
    /// distinct schemas cannot collide. **Order-insensitive where order is not
    /// semantic:** states and state-vars are sorted by name (both are decoded by
    /// name, so declaration order must not read as drift); `state_args` /
    /// `enter_args` / `exit_args` keep index order (they are positional).
    pub fn fingerprint(&self) -> String {
        // length-prefix: `N:bytes`
        fn lp(out: &mut String, s: &str) {
            out.push_str(&s.len().to_string());
            out.push(':');
            out.push_str(s);
        }
        let mut states: Vec<&StateManifest> = self.states.iter().collect();
        states.sort_by(|a, b| a.name.cmp(&b.name));
        let mut out = String::from("frame-persist-manifest:1");
        for s in states {
            out.push_str("|S");
            lp(&mut out, &s.name);
            out.push_str("|V");
            let mut vars: Vec<&(String, String)> = s.state_vars.iter().collect();
            vars.sort_by(|a, b| a.0.cmp(&b.0));
            for (n, t) in vars {
                lp(&mut out, n);
                lp(&mut out, t);
            }
            out.push_str("|A");
            for t in &s.state_args {
                lp(&mut out, t);
            }
            out.push_str("|E");
            for t in &s.enter_args {
                lp(&mut out, t);
            }
            out.push_str("|X");
            for t in &s.exit_args {
                lp(&mut out, t);
            }
        }
        out.push_str("|D");
        let mut domain: Vec<&(String, String)> = self.domain.iter().collect();
        domain.sort_by(|a, b| a.0.cmp(&b.0));
        for (n, t) in domain {
            lp(&mut out, n);
            lp(&mut out, t);
        }
        out
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
    // Persisted domain fields only (skip `@@[no_persist]`, matching every
    // backend's save/restore loop), with their raw Frame type strings.
    let domain = system
        .domain
        .iter()
        .filter(|v| !v.attributes.iter().any(|a| a.name == "no_persist"))
        .map(|v| (v.name.clone(), raw_type(&v.var_type)))
        .collect();
    PersistManifest { states, domain }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sm(name: &str, vars: &[(&str, &str)], args: &[&str]) -> StateManifest {
        StateManifest {
            name: name.to_string(),
            state_vars: vars
                .iter()
                .map(|(n, t)| (n.to_string(), t.to_string()))
                .collect(),
            state_args: args.iter().map(|t| t.to_string()).collect(),
            enter_args: vec![],
            exit_args: vec![],
        }
    }

    fn pm(states: Vec<StateManifest>) -> PersistManifest {
        PersistManifest {
            states,
            domain: vec![],
        }
    }

    #[test]
    fn fingerprint_is_order_insensitive_for_states_and_vars() {
        // Same schema, different declaration order of states and of state-vars
        // (both decoded by name) must yield the SAME fingerprint — reordering is
        // not a schema change, so it must not read as drift.
        let a = pm(vec![
            sm("Alpha", &[("x", "int"), ("y", "Vec2")], &["int"]),
            sm("Beta", &[], &[]),
        ]);
        let b = pm(vec![
            sm("Beta", &[], &[]),
            sm("Alpha", &[("y", "Vec2"), ("x", "int")], &["int"]),
        ]);
        assert_eq!(a.fingerprint(), b.fingerprint());
    }

    #[test]
    fn fingerprint_distinguishes_a_changed_type() {
        // A state-var type change (int -> Vec2) is real drift: fingerprints differ.
        let a = pm(vec![sm("S", &[("v", "int")], &[])]);
        let b = pm(vec![sm("S", &[("v", "Vec2")], &[])]);
        assert_ne!(a.fingerprint(), b.fingerprint());
    }

    #[test]
    fn fingerprint_arg_order_is_semantic() {
        // state_args are positional (index-keyed) — swapping arg types IS drift.
        let a = pm(vec![sm("S", &[], &["int", "Vec2"])]);
        let b = pm(vec![sm("S", &[], &["Vec2", "int"])]);
        assert_ne!(a.fingerprint(), b.fingerprint());
    }

    #[test]
    fn fingerprint_length_prefix_prevents_delimiter_collision() {
        // Length-prefixing means type strings containing the framing chars
        // (`|`, `:`) still can't collide two distinct schemas.
        let a = pm(vec![sm("S", &[("a", "X|Y")], &[])]);
        let b = pm(vec![sm("S", &[("a", "X"), ("Y", "")], &[])]);
        assert_ne!(a.fingerprint(), b.fingerprint());
    }

    #[test]
    fn fingerprint_covers_domain_type_change() {
        // A domain field's type changing (int -> Vec2) is drift too; domain is
        // name-keyed, so order-insensitive.
        let base = || PersistManifest {
            states: vec![sm("S", &[], &[])],
            domain: vec![
                ("marker".to_string(), "int".to_string()),
                ("pos".to_string(), "Vec2".to_string()),
            ],
        };
        let mut reordered = base();
        reordered.domain.reverse();
        assert_eq!(base().fingerprint(), reordered.fingerprint());

        let mut changed = base();
        changed.domain[0].1 = "long".to_string();
        assert_ne!(base().fingerprint(), changed.fingerprint());
    }
}
