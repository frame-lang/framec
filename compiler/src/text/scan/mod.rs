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
    let mut i = src.content_start();
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
                parts: parts::native_parts(&lx, bytes, water_start, start),
            }));
        }
        let item = read_pragma(&lx, bytes, start)?;
        water_start = item.span().end;
        items.push(item);
    }
    if water_start < n {
        items.push(Item::Native(NativeItem {
            span: Span::new(water_start, n),
            parts: parts::native_parts(&lx, bytes, water_start, n),
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
    use crate::tree::Node;
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
    use crate::tree::Node;
    let lx = Lexer::new(bytes, target);
    match read_pragma(&lx, bytes, at) {
        Ok(item) => item.span().end,
        Err(_) => bytes.len(),
    }
}

/// Leaf for the `Segmenter` system: if a comment or literal starts at `i`, its end offset;
/// otherwise `i`. The per-target forms come from `target` — this is exactly the opaque-skip
/// the walk needs so a `@@` inside a string or comment is never mistaken for an item.
pub fn skip_opaque_at(bytes: &[u8], i: usize, target: Target) -> usize {
    let lx = Lexer::new(bytes, target);
    if let Ok(Some(end)) = lx.comment_at(i) {
        return end;
    }
    if let Ok(Some(l)) = lx.literal_at(i) {
        return l.span.end;
    }
    i
}

/// Read one `@@…` island starting at `at` (which points at the first `@`).
fn read_pragma(lx: &Lexer, bytes: &[u8], at: usize) -> Result<Item, SegmentError> {
    let after = at + 2;
    let word = read_word(bytes, after);
    let word_text = std::str::from_utf8(&bytes[after..word]).unwrap_or("");

    match word_text {
        "system" => {
            let (name, private, params, brace) = read_name_params_brace(bytes, word)?;
            let end = close_brace(lx, bytes, brace, &name)?;
            let span = Span::new(at, end);
            Ok(Item::System(SystemItem {
                span,
                name,
                sections: sections::sections(lx, bytes, span),
                params,
                private,
            }))
        }
        "fsm" => {
            let (name, _private, _params, brace) = read_name_params_brace(bytes, word)?;
            let end = close_brace(lx, bytes, brace, &name)?;
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
) -> Result<(String, bool, SystemParams, usize), SegmentError> {
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
    if first == "private" || first == "public" {
        let (ss, se, _) = read_word(bytes, after_first);
        if se > ss {
            private = first == "private";
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
        let mut d = 0i32;
        while i < bytes.len() {
            match bytes[i] {
                b'(' => d += 1,
                b')' => {
                    d -= 1;
                    if d == 0 {
                        break;
                    }
                }
                _ => {}
            }
            i += 1;
        }
        let inner = String::from_utf8_lossy(&bytes[open + 1..i]).into_owned();
        params = split_system_params(&inner);
        i += 1;
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
    Ok((name, private, params, j))
}

/// Split `$(a: T), $>(b: T), c: T = d` into the three groups. Sigil decides the group;
/// each param is `name : type = default` (type/default verbatim).
fn split_system_params(inner: &str) -> SystemParams {
    let mut out = SystemParams::default();
    // Top-level comma split (respecting nested parens/brackets/angles for wrapped types).
    let mut parts = Vec::new();
    let b = inner.as_bytes();
    let (mut start, mut depth) = (0usize, 0i32);
    for (k, &c) in b.iter().enumerate() {
        match c {
            b'(' | b'[' | b'<' | b'{' => depth += 1,
            b')' | b']' | b'>' | b'}' => depth -= 1,
            b',' if depth == 0 => {
                parts.push(inner[start..k].trim());
                start = k + 1;
            }
            _ => {}
        }
    }
    if start < inner.len() {
        parts.push(inner[start..].trim());
    }
    for raw in parts {
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

fn read_name_then_brace(bytes: &[u8], mut i: usize) -> Result<(String, usize), SegmentError> {
    while i < bytes.len() && (bytes[i] == b' ' || bytes[i] == b'\t') {
        i += 1;
    }
    let ns = i;
    let ne = read_word(bytes, i);
    let name = String::from_utf8_lossy(&bytes[ns..ne]).into_owned();
    let mut j = ne;
    while j < bytes.len() && bytes[j] != b'{' {
        j += 1;
    }
    if j >= bytes.len() {
        return Err(SegmentError::UnclosedBody {
            open: Span::new(ns, bytes.len()),
            name,
        });
    }
    Ok((name, j))
}

/// Find the `}` matching the `{` at `open` — **literal- and comment-aware**.
///
/// This is the whole of #219 in one function. The old compiler had fifteen separate
/// brace-counters, each of which had learned a different subset of its own language's
/// literals, so a `}` inside a Ruby heredoc / a JS regex / a Lua long string closed a
/// block that was never open. Here there is one counter, and it asks the *same lexer*
/// that everything else asks.
fn close_brace(lx: &Lexer, bytes: &[u8], open: usize, name: &str) -> Result<usize, SegmentError> {
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
