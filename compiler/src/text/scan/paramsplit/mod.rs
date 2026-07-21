//! Top-level comma-split of a `@@system Name(...)` param-list interior, **dogfooded as an
//! `@@[scan(u8)]` COUNTER automaton** ([`paramsplit.frs`]), STRING-AWARE.
//!
//! The `@@system` replacement for the string-BLIND hand depth-0 comma loop that used to live in
//! [`super::split_system_params`] (C-final delegation 3 — the last production top-level-split
//! recognition cycle). From the param-list interior it emits the top-level comma-split EXTENTS
//! `(start, end)`; a `,` splits only when the merged `()[]<>{}` nesting `depth` is 0 AND it is
//! not inside a `"…"`-string default. STRING-AWARE by COMPOSITION (the `skip_string` leaf runs
//! the StringScan system, `"`-only — matching ParenBalance, which delimits this very interior).
//! The per-part sigil parse (`$(`=state / `$>(`=enter / bare=domain; `name: type = default`)
//! stays NATIVE — this machine produces only the split boundaries.
//!
//! `.gen.rs` regen: `framec-ng -l rust --emit paramsplit.frs | grep -v '^#!\[allow' >
//! paramsplit.gen.rs`.

use super::string_scan;

/// Skip a `"`-string via the StringScan system (composition), returning the offset past it;
/// if there is no string at `i`, return `i` unchanged. Identical to ParenBalance's leaf — one
/// string model for the whole `"`-only balance family.
fn skip_string(src: &[u8], i: usize) -> usize {
    if i < src.len() && src[i] == b'"' {
        string_scan::scan(src, i).unwrap_or(i)
    } else {
        i
    }
}

/// Push one emitted split extent into the machine's `parts` register (a leaf with no walk —
/// the `record_arg` / `record_hole` precedent).
fn record_part(parts: &mut Vec<(usize, usize)>, s: usize, e: usize) {
    parts.push((s, e));
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
    use super::{record_part, skip_string};
    include!("paramsplit.gen.rs");
}

/// The top-level comma-split extents of a param-list interior. Each `(s, e)` is a byte range
/// into `bytes`; a `,` splits only at depth 0 over `()[]<>{}` and outside a `"…"`-string. The
/// machine finds the boundaries; this wrapper only runs it and reads the register.
pub fn split(bytes: &[u8]) -> Vec<(usize, usize)> {
    let mut m = fsm::ParamSplit::over(bytes);
    m.scan_at(0);
    m.parts
}
