//! The native-code island dispatch, **dogfooded as an `@@[scan(u8)]` system** — the
//! `@@system` analogue of the hand [`super::parts`]`::native_parts`, and the culmination of
//! the island recognizers: it composes InstScan / EmbedScan / RefScan (which compose
//! ParenBalance, which composes StringScan) over one borrow.
//!
//! [`native_parts_scan.frs`] walks native code trying the recognizers in the SAME ORDER as
//! `native_parts` (comment, literal, instantiation, embed-call, ref) and records
//! `(kind, start, end)` parts. A differential test proves the sequence matches `native_parts`
//! at the (kind, span) level.
//!
//! `.gen.rs` regen: `framec-ng -l rust --emit native_parts_scan.frs | grep -v '^#!\[allow' >
//! native_parts_scan.gen.rs`.

use super::lex::Lexer;
use super::literals::Target;
use super::{embed_scan, inst_scan, ref_scan};

/// Try the island recognizers at `i`, in `native_parts` order. Returns `(kind, end)`:
/// 0=none 1=Literal(comment or string) 2=Ref 3=Instantiate 4=EmbedCall.
fn try_island(src: &[u8], i: usize, target: Target) -> (i32, usize) {
    let lx = Lexer::new(src, target);
    if let Ok(Some(end)) = lx.comment_at(i) {
        return (1, end.min(src.len()));
    }
    if let Ok(Some(l)) = lx.literal_at(i) {
        if l.span.end <= src.len() {
            return (1, l.span.end);
        }
    }
    if let Some((_, end)) = inst_scan::scan(src, i) {
        return (3, end);
    }
    if let Some((_, _, _, end)) = embed_scan::scan(src, i) {
        return (4, end);
    }
    if let Some((_, _, end)) = ref_scan::scan(src, i) {
        return (2, end);
    }
    (0, i)
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
    use super::{flush_text, record_part, try_island, Target};
    include!("native_parts_scan.gen.rs");
}

/// The parts of `bytes` as `(kind, start, end)` — the island dispatch driven by the system.
/// kind: 0=Text 1=Literal 2=Ref 3=Instantiate 4=EmbedCall.
pub fn parts(bytes: &[u8], target: Target) -> Vec<(i32, usize, usize)> {
    let mut m = fsm::NativePartsScan::over(bytes, target);
    m.scan_at(0);
    m.parts
}
