//! Embedded-system-call recognizer, **dogfooded as an `@@[scan(u8)]` system** — the
//! `@@system` analogue of the hand [`super::parts`]`::embed_call_at`, the last of this
//! session's byte-loop island recognizers, now undone.
//!
//! [`embed_scan.frs`] recognizes `@@:self.<field>.<method>(args)`, composing string-aware
//! [`super::paren_balance`] for the arg extent. A differential test proves the (field,
//! method, args, end) it yields matches `embed_call_at`.
//!
//! `.gen.rs` regen: `framec-ng -l rust --emit embed_scan.frs | grep -v '^#!\[allow' >
//! embed_scan.gen.rs`.

use super::paren_balance;

fn starts_self_dot(src: &[u8], i: usize) -> bool {
    let head = b"@@:self.";
    i + head.len() <= src.len() && &src[i..i + head.len()] == head
}
fn is_ident_at(src: &[u8], i: usize) -> bool {
    i < src.len() && (src[i].is_ascii_alphanumeric() || src[i] == b'_')
}
fn is_dot_at(src: &[u8], i: usize) -> bool {
    i < src.len() && src[i] == b'.'
}
fn skip_ws_at(src: &[u8], mut i: usize) -> usize {
    while i < src.len() && (src[i] == b' ' || src[i] == b'\t') {
        i += 1;
    }
    i
}
fn is_open_paren_at(src: &[u8], i: usize) -> bool {
    i < src.len() && src[i] == b'('
}
fn paren_end(src: &[u8], p: usize) -> usize {
    paren_balance::scan(src, p).unwrap_or(p)
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
    use super::{
        is_dot_at, is_ident_at, is_open_paren_at, paren_end, skip_ws_at, starts_self_dot,
    };
    include!("embed_scan.gen.rs");
}

/// An `@@:self.field.method(args)` embedded call at `bytes[i..]`, if any: `(field, method,
/// args, end)` where `end` is one past the closing paren — the same recognition
/// [`super::parts`]`::embed_call_at` does. `args` is the interior of the parens, trimmed.
pub fn scan(bytes: &[u8], i: usize) -> Option<(String, String, String, usize)> {
    let mut m = fsm::EmbedScan::over(bytes);
    if !m.scan_at(i) {
        return None;
    }
    let field = String::from_utf8_lossy(&bytes[m.field_start..m.field_end]).into_owned();
    let method = String::from_utf8_lossy(&bytes[m.method_start..m.method_end]).into_owned();
    let args = String::from_utf8_lossy(&bytes[m.paren_open + 1..m.cursor - 1])
        .trim()
        .to_string();
    Some((field, method, args, m.cursor))
}
