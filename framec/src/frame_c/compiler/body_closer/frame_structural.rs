//! Frame-structural body closer (used for the GraphViz pipeline).
//!
//! The other backends pair each language's `BodyCloser` with that
//! language's specific comment/string conventions — Rust skips `//`
//! and `/* */`, Python skips `#` and `'''`/`"""`, etc. The GraphViz
//! target, however, can be invoked on a `.frs` written for ANY
//! production target, so its body-closer has to be permissive about
//! the comment style the user picked.
//!
//! Concretely, this closer:
//! - Skips `//` line comments (Rust / Java / C-family / Go / JS /
//!   TS / Swift / Kotlin / PHP / Dart / GDScript style)
//! - Skips `#` line comments (Python / Ruby / Erlang style)
//! - Skips `/* ... */` block comments, including nested
//! - Skips `"..."` strings with `\`-escape handling
//! - Treats `'` as an ordinary byte rather than a char-literal
//!   opener — FRAMEC_BUGS #24's trigger
//!
//! Frame's outer structural syntax never depends on a character
//! literal having a closing `'`; users put `'` inside English-text
//! comments routinely (`// bar's note`). Not consuming a closing `'`
//! is safer than mis-treating English apostrophes as literal
//! openers and reading off the end of the buffer looking for the
//! match.

use super::{BodyCloser, CloseError, CloseErrorKind};

/// Attempt to scan a single-quoted char literal starting at `i`
/// (which must point at `'`). Returns `Some(end)` — position past
/// the closing `'` — if the next bytes match a recognized char-
/// literal shape; otherwise `None`, signaling that `'` should be
/// treated as an ordinary byte (English apostrophe, Rust lifetime,
/// or text-like content).
///
/// Recognized shapes (all of length ≤ 12 bytes total):
/// - `'X'`           — single non-`'`/`\` byte then `'`
/// - `'\X'`          — backslash escape (`\n`, `\t`, `\\`, `\'`, `\"`, etc.)
/// - `'\u{HHHHHH}'`  — Rust-style unicode escape (`\u{` then 1–6 hex,
///                    then `}` then `'`)
pub(super) fn scan_char_literal(bytes: &[u8], i: usize, end: usize) -> Option<usize> {
    if bytes[i] != b'\'' || i + 1 >= end {
        return None;
    }
    // Single non-special byte form: `'X'`
    if bytes[i + 1] != b'\\' && bytes[i + 1] != b'\'' && i + 2 < end && bytes[i + 2] == b'\'' {
        return Some(i + 3);
    }
    // Escape forms.
    if bytes[i + 1] == b'\\' && i + 2 < end {
        // Unicode escape: `'\u{...}'`
        if bytes[i + 2] == b'u' && i + 3 < end && bytes[i + 3] == b'{' {
            let mut j = i + 4;
            while j < end
                && (bytes[j].is_ascii_hexdigit() || bytes[j] == b'_')
                && j - (i + 4) < 6
            {
                j += 1;
            }
            if j < end && bytes[j] == b'}' && j + 1 < end && bytes[j + 1] == b'\'' {
                return Some(j + 2);
            }
            return None;
        }
        // Simple escape: `'\X'`
        if i + 3 < end && bytes[i + 3] == b'\'' {
            return Some(i + 4);
        }
    }
    None
}

pub struct BodyCloserFrameStructural;

impl BodyCloser for BodyCloserFrameStructural {
    fn close_byte(&mut self, bytes: &[u8], open_brace_index: usize) -> Result<usize, CloseError> {
        if open_brace_index >= bytes.len() || bytes[open_brace_index] != b'{' {
            return Err(CloseError {
                kind: CloseErrorKind::UnmatchedBraces,
                message: format!(
                    "expected `{{` at position {}, got byte {:?}",
                    open_brace_index,
                    bytes.get(open_brace_index)
                ),
            });
        }
        let mut i = open_brace_index + 1;
        let end = bytes.len();
        let mut depth: i32 = 1;
        while i < end {
            let b = bytes[i];
            // Block comment: /* ... */ (nested supported)
            if b == b'/' && i + 1 < end && bytes[i + 1] == b'*' {
                let mut j = i + 2;
                let mut nested: i32 = 1;
                while j < end && nested > 0 {
                    if j + 1 < end && bytes[j] == b'/' && bytes[j + 1] == b'*' {
                        nested += 1;
                        j += 2;
                    } else if j + 1 < end && bytes[j] == b'*' && bytes[j + 1] == b'/' {
                        nested -= 1;
                        j += 2;
                    } else {
                        j += 1;
                    }
                }
                if nested > 0 {
                    return Err(CloseError {
                        kind: CloseErrorKind::UnterminatedComment,
                        message: "unterminated /* */ block comment".to_string(),
                    });
                }
                i = j;
                continue;
            }
            // Line comment: // ... to end of line
            if b == b'/' && i + 1 < end && bytes[i + 1] == b'/' {
                let mut j = i + 2;
                while j < end && bytes[j] != b'\n' {
                    j += 1;
                }
                i = j;
                continue;
            }
            // Line comment: # ... to end of line. Frame source files
            // for Python-target / Ruby-target use this style.
            if b == b'#' {
                let mut j = i + 1;
                while j < end && bytes[j] != b'\n' {
                    j += 1;
                }
                i = j;
                continue;
            }
            // Double-quoted string with `\`-escape support.
            if b == b'"' {
                let mut j = i + 1;
                while j < end {
                    if bytes[j] == b'\\' && j + 1 < end {
                        j += 2;
                        continue;
                    }
                    if bytes[j] == b'"' {
                        j += 1;
                        break;
                    }
                    j += 1;
                }
                i = j;
                continue;
            }
            // Single-quoted char literal: `'c'`, `'\''`, `'"'`, etc.
            // FRAMEC_BUGS #25 sub-case A: without this branch, a `"`
            // inside a char literal (`'"'`) was misread as the start
            // of a string literal — the scanner then looked for a
            // closing `"` across the rest of the file and trashed
            // the brace-balance computation.
            //
            // Only treat `'` as a literal opener when the character
            // sequence ahead actually looks like a char literal:
            //   - `'X'`       (single non-`'`/`\` char)
            //   - `'\X'`      (backslash escape; X is escaped char)
            //   - `'\u{XX}'`  (Rust-style unicode escape — accept up
            //                 to 8 hex digits between `{` and `}`)
            //
            // Otherwise — `'` followed by alphanumeric text without
            // a closing `'` within a short window (e.g. English
            // `bar's`, `'static` lifetime) — leave `'` as an ordinary
            // byte. The bound (8) is generous enough for legitimate
            // char literals and tight enough that English apostrophes
            // don't accidentally consume a closing `'` somewhere
            // later in the line.
            if b == b'\'' && i + 1 < end {
                if let Some(after) = scan_char_literal(bytes, i, end) {
                    i = after;
                    continue;
                }
            }
            // Brace tracking.
            if b == b'{' {
                depth += 1;
                i += 1;
                continue;
            }
            if b == b'}' {
                depth -= 1;
                if depth == 0 {
                    return Ok(i);
                }
                i += 1;
                continue;
            }
            // Everything else (including `'` — see module docs) is
            // an ordinary byte.
            i += 1;
        }
        Err(CloseError {
            kind: CloseErrorKind::UnmatchedBraces,
            message: format!(
                "unmatched `{{` at position {} — depth still {} at end of input",
                open_brace_index, depth
            ),
        })
    }
}
