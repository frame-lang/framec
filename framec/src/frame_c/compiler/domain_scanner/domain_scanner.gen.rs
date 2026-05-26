
// RFC-0035 Round 9 — the `domain:` section line-scanner as a Frame FSM.
//
// Replaces the hand-rolled outer byte-walk in `pipeline_parser/domain.rs`
// (`parse_domain`). The `domain:` section is a line-oriented scanner: each
// physical line is one of {blank, `}` close, `@@[...]` attribute, a section
// keyword that ends the section, a comment, or a `[const] name [: type]
// [= init]` field}. That classification is the state machine; the `$Scan`
// state self-loops over lines, accumulating `DomainVar`s.
//
// The FSM OWNS the scan (RFC-0039 B1): `bytes` / `pos` / `vars` / the
// pending comment + attribute buffers live in domain fields. It delegates the
// two genuinely-structured sub-scans to the existing dogfooded FSMs:
//   - `@@[name(args?)]`           → `scan_attribute` (AttributeScannerFsm)
//   - a field's `= init` RHS      → `ExprScannerFsm`
// The caller (`Parser::parse_domain`) builds this system, runs `scan()`, lifts
// `vars` out, sets the lexer cursor to `result_cursor`, then drains the lexer's
// buffered tokens (token-level bookkeeping that needs the lexer, not bytes).
//
// Errors thread through `error` (Frame handlers return ()); the caller returns
// it. Byte-output identical to the recursive form — snapshot + matrix + fuzz
// are the parity gate.

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
mod _domain_scanner_fsm_framec {
    use super::*;
    extern crate alloc;
    use alloc::{vec, format};
    #[derive(Clone, Debug)]
    #[allow(dead_code, non_camel_case_types)]
    enum DomainScannerFsmFrameEvent {
        Scan {  },
        FrameEnter {},
        FrameExit {},
    }

    #[derive(Clone)]
    #[allow(dead_code, non_camel_case_types)]
    enum DomainScannerFsmFrameReturn {
        _Lifecycle(alloc::rc::Rc<dyn core::any::Any>),
    }

    #[allow(dead_code)]
    impl DomainScannerFsmFrameEvent {
        fn name(&self) -> &'static str {
            match self {
                DomainScannerFsmFrameEvent::Scan { .. } => "scan",
                DomainScannerFsmFrameEvent::FrameEnter { .. } => "$>",
                DomainScannerFsmFrameEvent::FrameExit { .. } => "<$",
            }
        }
    }

    #[derive(Clone, Debug)]
    #[allow(dead_code, non_camel_case_types)]
    enum DomainScannerFsmFrameValue {
        Int(i64),
        Float(f64),
        Bool(bool),
        Str(String),
        List(Vec<Self>),
        Dict(alloc::collections::BTreeMap<String, Self>),
    }

    #[allow(dead_code, non_camel_case_types)]
    struct DomainScannerFsmFrameContext {
        event: alloc::rc::Rc<DomainScannerFsmFrameEvent>,
        _return: Option<DomainScannerFsmFrameReturn>,
        _data: alloc::collections::BTreeMap<String, DomainScannerFsmFrameValue>,
        _transitioned: bool,
    }

    impl DomainScannerFsmFrameContext {
        fn new(event: alloc::rc::Rc<DomainScannerFsmFrameEvent>, default_return: Option<DomainScannerFsmFrameReturn>) -> Self {
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
    enum DomainScannerFsmStateContext {
        Start,
        Scan,
        Done,
        __NoContext,
    }

    impl Default for DomainScannerFsmStateContext {
        fn default() -> Self {
            DomainScannerFsmStateContext::Start
        }
    }

    #[allow(dead_code, non_camel_case_types)]
    #[derive(Clone)]
    struct DomainScannerFsmCompartment {
        state: String,
        state_context: DomainScannerFsmStateContext,
        forward_event: Option<DomainScannerFsmFrameEvent>,
        parent_compartment: Option<Box<DomainScannerFsmCompartment>>,
    }

    impl DomainScannerFsmCompartment {
        fn new(state: &str) -> Self {
            let state_context = match state {
                "Start" => DomainScannerFsmStateContext::Start,
                "Scan" => DomainScannerFsmStateContext::Scan,
                "Done" => DomainScannerFsmStateContext::Done,
                _ => DomainScannerFsmStateContext::__NoContext,
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
    pub struct DomainScannerFsm {
        _state_stack: Vec<DomainScannerFsmCompartment>,
        __compartment: DomainScannerFsmCompartment,
        __next_compartment: Option<DomainScannerFsmCompartment>,
        _context_stack: Vec<DomainScannerFsmFrameContext>,
        pub bytes: Vec<u8>,
        pub pos: usize,
        pub vars: Vec<DomainVar>,
        pub pending_doc: Vec<String>,
        pub pending_attrs: Vec<crate::frame_c::compiler::frame_ast::Attribute>,
        pub result_cursor: usize,
        pub error: Option<ParseError>,
    }

    #[allow(non_snake_case)]
    impl DomainScannerFsm {
        pub fn new() -> Self {
            Self {
                _state_stack: Vec::new(),
                _context_stack: Vec::new(),
                bytes: Vec::new(),
                pos: 0,
                vars: Vec::new(),
                pending_doc: Vec::new(),
                pending_attrs: Vec::new(),
                result_cursor: 0,
                error: None,
                __compartment: DomainScannerFsmCompartment::new("Start"),
                __next_compartment: None,
            }
        }

        pub fn __create() -> Self {
            let mut c = Self::new();
            c.__compartment = c.__prepareEnter("Start");
            let __e = alloc::rc::Rc::new(DomainScannerFsmFrameEvent::FrameEnter {});
            let __ctx = DomainScannerFsmFrameContext::new(alloc::rc::Rc::clone(&__e), None);
            c._context_stack.push(__ctx);
            c.__kernel(&__e);
            c._context_stack.pop();
            c
        }

        fn __hsm_chain(&mut self, leaf: &str) -> &'static [&'static str] {
            match leaf {
                "Start" => &["Start"],
                "Scan" => &["Scan"],
                "Done" => &["Done"],
                _ => &[],
            }
        }

        fn __prepareEnter(&mut self, leaf: &str) -> DomainScannerFsmCompartment {
            let chain = self.__hsm_chain(leaf);
            let mut comp: Option<DomainScannerFsmCompartment> = None;
            for name in chain.iter() {
                let mut new_comp = DomainScannerFsmCompartment::new(name);
                if let Some(parent) = comp.take() {
                    new_comp.parent_compartment = Some(Box::new(parent));
                }
                comp = Some(new_comp);
            }
            comp.expect("chain must contain at least the leaf state")
        }

        fn __kernel(&mut self, __e: &alloc::rc::Rc<DomainScannerFsmFrameEvent>) {
            // Route event to current state.
            self.__router(__e);
            // Drain any transitions queued by the handler.
            while self.__next_compartment.is_some() {
                let next_compartment = self.__next_compartment.take().expect("invariant: while-loop guard checked is_some()");
                // Exit the current (leaf) state. RFC-0025.1: exit args live in the
                // source state's typed ctx (written at the transition site), so the
                // synthesized `<$` event carries no payload.
                let exit_event = alloc::rc::Rc::new(DomainScannerFsmFrameEvent::FrameExit {});
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
                        let enter_event = alloc::rc::Rc::new(DomainScannerFsmFrameEvent::FrameEnter {});
                        self.__router(&enter_event);
                    }
                    Some(fwd) if matches!(fwd, DomainScannerFsmFrameEvent::FrameEnter { .. }) => {
                        // Forwarded event IS $> — dispatch directly so the
                        // destination's $> handler receives the caller's payload.
                        let fwd_rc = alloc::rc::Rc::new(fwd);
                        self.__router(&fwd_rc);
                    }
                    Some(fwd) => {
                        // Forwarded event is not $> — initialize the destination
                        // with a fresh $>, then dispatch the forward.
                        let enter_event = alloc::rc::Rc::new(DomainScannerFsmFrameEvent::FrameEnter {});
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

        fn __router(&mut self, __e: &alloc::rc::Rc<DomainScannerFsmFrameEvent>) {
            let __ev: &DomainScannerFsmFrameEvent = __e;
            match self.__compartment.state.as_str() {
                "Start" => self._state_Start(__ev),
                "Scan" => self._state_Scan(__ev),
                "Done" => self._state_Done(__ev),
                _ => {}
            }
        }

        fn __transition(&mut self, next_compartment: DomainScannerFsmCompartment) {
            self.__next_compartment = Some(next_compartment);
        }

        pub fn scan(&mut self) {
            let __e = alloc::rc::Rc::new(DomainScannerFsmFrameEvent::Scan {});
            let mut __ctx = DomainScannerFsmFrameContext::new(alloc::rc::Rc::clone(&__e), None);
            self._context_stack.push(__ctx);
            self.__kernel(&__e);
            self._context_stack.pop();
        }

        fn _state_Start(&mut self, __e: &DomainScannerFsmFrameEvent) {
            match __e {
                DomainScannerFsmFrameEvent::Scan { .. } => { self._s_Start_hdl_user_scan(__e); }
                _ => {}
            }
        }

        // One physical line per entry. Self-loops until the section ends.
        fn _state_Scan(&mut self, __e: &DomainScannerFsmFrameEvent) {
            match __e {
                DomainScannerFsmFrameEvent::FrameEnter { .. } => { self._s_Scan_hdl_frame_enter(__e); }
                _ => {}
            }
        }

        fn _state_Done(&mut self, __e: &DomainScannerFsmFrameEvent) {
            match __e {
                _ => {}
            }
        }

        fn _s_Start_hdl_user_scan(&mut self, __e: &DomainScannerFsmFrameEvent) {
            let mut __compartment = self.__prepareEnter("Scan");
            self.__transition(__compartment);
            return;
        }

        fn _s_Scan_hdl_frame_enter(&mut self, __e: &DomainScannerFsmFrameEvent) {
            let src = &self.bytes;
            let n = src.len();
            let mut pos = self.pos;
            
            // Skip blank lines / leading newlines.
            while pos < n && (src[pos] == b'\n' || src[pos] == b'\r') {
                pos += 1;
            }
            if pos >= n {
                self.pos = pos;
                self.result_cursor = pos;
                let mut __compartment = self.__prepareEnter("Done");
                self.__transition(__compartment);
                return;
            }
            
            // Indentation → start of content.
            let line_start = pos;
            while pos < n && (src[pos] == b' ' || src[pos] == b'\t') {
                pos += 1;
            }
            if pos >= n {
                self.pos = pos;
                self.result_cursor = pos;
                let mut __compartment = self.__prepareEnter("Done");
                self.__transition(__compartment);
                return;
            }
            
            // `}` closes the @@system block — hand it back unconsumed.
            if src[pos] == b'}' {
                self.pos = line_start;
                self.result_cursor = line_start;
                let mut __compartment = self.__prepareEnter("Done");
                self.__transition(__compartment);
                return;
            }
            
            // `@@[name(args?)]` attributes — accumulate for the next field.
            // Back-to-back same-line attrs loop; an attr ending the line
            // restarts the scan.
            while pos + 2 < n && src[pos] == b'@' && src[pos + 1] == b'@' && src[pos + 2] == b'[' {
                let attr_start = pos;
                let span = crate::frame_c::compiler::attribute_scanner::scan_attribute(src, attr_start);
                let name = std::str::from_utf8(span.name(src)).unwrap_or("").to_string();
                let args = span
                    .args_inner(src)
                    .map(|b| std::str::from_utf8(b).unwrap_or("").to_string());
                pos = span.end_pos;
                self.pending_attrs.push(crate::frame_c::compiler::frame_ast::Attribute {
                    name,
                    args,
                    span: Span::new(attr_start, pos),
                });
                while pos < n && (src[pos] == b' ' || src[pos] == b'\t') {
                    pos += 1;
                }
                if pos < n && (src[pos] == b'\n' || src[pos] == b'\r') {
                    break;
                }
            }
            if !self.pending_attrs.is_empty() && pos < n && (src[pos] == b'\n' || src[pos] == b'\r') {
                self.pos = pos;
                let mut __compartment = self.__prepareEnter("Scan");
                self.__transition(__compartment);
                return;
            }
            
            // Peek the leading word; a section keyword + `:` ends `domain:`.
            let word_start = pos;
            while pos < n && (src[pos].is_ascii_alphanumeric() || src[pos] == b'_') {
                pos += 1;
            }
            let word = std::str::from_utf8(&src[word_start..pos]).unwrap_or("");
            let mut check_pos = pos;
            while check_pos < n && (src[check_pos] == b' ' || src[check_pos] == b'\t') {
                check_pos += 1;
            }
            let followed_by_colon = check_pos < n && src[check_pos] == b':';
            if followed_by_colon
                && matches!(word, "interface" | "machine" | "actions" | "operations" | "domain")
            {
                self.pos = line_start;
                self.result_cursor = line_start;
                let mut __compartment = self.__prepareEnter("Done");
                self.__transition(__compartment);
                return;
            }
            
            // Canonical field: [const] name [: type] [= init].
            let field_start = word_start;
            let (is_const, name) = if word == "const" {
                while pos < n && (src[pos] == b' ' || src[pos] == b'\t') {
                    pos += 1;
                }
                let name_start = pos;
                while pos < n && (src[pos].is_ascii_alphanumeric() || src[pos] == b'_') {
                    pos += 1;
                }
                (true, std::str::from_utf8(&src[name_start..pos]).unwrap_or("").to_string())
            } else {
                (false, word.to_string())
            };
            
            // Empty name ⇒ comment-only / unparseable line: capture trivia.
            if name.is_empty() {
                let line_text_start = word_start;
                while pos < n && src[pos] != b'\n' {
                    pos += 1;
                }
                let raw = std::str::from_utf8(&src[line_text_start..pos])
                    .unwrap_or("")
                    .trim_end_matches('\r')
                    .trim_end();
                if !raw.is_empty() {
                    self.pending_doc.push(raw.to_string());
                }
                self.pos = pos;
                let mut __compartment = self.__prepareEnter("Scan");
                self.__transition(__compartment);
                return;
            }
            
            // Optional `: type` — scan the type slot to a top-level `=`
            // (bracket-aware for generics).
            while pos < n && (src[pos] == b' ' || src[pos] == b'\t') {
                pos += 1;
            }
            let var_type = if pos < n && src[pos] == b':' {
                pos += 1;
                while pos < n && (src[pos] == b' ' || src[pos] == b'\t') {
                    pos += 1;
                }
                let type_start = pos;
                let mut bracket_depth: i32 = 0;
                while pos < n && src[pos] != b'\n' {
                    match src[pos] {
                        b'<' | b'(' | b'[' | b'{' => { bracket_depth += 1; pos += 1; }
                        b'>' | b')' | b']' | b'}' => { bracket_depth -= 1; pos += 1; }
                        b'=' if bracket_depth == 0 => break,
                        _ => { pos += 1; }
                    }
                }
                let type_text = std::str::from_utf8(&src[type_start..pos]).unwrap_or("").trim().to_string();
                if type_text.is_empty() { Type::Unknown } else { Type::Custom(type_text) }
            } else {
                Type::Unknown
            };
            
            while pos < n && (src[pos] == b' ' || src[pos] == b'\t') {
                pos += 1;
            }
            
            // Optional `= init`.
            if pos >= n || src[pos] == b'\n' || src[pos] != b'=' {
                while pos < n && src[pos] != b'\n' {
                    pos += 1;
                }
                self.vars.push(DomainVar {
                    name,
                    var_type,
                    initializer_text: None,
                    is_const,
                    leading_comments: std::mem::take(&mut self.pending_doc),
                    attributes: std::mem::take(&mut self.pending_attrs),
                    span: Span::new(field_start, pos),
                });
                self.pos = pos;
                let mut __compartment = self.__prepareEnter("Scan");
                self.__transition(__compartment);
                return;
            }
            pos += 1; // consume '='
            while pos < n && (src[pos] == b' ' || src[pos] == b'\t') {
                pos += 1;
            }
            
            // Init capture. A `(` with no same-line matching `)` is a
            // multi-line wrapper; otherwise the dogfooded ExprScannerFsm
            // captures the balanced RHS (stops at the depth-0 newline).
            let init_text = if pos < n && src[pos] == b'(' {
                let paren_pos = pos;
                let mut check = pos + 1;
                let mut depth = 1i32;
                let mut same_line = false;
                while check < n && src[check] != b'\n' {
                    match src[check] {
                        b'(' => depth += 1,
                        b')' => { depth -= 1; if depth == 0 { same_line = true; break; } }
                        _ => {}
                    }
                    check += 1;
                }
                if same_line {
                    let init_start = pos;
                    while pos < n && src[pos] != b'\n' { pos += 1; }
                    std::str::from_utf8(&src[init_start..pos]).unwrap_or("").trim_end().to_string()
                } else {
                    pos = paren_pos + 1;
                    let wrapper_content_start = pos;
                    let mut depth = 1i32;
                    while pos < n && depth > 0 {
                        match src[pos] {
                            b'(' | b'[' | b'{' => depth += 1,
                            b')' | b']' | b'}' => { depth -= 1; if depth == 0 { break; } }
                            b'"' => {
                                pos += 1;
                                while pos < n && src[pos] != b'"' {
                                    if src[pos] == b'\\' && pos + 1 < n { pos += 1; }
                                    pos += 1;
                                }
                            }
                            b'\'' => {
                                pos += 1;
                                while pos < n && src[pos] != b'\'' {
                                    if src[pos] == b'\\' && pos + 1 < n { pos += 1; }
                                    pos += 1;
                                }
                            }
                            _ => {}
                        }
                        pos += 1;
                    }
                    if depth != 0 {
                        self.error = Some(ParseError {
                            message: format!("domain field '{}': unterminated multi-line initializer '('", name),
                            span: Span::new(paren_pos, pos),
                        });
                        let mut __compartment = self.__prepareEnter("Done");
                        self.__transition(__compartment);
                        return;
                    }
                    let wrapper_content_end = pos - 1;
                    let init = std::str::from_utf8(&src[wrapper_content_start..wrapper_content_end])
                        .unwrap_or("").trim().to_string();
                    while pos < n && src[pos] != b'\n' {
                        if src[pos] != b' ' && src[pos] != b'\t' {
                            self.error = Some(ParseError {
                                message: format!("domain field '{}': unexpected tokens after closing ')'", name),
                                span: Span::new(pos, pos + 1),
                            });
                            let mut __compartment = self.__prepareEnter("Done");
                            self.__transition(__compartment);
                            return;
                        }
                        pos += 1;
                    }
                    init
                }
            } else {
                let mut expr = _ds_expr::ExprScannerFsm::new();
                expr.bytes = src.to_vec();
                expr.pos = pos;
                expr.end = n;
                expr.do_scan();
                let init_end = expr.result_end;
                let init = std::str::from_utf8(&src[pos..init_end]).unwrap_or("").trim_end().to_string();
                pos = init_end;
                init
            };
            
            let init_opt = if init_text.is_empty() { None } else { Some(init_text) };
            self.vars.push(DomainVar {
                name,
                var_type,
                initializer_text: init_opt,
                is_const,
                leading_comments: std::mem::take(&mut self.pending_doc),
                attributes: std::mem::take(&mut self.pending_attrs),
                span: Span::new(field_start, pos),
            });
            self.pos = pos;
            let mut __compartment = self.__prepareEnter("Scan");
            self.__transition(__compartment);
            return;
        }
    }
}
pub use _domain_scanner_fsm_framec::*;
