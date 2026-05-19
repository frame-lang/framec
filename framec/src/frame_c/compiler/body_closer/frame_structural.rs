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
