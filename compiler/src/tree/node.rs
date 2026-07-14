//! **RECURSIVE TOTALITY** — one checker, and no node can opt out of it.
//!
//! # The invariant
//!
//! > For **every** node in the tree: the node's children are in source order, and
//! > their spans **partition the node's span exactly** — no gaps, no overlaps.
//!
//! Applied at the root, this says every byte of the file is in the tree. Applied at a
//! handler body, it says every byte of the body is in the tree. It is the *same*
//! property, and that is the point: it cannot be true at the top and quietly false
//! three levels down.
//!
//! # Why this exists as a trait rather than a per-type assertion
//!
//! **The old compiler was total at the file level too.**
//!
//! It had an AST of the system *skeleton* — and **no AST of handler bodies**. Below
//! the handler's opening brace it was a flat segment stream with native code as an
//! opaque string. So downstream passes re-derived structure by reading text, and
//! twenty-five shipped bugs came from exactly that. Every single one of them lives
//! *below* the line where its tree stopped.
//!
//! A per-type check would have passed. `FileAst` covered every byte; `SystemAst`
//! covered every byte; and `HandlerBody` was a `String`, so there was nothing to
//! check and nobody noticed. The gap was **invisible to any check that a type opts
//! into**.
//!
//! So the check is a trait, `check_total` walks it blindly, and a node that holds an
//! undecomposed span has to say so out loud by returning no children — at which point
//! [`Node::is_leaf_on_purpose`] forces the author to state *why*. A blob cannot hide
//! behind "nobody wrote a test for that level."

use crate::Span;

/// Any node in the tree.
pub trait Node {
    /// The bytes this node owns.
    fn span(&self) -> Span;

    /// Children, in source order. Their spans MUST partition `self.span()`.
    ///
    /// A node with no children is a **leaf**, and a leaf is a claim: *"these bytes
    /// have no further structure that framec is entitled to know."* That claim is
    /// checked — see [`Node::is_leaf_on_purpose`].
    fn children(&self) -> Vec<&dyn Node>;

    /// What kind of node this is. For diagnostics and `--dump-ast`.
    fn kind(&self) -> &'static str;

    /// **A leaf must justify itself.**
    ///
    /// Returning `true` means: *"this span is genuinely atomic — there is nothing
    /// inside it framec has any business knowing."* True for a keyword, a name, a
    /// span of whitespace, and for the *contents* of a native statement (framec must
    /// never interpret those bytes).
    ///
    /// Returning `false` means: *"this span HAS structure and I have not decomposed
    /// it yet."* That is the old compiler's handler body, and `check_total` reports
    /// it as an **undecomposed blob** rather than silently accepting it.
    ///
    /// The default is `false` — **the burden of proof is on the leaf.** A node that
    /// says nothing is assumed to be hiding something, because that is what actually
    /// happened.
    fn is_leaf_on_purpose(&self) -> bool {
        false
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum Defect {
    Gap {
        parent: &'static str,
        from: usize,
        to: usize,
    },
    Overlap {
        parent: &'static str,
        at: usize,
    },
    OutOfOrder {
        parent: &'static str,
        at: usize,
    },
    /// A node that has structure inside it and has not decomposed it.
    ///
    /// **This is the old compiler's handler body, named.** It is not an error the
    /// user can cause; it is the rebuild being incomplete, and it is reported rather
    /// than tolerated.
    UndecomposedBlob {
        kind: &'static str,
        span: Span,
    },
}

impl std::fmt::Display for Defect {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Defect::Gap { parent, from, to } => write!(
                f,
                "COMPILER BUG: in `{parent}`, bytes {from}..{to} belong to no child — \
                 the tree is not total"
            ),
            Defect::Overlap { parent, at } => write!(
                f,
                "COMPILER BUG: in `{parent}`, byte {at} belongs to two children"
            ),
            Defect::OutOfOrder { parent, at } => write!(
                f,
                "COMPILER BUG: in `{parent}`, children are not in source order at byte {at}"
            ),
            Defect::UndecomposedBlob { kind, span } => write!(
                f,
                "UNDECOMPOSED: `{kind}` at {}..{} has structure and no children. This is \
                 the old compiler's handler body — a span framec kept as text instead of \
                 a tree, which is where all 25 bugs lived. Either parse it, or declare \
                 `is_leaf_on_purpose` and say why.",
                span.start, span.end
            ),
        }
    }
}

/// Walk the whole tree and check the invariant at **every** node.
pub fn check_total(root: &dyn Node) -> Result<(), Defect> {
    let kids = root.children();

    if kids.is_empty() {
        // A leaf must have justified itself.
        if !root.is_leaf_on_purpose() && !root.span().is_empty() {
            return Err(Defect::UndecomposedBlob {
                kind: root.kind(),
                span: root.span(),
            });
        }
        return Ok(());
    }

    let parent = root.kind();
    let s = root.span();
    let mut cursor = s.start;

    for k in &kids {
        let ks = k.span();
        if ks.start < cursor {
            return Err(Defect::Overlap {
                parent,
                at: ks.start,
            });
        }
        if ks.start > cursor {
            return Err(Defect::Gap {
                parent,
                from: cursor,
                to: ks.start,
            });
        }
        cursor = ks.end;
    }
    if cursor != s.end {
        return Err(Defect::Gap {
            parent,
            from: cursor,
            to: s.end,
        });
    }

    for k in kids {
        check_total(k)?;
    }
    Ok(())
}

/// Count every node, by kind. The **granularity snapshot** — the assertion that
/// coverage structurally cannot make.
///
/// A handler body that decomposes to *one* statement when the source has four lines
/// satisfies coverage **perfectly** and is wrong. Coverage says "every byte is
/// somewhere"; only granularity says "and it is in the *right* somewhere." This is
/// what catches an island silently classified as water.
pub fn census(root: &dyn Node, out: &mut std::collections::BTreeMap<&'static str, usize>) {
    *out.entry(root.kind()).or_insert(0) += 1;
    for k in root.children() {
        census(k, out);
    }
}
