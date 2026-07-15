//! Decompose a `@@system`'s interior into sections. **Partitions the system's span.**
//!
//! `@@system Name(params) {` … `interface:` … `machine:` … `domain:` … `}`
//!
//! Every byte of the system belongs to exactly one section — including the header,
//! the closing brace, and every blank line between sections. There is no "just
//! formatting" category, because a byte with no node is a byte some later pass will
//! have to *guess* about, and guessing is what we are here to delete.

use super::lex::Lexer;
use super::machine::{decl_section, machine_section};
use crate::tree::{HeaderSection, Section, TriviaNode};
use crate::Span;

/// The section keywords. Frame's own vocabulary — a closed set, and framec's to know.
const KEYWORDS: &[&str] = &["interface", "machine", "domain", "actions", "operations"];

/// Build the right section for keyword index `idx`.
///
/// `actions:` and `operations:` members have NATIVE bodies; `interface:` and `domain:`
/// members do not. That distinction is not cosmetic — it decides whether the member's
/// braces contain the user's code (which framec must decompose but never interpret) or
/// nothing at all.
fn build(idx: usize, lx: &Lexer, bytes: &[u8], span: Span, kw: Span) -> Section {
    match idx {
        0 => Section::Interface(decl_section(lx, bytes, span, kw, false)),
        1 => Section::Machine(machine_section(lx, bytes, span, kw)),
        2 => Section::Domain(decl_section(lx, bytes, span, kw, false)),
        3 => Section::Actions(decl_section(lx, bytes, span, kw, true)),
        4 => Section::Operations(decl_section(lx, bytes, span, kw, true)),
        _ => unreachable!("KEYWORDS has 5 entries"),
    }
}

/// Split `[open_brace+1, close_brace)` into sections; add the header and the close.
pub fn sections(lx: &Lexer, bytes: &[u8], sys: Span) -> Vec<Section> {
    let mut out = Vec::new();

    // The header runs from `@@system` up to and including the opening `{`.
    let mut i = sys.start;
    while i < sys.end && bytes[i] != b'{' {
        i += 1;
    }
    let body_start = (i + 1).min(sys.end);
    out.push(Section::Header(HeaderSection {
        span: Span::new(sys.start, body_start),
    }));

    // The system's closing `}` is the last byte of its span.
    let close_start = sys.end.saturating_sub(1);

    // Find each section keyword at the top level of the system body — i.e. at brace
    // depth 0 relative to the body, and NOT inside a string or comment. (A `machine:`
    // written inside a native string is not a section. The old compiler's fifteen
    // hand-written brace counters each knew a different subset of their language's
    // literals, which is exactly how a `}` inside a Ruby heredoc closed a block that
    // was never open — #219. Here there is one lexer and everyone asks it.)
    // **Production runs the dogfooded SectionScan system** (docs/JOURNAL.md); the hand
    // `section_keyword_starts` remains only as the differential-test oracle.
    let starts =
        super::section_scan::keyword_starts(bytes, body_start, close_start, lx.target());

    // Sections run from their keyword to the next keyword (or to the closing brace).
    let mut cursor = body_start;
    for (n, &(kw_start, kw_end, idx)) in starts.iter().enumerate() {
        if cursor < kw_start {
            out.push(Section::Trivia(TriviaNode {
                span: Span::new(cursor, kw_start),
            }));
        }
        let sec_end = starts.get(n + 1).map(|s| s.0).unwrap_or(close_start);
        out.push(build(
            idx,
            lx,
            bytes,
            Span::new(kw_start, sec_end),
            Span::new(kw_start, kw_end),
        ));
        cursor = sec_end;
    }

    if cursor < close_start {
        out.push(Section::Trivia(TriviaNode {
            span: Span::new(cursor, close_start),
        }));
    }
    out.push(Section::Close(TriviaNode {
        span: Span::new(close_start, sys.end),
    }));
    out
}

/// The section-keyword starts `(kw_start, kw_end, idx)` at brace depth 0 in a system body,
/// skipping strings/comments (via the one lexer, so a `machine:` inside a string is not a
/// section — #219). **The reference the dogfooded `SectionScan` system is proven against.**
pub fn section_keyword_starts(
    lx: &Lexer,
    bytes: &[u8],
    body_start: usize,
    close_start: usize,
) -> Vec<(usize, usize, usize)> {
    let mut starts: Vec<(usize, usize, usize)> = Vec::new();
    let mut j = body_start;
    let mut depth = 0i32;
    while j < close_start {
        if let Ok(Some(end)) = lx.comment_at(j) {
            j = end;
            continue;
        }
        if let Ok(Some(l)) = lx.literal_at(j) {
            j = l.span.end;
            continue;
        }
        match bytes[j] {
            b'{' => depth += 1,
            b'}' => depth -= 1,
            _ => {}
        }
        if depth == 0 && is_word_start(bytes, j) {
            for (idx, kw) in KEYWORDS.iter().enumerate() {
                let k = kw.as_bytes();
                if bytes.len() >= j + k.len() && &bytes[j..j + k.len()] == k {
                    let mut p = j + k.len();
                    while p < close_start && (bytes[p] == b' ' || bytes[p] == b'\t') {
                        p += 1;
                    }
                    if p < close_start && bytes[p] == b':' {
                        starts.push((j, p + 1, idx));
                    }
                }
            }
        }
        j += 1;
    }
    starts
}

fn is_word_start(bytes: &[u8], i: usize) -> bool {
    if i > 0 && (bytes[i - 1].is_ascii_alphanumeric() || bytes[i - 1] == b'_') {
        return false;
    }
    bytes
        .get(i)
        .map(|b| b.is_ascii_alphabetic())
        .unwrap_or(false)
}
