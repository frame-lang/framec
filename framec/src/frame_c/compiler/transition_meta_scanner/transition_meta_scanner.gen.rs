
// RFC-0035 Round 11 — the transition-string metadata parser as a Frame FSM.
//
// `metadata.rs::extract_segment_metadata` is a flat per-kind dispatch, NOT a
// scanner — so it is not itself an FSM candidate. Its one genuinely
// parser-shaped arm is the `Transition` case: it decomposes a transition
// string against the grammar
//
//     (exit_args)? -> (=>)? (enter_args)? ( $State(state_args)? | pop$ )  "label"?
//
// into its components. That grammar has real positional structure, so it
// becomes a multi-state walk — one state per grammar element, each extracting
// its piece into a domain field, threading `arrow_pos` / `last_state_start`
// forward. The wrapper assembles the `SegmentMetadata::Transition` from the
// fields. Byte-identical to the hand-rolled extraction; the matrix + snapshot
// suites are the gate.
//
// (The rest of metadata.rs stays a native `match kind` dispatch — forcing it
// into states would be decorative, RFC-0035 Round 8's lesson.)

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
mod _transition_meta_scanner_fsm_framec {
    use super::*;
    extern crate alloc;
    use alloc::{vec, format};
    #[derive(Clone, Debug)]
    #[allow(dead_code, non_camel_case_types)]
    enum TransitionMetaScannerFsmFrameEvent {
        Parse {  },
        FrameEnter {},
        FrameExit {},
    }

    #[derive(Clone)]
    #[allow(dead_code, non_camel_case_types)]
    enum TransitionMetaScannerFsmFrameReturn {
        _Lifecycle(alloc::rc::Rc<dyn core::any::Any>),
    }

    #[allow(dead_code)]
    impl TransitionMetaScannerFsmFrameEvent {
        fn name(&self) -> &'static str {
            match self {
                TransitionMetaScannerFsmFrameEvent::Parse { .. } => "parse",
                TransitionMetaScannerFsmFrameEvent::FrameEnter { .. } => "$>",
                TransitionMetaScannerFsmFrameEvent::FrameExit { .. } => "<$",
            }
        }
    }

    #[derive(Clone, Debug)]
    #[allow(dead_code, non_camel_case_types)]
    enum TransitionMetaScannerFsmFrameValue {
        Int(i64),
        Float(f64),
        Bool(bool),
        Str(String),
        List(Vec<Self>),
        Dict(alloc::collections::BTreeMap<String, Self>),
    }

    #[allow(dead_code, non_camel_case_types)]
    struct TransitionMetaScannerFsmFrameContext {
        event: alloc::rc::Rc<TransitionMetaScannerFsmFrameEvent>,
        _return: Option<TransitionMetaScannerFsmFrameReturn>,
        _data: alloc::collections::BTreeMap<String, TransitionMetaScannerFsmFrameValue>,
        _transitioned: bool,
    }

    impl TransitionMetaScannerFsmFrameContext {
        fn new(event: alloc::rc::Rc<TransitionMetaScannerFsmFrameEvent>, default_return: Option<TransitionMetaScannerFsmFrameReturn>) -> Self {
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
    enum TransitionMetaScannerFsmStateContext {
        Start,
        Target,
        ExitArgs,
        EnterArgs,
        StateArgs,
        LabelForward,
        Done,
        __NoContext,
    }

    impl Default for TransitionMetaScannerFsmStateContext {
        fn default() -> Self {
            TransitionMetaScannerFsmStateContext::Start
        }
    }

    #[allow(dead_code, non_camel_case_types)]
    #[derive(Clone)]
    struct TransitionMetaScannerFsmCompartment {
        state: String,
        state_context: TransitionMetaScannerFsmStateContext,
        forward_event: Option<TransitionMetaScannerFsmFrameEvent>,
        parent_compartment: Option<Box<TransitionMetaScannerFsmCompartment>>,
    }

    impl TransitionMetaScannerFsmCompartment {
        fn new(state: &str) -> Self {
            let state_context = match state {
                "Start" => TransitionMetaScannerFsmStateContext::Start,
                "Target" => TransitionMetaScannerFsmStateContext::Target,
                "ExitArgs" => TransitionMetaScannerFsmStateContext::ExitArgs,
                "EnterArgs" => TransitionMetaScannerFsmStateContext::EnterArgs,
                "StateArgs" => TransitionMetaScannerFsmStateContext::StateArgs,
                "LabelForward" => TransitionMetaScannerFsmStateContext::LabelForward,
                "Done" => TransitionMetaScannerFsmStateContext::Done,
                _ => TransitionMetaScannerFsmStateContext::__NoContext,
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
    pub struct TransitionMetaScannerFsm {
        _state_stack: Vec<TransitionMetaScannerFsmCompartment>,
        __compartment: TransitionMetaScannerFsmCompartment,
        __next_compartment: Option<TransitionMetaScannerFsmCompartment>,
        _context_stack: Vec<TransitionMetaScannerFsmFrameContext>,
        pub bytes: Vec<u8>,
        pub arrow_pos: usize,
        pub last_state_start: usize,
        pub has_pop: bool,
        pub is_forward: bool,
        pub target: String,
        pub exit_args: Option<String>,
        pub enter_args: Option<String>,
        pub state_args: Option<String>,
        pub label: Option<String>,
    }

    #[allow(non_snake_case)]
    impl TransitionMetaScannerFsm {
        pub fn new() -> Self {
            Self {
                _state_stack: Vec::new(),
                _context_stack: Vec::new(),
                bytes: Vec::new(),
                arrow_pos: 0,
                last_state_start: 0,
                has_pop: false,
                is_forward: false,
                target: String::new(),
                exit_args: None,
                enter_args: None,
                state_args: None,
                label: None,
                __compartment: TransitionMetaScannerFsmCompartment::new("Start"),
                __next_compartment: None,
            }
        }

        pub fn __create() -> Self {
            let mut c = Self::new();
            c.__compartment = c.__prepareEnter("Start");
            let __e = alloc::rc::Rc::new(TransitionMetaScannerFsmFrameEvent::FrameEnter {});
            let __ctx = TransitionMetaScannerFsmFrameContext::new(alloc::rc::Rc::clone(&__e), None);
            c._context_stack.push(__ctx);
            c.__kernel(&__e);
            c._context_stack.pop();
            c
        }

        fn __hsm_chain(&mut self, leaf: &str) -> &'static [&'static str] {
            match leaf {
                "Start" => &["Start"],
                "Target" => &["Target"],
                "ExitArgs" => &["ExitArgs"],
                "EnterArgs" => &["EnterArgs"],
                "StateArgs" => &["StateArgs"],
                "LabelForward" => &["LabelForward"],
                "Done" => &["Done"],
                _ => &[],
            }
        }

        fn __prepareEnter(&mut self, leaf: &str) -> TransitionMetaScannerFsmCompartment {
            let chain = self.__hsm_chain(leaf);
            let mut comp: Option<TransitionMetaScannerFsmCompartment> = None;
            for name in chain.iter() {
                let mut new_comp = TransitionMetaScannerFsmCompartment::new(name);
                if let Some(parent) = comp.take() {
                    new_comp.parent_compartment = Some(Box::new(parent));
                }
                comp = Some(new_comp);
            }
            comp.expect("chain must contain at least the leaf state")
        }

        fn __kernel(&mut self, __e: &alloc::rc::Rc<TransitionMetaScannerFsmFrameEvent>) {
            // Route event to current state.
            self.__router(__e);
            // Drain any transitions queued by the handler.
            while self.__next_compartment.is_some() {
                let next_compartment = self.__next_compartment.take().expect("invariant: while-loop guard checked is_some()");
                // Exit the current (leaf) state. RFC-0025.1: exit args live in the
                // source state's typed ctx (written at the transition site), so the
                // synthesized `<$` event carries no payload.
                let exit_event = alloc::rc::Rc::new(TransitionMetaScannerFsmFrameEvent::FrameExit {});
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
                        let enter_event = alloc::rc::Rc::new(TransitionMetaScannerFsmFrameEvent::FrameEnter {});
                        self.__router(&enter_event);
                    }
                    Some(fwd) if matches!(fwd, TransitionMetaScannerFsmFrameEvent::FrameEnter { .. }) => {
                        // Forwarded event IS $> — dispatch directly so the
                        // destination's $> handler receives the caller's payload.
                        let fwd_rc = alloc::rc::Rc::new(fwd);
                        self.__router(&fwd_rc);
                    }
                    Some(fwd) => {
                        // Forwarded event is not $> — initialize the destination
                        // with a fresh $>, then dispatch the forward.
                        let enter_event = alloc::rc::Rc::new(TransitionMetaScannerFsmFrameEvent::FrameEnter {});
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

        fn __router(&mut self, __e: &alloc::rc::Rc<TransitionMetaScannerFsmFrameEvent>) {
            let __ev: &TransitionMetaScannerFsmFrameEvent = __e;
            match self.__compartment.state.as_str() {
                "Start" => self._state_Start(__ev),
                "Target" => self._state_Target(__ev),
                "ExitArgs" => self._state_ExitArgs(__ev),
                "EnterArgs" => self._state_EnterArgs(__ev),
                "StateArgs" => self._state_StateArgs(__ev),
                "LabelForward" => self._state_LabelForward(__ev),
                "Done" => self._state_Done(__ev),
                _ => {}
            }
        }

        fn __transition(&mut self, next_compartment: TransitionMetaScannerFsmCompartment) {
            self.__next_compartment = Some(next_compartment);
        }

        pub fn parse(&mut self) {
            let __e = alloc::rc::Rc::new(TransitionMetaScannerFsmFrameEvent::Parse {});
            let mut __ctx = TransitionMetaScannerFsmFrameContext::new(alloc::rc::Rc::clone(&__e), None);
            self._context_stack.push(__ctx);
            self.__kernel(&__e);
            self._context_stack.pop();
        }

        fn _state_Start(&mut self, __e: &TransitionMetaScannerFsmFrameEvent) {
            match __e {
                TransitionMetaScannerFsmFrameEvent::Parse { .. } => { self._s_Start_hdl_user_parse(__e); }
                _ => {}
            }
        }

        // Target state: the LAST `$Uppercase` identifier (pop$ has none).
        fn _state_Target(&mut self, __e: &TransitionMetaScannerFsmFrameEvent) {
            match __e {
                TransitionMetaScannerFsmFrameEvent::FrameEnter { .. } => { self._s_Target_hdl_frame_enter(__e); }
                _ => {}
            }
        }

        // Exit args: `(args)` before the `->`.
        fn _state_ExitArgs(&mut self, __e: &TransitionMetaScannerFsmFrameEvent) {
            match __e {
                TransitionMetaScannerFsmFrameEvent::FrameEnter { .. } => { self._s_ExitArgs_hdl_frame_enter(__e); }
                _ => {}
            }
        }

        // Enter args: `(args)` after `->` and before the `$State`.
        fn _state_EnterArgs(&mut self, __e: &TransitionMetaScannerFsmFrameEvent) {
            match __e {
                TransitionMetaScannerFsmFrameEvent::FrameEnter { .. } => { self._s_EnterArgs_hdl_frame_enter(__e); }
                _ => {}
            }
        }

        // State args: `(args)` immediately after the `$State` name.
        fn _state_StateArgs(&mut self, __e: &TransitionMetaScannerFsmFrameEvent) {
            match __e {
                TransitionMetaScannerFsmFrameEvent::FrameEnter { .. } => { self._s_StateArgs_hdl_frame_enter(__e); }
                _ => {}
            }
        }

        // Label (`"..."` after `->`) and event-forwarding (`=>` before target).
        fn _state_LabelForward(&mut self, __e: &TransitionMetaScannerFsmFrameEvent) {
            match __e {
                TransitionMetaScannerFsmFrameEvent::FrameEnter { .. } => { self._s_LabelForward_hdl_frame_enter(__e); }
                _ => {}
            }
        }

        fn _state_Done(&mut self, __e: &TransitionMetaScannerFsmFrameEvent) {
            match __e {
                _ => {}
            }
        }

        fn _s_Start_hdl_user_parse(&mut self, __e: &TransitionMetaScannerFsmFrameEvent) {
            let mut __compartment = self.__prepareEnter("Target");
            self.__transition(__compartment);
            return;
        }

        fn _s_Target_hdl_frame_enter(&mut self, __e: &TransitionMetaScannerFsmFrameEvent) {
            let trimmed = std::str::from_utf8(&self.bytes).unwrap_or("");
            self.has_pop = trimmed.contains("pop$");
            let bytes = &self.bytes;
            let mut last_state_start = 0;
            for i in 0..bytes.len() {
                if bytes[i] == b'$' && i + 1 < bytes.len() && bytes[i + 1].is_ascii_uppercase() {
                    last_state_start = i;
                }
            }
            self.last_state_start = last_state_start;
            if last_state_start > 0 {
                let mut j = last_state_start + 1;
                while j < bytes.len() && (bytes[j].is_ascii_alphanumeric() || bytes[j] == b'_') {
                    j += 1;
                }
                self.target = String::from_utf8_lossy(&bytes[last_state_start + 1..j]).to_string();
            }
            let mut __compartment = self.__prepareEnter("ExitArgs");
            self.__transition(__compartment);
            return;
        }

        fn _s_ExitArgs_hdl_frame_enter(&mut self, __e: &TransitionMetaScannerFsmFrameEvent) {
            let trimmed = std::str::from_utf8(&self.bytes).unwrap_or("");
            let arrow_pos = trimmed.find("->").unwrap_or(0);
            self.arrow_pos = arrow_pos;
            let before_arrow = trimmed[..arrow_pos].trim();
            self.exit_args = if before_arrow.starts_with('(') {
                let inner = before_arrow.trim_start_matches('(').trim_end_matches(')');
                if !inner.is_empty() {
                    Some(inner.to_string())
                } else {
                    None
                }
            } else {
                None
            };
            let mut __compartment = self.__prepareEnter("EnterArgs");
            self.__transition(__compartment);
            return;
        }

        fn _s_EnterArgs_hdl_frame_enter(&mut self, __e: &TransitionMetaScannerFsmFrameEvent) {
            let trimmed = std::str::from_utf8(&self.bytes).unwrap_or("");
            let after_arrow = &trimmed[self.arrow_pos + 2..];
            self.enter_args = if let Some(paren_start) = after_arrow.find('(') {
                let state_pos = after_arrow.find('$').unwrap_or(after_arrow.len());
                if paren_start < state_pos {
                    let paren_text = &after_arrow[paren_start..];
                    let mut depth = 0;
                    let mut end = 0;
                    for (k, &b) in paren_text.as_bytes().iter().enumerate() {
                        if b == b'(' {
                            depth += 1;
                        }
                        if b == b')' {
                            depth -= 1;
                            if depth == 0 {
                                end = k + 1;
                                break;
                            }
                        }
                    }
                    let inner = &paren_text[1..end.saturating_sub(1)];
                    if !inner.is_empty() {
                        Some(inner.to_string())
                    } else {
                        None
                    }
                } else {
                    None
                }
            } else {
                None
            };
            let mut __compartment = self.__prepareEnter("StateArgs");
            self.__transition(__compartment);
            return;
        }

        fn _s_StateArgs_hdl_frame_enter(&mut self, __e: &TransitionMetaScannerFsmFrameEvent) {
            let bytes = &self.bytes;
            let last_state_start = self.last_state_start;
            self.state_args = if last_state_start > 0 {
                let mut j = last_state_start + 1;
                while j < bytes.len() && (bytes[j].is_ascii_alphanumeric() || bytes[j] == b'_') {
                    j += 1;
                }
                if j < bytes.len() && bytes[j] == b'(' {
                    let mut depth = 0;
                    let mut end = j;
                    for k in j..bytes.len() {
                        if bytes[k] == b'(' {
                            depth += 1;
                        }
                        if bytes[k] == b')' {
                            depth -= 1;
                            if depth == 0 {
                                end = k + 1;
                                break;
                            }
                        }
                    }
                    let inner = String::from_utf8_lossy(&bytes[j + 1..end.saturating_sub(1)]).to_string();
                    if !inner.is_empty() {
                        Some(inner)
                    } else {
                        None
                    }
                } else {
                    None
                }
            } else {
                None
            };
            let mut __compartment = self.__prepareEnter("LabelForward");
            self.__transition(__compartment);
            return;
        }

        fn _s_LabelForward_hdl_frame_enter(&mut self, __e: &TransitionMetaScannerFsmFrameEvent) {
            let trimmed = std::str::from_utf8(&self.bytes).unwrap_or("");
            let after_arrow = &trimmed[self.arrow_pos + 2..];
            self.label = after_arrow.find('"').and_then(|q_start| {
                let rest = &after_arrow[q_start + 1..];
                rest.find('"').map(|q_end| rest[..q_end].to_string())
            });
            self.is_forward = if let Some(ap) = trimmed.find("->") {
                let after = &trimmed[ap + 2..];
                let tp = after
                    .find('$')
                    .or_else(|| after.find("pop$"))
                    .unwrap_or(after.len());
                after[..tp].contains("=>")
            } else {
                false
            };
            let mut __compartment = self.__prepareEnter("Done");
            self.__transition(__compartment);
            return;
        }
    }
}
pub use _transition_meta_scanner_fsm_framec::*;
