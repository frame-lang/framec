//! Declaration-site `@@system Name(...)` header param parser, **dogfooded as an `@@[scan(u8)]`
//! system** ([`param_scan.frs`]) — the `@@system` sibling of [`super::arg_scan`] (call-site args),
//! REPLACING the string-blind + sigil-blind hand split (the retired `ParamSplit` counter + the
//! native sigil parse that used to live in [`super::split_system_params`]).
//!
//! ONE left-to-right walk over the ALREADY-`(`-balanced interior emits, per param, its GROUP
//! (`$(`=state / `$>(`=enter / bare=domain) and its TRIMMED body span. A group's closer is the
//! BALANCED `)` found by the walk (killing the hand's `trim_end_matches(')')` truncation of
//! `$(g: int = f(1))`); the `>` of a `$>(` sigil is consumed as part of the 3-byte sigil and is
//! NEVER bracket-counted (killing the `$>(` `>` miscount that dropped a trailing param).
//!
//! **Angles fork, they are never guessed** (ArgScan's Option C, minus arity). Within one list
//! either every counted `<`/`>` is a bracket pair (hypothesis G — `Map<K,V>`) or none is
//! (hypothesis O — operators). The single pass computes both; [`parse_decl`] takes G iff it is
//! SELF-CONSISTENT (`g_viable` — no `>` at adepth 0, no unclosed `<`), else O. A declaration has no
//! declared arity to adjudicate with, so self-consistency IS the adjudicator — the exact collapse
//! of ArgScan's `primary` selection into a single reading.
//!
//! Opacity is `"`-only [`super::string_scan`] (NOT target-aware OpaqueScan): it AGREES with
//! ParenBalance, which delimits this very interior `"`-only, and it dodges the Rust `'a`-lifetime
//! hazard (a `'…'` char default stays CARRIED — F5 #2; void condition = a target-aware
//! char-vs-lifetime leaf). The machine is TOTAL: malformed input degrades to a VERBATIM tail.
//!
//! `.gen.rs` regen: `framec-ng -l rust --emit param_scan.frs | grep -v '^#!\[allow' >
//! param_scan.gen.rs`.

use super::arg_scan::{angle_guard, is_sigil_enter, is_sigil_state};
use super::string_scan;
use crate::tree::body::ParamGroup;

/// Skip a `"`-string via the StringScan system (composition), returning the offset past it; if
/// there is no `"`-string at `i`, return `i` unchanged. Identical to ParamSplit's/ParenBalance's
/// leaf — one string model for the whole `"`-only balance family (target-blind; `'` is ignored).
fn skip_string(src: &[u8], i: usize) -> usize {
    if i < src.len() && src[i] == b'"' {
        string_scan::scan(src, i).unwrap_or(i)
    } else {
        i
    }
}

/// Push one O-hypothesis param record. Normalizes `!has_val` to the empty span `(cur, cur)` — an
/// empty body is `vs == ve`, no sentinel (the `record_arg` precedent, minus the name span).
fn record_part(
    parts: &mut Vec<(i32, usize, usize, bool)>,
    group: i32,
    has_val: bool,
    vs: usize,
    ve: usize,
    cur: usize,
    g_end: bool,
) {
    if has_val {
        parts.push((group, vs, ve, g_end));
    } else {
        parts.push((group, cur, cur, g_end));
    }
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
    use super::{angle_guard, is_sigil_enter, is_sigil_state, record_part, skip_string};
    include!("param_scan.gen.rs");
}

fn group_of(g: i32) -> ParamGroup {
    match g {
        1 => ParamGroup::State,
        2 => ParamGroup::Enter,
        _ => ParamGroup::Domain,
    }
}

fn lossy(bytes: &[u8], s: usize, e: usize) -> String {
    String::from_utf8_lossy(&bytes[s..e]).into_owned()
}

/// The O reading — each record materialized straight into `(group, trimmed body)`.
fn reading_o(bytes: &[u8], recs: &[(i32, usize, usize, bool)]) -> Vec<(ParamGroup, String)> {
    recs.iter()
        .map(|r| (group_of(r.0), lossy(bytes, r.1, r.2)))
        .collect()
}

/// The G reading as a FOLD over the O records + RAW byte spans — never a second walk. G boundaries
/// = recorded boundaries with `g_end`. Each G-param covers a run of O-records `r_i..r_j`; it takes
/// `group` from `r_i` and its body from the RAW span `r_i.vs .. r_j.ve`, so any straddled comma
/// bytes ride along verbatim. No byte-level decision is made here — a function, not a machine.
fn merge_g(bytes: &[u8], recs: &[(i32, usize, usize, bool)]) -> Vec<(ParamGroup, String)> {
    let mut out = Vec::new();
    let mut run_start: Option<usize> = None;
    for (idx, r) in recs.iter().enumerate() {
        let i = *run_start.get_or_insert(idx);
        if r.3 || idx + 1 == recs.len() {
            let r0 = &recs[i];
            out.push((group_of(r0.0), lossy(bytes, r0.1, r.2)));
            run_start = None;
        }
    }
    out
}

/// The relationship between the two angle hypotheses on a header param list — the
/// declaration-site collapse of [`super::arg_scan::AngleReading`], minus arity (a
/// declaration has nothing to adjudicate against). The EMITTED reading is always
/// [`ParamsOut::primary`] (favor-the-template); this only records whether the operator
/// reading ALSO survived, which the validation pass needs for `W417` (RFC-0060).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParamReading {
    /// Hypotheses coincide, or the list refused / has no counted angles.
    Inert,
    /// Only the operator reading is viable; `primary` IS that sole reading.
    Operators,
    /// Both viable and divergent: `primary` = G (brackets), payload = the O (operators)
    /// reading bodies.
    Forked(Vec<(ParamGroup, String)>),
}

/// The forked parse: the emitted reading plus the angle relationship. `primary` is
/// BYTE-IDENTICAL to the pre-RFC-0060 `parse_decl` output — favor-the-template is
/// unchanged; `W417` is additive visibility, never an emission change.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParamsOut {
    pub primary: Vec<(ParamGroup, String)>,
    pub angles: ParamReading,
}

/// Parse the header param-list interior, RETAINING both angle readings. Mirrors
/// `arg_scan::parse`'s `primary`/`AngleReading` selection exactly (a declaration has no
/// arity, so G is favored iff self-consistent): O on refusal / no-angles / `!g_viable` /
/// all-boundaries-shared → `Inert`/`Operators`; else G (`merge_g`) with the O reading as
/// the `Forked` payload. NO `.frs` change — the machine already stamps `g_viable`/`g_end`;
/// the pre-RFC-0060 wrapper simply discarded the losing reading.
pub fn parse_decl_forked(bytes: &[u8]) -> ParamsOut {
    let mut m = fsm::ParamScan::over(bytes);
    m.scan_at(0);
    let o = reading_o(bytes, &m.parts);
    let (primary, angles) = if m.refusal != 0 || !m.angle_touched {
        (o, ParamReading::Inert)
    } else if !m.g_viable {
        (o, ParamReading::Operators)
    } else if m.parts.iter().all(|r| r.3) {
        (o, ParamReading::Inert)
    } else {
        (merge_g(bytes, &m.parts), ParamReading::Forked(o))
    };
    ParamsOut { primary, angles }
}

/// The EMITTED reading only — the angle-self-consistency (`g_viable`) reading,
/// byte-identical to before RFC-0060. Favor-the-template: O on refusal / no-angles /
/// `!g_viable` / all-boundaries-shared; else G. The two emit-side consumers
/// (`param_names`, `params_split`) and the tree's `state`/`enter`/`domain` groups all
/// take this; the fork rides the tree separately via [`parse_decl_forked`].
pub fn parse_decl(bytes: &[u8]) -> Vec<(ParamGroup, String)> {
    parse_decl_forked(bytes).primary
}
