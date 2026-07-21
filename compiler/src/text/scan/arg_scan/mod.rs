//! The instantiation ARG-LIST parser, **dogfooded as an `@@[scan(u8)]` system** — the
//! `@@system` replacement for the hand [`super::parts`]`::parse_inst_args` +
//! `split_top_commas` + `split_top_eq` (inventory M6): the only production machinery in
//! parts.rs that had no named-system counterpart.
//!
//! [`arg_scan.frs`] walks the interior `[from, to)` of a `@@Name(...)` call ONCE: top-level
//! commas end arguments; a `$(`/`$>(` sigil at an argument start opens a group whose closer
//! is the BALANCED `)` found by the walk (the hand's `trim_end_matches(')')` ate the user's
//! own closer in `$(g(1))`); a qualifying top-level `=` with an IDENTIFIER left-hand side
//! names the argument. One alphabet, one string model — the hand code had two counter
//! automata with different alphabets and three private `"`/`'`-only skippers.
//!
//! **Angles fork, they are never guessed** (design record §11, Option C): within one list
//! either every counted `<`/`>` is a bracket pair (hypothesis G — the hand comma splitter's
//! alphabet, disciplined) or none is (hypothesis O — the hand eq splitter's alphabet). The
//! machine's single pass computes both — the records ARE the O segmentation, and each
//! record's `g_end` bit marks the boundaries that hold under G too. This wrapper folds the
//! G candidate from those records ([`merge_g`]: recorded facts + RAW byte spans, never a
//! second walk) and reports the relationship as [`AngleReading`]: `Inert` (the hypotheses
//! coincide), `Operators` (G nonviable — the sole O reading), or `Forked` (both viable,
//! boundaries diverged: primary = G, payload = O). The choice between forked candidates
//! belongs to the party that can make it — declared arity, via `validate::adjudicate` —
//! downstream. Operator digraphs `<=` `>=` `->` `=>` are guard-excluded; the mechanism is
//! target-blind (no generics table, no language-spec follower sets).
//!
//! String-blindness is killed by COMPOSITION: opacity is [`super::opaque_scan`] via
//! `machine::skip_opaque` (comments, chars, triples, raws — the same model the whole
//! grammar uses). The machine is TOTAL: malformed input degrades to a VERBATIM tail with a
//! named `refusal` register (UnterminatedOpaque / StrayCloser / TrailingAfterGroup /
//! UnclosedGroup) — the hand's silent swallow-to-end, made observable for the deferred
//! §1167 validation layer. A refusal supersedes the fork: a malformed list adjudicates
//! nothing (`angles == Inert`).
//!
//! The machine records trimmed SPANS; this wrapper materializes `InstArg`s (the file-wide
//! `from_utf8_lossy` policy lives HERE, not in the machine, so the raw bytes stay
//! recoverable). framec owns the WALK; the native leaves answer only facts (sigil prefix,
//! digraph guards, eq guard bytes, ident span) or run sub-systems — no walk lives in a
//! leaf (D3).
//!
//! `.gen.rs` regen: `framec-ng -l rust --emit arg_scan.frs | grep -v '^#!\[allow' >
//! arg_scan.gen.rs`.

use super::literals::Target;
use super::machine;
use super::opaque_scan::{opaque_at, OpaqueAt};
use crate::tree::body::{InstArg, ParamGroup};

/// Exact 2-byte prefix compare: a `$(` sigil at `i` (State group). Sigils are recognized
/// at an argument start ONLY — and exactly-prefixed, which is also what keeps `$ (x)` a
/// legal Java call (`$` is a Java identifier).
fn is_sigil_state(src: &[u8], i: usize, to: usize) -> bool {
    i + 1 < to && src[i] == b'$' && src[i + 1] == b'('
}
/// Exact 3-byte prefix compare: a `$>(` sigil at `i` (Enter group).
fn is_sigil_enter(src: &[u8], i: usize, to: usize) -> bool {
    i + 2 < to && src[i] == b'$' && src[i + 1] == b'>' && src[i + 2] == b'('
}
/// Skip a whole opaque region (comment/literal) at `i`, or `i` unchanged. REUSES the
/// existing `pub(crate)` [`machine::skip_opaque`] run-and-unwrap (design F4 resolution) —
/// byte-identical policy to delim_balance's leaf: a comment clamps to `limit`, a literal
/// that overruns `limit` is not consumed, unterminated/none → `i`. Only runs OpaqueScan —
/// no walk lives here (D3).
fn opaque_skip(src: &[u8], i: usize, limit: usize, target: Target) -> usize {
    machine::skip_opaque(src, i, limit, target).unwrap_or(i)
}
/// Does an opaque region OPEN at `i` but never close? Distinguished from `skip_opaque`'s
/// merged `None` because refusal 1 (UnterminatedOpaque) must tell them apart — the
/// delim_balance `opaque_unterminated` idiom.
fn opaque_unterm(src: &[u8], i: usize, target: Target) -> bool {
    matches!(opaque_at(src, i, target), OpaqueAt::Unterminated)
}
/// Operator-digraph guard for the byte at `i` (a `<` or `>`), O(1) byte compares
/// (`eq_guard_ok` precedent): `<=` never opens; `>=`, `->`, `=>` never close. Guarded
/// bytes are not counted — ordinary content under BOTH hypotheses.
fn angle_guard(src: &[u8], i: usize, from: usize, to: usize) -> bool {
    if src[i] == b'<' {
        return i + 1 < to && src[i + 1] == b'=';
    }
    (i + 1 < to && src[i + 1] == b'=') || (i > from && (src[i - 1] == b'-' || src[i - 1] == b'='))
}
/// The hand's exact `=` operator guard (parts.rs split_top_eq): prev byte not one of
/// `= ! < >`, next byte not `=` — so `==`, `!=`, `<=`, `>=` never split.
fn eq_guard_ok(src: &[u8], i: usize, from: usize, to: usize) -> bool {
    let prev_ok = i == from || !matches!(src[i - 1], b'=' | b'!' | b'<' | b'>');
    let next_ok = i + 1 >= to || src[i + 1] != b'=';
    prev_ok && next_ok
}
/// Is `[s, e)` a (nonempty) identifier? Bounded predicate over an already-delimited span
/// (`stmt_scan::has_pop` precedent). Load-bearing for L27/L28 (a named split requires an
/// identifier name) and for Lemma 3(i) (naming never diverges between the hypotheses).
fn is_ident_span(src: &[u8], s: usize, e: usize) -> bool {
    if s >= e {
        return false;
    }
    (src[s].is_ascii_alphabetic() || src[s] == b'_')
        && src[s + 1..e]
            .iter()
            .all(|&b| b.is_ascii_alphanumeric() || b == b'_')
}
/// Push one O-hypothesis arg record (`record_part` precedent). Normalizes `!has_val` to
/// the empty span `(cur, cur)` — an empty value is `vs == ve`, no sentinel.
#[allow(clippy::too_many_arguments)]
fn record_arg(
    args: &mut Vec<(i32, bool, usize, usize, usize, usize, bool)>,
    group: i32,
    has_name: bool,
    ns: usize,
    ne: usize,
    has_val: bool,
    vs: usize,
    ve: usize,
    cur: usize,
    g_end: bool,
) {
    if has_val {
        args.push((group, has_name, ns, ne, vs, ve, g_end));
    } else {
        args.push((group, has_name, ns, ne, cur, cur, g_end));
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
    use super::{
        angle_guard, eq_guard_ok, is_ident_span, is_sigil_enter, is_sigil_state, opaque_skip,
        opaque_unterm, record_arg, Target,
    };
    include!("arg_scan.gen.rs");
}

/// Why the tail of the interior was taken VERBATIM (the hand's silent swallow-to-end,
/// named). `None` on a well-formed list. Dropped by the production seam until the §1167
/// validation layer lands (recorded leave-latent).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refusal {
    None,
    UnterminatedOpaque,
    StrayCloser,
    TrailingAfterGroup,
    UnclosedGroup,
}

/// One reading of the argument list: materialized args + the hand's any-arg-named flag.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    pub args: Vec<InstArg>,
    pub named: bool,
}

/// The relationship between the two angle hypotheses on this list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AngleReading {
    /// No counted depth-0 angles, OR angles present but every boundary holds under both
    /// hypotheses (all `g_end`) — nothing to adjudicate. Also the refusal shape: a
    /// malformed list adjudicates nothing.
    Inert,
    /// Angles counted but hypothesis G is nonviable (a `>` with no open `<`, or an
    /// unclosed `<` at end): `primary` is the sole O reading.
    Operators,
    /// Both hypotheses viable, boundaries diverged: `primary` = G (brackets), payload = O
    /// (operators). Invariant: `primary.args.len() < alt.args.len()` (Lemma 3(ii)).
    Forked(Candidate),
}

/// The parse result. `primary` is the G candidate when `Forked`, the sole reading
/// otherwise. The refusal channel and `dropped_empty` are observability: the production
/// seam drops them today (hand-shaped verbatim degradation; void condition = §1167).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArgsOut {
    pub primary: Candidate,
    pub angles: AngleReading,
    pub refusal: Refusal,
    /// Comma-delimited empty segments ONLY (`a,,b`, leading `,`) — a ws tail after a
    /// trailing comma ends at end-of-interior, not at a comma, and is not counted.
    pub dropped_empty: i32,
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

/// Materialize the O candidate straight from the records (trimmed spans → lossy strings —
/// the file-wide value-fidelity policy, held deliberately).
fn candidate_o(bytes: &[u8], recs: &[(i32, bool, usize, usize, usize, usize, bool)]) -> Candidate {
    let mut named = false;
    let args = recs
        .iter()
        .map(|r| {
            let name = if r.1 {
                named = true;
                Some(lossy(bytes, r.2, r.3))
            } else {
                None
            };
            InstArg {
                group: group_of(r.0),
                name,
                value: lossy(bytes, r.4, r.5),
            }
        })
        .collect();
    Candidate { args, named }
}

/// The G candidate as a FOLD over the O records + RAW byte spans — never a second walk
/// (§11.2). G boundaries = recorded boundaries with `g_end` (Lemma 2). Each G-arg covers a
/// run of O-records `r_i..r_j`; it takes `group` and `name` from `r_i` (run-initial names
/// are valid under G — Lemma 3(i)) and its value from the RAW span `r_i.vs .. r_j.ve`, so
/// dropped-empty bytes, later-segment `name=` bytes, and separators ride along verbatim
/// (never reassembled from materialized strings). When `r_i` is a State/Enter group and
/// `j > i` (angle-straddling pathologies only), the value keeps the `)`-and-beyond bytes —
/// the same junk-surfaced-verbatim shape as refusal 3, adjudicated like everything else.
/// No byte-level decision is made here: it reads records and slices spans — a function,
/// not a machine.
fn merge_g(bytes: &[u8], recs: &[(i32, bool, usize, usize, usize, usize, bool)]) -> Candidate {
    let mut args = Vec::new();
    let mut named = false;
    let mut run_start: Option<usize> = None;
    for (idx, r) in recs.iter().enumerate() {
        let i = *run_start.get_or_insert(idx);
        // A run closes at a `g_end` record (or, defensively, at the final record — under
        // a viable G the final record always carries `g_end`).
        if r.6 || idx + 1 == recs.len() {
            let r0 = &recs[i];
            let name = if r0.1 {
                named = true;
                Some(lossy(bytes, r0.2, r0.3))
            } else {
                None
            };
            args.push(InstArg {
                group: group_of(r0.0),
                name,
                value: lossy(bytes, r0.4, r.5),
            });
            run_start = None;
        }
    }
    Candidate { args, named }
}

/// Parse the arg-list interior `[from, to)` by running the `ArgScan` system, then build
/// the fork per §11.2: refusal → `Inert` (fork suppressed — a malformed list adjudicates
/// nothing); no counted angles → `Inert`; G nonviable → `Operators` (the sole O reading);
/// every boundary shared → `Inert` (the hypotheses coincide); else → `Forked` with
/// primary = G, payload = O.
pub fn parse(bytes: &[u8], from: usize, to: usize, target: Target) -> ArgsOut {
    let mut m = fsm::ArgScan::over(bytes, target, from, to);
    m.scan_at(from);
    let refusal = match m.refusal {
        1 => Refusal::UnterminatedOpaque,
        2 => Refusal::StrayCloser,
        3 => Refusal::TrailingAfterGroup,
        4 => Refusal::UnclosedGroup,
        _ => Refusal::None,
    };
    let o = candidate_o(bytes, &m.args);
    let (primary, angles) = if refusal != Refusal::None || !m.angle_touched {
        (o, AngleReading::Inert)
    } else if !m.g_viable {
        (o, AngleReading::Operators)
    } else if m.args.iter().all(|r| r.6) {
        (o, AngleReading::Inert)
    } else {
        (merge_g(bytes, &m.args), AngleReading::Forked(o))
    };
    ArgsOut {
        primary,
        angles,
        refusal,
        dropped_empty: m.dropped_empty,
    }
}
