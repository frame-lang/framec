//! The native-code island dispatch, **dogfooded as an `@@[scan(u8)]` system** — the
//! PRODUCTION walk behind [`super::parts`]`::native_parts` (the construction driver).
//! Walks `[from, limit)` of the full buffer trying the recognizers in the hand order
//! (opaque → inst → embed → ref) and records `(kind, start, end)`: 0=Text
//! 1=Literal 2=Ref 3=Instantiate 4=EmbedCall 5=Comment. The opaque arm applies the
//! kind-aware `to`-policy (comment clamps, literal rejects overrun, unterminated falls
//! through to water — ledger T-N1..T-N4). Leaves are Category A: run-and-unwrap
//! compositions of OpaqueScan / InstScan / EmbedScan / RefScan + O(1) policy.
//!
//! `.gen.rs` regen: `framec-ng -l rust --emit native_parts_scan.frs | grep -v '^#!\[allow' >
//! native_parts_scan.gen.rs`.

use super::literals::Target;
use super::{embed_scan, inst_scan, ref_scan};

/// Try the island recognizers at `i`, in the hand `native_parts` order, under the kind-aware
/// `limit` policy. Returns `(kind, end)`: 0=none 1=Literal 2=Ref 3=Instantiate 4=EmbedCall
/// 5=Comment.
///
/// Category A: a run-and-unwrap composition of four systems + O(1) policy — no loop, no
/// counter; every walk is inside a composed system. The opaque arm is the SAME kind-split
/// policy `machine.rs::skip_opaque` shipped in Item 3a, kind kept: a COMMENT that runs past
/// `limit` is CLAMPED and still a comment (T-N4); a LITERAL that overruns `limit` is REJECTED
/// and falls through (T-N3); `Unterminated`/`None` fall through to the island arms and then
/// to water (T-N1/T-N2 — the carried swallows). OpaqueScan runs on the FULL buffer (DP-4);
/// the island recognizers run under the hand's own `&src[..limit]` bound.
fn try_island(src: &[u8], i: usize, limit: usize, target: Target) -> (i32, usize) {
    match super::opaque_scan::opaque_at(src, i, target) {
        super::opaque_scan::OpaqueAt::Comment(end) => return (5, end.min(limit)),
        super::opaque_scan::OpaqueAt::Literal(end) => {
            if end <= limit {
                return (1, end);
            }
        }
        super::opaque_scan::OpaqueAt::None | super::opaque_scan::OpaqueAt::Unterminated => {}
    }
    if let Some((_, end)) = inst_scan::scan(&src[..limit], i) {
        return (3, end);
    }
    if let Some((_, _, _, end)) = embed_scan::scan(&src[..limit], i) {
        return (4, end);
    }
    if let Some((_, _, end)) = ref_scan::scan(&src[..limit], i) {
        return (2, end);
    }
    (0, i)
}

/// Δ3 (T-N1/T-N2, DP-1): does an opaque body OPEN at `i` but never close (unterminated)? When it
/// does, the walk stops scanning its interior for islands — the rescued interior becomes ONE plain
/// Text run to `limit`. The lexer's refusal is HONORED (a `FrameRef` inside what the user meant as
/// unterminated string/comment content is content, not code — the #224/#215 corruption class),
/// not carried. Per the DP-1 ruling `native_parts` grows NO diagnostics channel; the target
/// compiler still reports the user's real (unterminated) error. Category A: runs OpaqueScan and
/// reads the `unterminated` register.
fn unterminated_at(src: &[u8], i: usize, target: Target) -> bool {
    matches!(
        super::opaque_scan::opaque_at(src, i, target),
        super::opaque_scan::OpaqueAt::Unterminated
    )
}

fn flush_text(parts: &mut Vec<(i32, usize, usize)>, from: usize, to: usize) {
    if from < to {
        parts.push((0, from, to));
    }
}
fn record_part(parts: &mut Vec<(i32, usize, usize)>, kind: i32, from: usize, to: usize) {
    parts.push((kind, from, to));
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
    use super::{flush_text, record_part, try_island, unterminated_at, Target};
    include!("native_parts_scan.gen.rs");
}

/// The parts of `bytes[from..to)` as `(kind, start, end)` triples partitioning the range —
/// the island dispatch driven by the system. kind: 0=Text 1=Literal 2=Ref 3=Instantiate
/// 4=EmbedCall 5=Comment. `from` is constructor config (`text_start`); `to` is the `limit`
/// register; the machine walks the FULL buffer under them (DP-4 — a slice cannot express the
/// comment-clamp/literal-reject asymmetry at `to`).
pub fn parts(bytes: &[u8], from: usize, to: usize, target: Target) -> Vec<(i32, usize, usize)> {
    debug_assert!(
        from <= to && to <= bytes.len(),
        "native parts walk bounds: from {from} <= to {to} <= len {}",
        bytes.len()
    );
    let mut m = fsm::NativePartsScan::over(bytes, target, to, from);
    m.scan_at(from);
    m.parts
}
