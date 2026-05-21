
// Frame-structural syntax skipper — dogfooded state machine for
// the GraphViz pipeline.
//
// Mirrors `rust_skipper.frs` but additionally recognizes `#` line
// comments (Python / Ruby / Erlang style). Treats `'X'` as a Rust
// char literal and `'static` / `'a` style lifetimes as ordinary
// bytes — the same heuristic the Rust skipper uses, kept in sync
// so `.frs` source that compiles to either target renders to
// graphviz without divergence.

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
mod _frame_structural_syntax_skipper_fsm_framec {
    use super::*;
    extern crate alloc;
    use alloc::{vec, format};
    #[derive(Clone, Debug)]
    #[allow(dead_code, non_camel_case_types)]
    enum FrameStructuralSyntaxSkipperFsmFrameEvent {
        DoSkipComment {  },
        DoSkipString {  },
        DoFindLineEnd {  },
        DoBalancedParenEnd {  },
        FrameEnter {},
        FrameExit {},
    }

    #[derive(Clone)]
    #[allow(dead_code, non_camel_case_types)]
    enum FrameStructuralSyntaxSkipperFsmFrameReturn {
        _Lifecycle(alloc::rc::Rc<dyn core::any::Any>),
    }

    #[allow(dead_code)]
    impl FrameStructuralSyntaxSkipperFsmFrameEvent {
        fn name(&self) -> &'static str {
            match self {
                FrameStructuralSyntaxSkipperFsmFrameEvent::DoSkipComment { .. } => "do_skip_comment",
                FrameStructuralSyntaxSkipperFsmFrameEvent::DoSkipString { .. } => "do_skip_string",
                FrameStructuralSyntaxSkipperFsmFrameEvent::DoFindLineEnd { .. } => "do_find_line_end",
                FrameStructuralSyntaxSkipperFsmFrameEvent::DoBalancedParenEnd { .. } => "do_balanced_paren_end",
                FrameStructuralSyntaxSkipperFsmFrameEvent::FrameEnter { .. } => "$>",
                FrameStructuralSyntaxSkipperFsmFrameEvent::FrameExit { .. } => "<$",
            }
        }
    }

    #[derive(Clone, Debug)]
    #[allow(dead_code, non_camel_case_types)]
    enum FrameStructuralSyntaxSkipperFsmFrameValue {
        Int(i64),
        Float(f64),
        Bool(bool),
        Str(String),
        List(Vec<Self>),
        Dict(alloc::collections::BTreeMap<String, Self>),
    }

    #[allow(dead_code, non_camel_case_types)]
    struct FrameStructuralSyntaxSkipperFsmFrameContext {
        event: alloc::rc::Rc<FrameStructuralSyntaxSkipperFsmFrameEvent>,
        _return: Option<FrameStructuralSyntaxSkipperFsmFrameReturn>,
        _data: alloc::collections::BTreeMap<String, FrameStructuralSyntaxSkipperFsmFrameValue>,
        _transitioned: bool,
    }

    impl FrameStructuralSyntaxSkipperFsmFrameContext {
        fn new(event: alloc::rc::Rc<FrameStructuralSyntaxSkipperFsmFrameEvent>, default_return: Option<FrameStructuralSyntaxSkipperFsmFrameReturn>) -> Self {
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
    enum FrameStructuralSyntaxSkipperFsmStateContext {
        Init,
        SkipComment,
        SkipString,
        FindLineEnd,
        BalancedParenEnd,
        Empty,
    }

    impl Default for FrameStructuralSyntaxSkipperFsmStateContext {
        fn default() -> Self {
            FrameStructuralSyntaxSkipperFsmStateContext::Init
        }
    }

    #[allow(dead_code, non_camel_case_types)]
    #[derive(Clone)]
    struct FrameStructuralSyntaxSkipperFsmCompartment {
        state: String,
        state_context: FrameStructuralSyntaxSkipperFsmStateContext,
        forward_event: Option<FrameStructuralSyntaxSkipperFsmFrameEvent>,
        parent_compartment: Option<Box<FrameStructuralSyntaxSkipperFsmCompartment>>,
    }

    impl FrameStructuralSyntaxSkipperFsmCompartment {
        fn new(state: &str) -> Self {
            let state_context = match state {
                "Init" => FrameStructuralSyntaxSkipperFsmStateContext::Init,
                "SkipComment" => FrameStructuralSyntaxSkipperFsmStateContext::SkipComment,
                "SkipString" => FrameStructuralSyntaxSkipperFsmStateContext::SkipString,
                "FindLineEnd" => FrameStructuralSyntaxSkipperFsmStateContext::FindLineEnd,
                "BalancedParenEnd" => FrameStructuralSyntaxSkipperFsmStateContext::BalancedParenEnd,
                _ => FrameStructuralSyntaxSkipperFsmStateContext::Empty,
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
    pub struct FrameStructuralSyntaxSkipperFsm {
        _state_stack: Vec<FrameStructuralSyntaxSkipperFsmCompartment>,
        __compartment: FrameStructuralSyntaxSkipperFsmCompartment,
        __next_compartment: Option<FrameStructuralSyntaxSkipperFsmCompartment>,
        _context_stack: Vec<FrameStructuralSyntaxSkipperFsmFrameContext>,
        pub bytes: Vec<u8>,
        pub pos: usize,
        pub end: usize,
        pub result_pos: usize,
        pub success: usize,
    }

    #[allow(non_snake_case)]
    impl FrameStructuralSyntaxSkipperFsm {
        pub fn new() -> Self {
            Self {
                _state_stack: Vec::new(),
                _context_stack: Vec::new(),
                bytes: Vec::new(),
                pos: 0,
                end: 0,
                result_pos: 0,
                success: 1,
                __compartment: FrameStructuralSyntaxSkipperFsmCompartment::new("Init"),
                __next_compartment: None,
            }
        }

        pub fn __create() -> Self {
            let mut c = Self::new();
            c.__compartment = c.__prepareEnter("Init");
            let __e = alloc::rc::Rc::new(FrameStructuralSyntaxSkipperFsmFrameEvent::FrameEnter {});
            let __ctx = FrameStructuralSyntaxSkipperFsmFrameContext::new(alloc::rc::Rc::clone(&__e), None);
            c._context_stack.push(__ctx);
            c.__kernel(&__e);
            c._context_stack.pop();
            c
        }

        fn __hsm_chain(&mut self, leaf: &str) -> &'static [&'static str] {
            match leaf {
                "Init" => &["Init"],
                "SkipComment" => &["SkipComment"],
                "SkipString" => &["SkipString"],
                "FindLineEnd" => &["FindLineEnd"],
                "BalancedParenEnd" => &["BalancedParenEnd"],
                _ => &[],
            }
        }

        fn __prepareEnter(&mut self, leaf: &str) -> FrameStructuralSyntaxSkipperFsmCompartment {
            let chain = self.__hsm_chain(leaf);
            let mut comp: Option<FrameStructuralSyntaxSkipperFsmCompartment> = None;
            for name in chain.iter() {
                let mut new_comp = FrameStructuralSyntaxSkipperFsmCompartment::new(name);
                if let Some(parent) = comp.take() {
                    new_comp.parent_compartment = Some(Box::new(parent));
                }
                comp = Some(new_comp);
            }
            comp.expect("chain must contain at least the leaf state")
        }

        fn __kernel(&mut self, __e: &alloc::rc::Rc<FrameStructuralSyntaxSkipperFsmFrameEvent>) {
            // Route event to current state.
            self.__router(__e);
            // Drain any transitions queued by the handler.
            while self.__next_compartment.is_some() {
                let next_compartment = self.__next_compartment.take().expect("invariant: while-loop guard checked is_some()");
                // Exit the current (leaf) state. RFC-0025.1: exit args live in the
                // source state's typed ctx (written at the transition site), so the
                // synthesized `<$` event carries no payload.
                let exit_event = alloc::rc::Rc::new(FrameStructuralSyntaxSkipperFsmFrameEvent::FrameExit {});
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
                        let enter_event = alloc::rc::Rc::new(FrameStructuralSyntaxSkipperFsmFrameEvent::FrameEnter {});
                        self.__router(&enter_event);
                    }
                    Some(fwd) if matches!(fwd, FrameStructuralSyntaxSkipperFsmFrameEvent::FrameEnter { .. }) => {
                        // Forwarded event IS $> — dispatch directly so the
                        // destination's $> handler receives the caller's payload.
                        let fwd_rc = alloc::rc::Rc::new(fwd);
                        self.__router(&fwd_rc);
                    }
                    Some(fwd) => {
                        // Forwarded event is not $> — initialize the destination
                        // with a fresh $>, then dispatch the forward.
                        let enter_event = alloc::rc::Rc::new(FrameStructuralSyntaxSkipperFsmFrameEvent::FrameEnter {});
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

        fn __router(&mut self, __e: &alloc::rc::Rc<FrameStructuralSyntaxSkipperFsmFrameEvent>) {
            let __ev: &FrameStructuralSyntaxSkipperFsmFrameEvent = __e;
            match self.__compartment.state.as_str() {
                "Init" => self._state_Init(__ev),
                "SkipComment" => self._state_SkipComment(__ev),
                "SkipString" => self._state_SkipString(__ev),
                "FindLineEnd" => self._state_FindLineEnd(__ev),
                "BalancedParenEnd" => self._state_BalancedParenEnd(__ev),
                _ => {}
            }
        }

        fn __transition(&mut self, next_compartment: FrameStructuralSyntaxSkipperFsmCompartment) {
            self.__next_compartment = Some(next_compartment);
        }

        pub fn do_skip_comment(&mut self) {
            let __e = alloc::rc::Rc::new(FrameStructuralSyntaxSkipperFsmFrameEvent::DoSkipComment {});
            let mut __ctx = FrameStructuralSyntaxSkipperFsmFrameContext::new(alloc::rc::Rc::clone(&__e), None);
            self._context_stack.push(__ctx);
            self.__kernel(&__e);
            self._context_stack.pop();
        }

        pub fn do_skip_string(&mut self) {
            let __e = alloc::rc::Rc::new(FrameStructuralSyntaxSkipperFsmFrameEvent::DoSkipString {});
            let mut __ctx = FrameStructuralSyntaxSkipperFsmFrameContext::new(alloc::rc::Rc::clone(&__e), None);
            self._context_stack.push(__ctx);
            self.__kernel(&__e);
            self._context_stack.pop();
        }

        pub fn do_find_line_end(&mut self) {
            let __e = alloc::rc::Rc::new(FrameStructuralSyntaxSkipperFsmFrameEvent::DoFindLineEnd {});
            let mut __ctx = FrameStructuralSyntaxSkipperFsmFrameContext::new(alloc::rc::Rc::clone(&__e), None);
            self._context_stack.push(__ctx);
            self.__kernel(&__e);
            self._context_stack.pop();
        }

        pub fn do_balanced_paren_end(&mut self) {
            let __e = alloc::rc::Rc::new(FrameStructuralSyntaxSkipperFsmFrameEvent::DoBalancedParenEnd {});
            let mut __ctx = FrameStructuralSyntaxSkipperFsmFrameContext::new(alloc::rc::Rc::clone(&__e), None);
            self._context_stack.push(__ctx);
            self.__kernel(&__e);
            self._context_stack.pop();
        }

        fn _state_Init(&mut self, __e: &FrameStructuralSyntaxSkipperFsmFrameEvent) {
            match __e {
                FrameStructuralSyntaxSkipperFsmFrameEvent::DoBalancedParenEnd { .. } => { self._s_Init_hdl_user_do_balanced_paren_end(__e); }
                FrameStructuralSyntaxSkipperFsmFrameEvent::DoFindLineEnd { .. } => { self._s_Init_hdl_user_do_find_line_end(__e); }
                FrameStructuralSyntaxSkipperFsmFrameEvent::DoSkipComment { .. } => { self._s_Init_hdl_user_do_skip_comment(__e); }
                FrameStructuralSyntaxSkipperFsmFrameEvent::DoSkipString { .. } => { self._s_Init_hdl_user_do_skip_string(__e); }
                _ => {}
            }
        }

        fn _state_SkipComment(&mut self, __e: &FrameStructuralSyntaxSkipperFsmFrameEvent) {
            match __e {
                FrameStructuralSyntaxSkipperFsmFrameEvent::FrameEnter { .. } => { self._s_SkipComment_hdl_frame_enter(__e); }
                _ => {}
            }
        }

        fn _state_SkipString(&mut self, __e: &FrameStructuralSyntaxSkipperFsmFrameEvent) {
            match __e {
                FrameStructuralSyntaxSkipperFsmFrameEvent::FrameEnter { .. } => { self._s_SkipString_hdl_frame_enter(__e); }
                _ => {}
            }
        }

        fn _state_FindLineEnd(&mut self, __e: &FrameStructuralSyntaxSkipperFsmFrameEvent) {
            match __e {
                FrameStructuralSyntaxSkipperFsmFrameEvent::FrameEnter { .. } => { self._s_FindLineEnd_hdl_frame_enter(__e); }
                _ => {}
            }
        }

        fn _state_BalancedParenEnd(&mut self, __e: &FrameStructuralSyntaxSkipperFsmFrameEvent) {
            match __e {
                FrameStructuralSyntaxSkipperFsmFrameEvent::FrameEnter { .. } => { self._s_BalancedParenEnd_hdl_frame_enter(__e); }
                _ => {}
            }
        }

        fn _s_Init_hdl_user_do_balanced_paren_end(&mut self, __e: &FrameStructuralSyntaxSkipperFsmFrameEvent) {
            let mut __compartment = self.__prepareEnter("BalancedParenEnd");
            self.__transition(__compartment);
            return;
        }

        fn _s_Init_hdl_user_do_find_line_end(&mut self, __e: &FrameStructuralSyntaxSkipperFsmFrameEvent) {
            let mut __compartment = self.__prepareEnter("FindLineEnd");
            self.__transition(__compartment);
            return;
        }

        fn _s_Init_hdl_user_do_skip_comment(&mut self, __e: &FrameStructuralSyntaxSkipperFsmFrameEvent) {
            let mut __compartment = self.__prepareEnter("SkipComment");
            self.__transition(__compartment);
            return;
        }

        fn _s_Init_hdl_user_do_skip_string(&mut self, __e: &FrameStructuralSyntaxSkipperFsmFrameEvent) {
            let mut __compartment = self.__prepareEnter("SkipString");
            self.__transition(__compartment);
            return;
        }

        fn _s_SkipComment_hdl_frame_enter(&mut self, __e: &FrameStructuralSyntaxSkipperFsmFrameEvent) {
            // `//` line comment.
            if let Some(j) = skip_line_comment(&self.bytes, self.pos, self.end) {
                self.result_pos = j;
                self.success = 1;
                return
            }
            // `#` line comment (Python / Ruby / Erlang).
            if let Some(j) = skip_hash_line_comment(&self.bytes, self.pos, self.end) {
                self.result_pos = j;
                self.success = 1;
                return
            }
            // Nested `/* … */` block comment.
            let i = self.pos;
            let end = self.end;
            let bytes = &self.bytes;
            if i + 1 < end && bytes[i] == b'/' && bytes[i + 1] == b'*' {
                let mut j = i + 2;
                let mut depth: i32 = 1;
                while j + 1 < end && depth > 0 {
                    if bytes[j] == b'/' && bytes[j + 1] == b'*' {
                        depth += 1;
                        j += 2;
                        continue;
                    }
                    if bytes[j] == b'*' && bytes[j + 1] == b'/' {
                        depth -= 1;
                        j += 2;
                        continue;
                    }
                    j += 1;
                }
                self.result_pos = j;
                self.success = 1;
                return
            }
            self.success = 0;
        }

        fn _s_SkipString_hdl_frame_enter(&mut self, __e: &FrameStructuralSyntaxSkipperFsmFrameEvent) {
            // Delegate to the shared Rust string/char skipper —
            // identical to what `rust_skipper.frs` uses. Handles
            // `"..."` strings, `'X'` / `'\X'` char literals, and
            // treats lifetimes (`'static`, `'a`) as ordinary
            // bytes (returns None).
            if let Some(j) = skip_rust_string(&self.bytes, self.pos, self.end) {
                self.result_pos = j;
                self.success = 1;
                return
            }
            self.success = 0;
        }

        fn _s_FindLineEnd_hdl_frame_enter(&mut self, __e: &FrameStructuralSyntaxSkipperFsmFrameEvent) {
            let end = self.end;
            let bytes = &self.bytes;
            let mut j = self.pos;
            let mut in_string: u8 = 0;
            
            while j < end {
                let b = bytes[j];
                if b == b'\n' { break; }
            
                if in_string != 0 {
                    if b == b'\\' { j += 2; continue; }
                    if b == in_string { in_string = 0; }
                    j += 1;
                    continue;
                }
            
                if b == b';' || b == b'}' { break; }
                if b == b'/' && j + 1 < end && (bytes[j + 1] == b'/' || bytes[j + 1] == b'*') { break; }
                if b == b'#' { break; }
            
                if b == b'\'' || b == b'"' {
                    in_string = b;
                    j += 1;
                    continue;
                }
            
                j += 1;
            }
            self.result_pos = j;
        }

        fn _s_BalancedParenEnd_hdl_frame_enter(&mut self, __e: &FrameStructuralSyntaxSkipperFsmFrameEvent) {
            if let Some(j) = balanced_paren_end_c_like(&self.bytes, self.pos, self.end) {
                self.result_pos = j;
                self.success = 1;
                return
            }
            self.success = 0;
        }
    }
}
pub use _frame_structural_syntax_skipper_fsm_framec::*;

