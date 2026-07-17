//! Balanced-`{}` extent recognizer, **dogfooded as an `@@[scan(u8)]` system**
//! ([`brace_balance.frs`]).
//!
//! A counter automaton: from a `{` at position `i` it finds the matching `}` and returns the
//! offset one past it. Used by [`super::string_scan`] to skip a Python interpolation hole
//! (`{…}`) whole. Not string-aware — it counts raw braces, matching the hand
//! `Lexer::hole_at`, which the whole StringScan conversion replaces.
//!
//! `.gen.rs` regen: edit `brace_balance.frs`, then
//! `framec-ng -l rust --emit brace_balance.frs | grep -v '^#!\[allow' > brace_balance.gen.rs`.

mod fsm {
    #![allow(
        dead_code,
        unused_parens,
        non_snake_case,
        unused_variables,
        unused_mut,
        unused_imports
    )]
    include!("brace_balance.gen.rs");
}

/// From a `{` at `bytes[i]`, return the offset one past its matching `}`, or `None` if it is
/// never closed. The machine finds the extent; this wrapper only runs it and reads `cursor`.
pub fn scan(bytes: &[u8], i: usize) -> Option<usize> {
    let mut m = fsm::BraceBalance::over(bytes);
    if m.scan_at(i) {
        Some(m.cursor)
    } else {
        None
    }
}

// SCAFFOLDING (conversion-internal): a white-box unit battery for the `BraceBalance`
// counter-automaton system. Every expected value is HAND-COMPUTED from the counting
// rule (`{` = +1, `}` = -1, accept one past the `}` that returns depth to exactly 0
// having been positive) — never read back from the machine. It covers the forms it
// recognizes (balanced, nested, deeply nested), the edges (empty, EOF, past-EOF,
// nonzero offset), and the adversarial long tail (unbalanced both ways, leading `}`,
// trailing content, and — load-bearing — the DELIBERATELY not-string-aware behaviour
// (R6): a `}` inside a `"…"` still closes, because this counter matches the hand
// `Lexer::hole_at`, which is likewise string-blind. Locking that here means Item 4's
// string-aware hole work is a visible CHANGE, not a silent drift.
// PROMOTABLE-LATER: language-agnostic input→extent spec of the machine; harvestable as
// an `@@[scan]` fixture once shipping supports `@@[scan(u8)]`-on-`@@system` (#209).
#[cfg(test)]
mod tests {
    use super::scan;

    #[test]
    fn matches_the_closing_brace_one_past() {
        assert_eq!(scan(b"{ab}", 0), Some(4)); // one past the `}`
        assert_eq!(scan(b"{}", 0), Some(2)); // empty pair
        assert_eq!(scan(b"{a{b}c}xy", 0), Some(7)); // nested; trailing `xy` untouched
        assert_eq!(scan(b"{{{}}}", 0), Some(6)); // three deep
        assert_eq!(scan(b"{a}bcd", 0), Some(3)); // closes early, trailing content ignored
    }

    #[test]
    fn nonzero_offset_and_non_brace_prefix() {
        // Production skips a Python hole from the `{`; but the raw machine also tolerates
        // a non-`{` prefix, accepting at the `}` that first returns depth to 0.
        assert_eq!(scan(b"x{y}", 1), Some(4)); // start ON the `{`
        assert_eq!(scan(b"a{b}", 0), Some(4)); // start BEFORE the `{`: `a` is depth-0 filler
    }

    #[test]
    fn not_string_aware_by_design() {
        // R6: a `}` inside what looks like a string still closes the count — the machine
        // counts RAW braces, matching the string-blind hand `hole_at`. (This is correct
        // for Item 1's skip-only use; Item 4 must change it, and this test will flip.)
        assert_eq!(scan(b"{\"}\"}", 0), Some(3)); // closes at the FIRST `}`, inside the "…"
        assert_eq!(scan(b"{a\\}b}", 0), Some(4)); // a `\` does NOT escape the `}` here
    }

    #[test]
    fn unclosed_is_none() {
        assert_eq!(scan(b"{ab", 0), None); // never closes
        assert_eq!(scan(b"{a{b}", 0), None); // inner closes, outer does not
        assert_eq!(scan(b"{", 0), None); // lone opener
        assert_eq!(scan(b"{{{}}", 0), None); // one short of balance
    }

    #[test]
    fn no_open_is_none() {
        assert_eq!(scan(b"", 0), None); // empty
        assert_eq!(scan(b"abc", 0), None); // no braces at all
        assert_eq!(scan(b"}{}", 0), None); // leading `}` drives depth negative; never nets to 0-from-1
        assert_eq!(scan(b"ab}", 0), None); // a `}` with no prior `{`
    }

    #[test]
    fn eof_and_past_eof() {
        assert_eq!(scan(b"{ab}", 4), None); // start AT end-of-input
        assert_eq!(scan(b"{ab}", 99), None); // start past end-of-input
    }
}
