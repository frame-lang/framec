
// ExprScanner — PDA (pushdown automaton) for scanning a balanced region.
//
// Scans from `pos` to a terminator at depth 0, respecting nested `()[]{}` and
// string literals with escape handling. The terminator set is configurable via
// the `stop_*` flags (defaults preserve the original RHS-expression behavior:
// `;` inclusive / `\n` exclusive):
//
//   stop_semicolon (default true)  — `;` at depth 0 terminates, included.
//   stop_newline   (default true)  — `\n` at depth 0 terminates, excluded.
//   stop_comma     (default false) — `,` at depth 0 terminates, excluded.
//   stop_close_paren (default false) — `)` at depth 0 terminates, excluded.
//
// This is the single balanced-delimiter scan primitive: the assignment-RHS
// consumers (domain `= init`, `@@:return = expr`, `$.var = expr`) take the
// default `;`/`\n` terminators; `call_args` takes the `,` (arg split) and `)`
// (matching-close) terminators. Replaces 3 inline scanners in unified.rs plus
// the two hand-rolled paren/comma scanners in pipeline_parser/call_args.rs.

#[allow(dead_code)]
#[allow(non_camel_case_types)]
#[allow(non_snake_case)]
#[allow(unused_variables)]
#[allow(unused_mut)]
#[allow(unused_imports)]
#[allow(clippy::assign_op_pattern)]
#[allow(clippy::clone_on_copy)]
#[allow(clippy::derivable_impls)]
#[allow(clippy::match_single_binding)]
#[allow(clippy::needless_return)]
#[allow(clippy::new_without_default)]
#[allow(clippy::single_match)]
mod _expr_scanner_fsm_framec {
    use super::*;
    extern crate alloc;
    use alloc::{vec, format};
    #[derive(Clone, Debug)]
    #[allow(dead_code, non_camel_case_types)]
    enum ExprScannerFsmFrameEvent {
        DoScan {  },
        FrameEnter {},
        FrameExit {},
    }

    #[derive(Clone)]
    #[allow(dead_code, non_camel_case_types)]
    enum ExprScannerFsmFrameReturn {
        _Lifecycle(alloc::rc::Rc<dyn core::any::Any>),
    }

    #[allow(dead_code)]
    impl ExprScannerFsmFrameEvent {
        fn name(&self) -> &'static str {
            match self {
                ExprScannerFsmFrameEvent::DoScan { .. } => "do_scan",
                ExprScannerFsmFrameEvent::FrameEnter { .. } => "$>",
                ExprScannerFsmFrameEvent::FrameExit { .. } => "<$",
            }
        }
    }

    #[derive(Clone, Debug)]
    #[allow(dead_code, non_camel_case_types)]
    enum ExprScannerFsmFrameValue {
        Int(i64),
        Float(f64),
        Bool(bool),
        Str(String),
        List(Vec<Self>),
        Dict(alloc::collections::BTreeMap<String, Self>),
    }

    #[allow(dead_code, non_camel_case_types)]
    struct ExprScannerFsmFrameContext {
        event: alloc::rc::Rc<ExprScannerFsmFrameEvent>,
        _return: Option<ExprScannerFsmFrameReturn>,
        _data: alloc::collections::BTreeMap<String, ExprScannerFsmFrameValue>,
        _transitioned: bool,
    }

    impl ExprScannerFsmFrameContext {
        fn new(event: alloc::rc::Rc<ExprScannerFsmFrameEvent>, default_return: Option<ExprScannerFsmFrameReturn>) -> Self {
            Self {
                event,
                _return: default_return,
                _data: alloc::collections::BTreeMap::new(),
                _transitioned: false,
            }
        }
    }

    #[allow(dead_code, non_camel_case_types)]
    #[derive(Clone)]
    enum ExprScannerFsmStateContext {
        Init,
        Scanning,
        __NoContext,
    }

    impl Default for ExprScannerFsmStateContext {
        fn default() -> Self {
            ExprScannerFsmStateContext::Init
        }
    }

    #[allow(dead_code, non_camel_case_types)]
    #[derive(Clone)]
    struct ExprScannerFsmCompartment {
        state: String,
        state_context: ExprScannerFsmStateContext,
        forward_event: Option<ExprScannerFsmFrameEvent>,
        parent_compartment: Option<Box<ExprScannerFsmCompartment>>,
    }

    impl ExprScannerFsmCompartment {
        fn new(state: &str) -> Self {
            let state_context = match state {
                "Init" => ExprScannerFsmStateContext::Init,
                "Scanning" => ExprScannerFsmStateContext::Scanning,
                _ => ExprScannerFsmStateContext::__NoContext,
            };
            Self {
                state: state.to_string(),
                state_context,
                forward_event: None,
                parent_compartment: None,
            }
        }
    }

    #[allow(dead_code)]
    pub struct ExprScannerFsm {
        _state_stack: Vec<ExprScannerFsmCompartment>,
        __compartment: ExprScannerFsmCompartment,
        __next_compartment: Option<ExprScannerFsmCompartment>,
        _context_stack: Vec<ExprScannerFsmFrameContext>,
        pub bytes: Vec<u8>,
        pub pos: usize,
        pub end: usize,
        pub result_end: usize,
        // Configurable terminators. Defaults preserve the original
        // assignment-RHS behavior (`;` inclusive / `\n` exclusive).
        pub stop_semicolon: bool,
        pub stop_newline: bool,
        pub stop_comma: bool,
        pub stop_close_paren: bool,
    }

    #[allow(non_snake_case)]
    impl ExprScannerFsm {
        pub fn new() -> Self {
            Self {
                _state_stack: Vec::new(),
                _context_stack: Vec::new(),
                bytes: Vec::new(),
                pos: 0,
                end: 0,
                result_end: 0,
                stop_semicolon: true,
                stop_newline: true,
                stop_comma: false,
                stop_close_paren: false,
                __compartment: ExprScannerFsmCompartment::new("Init"),
                __next_compartment: None,
            }
        }

        pub fn __create() -> Self {
            let mut c = Self::new();
            c.__compartment = c.__prepareEnter("Init");
            let __e = alloc::rc::Rc::new(ExprScannerFsmFrameEvent::FrameEnter {});
            let __ctx = ExprScannerFsmFrameContext::new(alloc::rc::Rc::clone(&__e), None);
            c._context_stack.push(__ctx);
            c.__kernel(&__e);
            c._context_stack.pop();
            c
        }

        fn __hsm_chain(&mut self, leaf: &str) -> &'static [&'static str] {
            match leaf {
                "Init" => &["Init"],
                "Scanning" => &["Scanning"],
                _ => &[],
            }
        }

        fn __prepareEnter(&mut self, leaf: &str) -> ExprScannerFsmCompartment {
            let chain = self.__hsm_chain(leaf);
            let mut comp: Option<ExprScannerFsmCompartment> = None;
            for name in chain.iter() {
                let mut new_comp = ExprScannerFsmCompartment::new(name);
                if let Some(parent) = comp.take() {
                    new_comp.parent_compartment = Some(Box::new(parent));
                }
                comp = Some(new_comp);
            }
            comp.expect("chain must contain at least the leaf state")
        }

        fn __kernel(&mut self, __e: &alloc::rc::Rc<ExprScannerFsmFrameEvent>) {
            // Route event to current state.
            self.__router(__e);
            // Drain any transitions queued by the handler.
            while self.__next_compartment.is_some() {
                let next_compartment = self.__next_compartment.take().expect("invariant: while-loop guard checked is_some()");
                // Exit the current (leaf) state. RFC-0025.1: exit args live in the
                // source state's typed ctx (written at the transition site), so the
                // synthesized `<$` event carries no payload.
                let exit_event = alloc::rc::Rc::new(ExprScannerFsmFrameEvent::FrameExit {});
                self.__router(&exit_event);
                // Switch to the new compartment.
                self.__compartment = next_compartment;
                // Three-branch forward-event handling (RFC-0025 Track B.1: forward
                // event is matched on enum variant; $> recognition is now a
                // structural match, not a string compare).
                match self.__compartment.forward_event.take() {
                    None => {
                        // No forwarded event — synthesize a fresh $>. RFC-0025.1:
                        // enter args live in the destination's typed ctx.
                        let enter_event = alloc::rc::Rc::new(ExprScannerFsmFrameEvent::FrameEnter {});
                        self.__router(&enter_event);
                    }
                    Some(fwd) if matches!(fwd, ExprScannerFsmFrameEvent::FrameEnter { .. }) => {
                        // Forwarded event IS $> — dispatch directly so the
                        // destination's $> handler receives the caller's payload.
                        let fwd_rc = alloc::rc::Rc::new(fwd);
                        self.__router(&fwd_rc);
                    }
                    Some(fwd) => {
                        // Forwarded event is not $> — initialize the destination
                        // with a fresh $>, then dispatch the forward.
                        let enter_event = alloc::rc::Rc::new(ExprScannerFsmFrameEvent::FrameEnter {});
                        self.__router(&enter_event);
                        let fwd_rc = alloc::rc::Rc::new(fwd);
                        self.__router(&fwd_rc);
                    }
                }
                for ctx in self._context_stack.iter_mut() {
                    ctx._transitioned = true;
                }
            }
        }

        fn __router(&mut self, __e: &alloc::rc::Rc<ExprScannerFsmFrameEvent>) {
            let __ev: &ExprScannerFsmFrameEvent = __e;
            match self.__compartment.state.as_str() {
                "Init" => self._state_Init(__ev),
                "Scanning" => self._state_Scanning(__ev),
                _ => {}
            }
        }

        fn __transition(&mut self, next_compartment: ExprScannerFsmCompartment) {
            self.__next_compartment = Some(next_compartment);
        }

        pub fn do_scan(&mut self) {
            let __e = alloc::rc::Rc::new(ExprScannerFsmFrameEvent::DoScan {});
            let mut __ctx = ExprScannerFsmFrameContext::new(alloc::rc::Rc::clone(&__e), None);
            self._context_stack.push(__ctx);
            self.__kernel(&__e);
            self._context_stack.pop();
        }

        fn _state_Init(&mut self, __e: &ExprScannerFsmFrameEvent) {
            match __e {
                ExprScannerFsmFrameEvent::DoScan { .. } => { self._s_Init_hdl_user_do_scan(__e); }
                _ => {}
            }
        }

        fn _state_Scanning(&mut self, __e: &ExprScannerFsmFrameEvent) {
            match __e {
                ExprScannerFsmFrameEvent::FrameEnter { .. } => { self._s_Scanning_hdl_frame_enter(__e); }
                _ => {}
            }
        }

        fn _s_Init_hdl_user_do_scan(&mut self, __e: &ExprScannerFsmFrameEvent) {
            let mut __compartment = self.__prepareEnter("Scanning");
            self.__transition(__compartment);
            return;
        }

        fn _s_Scanning_hdl_frame_enter(&mut self, __e: &ExprScannerFsmFrameEvent) {
            let mut i = self.pos;
            let end = self.end;
            let bytes = &self.bytes;
            let mut depth: i32 = 0;
            let mut in_string: Option<u8> = None;
            
            while i < end {
                let b = bytes[i];
            
                // Handle string literals
                if let Some(q) = in_string {
                    // A raw newline inside an unterminated string means the
                    // opening quote was not a real delimiter — most often an
                    // apostrophe in a trailing comment (`a = None  # it's here`).
                    // In single-line mode (`stop_newline`) a string must not
                    // consume past the line and swallow the next field (#113).
                    // Stop here, excluding the newline.
                    if b == b'\n' && self.stop_newline {
                        break;
                    }
                    if b == b'\\' && i + 1 < end {
                        i += 2;
                        continue;
                    }
                    if b == q {
                        in_string = None;
                    }
                    i += 1;
                    continue;
                }
            
                // Enter string literal
                if b == b'"' || b == b'\'' {
                    in_string = Some(b);
                    i += 1;
                    continue;
                }
            
                // Track nesting depth (PDA stack via counter). The
                // configurable terminators are checked at depth 0 *before*
                // the generic closer/decrement arm so a depth-0 `)` can act
                // as a terminator (call_args matching-close) rather than
                // decrementing.
                match b {
                    b'(' | b'[' | b'{' => { depth += 1; }
                    b')' if depth == 0 && self.stop_close_paren => {
                        break; // matching close — excluded
                    }
                    b')' | b']' | b'}' => { depth = (depth - 1).max(0); }
                    b',' if depth == 0 && self.stop_comma => {
                        break; // arg separator — excluded
                    }
                    b';' if depth == 0 && self.stop_semicolon => {
                        i += 1; // Include the semicolon
                        break;
                    }
                    b'\n' if depth == 0 && self.stop_newline => {
                        // #185: a depth-0 newline ends the expression only if it
                        // looks complete. Keep scanning when the current line ends
                        // with a continuation operator, or the next non-blank line
                        // begins with one (a leading `.` method chain, a leading
                        // binary op). framec is type-ignorant, so this is a lexical
                        // heuristic, not a native parse. Balanced `()[]{}` already
                        // hold multi-line literals whole (depth > 0); this only
                        // rescues *unbracketed* continuations.
                        //
                        // NOTE (#123): this scanner is a PDA (native depth counter),
                        // so the heuristic lives in native Rust; converting the whole
                        // scanner to a real @@fsm is tracked separately.
                        //
                        // Trailing test: last non-space byte at/before the newline
                        // (skipping blank lines). Closers are NOT continuation —
                        // only dangling operators are. `=>` (arrow) counts, but a
                        // bare `>` does not (it closes a generic like `Vec<u8>`).
                        let mut j = i;
                        while j > self.pos {
                            let c = bytes[j - 1];
                            if c == b' ' || c == b'\t' || c == b'\r' || c == b'\n' {
                                j -= 1;
                            } else {
                                break;
                            }
                        }
                        // A complete statement/field never ends in a dangling
                        // binary/member/assign operator, so these are safe
                        // continuation signals. `=>` (arrow) continues; a bare
                        // `>` does not (it closes a generic like `Vec<u8>`).
                        // `,`/`:`/`<` are excluded — too easily a complete line.
                        let trailing = if j > self.pos { bytes[j - 1] } else { 0u8 };
                        let trailing_prev = if j > self.pos + 1 { bytes[j - 2] } else { 0u8 };
                        let trailing_cont = matches!(
                            trailing,
                            b'+' | b'-' | b'*' | b'/' | b'%' | b'&' | b'|'
                                | b'^' | b'.' | b'?' | b'='
                        ) || (trailing == b'>' && trailing_prev == b'=');
                        // Leading test: first non-space byte on the next non-blank
                        // line. Only `.`/`?`/`:` are safe here: a leading binary
                        // operator is ambiguous with a statement start (a native
                        // `*p`/`&x` deref, a unary `-`/`+`, or a Frame `-> $S` /
                        // `=> $^` control-flow line), which this scanner also
                        // feeds — so those must NOT count as continuation. A
                        // leading `.` method chain never starts a statement.
                        let mut k = i + 1;
                        while k < end {
                            let c = bytes[k];
                            if c == b' ' || c == b'\t' || c == b'\r' || c == b'\n' {
                                k += 1;
                            } else {
                                break;
                            }
                        }
                        let leading = if k < end { bytes[k] } else { 0u8 };
                        let leading_cont = matches!(leading, b'.' | b'?' | b':');
                        if !(trailing_cont || leading_cont) {
                            break; // complete — terminate, newline excluded
                        }
                        // else: continuation — fall through to `i += 1`, keep scanning
                    }
                    _ => {}
                }
                i += 1;
            }
            
            self.result_end = i;
        }
    }
}
pub use _expr_scanner_fsm_framec::*;

