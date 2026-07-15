//! `@@[persist]` — **faithful round-trip, out-of-band framing, from day one.**
//!
//! # The contract (RFC-0053, the foundation; layers deferred)
//!
//! > save -> restore into a FRESH instance -> observations indistinguishable from the
//! > original, including live control state, **identically on every reconstructable
//! > target**, with no target silently restoring a degraded or type-erased value.
//!
//! Three constraints, and the first is the one the old compiler got wrong:
//!
//! ## 1. Disambiguation is out-of-band. (This is #233.)
//!
//! The old compiler stored its type tag as an **inline map key** — `__frame_type__` —
//! **in the same namespace as the user's payload**. So a user `domain: dict` holding
//! `{"__frame_type__": "Point", "x": 99}` was silently restored **as a Point instance**.
//! Confirmed silent on Python, Ruby and PHP. It is verbatim the failure RFC-0053 calls
//! *foundational*: a user container that merely contains the marker key must **never** be
//! mis-restored as a typed instance.
//!
//! This is serde's *internally-tagged* representation, whose documented hazard is exactly
//! this collision. Every serializer that got it right — pickle (opcode, not a key),
//! `ObjectInputStream` (stream protocol), MessagePack ext, serde adjacent/external
//! tagging — puts type identity **out of band from the data namespace**.
//!
//! So framec wraps every typed value in an **envelope** whose slots are disjoint from any
//! user key:
//!
//! ```json
//! { "@f:t": "Point", "@f:v": { "x": 3, "y": 4 } }
//! ```
//!
//! The user's payload lives **only** in `@f:v`. The reviver reads the type **only** from
//! the envelope's `@f:t` slot — never from a user key. A user dict sitting inside `@f:v`
//! is therefore uninterpretable as a tag: it is *data*, wrapped, and it comes back a dict.
//! **The collision is not made unlikely; it is made structurally impossible.**
//!
//! ## 2. Type-ignorant codegen. One mechanism, no per-type branch.
//!
//! Enforced by the shape of this module: there is one save walk and one revive walk, both
//! generic over the value. Nothing here matches on a user type name — there is no place to
//! write such a match, because the per-target code is a fixed template plus the manifest's
//! type list, never a `match user_type`.
//!
//! ## 3. Closed-world safety floor. NON-deferrable.
//!
//! On the reflective route (type-in-snapshot: Python, JS, Ruby, PHP, Lua), restore MUST
//! resolve a blob-named type **only against types the program itself defines** — never
//! ambient globals or imports. The old compiler mostly got this right and it is kept;
//! Ruby leaked via a file-membership heuristic (a monkeypatched stdlib class became
//! resolvable), so the rebuild uses an **emitted lexical registry** of framec-known types
//! instead of enumerating a module or file.

use crate::resolve::SystemSym;

/// The reserved envelope keys. **Chosen to be disjoint from plausible user keys**, but
/// the disjointness is not what makes this safe — the *out-of-band framing* is. Even if a
/// user used these exact keys, they would land inside `@f:v` (the value slot) on save and
/// be escaped, never read as a tag. See `save`/`revive`.
pub const TAG: &str = "@f:t";
pub const VAL: &str = "@f:v";

/// Which persistence route a target takes.
///
/// This is a per-target FACT, in a table — not a `match lang` scattered through codegen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Route {
    /// The type identity travels **in the snapshot**; restore reconstructs it. Python,
    /// JS, Ruby, PHP, Lua. This is the route where #233 and the safety floor live.
    Reflective,
    /// The type is **fixed at codegen**; the target's serializer deserializes into the
    /// declared field type and never reads a type name from the blob. Go, Rust, Java,
    /// Kotlin, C#, Swift, C++, C, Dart. **Structurally immune to #233.**
    FixedType,
}

/// The typed slots of one state — its `$.` state-vars and its `(param)` state-args, each
/// `(name, type_text)`, the user's type text verbatim. This is what makes the compartment
/// **typed** rather than erased: a backend emits one variant per state carrying exactly
/// these fields (so serde/Gson/Codable marshal them natively — RFC-0056 full-compartment
/// fidelity), and the schema fingerprint covers them so control-state drift is caught too.
#[derive(Debug, Clone)]
pub struct StateSlots {
    pub name: String,
    pub vars: Vec<(String, String)>,
    pub args: Vec<(String, String)>,
}

/// Everything a backend needs to emit persistence for one system — derived **once** from
/// the symbol table (RFC-0054: one manifest, every backend derives from it).
#[derive(Debug)]
pub struct PersistManifest {
    /// Is this system persistent at all? (`@@[persist(..)]` + `@@[save]` + `@@[load]`.)
    pub enabled: bool,
    /// The system's name — a backend names the typed compartment after it (`<Sys>Comp`).
    pub sys: String,
    /// The save-method name (`@@[save(<name>)]`) — the user's chosen API, e.g. `snapshot`.
    pub save: String,
    /// The load-method name (`@@[load(<name>)]`) — e.g. `restore`.
    pub load: String,
    /// The blob type (`@@[persist(<blob_type>)]`) — the serialized form's type text.
    pub blob: String,
    /// The `domain:` fields that participate, in order. Excludes `@@[no_persist]`.
    /// Each is `(name, type_text)` — the type text is the USER'S and is never parsed.
    pub fields: Vec<(String, String)>,
    /// Every state's typed slots, in declaration order — the **control-state** shape that
    /// `save`/`load` must round-trip in full (the compartment AND the stack), not just the
    /// state name. See `StateSlots`.
    pub states: Vec<StateSlots>,
    /// The user-defined types this program declares — the **closed world** the reflective
    /// reviver may resolve against. Emitted as a lexical registry, never discovered by
    /// enumerating a module or file (which is how Ruby leaked).
    ///
    /// (Populated when the tree carries native type declarations; for now this is the
    /// set of field types that name something the program defines.)
    pub known_types: Vec<String>,
}

impl PersistManifest {
    /// Derive the manifest from a system. **One computation, shared by every backend.**
    pub fn derive(sym: &SystemSym) -> PersistManifest {
        let (save, load, blob) = match &sym.persist {
            Some(p) => (p.save.clone(), p.load.clone(), p.blob.clone()),
            None => (String::new(), String::new(), String::new()),
        };
        let states = sym
            .states
            .iter()
            .map(|st| StateSlots {
                name: st.name.clone(),
                vars: st
                    .state_vars
                    .iter()
                    .map(|v| {
                        let ty = match &v.ty {
                            crate::resolve::TypeRef::Opaque(t) => t.clone(),
                            crate::resolve::TypeRef::System(s)
                            | crate::resolve::TypeRef::WrappedSystem { system: s, .. } => s.clone(),
                            crate::resolve::TypeRef::None => "()".to_string(),
                        };
                        (v.name.clone(), ty)
                    })
                    .collect(),
                args: st
                    .state_params
                    .iter()
                    .map(|p| {
                        let ty = st
                            .state_param_types
                            .get(p)
                            .cloned()
                            .unwrap_or_else(|| "()".to_string());
                        (p.clone(), ty)
                    })
                    .collect(),
            })
            .collect();
        PersistManifest {
            enabled: sym.persist.is_some(),
            sys: sym.name.clone(),
            save,
            load,
            blob,
            states,
            fields: sym
                .domain
                .iter()
                .map(|f| {
                    let ty = match &f.ty {
                        crate::resolve::TypeRef::Opaque(t) => t.clone(),
                        crate::resolve::TypeRef::System(s)
                        | crate::resolve::TypeRef::WrappedSystem { system: s, .. } => s.clone(),
                        crate::resolve::TypeRef::None => "Object".to_string(),
                    };
                    (f.name.clone(), ty)
                })
                .collect(),
            known_types: Vec::new(),
        }
    }

    /// A stable schema string for this manifest — the RFC-0054 fingerprint. A restore
    /// whose snapshot schema does not match refuses (rather than silently mis-restoring
    /// into the wrong shape). Order-sensitive on purpose.
    pub fn schema(&self) -> String {
        let fields = self
            .fields
            .iter()
            .map(|(n, t)| format!("{n}:{t}"))
            .collect::<Vec<_>>()
            .join(",");
        // The control-state shape is part of the fingerprint: a snapshot taken before a
        // state gained a `$.` var (or a state was renamed) must refuse (E751), not silently
        // rebuild a mis-shaped compartment. Each state contributes its var/arg slots.
        let control = self
            .states
            .iter()
            .map(|s| {
                let vars = s
                    .vars
                    .iter()
                    .map(|(n, t)| format!("{n}:{t}"))
                    .collect::<Vec<_>>()
                    .join(";");
                let args = s
                    .args
                    .iter()
                    .map(|(n, t)| format!("{n}:{t}"))
                    .collect::<Vec<_>>()
                    .join(";");
                format!("{}({vars}|{args})", s.name)
            })
            .collect::<Vec<_>>()
            .join(",");
        format!("frame-persist:2|{fields}|{control}")
    }
}
