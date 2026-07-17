//! RESOLVE — the symbol table. **A pure walk over the tree. It cannot read a byte.**
//!
//! This module lives outside `crate::text`, so `Source::open` and `NativeText::finish`
//! are private to it. It could not grep source or emitted output if it wanted to.
//!
//! That is not a restriction we are working around; it is the design working. Every
//! fact this pass needs is already **on a node**, because the scanner put it there —
//! and the scanner put it there precisely because RESOLVE could not have fetched it
//! itself.
//!
//! > **RULE 1.** A pass may interrogate a node about facts *framec* put there. It may
//! > never interrogate a node about facts the *user* put there.
//!
//! A declaration's **name** is framec's (it is Frame's own vocabulary). A declaration's
//! **type** is the user's — carried verbatim, never parsed.
//!
//! # Types, and why we do not parse them
//!
//! framec has no type system. `Type` is the user's target-language text, and verbatim
//! type passthrough is a standing architectural boundary. So "resolve a field's type to
//! a system" means deciding whether one of these names a Frame system:
//!
//! ```text
//! kid: Child                kid: Optional[Child]        kid: Child?
//! kid: Rc<RefCell<Child>>   kid: std::shared_ptr<Child> kid: *Child
//! ```
//!
//! Exact-name lookup gets the first and misses the rest. Getting the rest means parsing
//! **sixteen type grammars** — which is the "never parse the user's code" rule broken
//! one level up, and the camel's nose.
//!
//! # The corpus corrected us, and the correction is better than the plan
//!
//! The plan was exact-name resolution on the *type text*, plus a diagnostic for wrapped
//! types. Then the corpus failed:
//!
//! ```frame
//! inner: Inner* = @@Inner()      // fixtures/c/16_marshal_embed.frm — works today
//! ```
//!
//! `Inner*` is not a wrapper a C programmer *chose*. It is **C's mandatory spelling**
//! for a system instance — C has no references, and `create` returns a pointer.
//! Telling that user to "just write `Inner`" is telling them to write something that is
//! not C.
//!
//! But look at the initializer: **`@@Inner()`**. That is *Frame's own syntax*. framec
//! already knows the field holds a system. **It never needed to read the type at all.**
//!
//! That is RULE 1, and we had walked straight past it — reading the *user's* text to
//! recover a fact *framec's own* text already stated.
//!
//! **The rule, in priority order:**
//!
//! 1. `= @@Sys(...)` — Frame's syntax. **Authoritative.** Zero type parsing.
//! 2. The type text is *exactly* a system name (`kid: Child`) — a convenience, still no
//!    parsing.
//! 3. The type *mentions* a system inside something else, and there is **no** `@@`
//!    initializer to settle it — a **diagnostic** (E640). framec suspects, cannot know,
//!    and says so rather than guessing.
//! 4. Otherwise: opaque. The user's type. framec knows nothing about it and needs to
//!    know nothing — the target toolchain does all the type work.

use crate::tree::{Decl, FileAst, Item, MachineMember, Section, StateMember, SystemParams};
use crate::Span;

#[derive(Debug)]
pub struct SymbolTable {
    pub systems: Vec<SystemSym>,
}

/// The three-attribute persistence contract: `@@[persist(<blob_type>)]` +
/// `@@[save(<name>)]` + `@@[load(<name>)]`. framec generates a save method (returns the blob)
/// and a load method (an instance method taking the blob), **named by the user** — so a
/// persisted system chooses its own API, and a system with a user method named `save` is not
/// clobbered. Bare `@@[persist]` (no names) is rejected with E814.
#[derive(Debug, Clone)]
pub struct Persist {
    /// The serialized-form type from `@@[persist(<blob_type>)]` (e.g. `str`, `String`).
    pub blob: String,
    /// The save-method name from `@@[save(<name>)]`.
    pub save: String,
    /// The load-method name from `@@[load(<name>)]`.
    pub load: String,
}

#[derive(Debug)]
pub struct SystemSym {
    pub name: String,
    pub span: Span,
    /// `@@system private Name` — reduced class visibility on targets that have it (Java).
    pub private: bool,
    /// Does this system's value get embedded in a persisted snapshot — either it is itself
    /// `@@[persist]`, or it is transitively a domain-field sub-system of one? On typed backends
    /// (Rust: serde) that decides whether the system STRUCT and its compartment must derive the
    /// serializer traits: a sub-system embedded in a parent's snapshot must be serializable/
    /// cloneable, while an ordinary system must NOT force the crate to depend on serde. Computed
    /// once, after all systems are known. See the reachability pass in `resolve`.
    pub persist_reachable: bool,
    /// `@@[async]` — the system's interface is asynchronous.
    pub is_async: bool,
    /// The persistence contract, if any (`@@[persist(..)]` + `@@[save]` + `@@[load]`).
    pub persist: Option<Persist>,
    /// `@@[scan(<elem>)]` — a positioned, borrowed-input scanner (RFC-0042.1 / #209). The
    /// element type (`u8`, `char`) the generated `SInput` trait yields. `None` for an
    /// ordinary system (which emits exactly as before — the no-op #209 requires).
    pub scan: Option<String>,
    /// Events the system accepts.
    pub interface: Vec<MethodSym>,
    pub states: Vec<StateSym>,
    pub domain: Vec<FieldSym>,
    pub actions: Vec<MethodSym>,
    /// Header params — `@@system Name($(s), $>(e), domain)`. Domain params are ctor args;
    /// state/enter params seed the start compartment (spec §203).
    pub params: SystemParams,
}

#[derive(Debug)]
pub struct MethodSym {
    pub name: String,
    pub span: Span,
    /// `async fetch(...)` — a MODIFIER, not part of the name.
    pub is_async: bool,
    /// Verbatim. Never parsed.
    pub params_text: Option<String>,
    pub return_text: Option<String>,
}

#[derive(Debug)]
pub struct StateSym {
    pub name: String,
    pub span: Span,
    /// The state's declared parameter NAMES. `$B(n: int)` -> ["n"].
    pub state_params: Vec<String>,
    /// Their declared TYPES, verbatim. `$B(n: int)` -> {"n": "int"}.
    /// The type text is the user's and is never parsed.
    pub state_param_types: std::collections::HashMap<String, String>,
    /// The parent state. `$Awake => $Live`.
    pub parent: Option<String>,
    pub handlers: Vec<HandlerSym>,
    pub state_vars: Vec<FieldSym>,
}

#[derive(Debug)]
pub struct HandlerSym {
    pub event: String,
    pub span: Span,
    pub params_text: String,
    pub return_text: Option<String>,
}

#[derive(Debug)]
pub struct FieldSym {
    pub name: String,
    pub span: Span,
    pub ty: TypeRef,
    /// The initializer expression, verbatim (the user's native code).
    pub init_text: Option<String>,
    /// If the init is `= @@Sys(...)`, the system name — Frame's own syntax, lowered to
    /// the target's constructor.
    pub init_system: Option<String>,
}

/// What a field's declared type refers to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TypeRef {
    /// The type text is **exactly** the name of a system in this file.
    System(String),
    /// The user's type, verbatim. framec knows nothing about it and needs to know
    /// nothing about it — the target toolchain does all the type work.
    Opaque(String),
    /// The type text **mentions** a known system, but is not exactly it — it is inside
    /// a wrapper framec does not parse and must not parse.
    ///
    /// This is a **diagnostic**, not a resolution. framec says what it sees and what to
    /// do about it, rather than guessing (which would be wrong on five spellings out of
    /// six) or silently treating it as opaque (which would break cross-file persist).
    WrappedSystem { text: String, system: String },
    /// No annotation.
    None,
}

/// Severity of a diagnostic. An `Error` blocks emission; a `Warning` is reported but the
/// compile still succeeds. Unreachable states, dead handlers, and the like are warnings —
/// they are worth telling the author about, but they do not make the program wrong.
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum Severity {
    Error,
    Warning,
}

/// A diagnostic. Every one carries a span, always.
#[derive(Debug, PartialEq, Eq)]
pub struct Diagnostic {
    pub code: &'static str,
    pub severity: Severity,
    pub span: Span,
    pub message: String,
}

/// Build the symbol table. **Pure.** Tree in, table out; no bytes, no text probes.
pub fn resolve(ast: &FileAst) -> (SymbolTable, Vec<Diagnostic>) {
    let mut systems = Vec::new();
    let mut diags = Vec::new();

    // Pass 1 — collect the system names, so a field can be resolved against a system
    // declared LATER in the file. (A single pass would make resolution depend on
    // declaration order, which is a footgun nobody expects from a compiler.)
    let known: Vec<String> = ast
        .items
        .iter()
        .filter_map(|i| match i {
            Item::System(s) => Some(s.name.clone()),
            _ => None,
        })
        .collect();

    // Pass 2 — build each system.
    //
    // An attribute applies to the NEXT system. Track the pending ones as we walk, rather
    // than searching backwards from the system (which would mean re-deriving position
    // from spans).
    let mut pending: Vec<String> = Vec::new();
    for item in &ast.items {
        if let Item::Pragma(p) = item {
            if let Some(a) = &p.attr {
                pending.push(a.clone());
            }
            continue;
        }
        let Item::System(sys) = item else {
            // Water between an attribute and its system does not carry it.
            if matches!(item, Item::Native(_)) && !pending.is_empty() {
                // blank lines are fine; real code is not. Keep it simple: keep pending.
            }
            continue;
        };
        let attrs = std::mem::take(&mut pending);

        // E730 — `@@system public Name` is redundant: systems are public by default. Diagnosed
        // target-agnostically (it is redundant everywhere), unlike `private` on a target without
        // class visibility (E731, which needs the target and lives in `target_diagnostics`).
        if sys.public_keyword {
            diags.push(Diagnostic {
                code: "E730",
                severity: Severity::Error,
                span: sys.span,
                message: format!(
                    "system `{}` declares redundant `public` — systems are public by default; \
                     drop the keyword (use `private` only to reduce visibility).",
                    sys.name
                ),
            });
        }

        // The three-attribute persistence contract. `@@[persist(<type>)]` names the blob type;
        // `@@[save(<name>)]` / `@@[load(<name>)]` name the generated methods. Bare `@@[persist]`
        // — or a persist attribute missing either method name — is E814: framec will not invent
        // an API for a persisted system, because the method names are the user's to choose.
        let arg = |key: &str| -> Option<String> {
            let paren = format!("{key}(");
            attrs
                .iter()
                .find_map(|a| a.strip_prefix(&paren).map(|r| r.trim_end_matches(')').trim().to_string()))
        };
        let persist_marked = attrs.iter().any(|a| a == "persist" || a.starts_with("persist("));
        let persist = if persist_marked {
            match (arg("persist"), arg("save"), arg("load")) {
                (Some(blob), Some(save), Some(load))
                    if !blob.is_empty() && !save.is_empty() && !load.is_empty() =>
                {
                    Some(Persist { blob, save, load })
                }
                _ => {
                    diags.push(Diagnostic {
                        code: "E814",
                        severity: Severity::Error,
                        span: sys.span,
                        message: format!(
                            "system `{}` is persistent but does not declare the full contract. \
                             A persisted system MUST declare `@@[persist(<blob_type>)]`, \
                             `@@[save(<name>)]`, and `@@[load(<name>)]` — bare `@@[persist]` is \
                             rejected because framec will not choose the save/load method names \
                             for you.",
                            sys.name
                        ),
                    });
                    None
                }
            }
        } else {
            None
        };

        let mut sym = SystemSym {
            name: sys.name.clone(),
            span: sys.span,
            private: sys.private,
            persist_reachable: false, // filled by the reachability pass below
            is_async: attrs.iter().any(|a| a == "async"),
            persist,
            // `@@[scan(u8)]` — the positioned-scanner element type (RFC-0042.1 / #209).
            // The system becomes a borrowed-input, cursor-driven scanner; the arg is the
            // element type the `SInput` trait reads (`u8`, `char`, …).
            scan: attrs.iter().find_map(|a| {
                a.strip_prefix("scan(")
                    .map(|r| r.trim_end_matches(')').trim().to_string())
            }),
            interface: Vec::new(),
            states: Vec::new(),
            domain: Vec::new(),
            actions: Vec::new(),
            params: sys.params.clone(),
        };

        for sec in &sys.sections {
            match sec {
                Section::Interface(d) => {
                    for m in &d.members {
                        if let Decl::Member(md) = m {
                            sym.interface.push(MethodSym {
                                name: md.name.clone(),
                                span: md.span,
                                is_async: md.is_async,
                                params_text: md.params_text.clone(),
                                return_text: md.type_text.clone(),
                            });
                        }
                    }
                }
                Section::Domain(d) => {
                    for m in &d.members {
                        if let Decl::Member(md) = m {
                            sym.domain.push(FieldSym {
                                name: md.name.clone(),
                                span: md.span,
                                init_text: md.init_text.clone(),
                                init_system: md.init_system.clone(),
                                ty: classify(
                                    md.type_text.as_deref(),
                                    md.init_system.as_deref(),
                                    &known,
                                    md.span,
                                    &mut diags,
                                ),
                            });
                        }
                    }
                }
                Section::Actions(d) | Section::Operations(d) => {
                    for m in &d.members {
                        match m {
                            Decl::Member(md) => sym.actions.push(MethodSym {
                                name: md.name.clone(),
                                span: md.span,
                                is_async: md.is_async,
                                params_text: md.params_text.clone(),
                                return_text: md.type_text.clone(),
                            }),
                            Decl::WithBody(b) => sym.actions.push(MethodSym {
                                name: b.name.clone(),
                                span: b.span,
                                is_async: false,
                                params_text: Some(b.params_text.clone()),
                                return_text: b.return_text.clone(),
                            }),
                            Decl::Trivia(_) => {}
                        }
                    }
                }
                Section::Machine(m) => {
                    for mm in &m.members {
                        let MachineMember::State(st) = mm else {
                            continue;
                        };
                        let mut ss = StateSym {
                            name: st.name.clone(),
                            span: st.span,
                            state_params: st.params.clone(),
                            state_param_types: st.param_types.clone(),
                            parent: st.parent.clone(),
                            handlers: Vec::new(),
                            state_vars: Vec::new(),
                        };
                        for member in &st.members {
                            match member {
                                StateMember::Handler(h) => ss.handlers.push(HandlerSym {
                                    event: h.event.clone(),
                                    span: h.span,
                                    params_text: h.params_text.clone(),
                                    return_text: h.return_text.clone(),
                                }),
                                StateMember::StateVar(v) => ss.state_vars.push(FieldSym {
                                    name: v.name.clone(),
                                    span: v.span,
                                    init_text: v.init_text.clone(),
                                    init_system: v.init_system.clone(),
                                    ty: classify(
                                        v.type_text.as_deref(),
                                        v.init_system.as_deref(),
                                        &known,
                                        v.span,
                                        &mut diags,
                                    ),
                                }),
                                StateMember::Trivia(_) => {}
                            }
                        }
                        sym.states.push(ss);
                    }
                }
                _ => {}
            }
        }

        // G1 — DERIVE the public interface from handled events. In Frame an event handled in
        // the machine IS callable: a system may omit the `interface:` block entirely, or
        // handle events beyond the ones it declares, and every such event still needs a
        // public router (`s.e()`) — external callers, self-calls and forwards all reach a
        // handler through it. So the interface is the DECLARED methods PLUS one derived method
        // per handled event not already declared, with the signature read off the handler.
        // Lifecycle events (`$>` enter / `<$` exit) are internal transition hooks, never
        // public, so they are excluded. Dedup by event name (an event handled in several
        // states is ONE public method); first handler wins the signature.
        {
            let mut seen: std::collections::HashSet<String> =
                sym.interface.iter().map(|m| m.name.clone()).collect();
            for st in &sym.states {
                for h in &st.handlers {
                    if h.event == "$>" || h.event == "<$" {
                        continue;
                    }
                    if seen.insert(h.event.clone()) {
                        sym.interface.push(MethodSym {
                            name: h.event.clone(),
                            span: h.span,
                            is_async: false,
                            params_text: Some(h.params_text.clone()),
                            return_text: h.return_text.clone(),
                        });
                    }
                }
            }
        }

        systems.push(sym);
    }

    // Persist-reachability: seed with the `@@[persist]` systems, then close over domain-field
    // edges — if a reachable system holds `field: Sub` (or `= @@Sub()`), `Sub`'s value is
    // embedded in that snapshot and `Sub` is reachable too. A typed backend (Rust/serde) derives
    // the serializer on exactly this set; every member's fields are serde by the persist contract
    // (a persisted system's domain must be serializable, transitively), so deriving is always
    // sound here and never forced on an unrelated system.
    let field_system = |f: &FieldSym| -> Option<String> {
        match (&f.init_system, &f.ty) {
            (Some(s), _) => Some(s.clone()),
            (None, TypeRef::System(s))
            | (None, TypeRef::WrappedSystem { system: s, .. }) => Some(s.clone()),
            _ => None,
        }
    };
    let mut reachable: std::collections::HashSet<String> = systems
        .iter()
        .filter(|s| s.persist.is_some())
        .map(|s| s.name.clone())
        .collect();
    loop {
        let mut added = false;
        for s in &systems {
            if !reachable.contains(&s.name) {
                continue;
            }
            for f in &s.domain {
                if let Some(sub) = field_system(f) {
                    if reachable.insert(sub) {
                        added = true;
                    }
                }
            }
        }
        if !added {
            break;
        }
    }
    for s in &mut systems {
        s.persist_reachable = reachable.contains(&s.name);
    }

    (SymbolTable { systems }, diags)
}

/// Classify a declared type. **The type text is never parsed.**
fn classify(
    text: Option<&str>,
    init_system: Option<&str>,
    known: &[String],
    span: Span,
    diags: &mut Vec<Diagnostic>,
) -> TypeRef {
    // (1) THE INITIALIZER SETTLES IT. `= @@Inner()` is Frame's own syntax — a fact
    // framec put there — and it is authoritative regardless of how the user had to
    // spell the type. `Inner*`, `Rc<RefCell<Inner>>`, `shared_ptr<Inner>`: all of them
    // are just the target's way of saying "an instance of Inner", and framec does not
    // need to know which. It asks its OWN syntax, not the user's.
    if let Some(sys) = init_system {
        if known.iter().any(|k| k == sys) {
            return TypeRef::System(sys.to_string());
        }
        // `= @@Nope()` naming a system that does not exist is a real error — but it is
        // an *instantiation* error, not a type error, and it belongs to VALIDATE.
    }

    let Some(t) = text.map(str::trim) else {
        return TypeRef::None;
    };
    if t.is_empty() {
        return TypeRef::None;
    }

    // (2) Exact name -> a system. A convenience; still no parsing.
    if known.iter().any(|k| k == t) {
        return TypeRef::System(t.to_string());
    }

    // Does the text MENTION a system, as a whole word, inside something else?
    //
    // Note carefully what this is and is not. It is NOT parsing the type: we are not
    // deciding that `Rc<RefCell<Child>>` is an `Rc` of a `RefCell` of a `Child`. We are
    // asking a much weaker, purely lexical question — "does the identifier `Child`
    // appear in here?" — in order to produce a *diagnostic*, never a resolution.
    //
    // The distinction matters. Resolving it would require knowing sixteen wrapper
    // grammars. Reporting it requires knowing none.
    // (3) The type MENTIONS a system, and no `@@` initializer settled the question.
    // framec suspects; it cannot know; it says so. It does not guess — guessing would
    // be wrong on five spellings out of six — and it does not silently shrug, which
    // would break cross-file persist.
    //
    // Note what this check is NOT. It is not parsing the type: we never decide that
    // `Rc<RefCell<Child>>` is an Rc of a RefCell of a Child. We ask the much weaker,
    // purely lexical question "does the identifier `Child` occur here as a word?" — and
    // only to produce a diagnostic, never a resolution. Resolving it would need sixteen
    // wrapper grammars. Reporting it needs none.
    for k in known {
        if mentions_word(t, k) {
            diags.push(Diagnostic {
                code: "E640",
                severity: Severity::Error,
                span,
                message: format!(
                    "the type `{t}` mentions the system `{k}`, but nothing tells framec \
                     that this field HOLDS a `{k}`.\n\
                     framec has no type system — a type is your target's text and passes \
                     through verbatim — so it will not read inside `{t}` to find out.\n\
                     Initialize it with Frame's own syntax and framec will know: \
                     `= @@{k}(...)`."
                ),
            });
            return TypeRef::WrappedSystem {
                text: t.to_string(),
                system: k.clone(),
            };
        }
    }

    // The user's type. framec knows nothing about it, and needs to know nothing.
    TypeRef::Opaque(t.to_string())
}

/// Whole-word containment. `Child` is mentioned in `Rc<RefCell<Child>>` but NOT in
/// `ChildProcess` or `GrandChild`.
fn mentions_word(hay: &str, word: &str) -> bool {
    let h = hay.as_bytes();
    let w = word.as_bytes();
    if w.is_empty() || h.len() < w.len() {
        return false;
    }
    for i in 0..=h.len() - w.len() {
        if &h[i..i + w.len()] != w {
            continue;
        }
        let before_ok = i == 0 || !(h[i - 1].is_ascii_alphanumeric() || h[i - 1] == b'_');
        let j = i + w.len();
        let after_ok = j == h.len() || !(h[j].is_ascii_alphanumeric() || h[j] == b'_');
        if before_ok && after_ok {
            return true;
        }
    }
    false
}


impl SystemSym {
    /// **Which state actually handles `event` when the machine is in `state`?**
    ///
    /// The nearest ancestor — including `state` itself — that declares a handler for it.
    /// `None` means nobody does, and the event is a no-op.
    ///
    /// This is the whole of hierarchical dispatch, resolved at COMPILE TIME from the
    /// symbol table. No runtime parent-chain walk, and no text anywhere.
    pub fn resolve_handler(&self, state: &str, event: &str) -> Option<&StateSym> {
        let mut cur = self.states.iter().find(|s| s.name == state)?;
        let mut guard = 0;
        loop {
            if cur.handlers.iter().any(|h| h.event == event) {
                return Some(cur);
            }
            let p = cur.parent.as_ref()?;
            cur = self.states.iter().find(|s| &s.name == p)?;
            // A cycle in the parent chain would hang the compiler. Refuse rather than spin.
            guard += 1;
            if guard > self.states.len() {
                return None;
            }
        }
    }

    /// The state that handles `event` for `state`'s PARENT — i.e. where `=> $^` goes.
    pub fn resolve_forward(&self, state: &str, event: &str) -> Option<&StateSym> {
        let s = self.states.iter().find(|s| s.name == state)?;
        let parent = s.parent.as_ref()?;
        self.resolve_handler(parent, event)
    }
}
