
// Java handler-body `await` rewriter — dogfooded state machine.
//
// Frame source uses `await EXPR` to await a future. Java has no
// `await` keyword — the `CompletableFuture<T>` API uses `.join()`
// (unchecked-exception variant of `.get()`). When framec emits a
// Java handler body that contains user source like
//   `self.tmp_a = await op("init");`
// this FSM rewrites it to
//   `self.tmp_a = op("init").join();`
//
// Skips `"..."` string literals and `//` / `/* */` comments so
// the literal `await` token inside those is preserved verbatim.
// Recognizes EXPR as an identifier chain (`a.b.c`) optionally
// followed by a balanced `(...)` argument list, including string
// literals nested in args.
//
// The companion `self.` → `this.` rewrite is done by the caller
// via `replace_outside_strings_and_comments` (already FSM-driven
// through the per-target skipper trait). This module's only job
// is the `await EXPR` → `EXPR.join()` transformation.
//
// Output: `result: String` — the rewritten body. `error_kind == 0`
// on success. The current implementation never errors (an
// unterminated string copies through to end-of-input rather than
// failing the rewrite); the field is kept for symmetry with the
// body_closer FSMs.

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
mod _java_await_rewrite_fsm_framec {
    use super::*;
    extern crate alloc;
    use alloc::{vec, format};
    #[derive(Clone, Debug)]
    #[allow(dead_code, non_camel_case_types)]
    enum JavaAwaitRewriteFsmFrameEvent {
        Rewrite {  },
        FrameEnter {},
        FrameExit {},
    }

    #[derive(Clone)]
    #[allow(dead_code, non_camel_case_types)]
    enum JavaAwaitRewriteFsmFrameReturn {
        _Lifecycle(alloc::rc::Rc<dyn core::any::Any>),
    }

    #[allow(dead_code)]
    impl JavaAwaitRewriteFsmFrameEvent {
        fn name(&self) -> &'static str {
            match self {
                JavaAwaitRewriteFsmFrameEvent::Rewrite { .. } => "rewrite",
                JavaAwaitRewriteFsmFrameEvent::FrameEnter { .. } => "$>",
                JavaAwaitRewriteFsmFrameEvent::FrameExit { .. } => "<$",
            }
        }
    }

    #[derive(Clone, Debug)]
    #[allow(dead_code, non_camel_case_types)]
    enum JavaAwaitRewriteFsmFrameValue {
        Int(i64),
        Float(f64),
        Bool(bool),
        Str(String),
        List(Vec<Self>),
        Dict(alloc::collections::BTreeMap<String, Self>),
    }

    #[allow(dead_code, non_camel_case_types)]
    struct JavaAwaitRewriteFsmFrameContext {
        event: alloc::rc::Rc<JavaAwaitRewriteFsmFrameEvent>,
        _return: Option<JavaAwaitRewriteFsmFrameReturn>,
        _data: alloc::collections::BTreeMap<String, JavaAwaitRewriteFsmFrameValue>,
        _transitioned: bool,
    }

    impl JavaAwaitRewriteFsmFrameContext {
        fn new(event: alloc::rc::Rc<JavaAwaitRewriteFsmFrameEvent>, default_return: Option<JavaAwaitRewriteFsmFrameReturn>) -> Self {
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
    enum JavaAwaitRewriteFsmStateContext {
        Init,
        Scanning,
        InLineComment,
        InBlockComment,
        InString,
        __NoContext,
    }

    impl Default for JavaAwaitRewriteFsmStateContext {
        fn default() -> Self {
            JavaAwaitRewriteFsmStateContext::Init
        }
    }

    #[allow(dead_code, non_camel_case_types)]
    #[derive(Clone)]
    struct JavaAwaitRewriteFsmCompartment {
        state: String,
        state_context: JavaAwaitRewriteFsmStateContext,
        forward_event: Option<JavaAwaitRewriteFsmFrameEvent>,
        parent_compartment: Option<Box<JavaAwaitRewriteFsmCompartment>>,
    }

    impl JavaAwaitRewriteFsmCompartment {
        fn new(state: &str) -> Self {
            let state_context = match state {
                "Init" => JavaAwaitRewriteFsmStateContext::Init,
                "Scanning" => JavaAwaitRewriteFsmStateContext::Scanning,
                "InLineComment" => JavaAwaitRewriteFsmStateContext::InLineComment,
                "InBlockComment" => JavaAwaitRewriteFsmStateContext::InBlockComment,
                "InString" => JavaAwaitRewriteFsmStateContext::InString,
                _ => JavaAwaitRewriteFsmStateContext::__NoContext,
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
    pub struct JavaAwaitRewriteFsm {
        _state_stack: Vec<JavaAwaitRewriteFsmCompartment>,
        __compartment: JavaAwaitRewriteFsmCompartment,
        __next_compartment: Option<JavaAwaitRewriteFsmCompartment>,
        _context_stack: Vec<JavaAwaitRewriteFsmFrameContext>,
        pub bytes: Vec<u8>,
        pub pos: usize,
        pub result: String,
        pub error_kind: usize,
        pub block_comment_nest: i32,
    }

    #[allow(non_snake_case)]
    impl JavaAwaitRewriteFsm {
        pub fn new() -> Self {
            Self {
                _state_stack: Vec::new(),
                _context_stack: Vec::new(),
                bytes: Vec::new(),
                pos: 0,
                result: String::new(),
                error_kind: 0,
                block_comment_nest: 0,
                __compartment: JavaAwaitRewriteFsmCompartment::new("Init"),
                __next_compartment: None,
            }
        }

        pub fn __create() -> Self {
            let mut c = Self::new();
            c.__compartment = c.__prepareEnter("Init");
            let __e = alloc::rc::Rc::new(JavaAwaitRewriteFsmFrameEvent::FrameEnter {});
            let __ctx = JavaAwaitRewriteFsmFrameContext::new(alloc::rc::Rc::clone(&__e), None);
            c._context_stack.push(__ctx);
            c.__kernel(&__e);
            c._context_stack.pop();
            c
        }

        fn __hsm_chain(&mut self, leaf: &str) -> &'static [&'static str] {
            match leaf {
                "Init" => &["Init"],
                "Scanning" => &["Scanning"],
                "InLineComment" => &["InLineComment"],
                "InBlockComment" => &["InBlockComment"],
                "InString" => &["InString"],
                _ => &[],
            }
        }

        fn __prepareEnter(&mut self, leaf: &str) -> JavaAwaitRewriteFsmCompartment {
            let chain = self.__hsm_chain(leaf);
            let mut comp: Option<JavaAwaitRewriteFsmCompartment> = None;
            for name in chain.iter() {
                let mut new_comp = JavaAwaitRewriteFsmCompartment::new(name);
                if let Some(parent) = comp.take() {
                    new_comp.parent_compartment = Some(Box::new(parent));
                }
                comp = Some(new_comp);
            }
            comp.expect("chain must contain at least the leaf state")
        }

        fn __kernel(&mut self, __e: &alloc::rc::Rc<JavaAwaitRewriteFsmFrameEvent>) {
            // Route event to current state.
            self.__router(__e);
            // Drain any transitions queued by the handler.
            while self.__next_compartment.is_some() {
                let next_compartment = self.__next_compartment.take().expect("invariant: while-loop guard checked is_some()");
                // Exit the current (leaf) state. RFC-0025.1: exit args live in the
                // source state's typed ctx (written at the transition site), so the
                // synthesized `<$` event carries no payload.
                let exit_event = alloc::rc::Rc::new(JavaAwaitRewriteFsmFrameEvent::FrameExit {});
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
                        let enter_event = alloc::rc::Rc::new(JavaAwaitRewriteFsmFrameEvent::FrameEnter {});
                        self.__router(&enter_event);
                    }
                    Some(fwd) if matches!(fwd, JavaAwaitRewriteFsmFrameEvent::FrameEnter { .. }) => {
                        // Forwarded event IS $> — dispatch directly so the
                        // destination's $> handler receives the caller's payload.
                        let fwd_rc = alloc::rc::Rc::new(fwd);
                        self.__router(&fwd_rc);
                    }
                    Some(fwd) => {
                        // Forwarded event is not $> — initialize the destination
                        // with a fresh $>, then dispatch the forward.
                        let enter_event = alloc::rc::Rc::new(JavaAwaitRewriteFsmFrameEvent::FrameEnter {});
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

        fn __router(&mut self, __e: &alloc::rc::Rc<JavaAwaitRewriteFsmFrameEvent>) {
            let __ev: &JavaAwaitRewriteFsmFrameEvent = __e;
            match self.__compartment.state.as_str() {
                "Init" => self._state_Init(__ev),
                "Scanning" => self._state_Scanning(__ev),
                "InLineComment" => self._state_InLineComment(__ev),
                "InBlockComment" => self._state_InBlockComment(__ev),
                "InString" => self._state_InString(__ev),
                _ => {}
            }
        }

        fn __transition(&mut self, next_compartment: JavaAwaitRewriteFsmCompartment) {
            self.__next_compartment = Some(next_compartment);
        }

        pub fn rewrite(&mut self) {
            let __e = alloc::rc::Rc::new(JavaAwaitRewriteFsmFrameEvent::Rewrite {});
            let mut __ctx = JavaAwaitRewriteFsmFrameContext::new(alloc::rc::Rc::clone(&__e), None);
            self._context_stack.push(__ctx);
            self.__kernel(&__e);
            self._context_stack.pop();
        }

        fn _state_Init(&mut self, __e: &JavaAwaitRewriteFsmFrameEvent) {
            match __e {
                JavaAwaitRewriteFsmFrameEvent::Rewrite { .. } => { self._s_Init_hdl_user_rewrite(__e); }
                _ => {}
            }
        }

        fn _state_Scanning(&mut self, __e: &JavaAwaitRewriteFsmFrameEvent) {
            match __e {
                JavaAwaitRewriteFsmFrameEvent::FrameEnter { .. } => { self._s_Scanning_hdl_frame_enter(__e); }
                _ => {}
            }
        }

        fn _state_InLineComment(&mut self, __e: &JavaAwaitRewriteFsmFrameEvent) {
            match __e {
                JavaAwaitRewriteFsmFrameEvent::FrameEnter { .. } => { self._s_InLineComment_hdl_frame_enter(__e); }
                _ => {}
            }
        }

        fn _state_InBlockComment(&mut self, __e: &JavaAwaitRewriteFsmFrameEvent) {
            match __e {
                JavaAwaitRewriteFsmFrameEvent::FrameEnter { .. } => { self._s_InBlockComment_hdl_frame_enter(__e); }
                _ => {}
            }
        }

        fn _state_InString(&mut self, __e: &JavaAwaitRewriteFsmFrameEvent) {
            match __e {
                JavaAwaitRewriteFsmFrameEvent::FrameEnter { .. } => { self._s_InString_hdl_frame_enter(__e); }
                _ => {}
            }
        }

        fn _s_Init_hdl_user_rewrite(&mut self, __e: &JavaAwaitRewriteFsmFrameEvent) {
            let mut __compartment = self.__prepareEnter("Scanning");
            self.__transition(__compartment);
            return;
        }

        fn _s_Scanning_hdl_frame_enter(&mut self, __e: &JavaAwaitRewriteFsmFrameEvent) {
            let n = self.bytes.len();
            while self.pos < n {
                let b = self.bytes[self.pos];
            
                // `//` line comment — emit through verbatim.
                if b == b'/' && self.pos + 1 < n && self.bytes[self.pos + 1] == b'/' {
                    self.result.push('/');
                    self.result.push('/');
                    self.pos += 2;
                    let mut __compartment = self.__prepareEnter("InLineComment");
                    self.__transition(__compartment);
                    return;
                }
                // `/* … */` block comment — emit through verbatim.
                if b == b'/' && self.pos + 1 < n && self.bytes[self.pos + 1] == b'*' {
                    self.result.push('/');
                    self.result.push('*');
                    self.pos += 2;
                    self.block_comment_nest = 1;
                    let mut __compartment = self.__prepareEnter("InBlockComment");
                    self.__transition(__compartment);
                    return;
                }
                // `"…"` string literal — emit through verbatim.
                if b == b'"' {
                    self.result.push('"');
                    self.pos += 1;
                    let mut __compartment = self.__prepareEnter("InString");
                    self.__transition(__compartment);
                    return;
                }
            
                // `await EXPR` — match at a token boundary so
                // identifiers like `awaiting_x` don't trigger.
                let at_boundary = self.pos == 0
                    || matches!(
                        self.bytes[self.pos - 1],
                        b' ' | b'\t' | b'\n' | b'\r' | b'(' | b',' | b';' | b'='
                    );
                let is_await = self.pos + 6 <= n
                    && self.bytes[self.pos] == b'a'
                    && self.bytes[self.pos + 1] == b'w'
                    && self.bytes[self.pos + 2] == b'a'
                    && self.bytes[self.pos + 3] == b'i'
                    && self.bytes[self.pos + 4] == b't'
                    && self.bytes[self.pos + 5] == b' ';
                if at_boundary && is_await {
                    // Skip the `await ` literal + any extra
                    // whitespace.
                    self.pos += 6;
                    while self.pos < n
                        && (self.bytes[self.pos] == b' '
                            || self.bytes[self.pos] == b'\t')
                    {
                        self.pos += 1;
                    }
                    // Capture identifier chain
                    // `[A-Za-z_][A-Za-z0-9_]*` with optional
                    // dotted suffixes.
                    let expr_start = self.pos;
                    if self.pos < n
                        && (self.bytes[self.pos].is_ascii_alphabetic()
                            || self.bytes[self.pos] == b'_')
                    {
                        self.pos += 1;
                        while self.pos < n
                            && (self.bytes[self.pos].is_ascii_alphanumeric()
                                || self.bytes[self.pos] == b'_'
                                || self.bytes[self.pos] == b'.')
                        {
                            self.pos += 1;
                        }
                    }
                    // Optional balanced `(...)` arg list with
                    // string-literal awareness so `(foo, "x)")`
                    // closes correctly.
                    if self.pos < n && self.bytes[self.pos] == b'(' {
                        let mut depth: i32 = 0;
                        while self.pos < n {
                            let c = self.bytes[self.pos];
                            // String literal inside args.
                            if c == b'"' {
                                self.pos += 1;
                                while self.pos < n {
                                    let d = self.bytes[self.pos];
                                    if d == b'\\' && self.pos + 1 < n {
                                        self.pos += 2;
                                        continue;
                                    }
                                    self.pos += 1;
                                    if d == b'"' {
                                        break;
                                    }
                                }
                                continue;
                            }
                            if c == b'(' {
                                depth += 1;
                                self.pos += 1;
                                continue;
                            }
                            if c == b')' {
                                depth -= 1;
                                self.pos += 1;
                                if depth == 0 {
                                    break;
                                }
                                continue;
                            }
                            self.pos += 1;
                        }
                    }
                    // Emit the captured expression then `.join()`.
                    // If no identifier was found (degenerate
                    // `await ;`), fall back to copying `await `
                    // verbatim so we don't corrupt the source —
                    // in practice the user's intent was a real
                    // call, but the FSM should be robust.
                    if self.pos > expr_start {
                        let captured = std::str::from_utf8(
                            &self.bytes[expr_start..self.pos],
                        )
                        .unwrap_or("");
                        self.result.push_str(captured);
                        self.result.push_str(".join()");
                    } else {
                        self.result.push_str("await ");
                    }
                    continue;
                }
            
                // Default: copy byte through.
                self.result.push(b as char);
                self.pos += 1;
            }
            self.error_kind = 0;
        }

        fn _s_InLineComment_hdl_frame_enter(&mut self, __e: &JavaAwaitRewriteFsmFrameEvent) {
            let n = self.bytes.len();
            while self.pos < n && self.bytes[self.pos] != b'\n' {
                self.result.push(self.bytes[self.pos] as char);
                self.pos += 1;
            }
            let mut __compartment = self.__prepareEnter("Scanning");
            self.__transition(__compartment);
            return;
        }

        fn _s_InBlockComment_hdl_frame_enter(&mut self, __e: &JavaAwaitRewriteFsmFrameEvent) {
            let n = self.bytes.len();
            while self.pos + 1 < n {
                let a = self.bytes[self.pos];
                let b = self.bytes[self.pos + 1];
                if a == b'/' && b == b'*' {
                    self.block_comment_nest += 1;
                    self.result.push('/');
                    self.result.push('*');
                    self.pos += 2;
                    continue;
                }
                if a == b'*' && b == b'/' {
                    self.block_comment_nest -= 1;
                    self.result.push('*');
                    self.result.push('/');
                    self.pos += 2;
                    if self.block_comment_nest == 0 {
                        let mut __compartment = self.__prepareEnter("Scanning");
                        self.__transition(__compartment);
                        return;
                    }
                    continue;
                }
                self.result.push(a as char);
                self.pos += 1;
            }
            // Tail without close — copy remainder.
            while self.pos < n {
                self.result.push(self.bytes[self.pos] as char);
                self.pos += 1;
            }
            self.error_kind = 0;
        }

        fn _s_InString_hdl_frame_enter(&mut self, __e: &JavaAwaitRewriteFsmFrameEvent) {
            let n = self.bytes.len();
            while self.pos < n {
                let b = self.bytes[self.pos];
                self.result.push(b as char);
                self.pos += 1;
                if b == b'\\' && self.pos < n {
                    // Pass the escaped character through.
                    self.result.push(self.bytes[self.pos] as char);
                    self.pos += 1;
                    continue;
                }
                if b == b'"' {
                    let mut __compartment = self.__prepareEnter("Scanning");
                    self.__transition(__compartment);
                    return;
                }
            }
            // Unterminated — accept and continue.
            self.error_kind = 0;
        }
    }
}
pub use _java_await_rewrite_fsm_framec::*;

