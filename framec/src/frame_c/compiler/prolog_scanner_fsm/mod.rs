//! Prolog scanner, dogfooded as an RFC-0042 `@@fsm` recognizer.
//!
//! [`prolog_scan.frs`] recognizes the same file-header prolog as the
//! hand-written [`super::prolog_scanner`]; this module wraps the generated
//! recognizer to return the same [`RegionSpan`], and a differential test
//! proves the two agree on a corpus of valid and invalid prologs.
//!
//! This is a dogfooding proof-of-concept: the hand-written `prolog_scanner`
//! remains the production scanner. It demonstrates expressing a genuinely
//! *regular* leaf recognizer (no depth counter, no token-stream output) as an
//! `@@fsm` — the narrow class of scanner that `@@fsm` is the right tool for.
//!
//! `.gen.rs` regen: edit `prolog_scan.frs`, run `framec -l rust`, rename the
//! output to `prolog_scan.gen.rs`, commit both (same workflow as the other
//! dogfooded scanners).

use super::native_region_scanner::RegionSpan;
use super::prolog_scanner::{PrologError, PrologErrorKind};

mod fsm {
    #![allow(dead_code, unused_parens, non_snake_case, unused_variables, unused_mut)]
    include!("prolog_scan.gen.rs");
}

/// Recognize the prolog with the `@@fsm` recognizer, returning the same
/// [`RegionSpan`] as [`super::prolog_scanner::PrologScanner::scan`] on success
/// (`start` = offset of `@@target`, `end` = newline/EOF offset).
///
/// Diagnostic granularity differs by design: the recognizer reports
/// accept/reject plus a single reject offset, so the three
/// [`PrologErrorKind`]s collapse to one here. The hand scanner remains the
/// source of typed errors; this wrapper exists for the differential test.
pub fn scan(bytes: &[u8]) -> Result<RegionSpan, PrologError> {
    // RFC-0042.1: the recognizer is generic over its input source, so it scans
    // the host's `&[u8]` directly (zero-copy); each byte is read as its code
    // point, so offsets stay byte-aligned.
    let m = fsm::PrologScan::new(bytes);
    if m.accepted {
        Ok(RegionSpan {
            start: m.line_start as usize,
            end: m.cursor,
        })
    } else {
        Err(PrologError {
            kind: PrologErrorKind::InvalidHead,
            message: "expected @@target prolog".into(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frame_c::compiler::prolog_scanner::PrologScanner;

    /// The `@@fsm` recognizer and the hand-written `PrologScanner` agree on
    /// `(accepted, span)` across a corpus of valid and invalid prologs.
    #[test]
    fn fsm_matches_hand_scanner() {
        let corpus: &[&[u8]] = &[
            // Accept.
            b"@@target rust",
            b"@@target rust\n",
            b"@@target  python_3\nmore lines",
            b"\t  @@target  go  \n",
            b"\xEF\xBB\xBF@@target c\n",     // BOM
            b"\xEF\xBB\xBF  @@target swift", // BOM + leading ws
            b"@@target\trust\n",             // tab separator
            // Reject.
            b"",                  // empty
            b"   ",               // all whitespace
            b"\n\n\t ",           // all whitespace + newlines
            b"x\n@@target rust",  // first non-ws is not '@'
            b"@@targ rust",       // wrong head
            b"@@targetx rust",    // glued head
            b"@@target",          // no separator / language
            b"@@target ",         // separator, no language
            b"@@target\n",        // no separator before newline
            b"@@target \nrust",   // language position is a newline
            b"@@target \rrust\n", // language position is a CR
        ];
        let hand = PrologScanner;
        for &input in corpus {
            let shown = String::from_utf8_lossy(input);
            let h = hand.scan(input);
            let f = scan(input);
            assert_eq!(
                h.is_ok(),
                f.is_ok(),
                "accept/reject mismatch on {:?}: hand={:?} fsm={:?}",
                shown,
                h.as_ref().map(|s| (s.start, s.end)),
                f.as_ref().map(|s| (s.start, s.end)),
            );
            if let (Ok(hs), Ok(fs)) = (&h, &f) {
                assert_eq!(
                    (hs.start, hs.end),
                    (fs.start, fs.end),
                    "span mismatch on {:?}",
                    shown,
                );
            }
        }
    }
}
