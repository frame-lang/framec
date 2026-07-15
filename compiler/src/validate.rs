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
use crate::tree::body::{NativePart, RefKind, Stmt};
use crate::tree::{FileAst, Item, MachineMember, Section, StateMember};

/// Check the tree against the symbol table.
pub fn validate(ast: &FileAst, syms: &SymbolTable) -> Vec<Diagnostic> {
    let mut out = Vec::new();

    for item in &ast.items {
        let Item::System(sys) = item else { continue };
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
                                check_refs(&n.parts, &sv, &domain, &iface, &sym.name, &mut out);
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
fn check_refs(
    parts: &[NativePart],
    state_vars: &[&str],
    domain: &[&str],
    iface: &[&str],
    system: &str,
    out: &mut Vec<Diagnostic>,
) {
    for p in parts {
        match p {
            NativePart::Ref(_r) => {
                // (Name-level checks land here once refs carry their parsed name.
                // The SHAPE is what matters today: the ref is a node, in the tree,
                // reachable without reading a byte.)
            }
            NativePart::Literal(l) => {
                for lp in &l.parts {
                    if let crate::tree::body::LiteralPart::Hole(h) = lp {
                        check_refs(&h.parts, state_vars, domain, iface, system, out);
                    }
                    // NOTE: no arm for Content. A Frame reference cannot be there,
                    // because the type has no variant that would put one there.
                }
            }
            NativePart::Text(_) => {}
            // `@@Name(...)` — the system-name/arity checks (§1167) are the deferred
            // closed-world validation layer; the SHAPE is here today.
            NativePart::Instantiate(_) => {}
            // `@@:self.field.method(...)` — the E609 field-is-a-member check is deferred;
            // the SHAPE is here today.
            NativePart::EmbedCall(_) => {}
        }
    }
    let _ = (state_vars, domain, iface, system, RefKind::StateVar);
}
