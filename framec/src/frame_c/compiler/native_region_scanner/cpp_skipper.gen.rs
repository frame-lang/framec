
// C++ syntax skipper — Frame-generated state machine.
// Delegates to shared helpers; adds C++ raw strings R"delim(...)delim"
//
// Helpers used:
//   skip_line_comment, skip_block_comment, skip_simple_string,
//   find_line_end_c_like, balanced_paren_end_c_like
// Inline: R"delim(...)delim" raw strings (checked before skip_simple_string)

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
mod _cpp_syntax_skipper_fsm_framec {
    use super::*;
    extern crate alloc;
    use alloc::{vec, format};
    #[derive(Clone, Debug)]
    #[allow(dead_code, non_camel_case_types)]
    enum CppSyntaxSkipperFsmFrameEvent {
        DoSkipComment {  },
        DoSkipString {  },
        DoFindLineEnd {  },
        DoBalancedParenEnd {  },
        FrameEnter {},
        FrameExit {},
    }

    #[derive(Clone)]
    #[allow(dead_code, non_camel_case_types)]
    enum CppSyntaxSkipperFsmFrameReturn {
        _Lifecycle(alloc::rc::Rc<dyn core::any::Any>),
    }

    #[allow(dead_code)]
    impl CppSyntaxSkipperFsmFrameEvent {
        fn name(&self) -> &'static str {
            match self {
                CppSyntaxSkipperFsmFrameEvent::DoSkipComment { .. } => "do_skip_comment",
                CppSyntaxSkipperFsmFrameEvent::DoSkipString { .. } => "do_skip_string",
                CppSyntaxSkipperFsmFrameEvent::DoFindLineEnd { .. } => "do_find_line_end",
                CppSyntaxSkipperFsmFrameEvent::DoBalancedParenEnd { .. } => "do_balanced_paren_end",
                CppSyntaxSkipperFsmFrameEvent::FrameEnter { .. } => "$>",
                CppSyntaxSkipperFsmFrameEvent::FrameExit { .. } => "<$",
            }
        }
    }

    #[derive(Clone, Debug)]
    #[allow(dead_code, non_camel_case_types)]
    enum CppSyntaxSkipperFsmFrameValue {
        Int(i64),
        Float(f64),
        Bool(bool),
        Str(String),
        List(Vec<Self>),
        Dict(alloc::collections::BTreeMap<String, Self>),
    }

    #[allow(dead_code, non_camel_case_types)]
    struct CppSyntaxSkipperFsmFrameContext {
        event: alloc::rc::Rc<CppSyntaxSkipperFsmFrameEvent>,
        _return: Option<CppSyntaxSkipperFsmFrameReturn>,
        _data: alloc::collections::BTreeMap<String, CppSyntaxSkipperFsmFrameValue>,
        _transitioned: bool,
    }

    impl CppSyntaxSkipperFsmFrameContext {
        fn new(event: alloc::rc::Rc<CppSyntaxSkipperFsmFrameEvent>, default_return: Option<CppSyntaxSkipperFsmFrameReturn>) -> Self {
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
    enum CppSyntaxSkipperFsmStateContext {
        Init,
        SkipComment,
        SkipString,
        FindLineEnd,
        BalancedParenEnd,
        __NoContext,
    }

    impl Default for CppSyntaxSkipperFsmStateContext {
        fn default() -> Self {
            CppSyntaxSkipperFsmStateContext::Init
        }
    }

    #[allow(dead_code, non_camel_case_types)]
    #[derive(Clone)]
    struct CppSyntaxSkipperFsmCompartment {
        state: String,
        state_context: CppSyntaxSkipperFsmStateContext,
        forward_event: Option<CppSyntaxSkipperFsmFrameEvent>,
        parent_compartment: Option<Box<CppSyntaxSkipperFsmCompartment>>,
    }

    impl CppSyntaxSkipperFsmCompartment {
        fn new(state: &str) -> Self {
            let state_context = match state {
                "Init" => CppSyntaxSkipperFsmStateContext::Init,
                "SkipComment" => CppSyntaxSkipperFsmStateContext::SkipComment,
                "SkipString" => CppSyntaxSkipperFsmStateContext::SkipString,
                "FindLineEnd" => CppSyntaxSkipperFsmStateContext::FindLineEnd,
                "BalancedParenEnd" => CppSyntaxSkipperFsmStateContext::BalancedParenEnd,
                _ => CppSyntaxSkipperFsmStateContext::__NoContext,
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
    pub struct CppSyntaxSkipperFsm {
        _state_stack: Vec<CppSyntaxSkipperFsmCompartment>,
        __compartment: CppSyntaxSkipperFsmCompartment,
        __next_compartment: Option<CppSyntaxSkipperFsmCompartment>,
        _context_stack: Vec<CppSyntaxSkipperFsmFrameContext>,
        pub bytes: Vec<u8>,
        pub pos: usize,
        pub end: usize,
        pub result_pos: usize,
        pub success: usize,
    }

    #[allow(non_snake_case)]
    impl CppSyntaxSkipperFsm {
        pub fn new() -> Self {
            Self {
                _state_stack: Vec::new(),
                _context_stack: Vec::new(),
                bytes: Vec::new(),
                pos: 0,
                end: 0,
                result_pos: 0,
                success: 1,
                __compartment: CppSyntaxSkipperFsmCompartment::new("Init"),
                __next_compartment: None,
            }
        }

        pub fn __create() -> Self {
            let mut c = Self::new();
            c.__compartment = c.__prepareEnter("Init");
            let __e = alloc::rc::Rc::new(CppSyntaxSkipperFsmFrameEvent::FrameEnter {});
            let __ctx = CppSyntaxSkipperFsmFrameContext::new(alloc::rc::Rc::clone(&__e), None);
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

        fn __prepareEnter(&mut self, leaf: &str) -> CppSyntaxSkipperFsmCompartment {
            let chain = self.__hsm_chain(leaf);
            let mut comp: Option<CppSyntaxSkipperFsmCompartment> = None;
            for name in chain.iter() {
                let mut new_comp = CppSyntaxSkipperFsmCompartment::new(name);
                if let Some(parent) = comp.take() {
                    new_comp.parent_compartment = Some(Box::new(parent));
                }
                comp = Some(new_comp);
            }
            comp.expect("chain must contain at least the leaf state")
        }

        fn __kernel(&mut self, __e: &alloc::rc::Rc<CppSyntaxSkipperFsmFrameEvent>) {
            // Route event to current state.
            self.__router(__e);
            // Drain any transitions queued by the handler.
            while self.__next_compartment.is_some() {
                let next_compartment = self.__next_compartment.take().expect("invariant: while-loop guard checked is_some()");
                // Exit the current (leaf) state. RFC-0025.1: exit args live in the
                // source state's typed ctx (written at the transition site), so the
                // synthesized `<$` event carries no payload.
                let exit_event = alloc::rc::Rc::new(CppSyntaxSkipperFsmFrameEvent::FrameExit {});
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
                        let enter_event = alloc::rc::Rc::new(CppSyntaxSkipperFsmFrameEvent::FrameEnter {});
                        self.__router(&enter_event);
                    }
                    Some(fwd) if matches!(fwd, CppSyntaxSkipperFsmFrameEvent::FrameEnter { .. }) => {
                        // Forwarded event IS $> — dispatch directly so the
                        // destination's $> handler receives the caller's payload.
                        let fwd_rc = alloc::rc::Rc::new(fwd);
                        self.__router(&fwd_rc);
                    }
                    Some(fwd) => {
                        // Forwarded event is not $> — initialize the destination
                        // with a fresh $>, then dispatch the forward.
                        let enter_event = alloc::rc::Rc::new(CppSyntaxSkipperFsmFrameEvent::FrameEnter {});
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

        fn __router(&mut self, __e: &alloc::rc::Rc<CppSyntaxSkipperFsmFrameEvent>) {
            let __ev: &CppSyntaxSkipperFsmFrameEvent = __e;
            match self.__compartment.state.as_str() {
                "Init" => self._state_Init(__ev),
                "SkipComment" => self._state_SkipComment(__ev),
                "SkipString" => self._state_SkipString(__ev),
                "FindLineEnd" => self._state_FindLineEnd(__ev),
                "BalancedParenEnd" => self._state_BalancedParenEnd(__ev),
                _ => {}
            }
        }

        fn __transition(&mut self, next_compartment: CppSyntaxSkipperFsmCompartment) {
            self.__next_compartment = Some(next_compartment);
        }

        pub fn do_skip_comment(&mut self) {
            let __e = alloc::rc::Rc::new(CppSyntaxSkipperFsmFrameEvent::DoSkipComment {});
            let mut __ctx = CppSyntaxSkipperFsmFrameContext::new(alloc::rc::Rc::clone(&__e), None);
            self._context_stack.push(__ctx);
            self.__kernel(&__e);
            self._context_stack.pop();
        }

        pub fn do_skip_string(&mut self) {
            let __e = alloc::rc::Rc::new(CppSyntaxSkipperFsmFrameEvent::DoSkipString {});
            let mut __ctx = CppSyntaxSkipperFsmFrameContext::new(alloc::rc::Rc::clone(&__e), None);
            self._context_stack.push(__ctx);
            self.__kernel(&__e);
            self._context_stack.pop();
        }

        pub fn do_find_line_end(&mut self) {
            let __e = alloc::rc::Rc::new(CppSyntaxSkipperFsmFrameEvent::DoFindLineEnd {});
            let mut __ctx = CppSyntaxSkipperFsmFrameContext::new(alloc::rc::Rc::clone(&__e), None);
            self._context_stack.push(__ctx);
            self.__kernel(&__e);
            self._context_stack.pop();
        }

        pub fn do_balanced_paren_end(&mut self) {
            let __e = alloc::rc::Rc::new(CppSyntaxSkipperFsmFrameEvent::DoBalancedParenEnd {});
            let mut __ctx = CppSyntaxSkipperFsmFrameContext::new(alloc::rc::Rc::clone(&__e), None);
            self._context_stack.push(__ctx);
            self.__kernel(&__e);
            self._context_stack.pop();
        }

        fn _state_Init(&mut self, __e: &CppSyntaxSkipperFsmFrameEvent) {
            match __e {
                CppSyntaxSkipperFsmFrameEvent::DoBalancedParenEnd { .. } => { self._s_Init_hdl_user_do_balanced_paren_end(__e); }
                CppSyntaxSkipperFsmFrameEvent::DoFindLineEnd { .. } => { self._s_Init_hdl_user_do_find_line_end(__e); }
                CppSyntaxSkipperFsmFrameEvent::DoSkipComment { .. } => { self._s_Init_hdl_user_do_skip_comment(__e); }
                CppSyntaxSkipperFsmFrameEvent::DoSkipString { .. } => { self._s_Init_hdl_user_do_skip_string(__e); }
                _ => {}
            }
        }

        fn _state_SkipComment(&mut self, __e: &CppSyntaxSkipperFsmFrameEvent) {
            match __e {
                CppSyntaxSkipperFsmFrameEvent::FrameEnter { .. } => { self._s_SkipComment_hdl_frame_enter(__e); }
                _ => {}
            }
        }

        fn _state_SkipString(&mut self, __e: &CppSyntaxSkipperFsmFrameEvent) {
            match __e {
                CppSyntaxSkipperFsmFrameEvent::FrameEnter { .. } => { self._s_SkipString_hdl_frame_enter(__e); }
                _ => {}
            }
        }

        fn _state_FindLineEnd(&mut self, __e: &CppSyntaxSkipperFsmFrameEvent) {
            match __e {
                CppSyntaxSkipperFsmFrameEvent::FrameEnter { .. } => { self._s_FindLineEnd_hdl_frame_enter(__e); }
                _ => {}
            }
        }

        fn _state_BalancedParenEnd(&mut self, __e: &CppSyntaxSkipperFsmFrameEvent) {
            match __e {
                CppSyntaxSkipperFsmFrameEvent::FrameEnter { .. } => { self._s_BalancedParenEnd_hdl_frame_enter(__e); }
                _ => {}
            }
        }

        fn _s_Init_hdl_user_do_balanced_paren_end(&mut self, __e: &CppSyntaxSkipperFsmFrameEvent) {
            let mut __compartment = self.__prepareEnter("BalancedParenEnd");
            self.__transition(__compartment);
            return;
        }

        fn _s_Init_hdl_user_do_find_line_end(&mut self, __e: &CppSyntaxSkipperFsmFrameEvent) {
            let mut __compartment = self.__prepareEnter("FindLineEnd");
            self.__transition(__compartment);
            return;
        }

        fn _s_Init_hdl_user_do_skip_comment(&mut self, __e: &CppSyntaxSkipperFsmFrameEvent) {
            let mut __compartment = self.__prepareEnter("SkipComment");
            self.__transition(__compartment);
            return;
        }

        fn _s_Init_hdl_user_do_skip_string(&mut self, __e: &CppSyntaxSkipperFsmFrameEvent) {
            let mut __compartment = self.__prepareEnter("SkipString");
            self.__transition(__compartment);
            return;
        }

        fn _s_SkipComment_hdl_frame_enter(&mut self, __e: &CppSyntaxSkipperFsmFrameEvent) {
            if let Some(j) = skip_line_comment(&self.bytes, self.pos, self.end) {
                self.result_pos = j;
                self.success = 1;
                return
            }
            if let Some(j) = skip_block_comment(&self.bytes, self.pos, self.end) {
                self.result_pos = j;
                self.success = 1;
                return
            }
            self.success = 0;
        }

        fn _s_SkipString_hdl_frame_enter(&mut self, __e: &CppSyntaxSkipperFsmFrameEvent) {
            let i = self.pos;
            let end = self.end;
            let bytes = &self.bytes;
            // C++ raw string: R"delim(...)delim" (must check before simple string)
            if i + 1 < end && bytes[i] == b'R' && bytes[i + 1] == b'"' {
                let mut j = i + 2;
                let mut delim: Vec<u8> = Vec::new();
                while j < end && bytes[j] != b'(' {
                    delim.push(bytes[j]);
                    j += 1;
                    if delim.len() > 32 {
                        self.success = 0;
                        return
                    }
                }
                if j >= end || bytes[j] != b'(' {
                    self.success = 0;
                    return
                }
                j += 1; // skip (
                while j < end {
                    if bytes[j] == b')' {
                        let mut k = j + 1;
                        let mut m: usize = 0;
                        while m < delim.len() && k < end && bytes[k] == delim[m] {
                            k += 1;
                            m += 1;
                        }
                        if m == delim.len() && k < end && bytes[k] == b'"' {
                            self.result_pos = k + 1;
                            self.success = 1;
                            return
                        }
                    }
                    j += 1;
                }
                self.result_pos = end;
                self.success = 1;
                return
            }
            // Simple string via shared helper
            if let Some(j) = skip_simple_string(&self.bytes, self.pos, self.end) {
                self.result_pos = j;
                self.success = 1;
                return
            }
            self.success = 0;
        }

        fn _s_FindLineEnd_hdl_frame_enter(&mut self, __e: &CppSyntaxSkipperFsmFrameEvent) {
            self.result_pos = find_line_end_c_like(&self.bytes, self.pos, self.end);
        }

        fn _s_BalancedParenEnd_hdl_frame_enter(&mut self, __e: &CppSyntaxSkipperFsmFrameEvent) {
            if let Some(j) = balanced_paren_end_c_like(&self.bytes, self.pos, self.end) {
                self.result_pos = j;
                self.success = 1;
                return
            }
            self.success = 0;
        }
    }
}
pub use _cpp_syntax_skipper_fsm_framec::*;

