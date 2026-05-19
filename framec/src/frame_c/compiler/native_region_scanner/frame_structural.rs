//! Frame-structural skipper used for the GraphViz pipeline.
//!
//! FRAMEC_BUGS #24: a `.frs` written for a production target (often
//! Rust) can still be invoked with `framec -l graphviz` to produce
//! a state-diagram. The GraphViz pipeline used the Python skipper as
//! a "neutral default," but Python's skipper treats `'` as a string
//! opener — which is wrong for `// bar's note` style comments
//! (English apostrophe inside a Rust-flavor line comment). The
//! Python skipper missed the `//` (Python uses `#`), saw `'`, started
//! looking for the close, and trashed the brace-balance computation.
//!
//! This module supplies a permissive skipper appropriate for outer
//! Frame structure: it recognizes both `//` (Rust / C-family) and
//! `#` (Python / Ruby) line comments, `/* ... */` block comments, and
//! `"..."` strings. `'` is intentionally NOT treated as a string
//! opener — Frame's structural syntax never relies on char-literal
//! parsing at this level, and English apostrophes in comments are
//! the more common case.

use super::unified::SyntaxSkipper;
use super::{NativeRegionScanner, ScanError, ScanResult};
use crate::frame_c::compiler::body_closer::frame_structural::BodyCloserFrameStructural;
use crate::frame_c::compiler::body_closer::BodyCloser;

/// Frame-structural NativeRegionScanner used for the GraphViz
/// pipeline (and any other "target-agnostic" callers). Routes the
/// shared scanner over our permissive skipper so apostrophes in
/// comments and `"` characters inside `'...'` char literals don't
/// derail the body walk.
pub struct NativeRegionScannerFrameStructural;

impl NativeRegionScanner for NativeRegionScannerFrameStructural {
    fn scan(&mut self, bytes: &[u8], open_brace_index: usize) -> Result<ScanResult, ScanError> {
        super::unified::scan_native_regions(&FrameStructuralSkipper, bytes, open_brace_index)
    }
}

pub struct FrameStructuralSkipper;

impl SyntaxSkipper for FrameStructuralSkipper {
    fn body_closer(&self) -> Box<dyn BodyCloser> {
        Box::new(BodyCloserFrameStructural)
    }

    /// Skip `//` line comment, `/* … */` block comment, or `#`
    /// line comment if one starts at `i`. Returns the position just
    /// past the comment, or `None` if no comment starts here.
    fn skip_comment(&self, bytes: &[u8], i: usize, end: usize) -> Option<usize> {
        if i >= end {
            return None;
        }
        // `//` line comment
        if i + 1 < end && bytes[i] == b'/' && bytes[i + 1] == b'/' {
            let mut j = i + 2;
            while j < end && bytes[j] != b'\n' {
                j += 1;
            }
            return Some(j);
        }
        // `#` line comment
        if bytes[i] == b'#' {
            let mut j = i + 1;
            while j < end && bytes[j] != b'\n' {
                j += 1;
            }
            return Some(j);
        }
        // `/* … */` block comment (with nesting)
        if i + 1 < end && bytes[i] == b'/' && bytes[i + 1] == b'*' {
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
            return Some(j);
        }
        None
    }

    /// Skip a `"..."` double-quoted string with `\`-escape handling.
    /// `'` is intentionally NOT treated as a string opener — see the
    /// module-level note.
    fn skip_string(&self, bytes: &[u8], i: usize, end: usize) -> Option<usize> {
        if i >= end || bytes[i] != b'"' {
            return None;
        }
        let mut j = i + 1;
        while j < end {
            if bytes[j] == b'\\' && j + 1 < end {
                j += 2;
                continue;
            }
            if bytes[j] == b'"' {
                return Some(j + 1);
            }
            j += 1;
        }
        // Unterminated — caller can decide how to handle it; we
        // return the end so the rest of the buffer isn't re-scanned.
        Some(end)
    }

    fn find_line_end(&self, bytes: &[u8], start: usize, end: usize) -> usize {
        let mut j = start;
        while j < end {
            // Stop at unescaped newline.
            if bytes[j] == b'\n' {
                return j;
            }
            // Or at start of a comment.
            if self.skip_comment(bytes, j, end).is_some() {
                return j;
            }
            // Skip strings as opaque (so a `;` inside `"x;y"` doesn't
            // terminate). We accept the caller's `;`/`\n` as the
            // boundary marker.
            if let Some(after_str) = self.skip_string(bytes, j, end) {
                j = after_str;
                continue;
            }
            // Statement-end markers used by the various target
            // languages — Frame's outer scanner stops at any of them.
            if bytes[j] == b';' {
                return j;
            }
            j += 1;
        }
        end
    }

    fn balanced_paren_end(&self, bytes: &[u8], i: usize, end: usize) -> Option<usize> {
        if i >= end || bytes[i] != b'(' {
            return None;
        }
        let mut j = i + 1;
        let mut depth: i32 = 1;
        while j < end {
            if let Some(after) = self.skip_string(bytes, j, end) {
                j = after;
                continue;
            }
            if let Some(after) = self.skip_comment(bytes, j, end) {
                j = after;
                continue;
            }
            match bytes[j] {
                b'(' => {
                    depth += 1;
                    j += 1;
                }
                b')' => {
                    depth -= 1;
                    j += 1;
                    if depth == 0 {
                        return Some(j);
                    }
                }
                _ => j += 1,
            }
        }
        None
    }
}
