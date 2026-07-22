//! VALIDATE — pure AST walks. **No text probes. Every diagnostic carries a span.**
//!
//! This pass lives outside `crate::text`, so it *cannot* read source bytes or emitted
//! output even if it wanted to. Compare the old compiler, whose validator hand-rolled
//! its own argument counter that tracked `(` and `)` and was blind to strings, chars,
//! comments and `[]`/`{}` — while codegen used a *different*, string-aware splitter.
//! Two functions, one question, different answers. So framec could **accept** one
//! transition and **emit** a different one:
//!
//! ```frame
//! -> $Active("hello, world", 9)    // E405: "declares 2 params but transition supplies 3"
//! -> $Active("a,b", 9)             // ACCEPTED into $Active(a, b, c) — and `c` was
//!                                  // silently left at its default. Exit 0. Wrong program.
//! ```
//!
//! That cannot recur here, because the validator has no bytes to count.

use crate::resolve::{Diagnostic, Severity, SymbolTable};
use crate::tree::body::{ArgAngles, InstArg, Instantiation, NativePart, ParamGroup, RefKind, Stmt};
use crate::tree::{
    FileAst, Item, MachineMember, Param, ParamAngles, Section, StateMember, SystemItem,
    SystemParams,
};

/// Check the tree against the symbol table.
pub fn validate(ast: &FileAst, syms: &SymbolTable) -> Vec<Diagnostic> {
    let mut out = Vec::new();

    for item in &ast.items {
        let Item::System(sys) = item else { continue };
        // W417 — an ambiguous declaration-site `<`/`>` fork in a header parameter default
        // (RFC-0060). Independent of name resolution: it is a property of the param-list
        // text framec scanned, recorded on the tree at scan, minted here.
        check_system_params(sys, &mut out);
        let Some(sym) = syms.systems.iter().find(|s| s.name == sys.name) else {
            continue;
        };
        let state_names: Vec<&str> = sym.states.iter().map(|s| s.name.as_str()).collect();
        let iface: Vec<&str> = sym.interface.iter().map(|m| m.name.as_str()).collect();
        let domain: Vec<&str> = sym.domain.iter().map(|f| f.name.as_str()).collect();

        // E403 — a cycle in the HSM parent chain (`$A => $B => $A`), which would infinite-loop
        // handler dispatch. Detected by the dogfooded HsmCycle graph-walker system: map each
        // state's parent name to its index (or -1 for a root) and ask the machine.
        let parents: Vec<i32> = sym
            .states
            .iter()
            .map(|s| {
                s.parent
                    .as_ref()
                    .and_then(|p| sym.states.iter().position(|x| &x.name == p))
                    .map(|idx| idx as i32)
                    .unwrap_or(-1)
            })
            .collect();
        if crate::text::scan::hsm_cycle::has_cycle(&parents) {
            out.push(Diagnostic {
                code: "E403",
                severity: Severity::Error,
                span: sym.span,
                message: format!(
                    "system `{}` has a cycle in its HSM parent chain (`$A => $B => $A`), \
                     which would infinite-loop handler dispatch",
                    sym.name
                ),
            });
        }

        for sec in &sys.sections {
            let Section::Machine(m) = sec else { continue };
            for mm in &m.members {
                let MachineMember::State(st) = mm else { continue };
                let sv: Vec<&str> = sym
                    .states
                    .iter()
                    .find(|s| s.name == st.name)
                    .map(|s| s.state_vars.iter().map(|v| v.name.as_str()).collect())
                    .unwrap_or_default();

                for member in &st.members {
                    let StateMember::Handler(h) = member else {
                        continue;
                    };
                    for stmt in &h.body.stmts {
                        match stmt {
                            // E402 — a transition to a state that does not exist.
                            Stmt::Transition(t) | Stmt::StackPush(t) => {
                                if let Some(target) = &t.target {
                                    if !state_names.contains(&target.as_str()) {
                                        out.push(Diagnostic {
                                            code: "E402",
                                            severity: Severity::Error,
                                            span: t.span,
                                            message: format!(
                                                "transition to `${target}`, but system \
                                                 `{}` has no state `${target}`. \
                                                 Known states: {}",
                                                sym.name,
                                                state_names
                                                    .iter()
                                                    .map(|s| format!("${s}"))
                                                    .collect::<Vec<_>>()
                                                    .join(", ")
                                            ),
                                        });
                                    }
                                }
                            }
                            // A native statement carries its Frame refs as NODES. So we
                            // check them by walking the tree — never by scanning text.
                            Stmt::Native(n) => {
                                check_refs(&n.parts, &sv, &domain, &iface, &sym.name, syms, &mut out);
                            }
                            // A frame assignment (`@@:self.x = e`): the LHS is a Frame ref (its
                            // membership is diagnosed here, Δ5/E408) and the RHS is native parts.
                            Stmt::Assign(a) => {
                                check_unknown_context(&a.lhs, &mut out);
                                check_refs(&a.rhs, &sv, &domain, &iface, &sym.name, syms, &mut out);
                            }
                            _ => {}
                        }
                    }
                }
            }
        }

        // W401 — a state that no transition, stack-push, or parent path can reach from the
        // start state (`sym.states[0]`, the state every backend enters) is dead code. Detected
        // by the dogfooded Reachability graph-walker: build the edge list — each transition /
        // stack-push target is an edge, plus a child→parent edge so a live child keeps its
        // parent live (an unhandled event forwards up to it) — and ask which nodes the walk
        // reaches. A warning, not an error: dead states are worth surfacing but do not make the
        // program wrong.
        if !sym.states.is_empty() {
            let idx_of = |name: &str| sym.states.iter().position(|s| s.name == name);
            let mut from: Vec<i32> = Vec::new();
            let mut to: Vec<i32> = Vec::new();
            for (i, s) in sym.states.iter().enumerate() {
                if let Some(pj) = s.parent.as_deref().and_then(idx_of) {
                    from.push(i as i32);
                    to.push(pj as i32);
                }
            }
            for sec in &sys.sections {
                let Section::Machine(m) = sec else { continue };
                for mm in &m.members {
                    let MachineMember::State(st) = mm else { continue };
                    let Some(si) = idx_of(&st.name) else { continue };
                    for member in &st.members {
                        let StateMember::Handler(h) = member else { continue };
                        for stmt in &h.body.stmts {
                            if let Stmt::Transition(t) | Stmt::StackPush(t) = stmt {
                                if let Some(tj) = t.target.as_deref().and_then(idx_of) {
                                    from.push(si as i32);
                                    to.push(tj as i32);
                                }
                            }
                        }
                    }
                }
            }
            let visited =
                crate::text::scan::reachability::reachable(&from, &to, sym.states.len(), 0);
            for (i, s) in sym.states.iter().enumerate() {
                if !visited[i] {
                    out.push(Diagnostic {
                        code: "W401",
                        severity: Severity::Warning,
                        span: s.span,
                        message: format!(
                            "state `${}` is unreachable in system `{}` — no transition, \
                             stack-push, or parent path reaches it from the start state `${}`",
                            s.name, sym.name, sym.states[0].name
                        ),
                    });
                }
            }
        }
    }
    out
}

/// Walk the Frame references inside a native statement — including the ones inside
/// interpolation **holes**, which are code, and *not* the ones inside string
/// **content**, which are the user's data.
///
/// The old compiler could not make that distinction: its scanner said a sigil in a
/// string is not a reference, its expression byte-loop said it is, and **both shipped**
/// (#224). Here there is nothing to decide — a `FrameRef` is a node, and a node inside
/// `LiteralPart::Content` cannot exist.
/// Δ5 (H-1): diagnose a Frame ref whose context word the scanner refused (`RefKind::Unknown`).
/// The scanner recognized the shape and left membership to semantics; this is that semantics —
/// a proper error (E408) carrying the ref's span. A known context is a no-op here.
fn check_unknown_context(r: &crate::tree::body::FrameRef, out: &mut Vec<Diagnostic>) {
    if r.kind == RefKind::Unknown {
        out.push(Diagnostic {
            code: "E408",
            severity: Severity::Error,
            span: r.span,
            message: format!(
                "unknown context reference `@@:{}` — its first segment is not a Frame context \
                 (`self`, `data`, `params`, `return`, `event`, `system`)",
                r.name
            ),
        });
    }
}

fn check_refs(
    parts: &[NativePart],
    state_vars: &[&str],
    domain: &[&str],
    iface: &[&str],
    system: &str,
    syms: &SymbolTable,
    out: &mut Vec<Diagnostic>,
) {
    for p in parts {
        match p {
            NativePart::Ref(r) => {
                // Δ5 (H-1): the validator OWNS membership. The scanner recognizes only the SHAPE
                // and, when the first segment is not a known context, reports `RefKind::Unknown`
                // — a refusal as data, never a guess. Diagnosing non-membership is semantics, so
                // it lives HERE (E408), not in the scanner. Every other kind is a recognized
                // context whose name-level checks stay deferred (the SHAPE is a node in the tree,
                // reachable without reading a byte).
                check_unknown_context(r, out);
            }
            NativePart::Literal(l) => {
                for lp in &l.parts {
                    if let crate::tree::body::LiteralPart::Hole(h) = lp {
                        check_refs(&h.parts, state_vars, domain, iface, system, syms, out);
                    }
                    // NOTE: no arm for Content. A Frame reference cannot be there,
                    // because the type has no variant that would put one there.
                }
            }
            NativePart::Text(_) => {}
            // `@@Name(...)` — the general system-name/arity checks (§1167) remain the
            // deferred closed-world validation layer. What lives here TODAY is the
            // angle-questioned scope only (E407, design record §11.3): when the scanner
            // carried an `Operators`/`Forked` angle reading and the system name resolves,
            // the declared arity adjudicates; neither-admissible / both-admissible is
            // diagnosed, never guessed. `Severity::Error` blocks emission, so emit never
            // sees an unadjudicated fork on the error path.
            NativePart::Instantiate(inst) => {
                if let Some(sys) = syms.systems.iter().find(|s| s.name == inst.name) {
                    match adjudicate(&sys.params, inst) {
                        Adjudication::Primary | Adjudication::Alt => {}
                        verdict => out.push(e407(&sys.params, inst, verdict)),
                    }
                }
            }
            // `@@:self.field.method(...)` — the E609 field-is-a-member check is deferred;
            // the SHAPE is here today.
            NativePart::EmbedCall(_) => {}
        }
    }
    let _ = (state_vars, domain, iface, system, RefKind::StateVar);
}

/// W417 — surface an ambiguous declaration-site `<`/`>` fork (RFC-0060), the declaration-
/// site sibling of the call-site arity error (E407). A declaration has no arity to
/// adjudicate with, so the weaker, semantic-free oracle is **parameter well-formedness**:
/// warn iff BOTH readings are well-formed parameter lists — every segment's name a bare
/// identifier. Exactly-one-well-formed (the common generic `Map<K, V>`, whose operator
/// reading yields the non-identifier segment `V>`) is taken in silence; neither-well-formed
/// is malformed input (the §1167 refusal channel), not W417.
///
/// This reads only the tree — the two readings' already-parsed `Param` names — never a byte
/// and never a type (Oceans Model): the adjudicator is an identifier check, not a type
/// check. Emission is UNCHANGED: `sys.params` already holds the favored G reading, so this
/// only decides whether to warn. Mirrors `adjudicate`/`e407` for the call site exactly —
/// both-admissible is diagnosed, never guessed.
fn check_system_params(sys: &SystemItem, out: &mut Vec<Diagnostic>) {
    let ParamAngles::Forked { alt, span } = &sys.params.angles else {
        return;
    };
    // The emitted (G/template) reading, across all three groups — group order is
    // irrelevant to well-formedness. The alternate (O/operators) reading rides `alt`.
    let primary_iter = || {
        sys.params
            .state
            .iter()
            .chain(&sys.params.enter)
            .chain(&sys.params.domain)
    };
    let primary_wf = primary_iter().all(|p| is_ident(&p.name));
    let alt_wf = alt.iter().all(|p| is_ident(&p.name));
    if !(primary_wf && alt_wf) {
        return; // exactly-one-well-formed → favor-the-template in silence; see doc.
    }
    let n_generic = sys.params.state.len() + sys.params.enter.len() + sys.params.domain.len();
    out.push(Diagnostic {
        code: "W417",
        severity: Severity::Warning,
        span: *span,
        message: format!(
            "ambiguous `<`/`>` in a parameter default of `@@system {name}(...)`\n  \
             read as a generic (framec favors the template when it cannot tell):\n    \
             as generic brackets ({n_generic} param{gs}): {generic}\n    \
             as comparison operators ({n_alt} params): {operators}\n  \
             framec cannot resolve this without a type system, so it guessed.\n  \
             help: parenthesize the comparison to disambiguate — e.g. wrap `(a < b)`",
            name = sys.name,
            gs = if n_generic == 1 { "" } else { "s" },
            generic = render_params(primary_iter()),
            n_alt = alt.len(),
            operators = render_params(alt.iter()),
        ),
    });
}

/// Is `s` a single bare identifier? The declaration-site well-formedness oracle: a
/// segment's NAME (its bytes before the first top-level `:`/`=`, already extracted by
/// `parse_one_param`) must be an identifier for the segment to be a valid parameter. No
/// type knowledge — identifier SHAPE only (type-ignorance preserved).
fn is_ident(s: &str) -> bool {
    let b = s.as_bytes();
    !b.is_empty()
        && (b[0].is_ascii_alphabetic() || b[0] == b'_')
        && b[1..].iter().all(|&c| c.is_ascii_alphanumeric() || c == b'_')
}

/// Render a reading's params as `` `name[: type][ = default]` `` each, joined ` · ` — the
/// E407 render style, reused for the W417 both-readings message.
fn render_params<'a>(params: impl Iterator<Item = &'a Param>) -> String {
    let rendered: Vec<String> = params
        .map(|p| {
            let mut s = p.name.clone();
            if let Some(t) = &p.ty {
                s.push_str(": ");
                s.push_str(t);
            }
            if let Some(d) = &p.default {
                s.push_str(" = ");
                s.push_str(d);
            }
            format!("`{s}`")
        })
        .collect();
    if rendered.is_empty() {
        "(none)".to_string()
    } else {
        rendered.join(" · ")
    }
}

/// The outcome of holding an instantiation's angle reading(s) against the declared params.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Adjudication {
    /// The primary candidate (`inst.args` — G when forked) proceeds.
    Primary,
    /// The alternate (O) candidate proceeds (`ArgAngles::Forked` payload).
    Alt,
    /// Neither reading matches the declared params — E407.
    NoneAdmissible,
    /// Both readings match (reachable only through defaults) — E407, diagnose never guess.
    BothAdmissible,
}

/// Adjudicate an instantiation's angle fork against the declared `SystemParams` — the ONE
/// shared function (design record §11.3), consumed by the `validate` walk (diagnostics)
/// and by `text::emit::driver::lower_instantiation` (candidate choice). Reads only the
/// tree and the declared params — no bytes. Lives HERE because this pass's charter is
/// exactly "arity questions answered on the tree, never by counting text" (the E405
/// story, module header).
///
/// `Inert` → `Primary` unchecked (nothing to adjudicate — general arity validation stays
/// §1167-deferred). `Operators` → the sole O reading either matches or nothing does.
/// `Forked` → exactly-one admissible picks; the tie (`BothAdmissible`, reachable only via
/// defaults) and the miss (`NoneAdmissible`) are diagnosed by the caller.
pub fn adjudicate(params: &SystemParams, inst: &Instantiation) -> Adjudication {
    match &inst.angles {
        ArgAngles::Inert => Adjudication::Primary,
        ArgAngles::Operators => {
            if admissible(params, &inst.args) {
                Adjudication::Primary
            } else {
                Adjudication::NoneAdmissible
            }
        }
        ArgAngles::Forked { alt_args, .. } => {
            match (admissible(params, &inst.args), admissible(params, alt_args)) {
                (true, false) => Adjudication::Primary,
                (false, true) => Adjudication::Alt,
                (false, false) => Adjudication::NoneAdmissible,
                (true, true) => Adjudication::BothAdmissible,
            }
        }
    }
}

/// Is a candidate arg list admissible against the declared params? Mirrors
/// `resolve_group`'s fill rules (text/emit/driver.rs) made DISCRIMINATING — adjudication
/// needs discrimination; filling stays permissive. Per group (State/Enter/Domain):
/// *named form* — every arg named, no duplicate names, every name declared, and every
/// declared param not provided by name has a default (the gate amendment: the
/// `unwrap_or_default()` empty slot is the inadmissible outcome); *positional form* —
/// provided count ≤ declared count and every declared param past the provided count has a
/// default; *mixed* named/positional — inadmissible (spec §1108).
fn admissible(params: &SystemParams, args: &[InstArg]) -> bool {
    group_admissible(&params.state, args, ParamGroup::State)
        && group_admissible(&params.enter, args, ParamGroup::Enter)
        && group_admissible(&params.domain, args, ParamGroup::Domain)
}

fn group_admissible(decls: &[Param], args: &[InstArg], group: ParamGroup) -> bool {
    let provided: Vec<&InstArg> = args.iter().filter(|a| a.group == group).collect();
    let named_ct = provided.iter().filter(|a| a.name.is_some()).count();
    if named_ct > 0 && named_ct < provided.len() {
        return false; // mixed named/positional (spec §1108)
    }
    if named_ct > 0 {
        // Named form: no duplicates, every name declared, unprovided declared params
        // must have defaults.
        for (idx, a) in provided.iter().enumerate() {
            let n = a.name.as_deref().unwrap_or_default();
            if provided[..idx].iter().any(|b| b.name.as_deref() == Some(n)) {
                return false;
            }
            if !decls.iter().any(|d| d.name == n) {
                return false;
            }
        }
        decls.iter().all(|d| {
            provided
                .iter()
                .any(|a| a.name.as_deref() == Some(d.name.as_str()))
                || d.default.is_some()
        })
    } else {
        // Positional form: count fits, all-defaulted tail.
        provided.len() <= decls.len() && decls[provided.len()..].iter().all(|d| d.default.is_some())
    }
}

/// Render one candidate's values for the E407 message, G-first ordering decided by the
/// caller.
fn render_args(args: &[InstArg]) -> String {
    if args.is_empty() {
        return "(none)".to_string();
    }
    args.iter()
        .map(|a| match &a.name {
            Some(n) => format!("`{}={}`", n, a.value),
            None => format!("`{}`", a.value),
        })
        .collect::<Vec<_>>()
        .join(" · ")
}

/// E407 (code provisional pending the diagnostic registry): an angle-questioned argument
/// list the declared arity cannot settle. Both interpretations are always rendered, G
/// first (owner item 6); the parenthesization escape is STRUCTURAL — bytes inside `()`
/// sit at depth ≥ 1 where the angle counter never counts, so parenthesizing removes the
/// byte from BOTH hypotheses' question entirely.
fn e407(params: &SystemParams, inst: &Instantiation, verdict: Adjudication) -> Diagnostic {
    let declared = format!(
        "`{}` declares state {} + enter {} + domain {}",
        inst.name,
        params.state.len(),
        params.enter.len(),
        params.domain.len()
    );
    let help = "help: parenthesize to fix the reading: wrap a comparison — `(a < b)` — or \
                the whole generic expression — `(new HashMap<K, V>())` — in parentheses";
    let message = match (&inst.angles, verdict) {
        (ArgAngles::Forked { alt_args, .. }, Adjudication::BothAdmissible) => format!(
            "ambiguous argument list for `@@{}(...)`: `<`/`>` reads two ways\n  \
             as generic brackets ({} args): {}\n  \
             as comparison/shift operators ({} args): {}\n  \
             {} — both readings match (defaults make both counts legal); parenthesize to choose\n  {}",
            inst.name,
            inst.args.len(),
            render_args(&inst.args),
            alt_args.len(),
            render_args(alt_args),
            declared,
            help
        ),
        (ArgAngles::Forked { alt_args, .. }, _) => format!(
            "ambiguous argument list for `@@{}(...)`: `<`/`>` reads two ways\n  \
             as generic brackets ({} args): {}\n  \
             as comparison/shift operators ({} args): {}\n  \
             {} — neither reading matches\n  {}",
            inst.name,
            inst.args.len(),
            render_args(&inst.args),
            alt_args.len(),
            render_args(alt_args),
            declared,
            help
        ),
        _ => format!(
            "argument list for `@@{}(...)`: angle brackets do not balance as a bracket \
             pair, so they were read as operators ({} args): {}\n  {} — the reading does \
             not match\n  {}",
            inst.name,
            inst.args.len(),
            render_args(&inst.args),
            declared,
            help
        ),
    };
    Diagnostic {
        code: "E407",
        severity: Severity::Error,
        span: inst.span,
        message,
    }
}
