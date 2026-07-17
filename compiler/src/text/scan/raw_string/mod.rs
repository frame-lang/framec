//! Rust raw-string extent recognizer, **dogfooded as an `@@[scan(u8)]` counter automaton**
//! ([`raw_string.frs`]) — the `@@system` that replaces the hand `Lexer::rust_raw`.
//!
//! Recognizes `r"…"`, `r#"…"#`, `r##"…"##`, and the `b`/`br` variants at position `i`,
//! returning the offset one past the close, or `None` if there is no raw string there. The
//! machine counts the opening `#` and matches the same count of closing `#`; this wrapper only
//! runs it and reads `cursor`.
//!
//! `.gen.rs` regen: edit `raw_string.frs`, then
//! `framec-ng -l rust --emit raw_string.frs | grep -v '^#!\[allow' > raw_string.gen.rs`.

mod fsm {
    #![allow(
        dead_code,
        unused_parens,
        non_snake_case,
        unused_variables,
        unused_mut,
        unused_imports
    )]
    include!("raw_string.gen.rs");
}

/// A three-way classification of position `i`, exposing the machine's `unterminated` register:
/// not a raw string here, a raw string whose extent ends at the given offset, or a raw string
/// that OPENS here (past the opening `"`) but is never closed — the case the hand `rust_raw`
/// signals with `Err`. `opaque_scan` needs the third arm to reproduce that Err.
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum RawAt {
    NotRaw,
    Extent(usize),
    Unterminated,
}

/// Classify a possible raw string at `bytes[i]`. The machine finds the extent (and, on reject,
/// records whether it had opened); this wrapper only runs it and reads its registers.
pub fn scan_kind(bytes: &[u8], i: usize) -> RawAt {
    if i >= bytes.len() {
        return RawAt::NotRaw;
    }
    let mut m = fsm::RawString::over(bytes);
    if m.scan_at(i) {
        RawAt::Extent(m.cursor)
    } else if m.unterminated {
        RawAt::Unterminated
    } else {
        RawAt::NotRaw
    }
}

/// From a possible raw string at `bytes[i]`, return the offset one past its close, or `None`
/// (whether it was never a raw string or opened-but-unterminated). Extent-only adapter over
/// [`scan_kind`].
pub fn scan(bytes: &[u8], i: usize) -> Option<usize> {
    match scan_kind(bytes, i) {
        RawAt::Extent(end) => Some(end),
        RawAt::NotRaw | RawAt::Unterminated => None,
    }
}

// SCAFFOLDING (conversion-internal): a white-box unit battery for the `RawString`
// counter-automaton system. Every expected value below is HAND-COMPUTED from the raw-
// string grammar (optional `b`, then `r`, then N `#`, then `"`; close = `"` + exactly
// N `#`) — never read back from the machine. It exercises every form the system
// recognizes (0..3 hashes, `b`/`br` variants), the edges (empty, EOF, escape-at-EOF —
// raw strings have NO escapes), and the adversarial long tail (a `"` as content, a
// short hash-run close that is content, an unterminated open, non-`{` starts, nonzero
// offsets). PROMOTABLE-LATER: it is a language-agnostic behavioral spec of the machine
// (input→extent), harvestable as an `@@[scan]` fixture once shipping supports
// `@@[scan(u8)]`-on-`@@system` (RFC-0042.1/#209) — cleanroom-only today.
#[cfg(test)]
mod tests {
    use super::{scan, scan_kind, RawAt};

    #[test]
    fn zero_hash_raw() {
        assert_eq!(scan(b"r\"abc\"", 0), Some(6)); // r"abc"
        assert_eq!(scan(b"r\"\"", 0), Some(3)); // r"" — empty zero-hash
        assert_eq!(scan(b"br\"x\"", 0), Some(5)); // br"x"
        assert_eq!(scan(b"br\"\"", 0), Some(4)); // br"" — empty byte-raw
    }

    #[test]
    fn every_hash_count() {
        assert_eq!(scan(b"r#\"a\"#", 0), Some(6)); // one hash
        assert_eq!(scan(b"r##\"a\"##", 0), Some(8)); // two hashes
        assert_eq!(scan(b"r###\"a\"###", 0), Some(10)); // three hashes
        assert_eq!(scan(b"r#\"\"#", 0), Some(5)); // one hash, empty body
        assert_eq!(scan(b"br#\"a\"#", 0), Some(7)); // byte-raw + one hash
    }

    #[test]
    fn hashed_close_must_match_count() {
        // r#"a"b"# — the inner `"` is content (not followed by a `#`); close is `"#`.
        assert_eq!(scan(b"r#\"a\"b\"#", 0), Some(8));
        // r##"x"#y"## (11 bytes) — needs two closing hashes; the inner `"#` (one hash) is
        // content, so the close is the final `"##`.
        assert_eq!(scan(b"r##\"x\"#y\"##", 0), Some(11));
        // r#"a"## — the close needs ONE hash; it lands at index 6, the trailing `#`
        // (index 6) is NOT consumed (content-after). Extent is 6, not 7.
        assert_eq!(scan(b"r#\"a\"##", 0), Some(6));
    }

    #[test]
    fn no_escapes_in_a_raw_string() {
        // r"a\" — a raw string has NO escapes, so the `\` is content and the following
        // `"` CLOSES the string. Extent covers r " a \ " = 5 bytes.
        assert_eq!(scan(b"r\"a\\\"", 0), Some(5));
        // r"\" — backslash then a closing quote (still no escape): extent 4.
        assert_eq!(scan(b"r\"\\\"", 0), Some(4));
    }

    #[test]
    fn nonzero_offset() {
        // Production calls `scan` at arbitrary cursor positions, not just 0.
        assert_eq!(scan(b"= r\"x\"", 2), Some(6));
        assert_eq!(scan(b"x = r#\"y\"#", 4), Some(10));
    }

    #[test]
    fn not_a_raw_string() {
        assert_eq!(scan(b"read", 0), None); // identifier starting with r
        assert_eq!(scan(b"\"plain\"", 0), None); // a plain string is not raw
        assert_eq!(scan(b"b\"bytes\"", 0), None); // b"..." (byte string) is NOT raw (no `r`)
        assert_eq!(scan(b"r", 0), None); // lone `r` at EOF
        assert_eq!(scan(b"br", 0), None); // `br` at EOF (no `#`/`"`)
        assert_eq!(scan(b"r#", 0), None); // hashes but no opening quote before EOF
        assert_eq!(scan(b"r#x", 0), None); // a `#` not followed by `"` → not raw
    }

    #[test]
    fn unterminated_is_none() {
        assert_eq!(scan(b"r#\"unterminated", 0), None);
        assert_eq!(scan(b"r\"", 0), None); // opener, no body/close
        assert_eq!(scan(b"r\"abc", 0), None); // body but never closes
        assert_eq!(scan(b"r##\"x\"#", 0), None); // one closing hash, needs two → unterminated
        assert_eq!(scan(b"r\"\\", 0), None); // backslash-at-EOF: no closing quote → unterminated
    }

    #[test]
    fn empty_and_eof() {
        assert_eq!(scan(b"", 0), None); // empty input
        assert_eq!(scan(b"r\"x\"", 4), None); // start AT end-of-input
        assert_eq!(scan(b"r\"x\"", 99), None); // start past end-of-input
    }

    // ---- scan_kind: the THREE-way classification (NotRaw / Extent / Unterminated) ----
    // Every expected value below is HAND-COMPUTED from the grammar (optional `b`, then
    // `r`, then N `#`, then `"`; close = `"` + exactly N `#`; NO escapes inside). The
    // `unterminated` register is what `opaque_scan` reads to reproduce the hand
    // `rust_raw`'s `Err`, so pinning it directly (not just the `scan` adapter) matters.

    #[test]
    fn scan_kind_not_raw() {
        // An identifier that merely starts with `r`/`b`.
        assert_eq!(scan_kind(b"read", 0), RawAt::NotRaw);
        assert_eq!(scan_kind(b"bread", 0), RawAt::NotRaw);
        // A plain string / byte string is not raw (no `r` before the `"`).
        assert_eq!(scan_kind(b"\"plain\"", 0), RawAt::NotRaw);
        assert_eq!(scan_kind(b"b\"bytes\"", 0), RawAt::NotRaw);
        // Lone / truncated openers that never reach the `"`.
        assert_eq!(scan_kind(b"r", 0), RawAt::NotRaw); // lone `r` at EOF
        assert_eq!(scan_kind(b"br", 0), RawAt::NotRaw); // `br` at EOF
        assert_eq!(scan_kind(b"r#", 0), RawAt::NotRaw); // hash but no `"` before EOF
        assert_eq!(scan_kind(b"r#x", 0), RawAt::NotRaw); // `#` not followed by `"`
        assert_eq!(scan_kind(b"", 0), RawAt::NotRaw); // empty input
        assert_eq!(scan_kind(b"r\"x\"", 99), RawAt::NotRaw); // start past EOF
    }

    #[test]
    fn scan_kind_extent() {
        assert_eq!(scan_kind(b"r\"abc\"", 0), RawAt::Extent(6)); // zero-hash
        assert_eq!(scan_kind(b"r\"\"", 0), RawAt::Extent(3)); // empty zero-hash
        assert_eq!(scan_kind(b"r#\"a\"#", 0), RawAt::Extent(6)); // one hash
        assert_eq!(scan_kind(b"r##\"x\"##", 0), RawAt::Extent(8)); // two hashes
        assert_eq!(scan_kind(b"r###\"a\"###", 0), RawAt::Extent(10)); // three hashes
        assert_eq!(scan_kind(b"br\"x\"", 0), RawAt::Extent(5)); // byte-raw, zero-hash
        assert_eq!(scan_kind(b"br#\"a\"#", 0), RawAt::Extent(7)); // byte-raw, one hash
        // `r#"a"b"#` — the inner `"` (not followed by `#`) is content; close is `"#`.
        assert_eq!(scan_kind(b"r#\"a\"b\"#", 0), RawAt::Extent(8));
        // `r#"a"##` — close needs ONE `#`; the trailing `#` is content-after, extent 6.
        assert_eq!(scan_kind(b"r#\"a\"##", 0), RawAt::Extent(6));
        // No escapes: in `r"a\"` the `\` is content and the following `"` CLOSES it.
        assert_eq!(scan_kind(b"r\"a\\\"", 0), RawAt::Extent(5));
        // Nonzero offset (production scans at arbitrary cursors).
        assert_eq!(scan_kind(b"= r\"x\"", 2), RawAt::Extent(6));
    }

    #[test]
    fn scan_kind_unterminated() {
        // Opens past the `"` but never closes → the register the hand path set via `Err`.
        assert_eq!(scan_kind(b"r#\"unterminated", 0), RawAt::Unterminated);
        assert_eq!(scan_kind(b"r\"abc", 0), RawAt::Unterminated); // body, no close
        assert_eq!(scan_kind(b"r\"", 0), RawAt::Unterminated); // opener, no body/close
        // One closing hash where two are required → still open at EOF.
        assert_eq!(scan_kind(b"r##\"x\"#", 0), RawAt::Unterminated);
        // Backslash-at-EOF is content (no escapes) → no closing `"` → unterminated.
        assert_eq!(scan_kind(b"r\"\\", 0), RawAt::Unterminated);
        // Byte-raw variant, unterminated.
        assert_eq!(scan_kind(b"br#\"x", 0), RawAt::Unterminated);
    }
}
