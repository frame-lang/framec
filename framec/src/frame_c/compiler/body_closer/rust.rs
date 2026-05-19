// Body closer for Rust language — Frame-generated state machine.
//
// Source: rust_lang.frs (Frame specification)
// Generated: rust_lang.gen.rs (via framec --target rust)
// This file: glue module wiring generated FSM to BodyCloser trait
//
// To regenerate:
//   ./target/release/framec framec/src/frame_c/compiler/body_closer/rust_lang.frs -l rust > framec/src/frame_c/compiler/body_closer/rust_lang.gen.rs

#![allow(unreachable_patterns)]
#![allow(unused_mut)]
#![allow(dead_code)]
#![allow(non_snake_case)]
#![allow(unused_variables)]

include!("rust_lang.gen.rs");

use super::{BodyCloser, CloseError, CloseErrorKind};

pub struct BodyCloserRust;

impl BodyCloser for BodyCloserRust {
    fn close_byte(&mut self, bytes: &[u8], open_brace_index: usize) -> Result<usize, CloseError> {
        let mut fsm = RustBodyCloserFsm::new();
        fsm.bytes = bytes.to_vec();
        fsm.pos = open_brace_index + 1;
        fsm.depth = 1;
        fsm.scan();
        match fsm.error_kind {
            0 => Ok(fsm.result_pos),
            1 => Err(CloseError {
                kind: CloseErrorKind::UnterminatedString,
                message: fsm.error_msg,
            }),
            2 => Err(CloseError {
                kind: CloseErrorKind::UnterminatedComment,
                message: fsm.error_msg,
            }),
            4 => Err(CloseError {
                kind: CloseErrorKind::UnterminatedRawString,
                message: fsm.error_msg,
            }),
            _ => Err(CloseError {
                kind: CloseErrorKind::UnmatchedBraces,
                message: fsm.error_msg,
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(src: &str) -> Result<usize, CloseError> {
        let bytes = src.as_bytes();
        let open = bytes.iter().position(|&b| b == b'{').expect("test source must contain {");
        BodyCloserRust.close_byte(bytes, open)
    }

    #[test]
    fn char_literal_still_recognised() {
        let src = "{ let c = 'a'; let d = '\\n'; }";
        let close_idx = close(src).expect("char literal body should close");
        assert_eq!(src.as_bytes()[close_idx], b'}');
    }

    #[test]
    fn label_does_not_enter_char_literal_mode() {
        // Regression for framec bug #27: a Rust labeled-block
        // (`'label: { ... break 'label X; ... }`) was being
        // miscounted because the apostrophe was treated as a
        // char-literal opener, consuming braces until a stray
        // `'` closed the (phantom) literal.
        let src = "{ let r = 'block: { if true { break 'block 1; } 2 }; }";
        let close_idx = close(src).expect("labeled block should close");
        assert_eq!(src.as_bytes()[close_idx], b'}');
        // The closing brace must be the last `}` in the source.
        assert_eq!(close_idx, src.len() - 1);
    }

    #[test]
    fn lifetime_does_not_enter_char_literal_mode() {
        let src = "{ let s: &'static str = \"hi\"; let _ = s; }";
        let close_idx = close(src).expect("'static lifetime should close");
        assert_eq!(src.as_bytes()[close_idx], b'}');
        assert_eq!(close_idx, src.len() - 1);
    }
}
