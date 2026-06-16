//! Unicode general-category / script class resolution for `@@fsm` regex
//! `\p{Name}` / `\P{Name}` (RFC-0042 §6.7, §11.6).
//!
//! These classes are *regular* — each is a finite set of codepoint ranges —
//! but the table data is large, so the v0.1 dialect excluded them (§6.5) and
//! the hand-written engine carries no Unicode tables of its own. v0.2 admits
//! them on the **`char` alphabet**, behind the `@@[allow(unicode_classes)]`
//! opt-in (enforced by the validator, not here). The range *data* is sourced
//! from `regex-syntax` — already in the build graph via `regex` — used purely
//! as a compile-time lookup; the recognition engine stays hand-written.
//!
//! [`resolve`] runs after parsing and before restrictions/Thompson, rewriting
//! every `ClassMember::Unicode` into concrete `ClassMember::Range`s so no
//! unresolved Unicode member ever reaches the DFA construction.

use super::ast::{CharClass, ClassMember, RegexAst, RegexNode, ShorthandKind, SpannedNode};
use super::{Alphabet, EngineDiagnostic};

/// Unicode codepoint ranges for a Perl shorthand class (`\d`/`\w`/`\s`) on the
/// `char` alphabet (RFC-0042 §6.7), sourced from `regex-syntax`. The `bytes`
/// alphabet keeps ASCII definitions (`super::thompson::shorthand_ranges`); the
/// three fixed patterns always parse, so an empty result is unreachable.
pub fn perl_ranges(kind: ShorthandKind) -> Vec<(u32, u32)> {
    use regex_syntax::hir::{Class, HirKind};
    let pat = match kind {
        ShorthandKind::Digit => "\\d",
        ShorthandKind::Word => "\\w",
        ShorthandKind::Whitespace => "\\s",
    };
    match regex_syntax::parse(pat).map(|h| h.into_kind()) {
        Ok(HirKind::Class(Class::Unicode(c))) => c
            .iter()
            .map(|r| (r.start() as u32, r.end() as u32))
            .collect(),
        _ => Vec::new(),
    }
}

/// Resolve every `\p{}`/`\P{}` member in `ast` into codepoint ranges, in
/// place. Returns `true` if any Unicode class was resolved (so the caller can
/// enforce the opt-in), or an `E722` diagnostic if a class is used on a
/// non-`char` alphabet or names an unknown category/script.
pub fn resolve(ast: &mut RegexAst, alphabet: Alphabet) -> Result<bool, EngineDiagnostic> {
    let mut used = false;
    resolve_node(&mut ast.root, alphabet, &mut used)?;
    Ok(used)
}

fn resolve_node(
    node: &mut SpannedNode,
    alphabet: Alphabet,
    used: &mut bool,
) -> Result<(), EngineDiagnostic> {
    let span = node.span;
    match &mut node.node {
        RegexNode::Class(class) => resolve_class(class, alphabet, span, used),
        RegexNode::Concat(items) | RegexNode::Alt(items) => {
            for it in items {
                resolve_node(it, alphabet, used)?;
            }
            Ok(())
        }
        RegexNode::Quantifier { inner, .. } | RegexNode::Group(inner) => {
            resolve_node(inner, alphabet, used)
        }
        _ => Ok(()),
    }
}

fn resolve_class(
    class: &mut CharClass,
    alphabet: Alphabet,
    span: super::Span,
    used: &mut bool,
) -> Result<(), EngineDiagnostic> {
    if !class
        .members
        .iter()
        .any(|m| matches!(m, ClassMember::Unicode { .. }))
    {
        return Ok(());
    }
    if alphabet != Alphabet::Char {
        return Err(EngineDiagnostic {
            code: "E722",
            span,
            message: "Unicode classes `\\p{...}` require the `char` alphabet".to_string(),
        });
    }
    let mut resolved: Vec<ClassMember> = Vec::with_capacity(class.members.len());
    for m in std::mem::take(&mut class.members) {
        match m {
            ClassMember::Unicode { name, negated } => {
                *used = true;
                let ranges = class_ranges(&name, negated).map_err(|message| EngineDiagnostic {
                    code: "E722",
                    span,
                    message,
                })?;
                for (lo, hi) in ranges {
                    resolved.push(ClassMember::Range { low: lo, high: hi });
                }
            }
            other => resolved.push(other),
        }
    }
    class.members = resolved;
    Ok(())
}

/// The codepoint ranges of a Unicode class `name` (complemented for `\P`),
/// from `regex-syntax`'s tables. `Err` names an unknown category/script.
fn class_ranges(name: &str, negated: bool) -> Result<Vec<(u32, u32)>, String> {
    use regex_syntax::hir::{Class, HirKind};
    let pat = format!("\\{}{{{}}}", if negated { 'P' } else { 'p' }, name);
    let hir = regex_syntax::parse(&pat).map_err(|_| {
        format!("unknown Unicode class `\\p{{{name}}}` (not a general category or script name)")
    })?;
    match hir.into_kind() {
        HirKind::Class(Class::Unicode(c)) => Ok(c
            .iter()
            .map(|r| (r.start() as u32, r.end() as u32))
            .collect()),
        _ => Err(format!(
            "`\\p{{{name}}}` did not resolve to a Unicode class"
        )),
    }
}
