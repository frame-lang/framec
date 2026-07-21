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

