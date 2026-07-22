//! `top_level_eq` — the top-level `=` finder, **dogfooded as an `@@[scan(u8)]` counter
//! automaton** ([`top_level_eq.frs`]). The shared correct-class primitive for the
//! default/init separator: [`find`] returns the offset of the FIRST `=` at bracket-depth 0
//! (Dyck-1 over `()[]{}`) AND angle-depth 0 (digraph-guarded `<>`), outside `"…"`-strings,
//! excluding the `== <= >= != =>` digraphs — or `to` if there is none.
//!
//! It RETIRES three byte-blind hand splits (#249): [`super::parse_one_param`]'s
//! `split_once('=')` (B2), [`super::decl_read`]'s `eq_or_end` `while != b'='` leaf (B9), and —
//! by composition through [`super::param_scan`] — the emit-side `params_split`/`param_names`
//! (B1). Because it halts at the FIRST top-level `=` (always TYPE position, where `<` is
//! unambiguously a generic opener), a single angle counter is exact — no fork, unlike
//! ParamScan. Opacity is `"`-only [`super::string_scan`] (matching ParamScan, dodging the
//! Rust `'a`-lifetime hazard — the residual char/lifetime gap is the same #219 carry).
//!
//! `.gen.rs` regen: `framec-ng -l rust --emit top_level_eq.frs | grep -v '^#!\[allow' >
//! top_level_eq.gen.rs`.

use super::arg_scan::angle_guard;
use super::string_scan;

/// Skip a `"`-string via the StringScan system (composition), returning the offset past it; if
/// there is no `"`-string at `i`, return `i` unchanged. Identical to ParamScan's/ParenBalance's
/// leaf — one string model for the whole `"`-only family (target-blind; `'` is ignored).
fn skip_string(src: &[u8], i: usize) -> usize {
    if i < src.len() && src[i] == b'"' {
        string_scan::scan(src, i).unwrap_or(i)
    } else {
        i
    }
}

/// Is the `=` at `i` a lone assignment separator (not part of `== <= >= != =>`)? O(1) byte
/// compares — the `arg_scan::eq_guard_ok` rule (prev not one of `= ! < >`, next not `=`) PLUS
/// `next != '>'` so a `=>` is never taken.
fn eq_is_sep(src: &[u8], i: usize, from: usize, to: usize) -> bool {
    let prev_ok = i == from || !matches!(src[i - 1], b'=' | b'!' | b'<' | b'>');
    let next_ok = i + 1 >= to || !matches!(src[i + 1], b'=' | b'>');
    prev_ok && next_ok
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
    use super::{angle_guard, eq_is_sep, skip_string};
    include!("top_level_eq.gen.rs");
}

/// The offset of the first top-level `=` (the default/init separator) in `bytes[from..to)`, or
/// `to` if there is none. Runs the `TopLevelEq` system; `eq_at` is the recorded boundary.
pub fn find(bytes: &[u8], from: usize, to: usize) -> usize {
    let mut m = fsm::TopLevelEq::over(bytes, from, to);
    m.scan_at(from);
    if m.found {
        m.eq_at
    } else {
        to
    }
}
