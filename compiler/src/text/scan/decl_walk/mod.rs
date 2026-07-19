//! The decl-section dispatch walk, **dogfooded as an `@@[scan(u8)]` system**
//! ([`decl_walk.frs`]) — the FOURTH section walk, completing the family
//! ([`super::machine_walk`] = states, [`super::state_walk`] = members, [`super::body_walk`] =
//! statements, [`super::segmenter`] = items).
//!
//! [`decl_starts`] returns the declaration-start offsets in a decl-section span
//! (`interface:` / `domain:` / `actions:` / `operations:`), plus the `unterminated_body`
//! register (ledger T2 — a body decl whose `{` never balanced was clamped to `limit`), driven
//! by the `DeclWalk` system: each start is recorded and the walk jumps past that decl's whole
//! extent (the `decl_end` leaf → [`super::machine::decl_extent`], the SAME single source the
//! driver builds the Member/BodyDecl nodes from), skipping opaque regions, whitespace, and
//! `@@[` attribute lines. [`super::machine`]'s `decl_section` is now a thin native driver over
//! these positions (Item 3d M-wire).
//!
//! `.gen.rs` regen: `framec-ng -l rust --emit decl_walk.frs | grep -v '^#!\[allow' >
//! decl_walk.gen.rs`.

use super::literals::Target;
use super::machine::{decl_extent, skip_opaque, to_end_of_line, DeclExtent};

/// Opaque-skip leaf: the offset past a comment/literal at `i` (kind-aware limit policy), or `i`
/// unchanged. Runs OpaqueScan via the shared `machine::skip_opaque`; no walk here (D3).
fn skip(src: &[u8], i: usize, limit: usize, target: Target) -> usize {
    skip_opaque(src, i, limit, target).unwrap_or(i)
}

/// Is the byte at `i` whitespace? The hand walk's exact byte class (`is_ascii_whitespace`).
fn is_ws(src: &[u8], i: usize) -> bool {
    i < src.len() && src[i].is_ascii_whitespace()
}

/// Does an `@@[attr]` attribute line open at `i`? (The `public Object ;` guard — an attribute
/// line is trivia, not a declaration.)
fn is_attr(src: &[u8], i: usize, limit: usize) -> bool {
    i + 3 <= limit && &src[i..i + 3] == b"@@["
}

/// The end of an attribute line: `machine::to_end_of_line` (shared with the hand walk).
fn attr_end(src: &[u8], i: usize, limit: usize) -> usize {
    to_end_of_line(src, i, limit)
}

/// The offset one past the declaration that starts at `i` — to end-of-line for a line decl,
/// past the matching `}` (clamped to `limit` when unbalanced, ledger T2) for a body decl. A
/// thin read of the single-source `machine::decl_extent` head, the SAME source the driver
/// builds the nodes from, so the boundary the walk finds and the extent the node carries
/// cannot drift. No walk here (D3).
fn decl_end(src: &[u8], i: usize, limit: usize, with_bodies: bool, target: Target) -> usize {
    match decl_extent(src, i, limit, with_bodies, target) {
        DeclExtent::Line { eol } => eol,
        DeclExtent::Body { end, .. } => end,
    }
}

/// Was the declaration at `i` an UNBALANCED body decl (its extent clamped to `limit`)? The
/// other thin read of `machine::decl_extent` — the T2 register's source.
fn decl_unterminated(src: &[u8], i: usize, limit: usize, with_bodies: bool, target: Target) -> bool {
    matches!(
        decl_extent(src, i, limit, with_bodies, target),
        DeclExtent::Body {
            unterminated: true,
            ..
        }
    )
}

/// Record a decl-start offset. (A leaf so the machine body stays free of `Vec` mechanics.)
fn record(v: &mut Vec<usize>, x: usize) {
    v.push(x);
}

mod fsm {
    #![allow(
        dead_code,
        unused_parens,
        non_snake_case,
        unused_variables,
        unused_mut,
        unused_imports
    )]
    use super::{attr_end, decl_end, decl_unterminated, is_attr, is_ws, record, skip, Target};
    include!("decl_walk.gen.rs");
}

/// The declaration-start offsets in `bytes[from..limit]` plus the `unterminated_body` register
/// (ledger T2: true iff some body decl's `{` never balanced and its extent was clamped to
/// `limit`), target-aware — driven by the `DeclWalk` system. Each start is recorded and the
/// walk jumps past that decl's whole extent, so only top-level declarations are returned.
/// (The register rides the return the way `body_walk::stmt_starts` returns its final depth.)
pub fn decl_starts(
    bytes: &[u8],
    from: usize,
    limit: usize,
    with_bodies: bool,
    target: Target,
) -> (Vec<usize>, bool) {
    let mut m = fsm::DeclWalk::over(bytes, target, limit, with_bodies);
    m.scan_at(from);
    (m.starts, m.unterminated_body)
}

/// The hand walk's boundary decisions, factored — kept ONLY as the `decl_starts`
/// differential-test oracle until the parity is locked and the hand recognition is deleted.
/// This is exactly the production `decl_section` boundary loop (machine.rs: skip-opaque / ws /
/// `@@[`-attr / record + extent-jump), stripped of node building; it shares the leaves
/// (`skip_opaque`, `is_attr`, `to_end_of_line`, `decl_end`) with the system, as every sibling
/// walk oracle does — the differential proves the WALK, which is the thing being converted.
/// Not used in production.
#[doc(hidden)]
pub fn decl_starts_hand(
    bytes: &[u8],
    from: usize,
    limit: usize,
    with_bodies: bool,
    target: Target,
) -> Vec<usize> {
    let mut starts = Vec::new();
    let mut i = from;
    while i < limit {
        if let Some(next) = skip_opaque(bytes, i, limit, target) {
            i = next;
            continue;
        }
        if bytes[i].is_ascii_whitespace() {
            i += 1;
            continue;
        }
        if is_attr(bytes, i, limit) {
            i = to_end_of_line(bytes, i, limit);
            continue;
        }
        starts.push(i);
        i = decl_end(bytes, i, limit, with_bodies, target);
    }
    starts
}

/// In-crate pins for the single-source `machine::decl_extent` head (it is `pub(crate)`, so its
/// facts — including the `open` offset the `decl_section` driver keys the Signature/body split
/// off — are pinned here, not in the integration battery). SCAFFOLDING.
#[cfg(test)]
mod decl_extent_tests {
    use super::super::machine::{decl_extent, DeclExtent};
    use super::Target;

    /// A body decl's extent: `open` at the first `{` of the first line, `end` one past the
    /// matching `}` (nested braces balanced), `unterminated` false.
    #[test]
    fn body_decl_extent_balanced() {
        let src = b"greet(n: str): str {\n    if x { y }\n}\nnext()\n";
        match decl_extent(src, 0, src.len(), true, Target::Rust) {
            DeclExtent::Body {
                open,
                end,
                unterminated,
            } => {
                assert_eq!(open, 19, "the first `{{` on the decl's first line");
                assert_eq!(src[open], b'{');
                assert_eq!(src[end - 1], b'}', "end is one past the matching `}}`");
                assert_eq!(end, 37);
                assert!(!unterminated);
            }
            DeclExtent::Line { .. } => panic!("with_bodies + `{{` on the line must fork Body"),
        }
    }

    /// T2: an unbalanced body clamps `end` to `limit` and REPORTS it (`unterminated: true`) —
    /// the hand `unwrap_or(limit)` behavior, now a value.
    #[test]
    fn body_decl_extent_unbalanced_clamps_and_reports() {
        let src = b"go() {\n    x = 1\n";
        match decl_extent(src, 0, src.len(), true, Target::Rust) {
            DeclExtent::Body {
                open,
                end,
                unterminated,
            } => {
                assert_eq!(open, 5);
                assert_eq!(end, src.len(), "clamped to limit");
                assert!(unterminated, "the clamp is reported, not erased");
            }
            DeclExtent::Line { .. } => panic!("must fork Body"),
        }
    }

    /// Without `with_bodies`, a `{` on the line is NOT a fork — the extent is the line
    /// (interface/domain semantics, verbatim from the hand `decl_section`).
    #[test]
    fn line_decl_extent_ignores_brace_without_with_bodies() {
        let src = b"go() { x }\nnext()\n";
        match decl_extent(src, 0, src.len(), false, Target::Rust) {
            DeclExtent::Line { eol } => assert_eq!(eol, 10, "to the first newline"),
            DeclExtent::Body { .. } => panic!("with_bodies=false must never fork Body"),
        }
    }

    /// T15 (walk-level shape): a `{` as the section's FINAL byte — `open == limit - 1`,
    /// `end == limit`, unterminated. The machine chain is panic-free here; the driver-level
    /// `Span::new(open + 1, end - 1)` hazard is the T15 ledger row, out of the walk's hands.
    #[test]
    fn t15_open_brace_as_final_byte() {
        let src = b"x() {";
        match decl_extent(src, 0, src.len(), true, Target::Rust) {
            DeclExtent::Body {
                open,
                end,
                unterminated,
            } => {
                assert_eq!(open, 4, "the `{{` is the final byte");
                assert_eq!(end, 5, "clamped to limit");
                assert!(unterminated);
            }
            DeclExtent::Line { .. } => panic!("must fork Body"),
        }
    }
}
