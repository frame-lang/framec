//! SEGMENT — the island grammar. **Pass [1].**
//!
//! Splits the file into ordered items: `Bom | Native | Pragma | System | Efsm`.
//! **Every byte is covered.** Spans only; nothing is interpreted.
//!
//! This module — and only this module — can see the source bytes
//! (`Source::open` is `pub(in crate::scan)`). Everything downstream holds spans
//! and opaque text.

pub mod lex;
pub mod literals;
pub mod machine;
pub mod parts;
pub mod sections;
/// The first scanner dogfooded as a Frame `@@[scan(u8)]` system (docs/JOURNAL.md).
pub mod string_scan;
/// Balanced-`()` extent, dogfooded as a Frame `@@[scan(u8)]` counter automaton.
pub mod paren_balance;
pub mod paramsplit;
/// Balanced-`{}` extent (Python holes), dogfooded as a Frame `@@[scan(u8)]` counter automaton.
pub mod brace_balance;
/// Rust raw-string extent, dogfooded as a Frame `@@[scan(u8)]` counter automaton.
pub mod raw_string;
/// Full string+comment skipper (per-target), dogfooded as a Frame `@@[scan(u8)]` system.
pub mod opaque_scan;
/// Opaque-aware balanced-delimiter extent, dogfooded as a Frame `@@[scan(u8)]` counter automaton.
pub mod delim_balance;
/// The machine-section state-start walk, dogfooded as a Frame `@@[scan(u8)]` system.
pub mod machine_walk;
/// The state-member start walk, dogfooded as a Frame `@@[scan(u8)]` system.
pub mod state_walk;
/// The handler-body statement start walk (+ brace depth), dogfooded as a Frame `@@[scan(u8)]` system.
pub mod body_walk;
/// The decl-section declaration-start walk, dogfooded as a Frame `@@[scan(u8)]` system.
pub mod decl_walk;
/// The decl-line reader (register transducer), dogfooded as a Frame `@@[scan(u8)]` system.
pub mod decl_read;
/// The state-head reader (total register transducer), dogfooded as a Frame `@@[scan(u8)]` system.
pub mod state_head_scan;
/// The handler-head reader (register transducer with refusal), dogfooded as a Frame `@@[scan(u8)]` system.
pub mod handler_head_scan;
/// Composition proof: a scan system that composes StringScan (docs/JOURNAL.md).
pub mod string_counter;
/// The item-level segmenter walk, dogfooded as a Frame @@[scan(u8)] system.
pub mod segmenter;
/// Frame-reference recognizer, dogfooded as a Frame @@[scan(u8)] system.
pub mod ref_scan;
/// System-instantiation recognizer, dogfooded as a Frame @@[scan(u8)] system.
pub mod inst_scan;
/// Embedded-system-call recognizer, dogfooded as a Frame @@[scan(u8)] system.
pub mod embed_scan;
/// The native-code island dispatch, dogfooded as a Frame @@[scan(u8)] system.
pub mod native_parts_scan;
/// The section backbone, dogfooded as a Frame @@[scan(u8)] system.
pub mod section_scan;
/// The statement-level classifier, dogfooded as a Frame @@[scan(u8)] system.
pub mod stmt_scan;
/// The instantiation arg-list parser (dual-counter angle fork), dogfooded as a Frame @@[scan(u8)] system.
pub mod arg_scan;
/// HSM parent-chain cycle detector, dogfooded as a plain @@system graph walker.
pub mod hsm_cycle;
pub mod reachability;
use super::{Source, Span};
use crate::tree::{
    BomItem, EfsmItem, FileAst, Item, NativeItem, Param, PragmaItem, SystemItem, SystemParams,
};
use lex::{LexError, Lexer};
use literals::Target;

#[derive(Debug)]
pub enum SegmentError {
    Lex(LexError),
    /// A `@@system`/`@@fsm` whose body never closes.
    UnclosedBody { open: Span, name: String },
    /// A `@@` we do not recognize.
    ///
    /// **This is a refusal, and it matters.** `@@` is *Frame's own namespace*. A
    /// malformed construct there is a Frame error, not native code — but the old
    /// compiler had no way to say so, so unrecognized `@@`/`$` forms fell through as
    /// water and were emitted verbatim into the target, where the target compiler
    /// (not framec) eventually complained. With no body tree, framec could not
    /// distinguish *"native code I must not interpret"* from *"Frame code I failed
    /// to parse"*.
    UnknownPragma { span: Span, text: String },
}

impl From<LexError> for SegmentError {
    fn from(e: LexError) -> Self {
        SegmentError::Lex(e)
    }
}

/// Pass [1]. Split the source into top-level items.
pub fn segment(src: &Source, target: Target) -> Result<FileAst, SegmentError> {
    let bytes = src.open(); // the one door
    let lx = Lexer::new(bytes, target);
    let n = bytes.len();

    let mut items: Vec<Item> = Vec::new();

    // The BOM is a node, not a special case. `unparse` reproduces it (so byte
    // coverage holds with no exception carved out); codegen ignores it (a BOM belongs
    // to the file it arrived in). The old compiler had neither behaviour: it saw
    // 0xEF where it expected '@', decided line 1 had no pragma, and silently
    // classified the entire `@@system` as native text (#214).
    let i = src.content_start();
    if i > 0 {
        items.push(Item::Bom(BomItem {
            span: Span::new(0, i),
        }));
    }

    // **Production drives the item boundaries with the dogfooded Segmenter system**
    // (docs/JOURNAL.md). It finds each top-level `@@`-at-start-of-line, skipping
    // strings/comments (a `@@` inside them is not a pragma — the #219 fix, now a Frame state
    // machine) and skipping each item's body. The water between items is decomposed by
    // `native_parts` (itself now running RefScan/InstScan/EmbedScan), and `read_pragma`
    // rebuilds each item — that node construction is transformation, legitimately native.
    let content_start = i;
    let starts = segmenter::item_starts(bytes, content_start, target);
    let mut water_start = content_start;
    for &start in &starts {
        if water_start < start {
            items.push(Item::Native(NativeItem {
                span: Span::new(water_start, start),
                parts: parts::native_parts(bytes, water_start, start, target),
            }));
        }
        let item = read_pragma(&lx, bytes, start)?;
        water_start = item.span().end;
        items.push(item);
    }
    if water_start < n {
        items.push(Item::Native(NativeItem {
            span: Span::new(water_start, n),
            parts: parts::native_parts(bytes, water_start, n, target),
        }));
    }

    let ast = FileAst {
        items,
        source_len: n,
    };

    // Both invariants, on every parse. A tree that fails these is a COMPILER BUG and
    // must not be handed to a later pass — that is how a silently-empty parse became
    // a silently-empty program.
    debug_assert!(
        ast.check_coverage().is_ok(),
        "coverage: {:?}",
        ast.check_coverage()
    );

    Ok(ast)
}

/// The hand item-boundary walk — kept ONLY as the differential-test oracle for the
/// dogfooded Segmenter system (production `segment` now runs the system). Returns the
/// top-level `@@`-item start offsets at or after `from`, skipping strings/comments and item
/// bodies.
pub fn hand_item_starts(bytes: &[u8], from: usize, target: Target) -> Vec<usize> {
    let lx = Lexer::new(bytes, target);
    let n = bytes.len();
    let mut starts = Vec::new();
    let mut i = from;
    let mut at_sol = true;
    while i < n {
        if let Ok(Some(end)) = lx.comment_at(i) {
            i = end;
            at_sol = false;
            continue;
        }
        if let Ok(Some(litr)) = lx.literal_at(i) {
            i = litr.span.end;
            at_sol = false;
            continue;
        }
        if at_sol {
            let mut j = i;
            while j < n && (bytes[j] == b' ' || bytes[j] == b'\t') {
                j += 1;
            }
            if j + 1 < n && bytes[j] == b'@' && bytes[j + 1] == b'@' {
                starts.push(j);
                i = match read_pragma(&lx, bytes, j) {
                    Ok(item) => item.span().end,
                    Err(_) => n,
                };
                at_sol = true;
                continue;
            }
        }
        at_sol = bytes[i] == b'\n';
        i += 1;
    }
    starts
}

/// Leaf for the `Segmenter` system: the end offset of the `@@…` item that starts at `at`
/// (past its closing brace, for a `@@system`/`@@fsm`; end of line for a pragma). Reuses the
/// hand `read_pragma` — item construction is transformation, legitimately native. On an
/// unclosed item, consume to end-of-input.
pub fn item_end_at(bytes: &[u8], at: usize, target: Target) -> usize {
    let lx = Lexer::new(bytes, target);
    match read_pragma(&lx, bytes, at) {
        Ok(item) => item.span().end,
        Err(_) => bytes.len(),
    }
}

/// Leaf for the `Segmenter` system: if a comment or literal starts at `i`, its end offset;
/// otherwise `i`. The per-target forms come from `target` — this is exactly the opaque-skip
/// the walk needs so a `@@` inside a string or comment is never mistaken for an item.
///
/// **Now the dogfooded `OpaqueScan` system** (`opaque_scan.frs`): the string/comment recognition
/// is a Frame `@@[scan(u8)]` machine, proven byte-for-byte identical to the retired hand lexer by
/// `tests/opaque_scan.rs` at every position. The old `comment_at`/`literal_at` funnel is gone.
pub fn skip_opaque_at(bytes: &[u8], i: usize, target: Target) -> usize {
    opaque_scan::opaque_extent(bytes, i, target).unwrap_or(i)
}

/// The retired hand implementation, kept ONLY as the differential-test oracle
/// (`tests/opaque_scan.rs`) until the parity is locked and the hand lexer recognition is
/// deleted. Not used in production.
#[doc(hidden)]
pub fn skip_opaque_at_hand(bytes: &[u8], i: usize, target: Target) -> usize {
    let lx = Lexer::new(bytes, target);
    if let Ok(Some(end)) = lx.comment_at(i) {
        return end;
    }
    if let Ok(Some(l)) = lx.literal_at(i) {
        return l.span.end;
    }
    i
}

/// The retired hand implementation of the FULL three-way classification — the differential-test
/// oracle for [`opaque_scan::opaque_at`] (`tests/opaque_scan.rs`). Not used in production. Maps
/// the hand `Lexer` verdicts to `OpaqueAt`: `comment_at` Ok(Some) → `Comment`, `literal_at`
/// Ok(Some) → `Literal`, an `Err` from either recognizer (an unterminated body) → `Unterminated`,
/// and Ok(None) from both → `None`. The dispatch order (comment before literal) mirrors the
/// machine's `$Start`.
#[doc(hidden)]
pub fn opaque_at_hand(bytes: &[u8], i: usize, target: Target) -> opaque_scan::OpaqueAt {
    use opaque_scan::OpaqueAt;
    let lx = Lexer::new(bytes, target);
    match lx.comment_at(i) {
        Ok(Some(end)) => return OpaqueAt::Comment(end),
        Err(_) => return OpaqueAt::Unterminated,
        Ok(None) => {}
    }
    match lx.literal_at(i) {
        Ok(Some(l)) => OpaqueAt::Literal(l.span.end),
        Err(_) => OpaqueAt::Unterminated,
        Ok(None) => OpaqueAt::None,
    }
}

/// Read one `@@…` island starting at `at` (which points at the first `@`).
fn read_pragma(lx: &Lexer, bytes: &[u8], at: usize) -> Result<Item, SegmentError> {
    let after = at + 2;
    let word = read_word(bytes, after);
    let word_text = std::str::from_utf8(&bytes[after..word]).unwrap_or("");

    match word_text {
        "system" => {
            let (name, private, public_keyword, params, brace) =
                read_name_params_brace(bytes, word)?;
            let end = close_brace(bytes, brace, &name, lx.target())?;
            let span = Span::new(at, end);
            Ok(Item::System(SystemItem {
                span,
                name,
                sections: sections::sections(lx, bytes, span),
                params,
                private,
                public_keyword,
            }))
        }
        "fsm" => {
            let (name, _private, _public, _params, brace) = read_name_params_brace(bytes, word)?;
            let end = close_brace(bytes, brace, &name, lx.target())?;
            Ok(Item::Efsm(EfsmItem {
                span: Span::new(at, end),
                name,
            }))
        }
        // `@@[attr]`, `@@import`, `@@:` … — a pragma line.
        _ => {
            let mut e = after;
            while e < bytes.len() && bytes[e] != b'\n' {
                e += 1;
            }
            if e < bytes.len() {
                e += 1; // include the newline
            }
            // `@@[async]` / `@@[persist]` / `@@[scan(u8)]` — the FULL bracket content, so
            // an argument (`scan(u8)`) survives for the resolver to split. A bare attr
            // (`async`) still comes through as just its name.
            let attr = if bytes.get(after) == Some(&b'[') {
                let ns = after + 1;
                let mut k = ns;
                while k < e && bytes[k] != b']' {
                    k += 1;
                }
                let content = String::from_utf8_lossy(&bytes[ns..k]).trim().to_string();
                if content.is_empty() {
                    None
                } else {
                    Some(content)
                }
            } else {
                None
            };
            Ok(Item::Pragma(PragmaItem {
                span: Span::new(at, e),
                attr,
            }))
        }
    }
}

fn read_word(bytes: &[u8], mut i: usize) -> usize {
    while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
        i += 1;
    }
    i
}

/// Read `Name (params)? (: bases)? {` — returning the name, the split header params, and
/// the opening-brace index. This is where system CONSTRUCTOR params come from (spec §203);
/// the old reader dropped them silently.
fn read_name_params_brace(
    bytes: &[u8],
    mut i: usize,
) -> Result<(String, bool, bool, SystemParams, usize), SegmentError> {
    let read_word = |bytes: &[u8], mut i: usize| -> (usize, usize, usize) {
        while i < bytes.len() && (bytes[i] == b' ' || bytes[i] == b'\t') {
            i += 1;
        }
        let s = i;
        while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
            i += 1;
        }
        (s, i, i)
    };
    // An optional visibility modifier — `@@system private Name` / `@@system public Name`. It
    // is a modifier only when ANOTHER identifier (the real name) follows; `@@system private {`
    // treats `private` as the name (an odd but unambiguous read). `public` is recognised here
    // so it is not mistaken for the name; its redundancy is diagnosed at resolve.
    let (fs, fe, after_first) = read_word(bytes, i);
    let first = String::from_utf8_lossy(&bytes[fs..fe]).into_owned();
    let mut private = false;
    let mut public_keyword = false;
    if first == "private" || first == "public" {
        let (ss, se, _) = read_word(bytes, after_first);
        if se > ss {
            private = first == "private";
            public_keyword = first == "public";
            i = ss;
        }
    }
    while i < bytes.len() && (bytes[i] == b' ' || bytes[i] == b'\t') {
        i += 1;
    }
    let ns = i;
    while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
        i += 1;
    }
    let name = String::from_utf8_lossy(&bytes[ns..i]).into_owned();
    while i < bytes.len() && (bytes[i] == b' ' || bytes[i] == b'\t') {
        i += 1;
    }
    // Optional `(params)`.
    let mut params = SystemParams::default();
    if i < bytes.len() && bytes[i] == b'(' {
        let open = i;
        // The matching `)` — via the ParenBalance @@system. Unlike the old naive `(`/`)`
        // counter this is string-aware: a `)` inside a "…"-string in a default value is
        // skipped, not counted. An unbalanced group runs off the end → UnclosedBody, exactly
        // as the hand loop did (it ran to EOF, then failed the skip-to-`{`).
        let Some(after_close) = paren_balance::scan(bytes, open) else {
            return Err(SegmentError::UnclosedBody {
                open: Span::new(ns, bytes.len()),
                name,
            });
        };
        let inner = String::from_utf8_lossy(&bytes[open + 1..after_close - 1]).into_owned();
        params = split_system_params(&inner);
        i = after_close;
    }
    // Skip an optional `: Base, Base` and anything up to `{`.
    let mut j = i;
    while j < bytes.len() && bytes[j] != b'{' {
        j += 1;
    }
    if j >= bytes.len() {
        return Err(SegmentError::UnclosedBody {
            open: Span::new(ns, bytes.len()),
            name,
        });
    }
    Ok((name, private, public_keyword, params, j))
}

/// Split `$(a: T), $>(b: T), c: T = d` into the three groups. Sigil decides the group;
/// each param is `name : type = default` (type/default verbatim).
fn split_system_params(inner: &str) -> SystemParams {
    let mut out = SystemParams::default();
    // Top-level comma-split extents via the ParamSplit @@system (string-aware — a `,` inside a
    // `"…"` default is not a separator; the old hand `(`/`)` depth loop was string-blind). The
    // per-part sigil parse below stays native.
    let b = inner.as_bytes();
    for (s, e) in paramsplit::split(b) {
        let raw = inner[s..e].trim();
        if raw.is_empty() {
            continue;
        }
        // Group sigil: `$>( … )` enter, `$( … )` state, else bare domain.
        let (group, body): (u8, &str) = if let Some(rest) = raw.strip_prefix("$>(") {
            (2, rest.trim_end_matches(')'))
        } else if let Some(rest) = raw.strip_prefix("$(") {
            (1, rest.trim_end_matches(')'))
        } else {
            (0, raw)
        };
        let param = parse_one_param(body);
        match group {
            1 => out.state.push(param),
            2 => out.enter.push(param),
            _ => out.domain.push(param),
        }
    }
    out
}

fn parse_one_param(body: &str) -> Param {
    // `name : type = default`
    let (lhs, default) = match body.split_once('=') {
        Some((l, d)) => (l.trim(), Some(d.trim().to_string())),
        None => (body.trim(), None),
    };
    let (name, ty) = match lhs.split_once(':') {
        Some((n, t)) => (n.trim().to_string(), Some(t.trim().to_string())),
        None => (lhs.to_string(), None),
    };
    Param { name, ty, default }
}

/// Find the `}` matching the `{` at `open` — **literal- and comment-aware**.
///
/// This is the whole of #219 in one function. The old compiler had fifteen separate
/// brace-counters, each of which had learned a different subset of its own language's
/// literals, so a `}` inside a Ruby heredoc / a JS regex / a Lua long string closed a
/// block that was never open. Here there is one counter — **the dogfooded `DelimBalance`
/// @@system** (`balanced_strict`), the same machine `machine.rs::balanced` uses, run with the
/// FAIL unterminated policy: an opaque body that OPENS but never closes makes the body malformed
/// (`None` → `UnclosedBody`), so a `}` buried in an unterminated string can never spuriously
/// close it. This discharges Item 2's BodyBalance residual onto DelimBalance. The
/// `close_brace_tests` module below locks the `is_err`/`Ok` parity against `close_brace_hand`.
fn close_brace(
    bytes: &[u8],
    open: usize,
    name: &str,
    target: Target,
) -> Result<usize, SegmentError> {
    match delim_balance::balanced_strict(bytes, open, bytes.len(), b'{', b'}', target) {
        Some(end) => Ok(end),
        None => Err(SegmentError::UnclosedBody {
            open: Span::new(open, bytes.len()),
            name: name.to_string(),
        }),
    }
}

// (differential unit tests for `close_brace` live in the `close_brace_tests` module at the
// bottom of this file — they must call the PRIVATE `close_brace`, so they cannot be an
// integration test in `compiler/tests/`.)

/// The retired hand `}`-matcher, kept ONLY as the differential-test oracle for `close_brace`
/// (the `close_brace_tests` module below) until the parity is locked and the hand lexer
/// recognition is deleted. Self-contained (builds its own `Lexer`); not used in production. Its `?` on an
/// unterminated body yields `Err(SegmentError::Lex(..))`; production maps the same situation to
/// `UnclosedBody` — the oracle asserts `is_err` parity, not variant equality (a better
/// diagnostic for the same refusal).
#[doc(hidden)]
pub fn close_brace_hand(
    bytes: &[u8],
    open: usize,
    name: &str,
    target: Target,
) -> Result<usize, SegmentError> {
    let lx = Lexer::new(bytes, target);
    let mut i = open;
    let mut depth = 0i32;
    while i < bytes.len() {
        if let Some(end) = lx.comment_at(i)? {
            i = end;
            continue;
        }
        if let Some(litr) = lx.literal_at(i)? {
            i = litr.span.end;
            continue;
        }
        match bytes[i] {
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return Ok(i + 1);
                }
            }
            _ => {}
        }
        i += 1;
    }
    Err(SegmentError::UnclosedBody {
        open: Span::new(open, bytes.len()),
        name: name.to_string(),
    })
}

// ============================================================================
// LEVEL-2: `close_brace` `}`-matcher parity — differential vs the retired hand matcher
// (`close_brace_hand`), the INDEPENDENT oracle. `close_brace` is a PRIVATE `fn`, so this
// is a unit-test module (option (b)) that calls it directly — it cannot be an integration
// test in `compiler/tests/`. At EVERY open position of every curated + fuzz input, across
// all four cleanroom targets, it asserts `is_err` parity and — when BOTH are `Ok` — extent
// equality. (Not variant equality: on an unterminated body production returns `UnclosedBody`
// while the hand `?` returns `Lex(..)`; both are refusals, so `is_err` is the contract.)
// The Unterminated→Err arm is driven by unterminated strings/comments/raw-strings; `}` bytes
// hidden inside strings/comments/raw-strings/triples/holes must NOT close the body.
// SCAFFOLDING: conversion-internal; needs the private `close_brace` + the hand oracle.
// ============================================================================
#[cfg(test)]
mod close_brace_tests {
    use super::literals::Target;

    const TARGETS: [Target; 4] = [Target::C, Target::Java, Target::Rust, Target::Python3];

    /// Machine (`close_brace`) vs the hand oracle (`close_brace_hand`) at a single open.
    fn agree_cb(bytes: &[u8], open: usize, target: Target) {
        let m = super::close_brace(bytes, open, "X", target);
        let h = super::close_brace_hand(bytes, open, "X", target);
        assert_eq!(
            m.is_err(),
            h.is_err(),
            "is_err parity broke: target {target:?}, open {open} of {bytes:?}: \
             machine={m:?} hand={h:?}"
        );
        if let (Ok(a), Ok(b)) = (&m, &h) {
            assert_eq!(
                a, b,
                "Ok extents differ: target {target:?}, open {open} of {bytes:?}"
            );
        }
    }

    /// Partition-aware variant (Δ1 fix-with-teeth): `close_brace` composes DelimBalance →
    /// OpaqueScan, whose hole delimitation is now string-AWARE, while `close_brace_hand` stays
    /// string-blind. So they agree (CARRIED) OR the machine diverges (a FIXED row — a closer
    /// hidden in a string inside a Python hole). On divergence the machine's matched close (if
    /// any) must still be a WELL-FORMED position past the opener; string-aware correctness is
    /// proven in `tests/opaque_scan.rs`. Returns true on a fixed row (for the teeth).
    fn agree_cb_or_fixed(bytes: &[u8], open: usize, target: Target) -> bool {
        let m = super::close_brace(bytes, open, "X", target);
        let h = super::close_brace_hand(bytes, open, "X", target);
        let diverged =
            m.is_err() != h.is_err() || matches!((&m, &h), (Ok(a), Ok(b)) if a != b);
        if diverged {
            if let Ok(x) = &m {
                assert!(
                    open < *x && *x <= bytes.len(),
                    "close_brace produced an INVALID extent on a Δ1 divergence: \
                     target {target:?}, open {open} of {bytes:?}: machine={m:?}"
                );
            }
        }
        diverged
    }

    /// Sweep EVERY position as the `open` — strictly stronger than only the real `{`
    /// offsets, and both functions receive the SAME open so the differential is exact.
    fn agree_all(src: &str, target: Target) {
        let b = src.as_bytes();
        for open in 0..b.len() {
            agree_cb(b, open, target);
        }
    }

    #[test]
    fn balanced_and_nested() {
        for &t in &TARGETS {
            agree_all("{}", t);
            agree_all("{ }", t);
            agree_all("{{{}}}", t); // deeply nested, all on one line
            agree_all("{ a { b } c }", t);
            agree_all("x = { one { two { three {} } } } y", t);
            agree_all("@@system X { -machine- state A {} state B {} }", t);
        }
    }

    #[test]
    fn brace_in_comment() {
        for &t in &[Target::C, Target::Java, Target::Rust] {
            agree_all("{ // } not a close\n }", t); // `}` in a line comment
            agree_all("{ /* } still open */ }", t); // `}` in a block comment
            agree_all("{ /* } */ /* } */ }", t); // two block comments each hiding a `}`
        }
        agree_all("{ # } in a python comment\n }", Target::Python3); // `}` in a `#` comment
    }

    #[test]
    fn brace_in_string() {
        for &t in &TARGETS {
            agree_all("{ \"a } b\" }", t); // `}` inside a `"` string
            agree_all("{ s = \"}}}}\"; }", t); // a run of `}` all inside a string
            agree_all("{ '}' }", t); // `}` inside a `'` (char/string) form
            agree_all("{ \"a \\\" } still in\" }", t); // escaped quote then a hidden `}`
        }
    }

    #[test]
    fn brace_in_rust_raw_and_python_triple() {
        // Rust raw strings — the `}` (and even a stray `"`) inside must not close.
        agree_all("{ r#\"a } b\"# }", Target::Rust);
        agree_all("{ let r = r\"}\"; }", Target::Rust);
        agree_all("{ br#\"}}}}\"# }", Target::Rust);
        agree_all("{ r##\"a\"# } still raw\"## }", Target::Rust); // `}` inside a 2-hash raw
        // Python triple strings and f-string holes.
        agree_all("{ x = \"\"\"a } b\"\"\" }", Target::Python3);
        agree_all("{ y = '''}''' }", Target::Python3);
        agree_all("{ f\"{a}\" }", Target::Python3); // an f-string hole `{a}` inside the body
        agree_all("{ f\"pre {b} post\" }", Target::Python3);
    }

    #[test]
    fn unterminated_bodies() {
        for &t in &TARGETS {
            agree_all("{ a b c", t); // never closes → Err (both)
            agree_all("{ x = \"unterminated string", t); // Unterminated literal → Err
        }
        for &t in &[Target::C, Target::Java, Target::Rust] {
            agree_all("{ /* unterminated comment", t); // Unterminated comment → Err
        }
        agree_all("{ \"\"\"unterminated triple", Target::Python3);
        agree_all("{ r#\"unterminated raw", Target::Rust);
        agree_all("{ s = \"line\nbreak\" }", Target::C); // newline-in-string → Unterminated → Err
    }

    // The `fail_unterm` policy is LOAD-BEARING, not a no-op. A `}` is buried inside an
    // UNTERMINATED string: the two DelimBalance policies must DIVERGE on it, and close_brace must
    // pick the safe one. Without this, `unterminated_bodies` (whose bodies hide no closer) would
    // pass even if `fail_unterm` were wired as a no-op — both policies return Err when there is
    // no buried `}`. Here there IS one, so:
    //   · TOLERATE (`balanced`) treats the never-closed `"` as bytes → finds the buried `}` → Some
    //   · FAIL (`balanced_strict`, used by `close_brace`) rejects the malformed body → None → Err
    // The oracle (`close_brace_hand`, hand `Lexer` with `?`) also Errs, so the flag is proven to
    // change the outcome in the direction close_brace requires.
    #[test]
    fn fail_unterm_policy_is_load_bearing() {
        // `{ "unterminated } ` — a `}` at index 16, inside a string with no closing quote.
        let src = b"{ \"unterminated } ";
        for &t in &TARGETS {
            // FAIL policy (production close_brace) + its hand oracle both reject.
            assert!(
                super::close_brace(src, 0, "X", t).is_err(),
                "close_brace (FAIL) must reject a `}}` buried in an unterminated string ({t:?})"
            );
            assert!(
                super::close_brace_hand(src, 0, "X", t).is_err(),
                "close_brace_hand oracle must reject too ({t:?})"
            );
            // TOLERATE policy finds the buried closer — proving the policies genuinely diverge
            // here, i.e. `fail_unterm` is what makes close_brace safe (not a no-op).
            assert_eq!(
                super::delim_balance::balanced(src, 0, src.len(), b'{', b'}', t),
                Some(17),
                "TOLERATE (balanced) must find the `}}` buried in the unterminated string ({t:?})"
            );
        }
    }

    // ---- Deterministic xorshift fuzz: `{` + random literal-long-tail body, swept. ----

    struct Rng(u64);
    impl Rng {
        fn new(seed: u64) -> Rng {
            let mut s = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15).wrapping_add(0x1234_5678);
            if s == 0 {
                s = 0xDEAD_BEEF;
            }
            Rng(s)
        }
        fn next_u64(&mut self) -> u64 {
            let mut x = self.0;
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            self.0 = x;
            x.wrapping_mul(0x2545_F491_4F6C_DD1D)
        }
        fn below(&mut self, n: usize) -> usize {
            (self.next_u64() % (n as u64)) as usize
        }
    }

    // Fragments biased toward the constructs that make `}`-matching hard: nested braces,
    // every comment/string/raw/triple opener+closer, and escapes.
    const FRAGS: &[&[u8]] = &[
        b"{", b"}", b"{{", b"}}", b"//", b"/*", b"*/", b"#", b"\"", b"'", b"\"\"\"", b"'''",
        b"r\"", b"r#\"", b"\"#", b"\\", b"\\\"", b"\n", b" ", b"a", b";", b"=", b"@@", b"br#\"",
    ];

    fn gen_body(rng: &mut Rng, max_frags: usize) -> Vec<u8> {
        let n = rng.below(max_frags + 1);
        let mut v = vec![b'{']; // the body always OPENS with a real `{` at index 0
        for _ in 0..n {
            v.extend_from_slice(FRAGS[rng.below(FRAGS.len())]);
        }
        v
    }

    #[test]
    fn fuzz_close_brace_all_targets() {
        let mut fixed = 0usize;
        for &t in &TARGETS {
            for seed in 0u64..1500 {
                let mut rng = Rng::new(seed ^ 0xC10B_E5A5);
                let b = gen_body(&mut rng, 10);
                for open in 0..b.len() {
                    // Partition-aware (Δ1): machine == hand (carried) OR a string-aware-hole
                    // FIXED row (well-formedness checked inside) — never a silent regression.
                    fixed += agree_cb_or_fixed(&b, open, t) as usize;
                }
            }
        }
        // Δ1 fix-with-teeth: the fuzz must actually reach the string-aware-hole FIXED class
        // (machine != string-blind hand), or the partition-aware differential is vacuous.
        assert!(
            fixed > 0,
            "fuzz never reached a Δ1 string-aware-hole divergence — the partition arm is vacuous"
        );
    }

    /// A fuzz arm that only ever produced `Err` (or only `Ok`) would test nothing. Prove the
    /// generated bodies reach BOTH a real matched close (`Ok`) and a refusal (`Err`,
    /// unterminated) many times, from `open == 0` (a guaranteed real `{`).
    #[test]
    fn fuzz_close_brace_has_teeth() {
        let mut oks = 0usize;
        let mut errs = 0usize;
        for &t in &TARGETS {
            for seed in 0u64..1500 {
                let mut rng = Rng::new(seed ^ 0xC10B_E5A5);
                let b = gen_body(&mut rng, 10);
                match super::close_brace(&b, 0, "X", t) {
                    Ok(_) => oks += 1,
                    Err(_) => errs += 1,
                }
            }
        }
        assert!(oks > 50, "too few Ok (matched close) results ({oks}) — fuzz lacks teeth");
        assert!(errs > 50, "too few Err (unterminated) results ({errs}) — fuzz lacks teeth");
    }
}
