
// RFC-0035 Round 12 — the Erlang smart-join emitter's paren-balance check as
// a Frame FSM.
//
// `lexical.rs::paren_balance_unclosed` answers "does this line have more open
// brackets than closes?" while respecting Erlang's lexical modes — string
// (`"..."`), quoted-atom (`'...'`), backslash escapes inside both, and `%`
// line comments. That quote/escape tracking IS a lexical state machine, so it
// becomes one literally: the FSM's STATE is the current lexical mode.
//
//   $Normal      — counting brackets; `"`→$InString, `'`→$InAtom, `%`→done
//   $InString    — inside "..."; `\`→$StringEscape, `"`→$Normal
//   $InAtom      — inside '...'; `\`→$AtomEscape, `'`→$Normal
//   $StringEscape / $AtomEscape — consume one escaped byte, return to the
//                  enclosing literal (so `\"` / `\'` don't close it)
//
// Brackets only ever change `depth` in $Normal — exactly the original's
// `if in_string || in_atom { continue }` guard, here expressed as the mode
// states themselves. The wrapper returns `depth > 0`. Bracket/quote/escape/
// comment are all ASCII, so a byte walk is identical to the original's char
// walk for the boolean result. Rest of `lexical.rs` is stateless string
// utilities (not scanners); `body_processor.rs` is a line-transform pipeline,
// not a clean FSM — neither is forced (RFC-0035 Round 8's lesson).

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
mod _paren_balance_fsm_framec {
    use super::*;
    extern crate alloc;
    use alloc::{vec, format};
    #[derive(Clone, Debug)]
    #[allow(dead_code, non_camel_case_types)]
    enum ParenBalanceFsmFrameEvent {
        Scan {  },
        FrameEnter {},
        FrameExit {},
    }

    #[derive(Clone)]
    #[allow(dead_code, non_camel_case_types)]
    enum ParenBalanceFsmFrameReturn {
        _Lifecycle(alloc::rc::Rc<dyn core::any::Any>),
    }

    #[allow(dead_code)]
    impl ParenBalanceFsmFrameEvent {
        fn name(&self) -> &'static str {
            match self {
                ParenBalanceFsmFrameEvent::Scan { .. } => "scan",
                ParenBalanceFsmFrameEvent::FrameEnter { .. } => "$>",
                ParenBalanceFsmFrameEvent::FrameExit { .. } => "<$",
            }
        }
    }

    #[derive(Clone, Debug)]
    #[allow(dead_code, non_camel_case_types)]
    enum ParenBalanceFsmFrameValue {
        Int(i64),
        Float(f64),
        Bool(bool),
        Str(String),
        List(Vec<Self>),
        Dict(alloc::collections::BTreeMap<String, Self>),
    }

    #[allow(dead_code, non_camel_case_types)]
    struct ParenBalanceFsmFrameContext {
        event: alloc::rc::Rc<ParenBalanceFsmFrameEvent>,
        _return: Option<ParenBalanceFsmFrameReturn>,
        _data: alloc::collections::BTreeMap<String, ParenBalanceFsmFrameValue>,
        _transitioned: bool,
    }

    impl ParenBalanceFsmFrameContext {
        fn new(event: alloc::rc::Rc<ParenBalanceFsmFrameEvent>, default_return: Option<ParenBalanceFsmFrameReturn>) -> Self {
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
    enum ParenBalanceFsmStateContext {
        Start,
        Normal,
        InString,
        StringEscape,
        InAtom,
        AtomEscape,
        Done,
        __NoContext,
    }

    impl Default for ParenBalanceFsmStateContext {
        fn default() -> Self {
            ParenBalanceFsmStateContext::Start
        }
    }

    #[allow(dead_code, non_camel_case_types)]
    #[derive(Clone)]
    struct ParenBalanceFsmCompartment {
        state: String,
        state_context: ParenBalanceFsmStateContext,
        forward_event: Option<ParenBalanceFsmFrameEvent>,
        parent_compartment: Option<Box<ParenBalanceFsmCompartment>>,
    }

    impl ParenBalanceFsmCompartment {
        fn new(state: &str) -> Self {
            let state_context = match state {
                "Start" => ParenBalanceFsmStateContext::Start,
                "Normal" => ParenBalanceFsmStateContext::Normal,
                "InString" => ParenBalanceFsmStateContext::InString,
                "StringEscape" => ParenBalanceFsmStateContext::StringEscape,
                "InAtom" => ParenBalanceFsmStateContext::InAtom,
                "AtomEscape" => ParenBalanceFsmStateContext::AtomEscape,
                "Done" => ParenBalanceFsmStateContext::Done,
                _ => ParenBalanceFsmStateContext::__NoContext,
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
    pub struct ParenBalanceFsm {
        _state_stack: Vec<ParenBalanceFsmCompartment>,
        __compartment: ParenBalanceFsmCompartment,
        __next_compartment: Option<ParenBalanceFsmCompartment>,
        _context_stack: Vec<ParenBalanceFsmFrameContext>,
        pub bytes: Vec<u8>,
        pub pos: usize,
        pub end: usize,
        pub depth: i32,
    }

    #[allow(non_snake_case)]
    impl ParenBalanceFsm {
        pub fn new() -> Self {
            Self {
                _state_stack: Vec::new(),
                _context_stack: Vec::new(),
                bytes: Vec::new(),
                pos: 0,
                end: 0,
                depth: 0,
                __compartment: ParenBalanceFsmCompartment::new("Start"),
                __next_compartment: None,
            }
        }

        pub fn __create() -> Self {
            let mut c = Self::new();
            c.__compartment = c.__prepareEnter("Start");
            let __e = alloc::rc::Rc::new(ParenBalanceFsmFrameEvent::FrameEnter {});
            let __ctx = ParenBalanceFsmFrameContext::new(alloc::rc::Rc::clone(&__e), None);
            c._context_stack.push(__ctx);
            c.__kernel(&__e);
            c._context_stack.pop();
            c
        }

        fn __hsm_chain(&mut self, leaf: &str) -> &'static [&'static str] {
            match leaf {
                "Start" => &["Start"],
                "Normal" => &["Normal"],
                "InString" => &["InString"],
                "StringEscape" => &["StringEscape"],
                "InAtom" => &["InAtom"],
                "AtomEscape" => &["AtomEscape"],
                "Done" => &["Done"],
                _ => &[],
            }
        }

        fn __prepareEnter(&mut self, leaf: &str) -> ParenBalanceFsmCompartment {
            let chain = self.__hsm_chain(leaf);
            let mut comp: Option<ParenBalanceFsmCompartment> = None;
            for name in chain.iter() {
                let mut new_comp = ParenBalanceFsmCompartment::new(name);
                if let Some(parent) = comp.take() {
                    new_comp.parent_compartment = Some(Box::new(parent));
                }
                comp = Some(new_comp);
            }
            comp.expect("chain must contain at least the leaf state")
        }

        fn __kernel(&mut self, __e: &alloc::rc::Rc<ParenBalanceFsmFrameEvent>) {
            // Route event to current state.
            self.__router(__e);
            // Drain any transitions queued by the handler.
            while self.__next_compartment.is_some() {
                let next_compartment = self.__next_compartment.take().expect("invariant: while-loop guard checked is_some()");
                // Exit the current (leaf) state. RFC-0025.1: exit args live in the
                // source state's typed ctx (written at the transition site), so the
                // synthesized `<$` event carries no payload.
                let exit_event = alloc::rc::Rc::new(ParenBalanceFsmFrameEvent::FrameExit {});
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
                        let enter_event = alloc::rc::Rc::new(ParenBalanceFsmFrameEvent::FrameEnter {});
                        self.__router(&enter_event);
                    }
                    Some(fwd) if matches!(fwd, ParenBalanceFsmFrameEvent::FrameEnter { .. }) => {
                        // Forwarded event IS $> — dispatch directly so the
                        // destination's $> handler receives the caller's payload.
                        let fwd_rc = alloc::rc::Rc::new(fwd);
                        self.__router(&fwd_rc);
                    }
                    Some(fwd) => {
                        // Forwarded event is not $> — initialize the destination
                        // with a fresh $>, then dispatch the forward.
                        let enter_event = alloc::rc::Rc::new(ParenBalanceFsmFrameEvent::FrameEnter {});
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

        fn __router(&mut self, __e: &alloc::rc::Rc<ParenBalanceFsmFrameEvent>) {
            let __ev: &ParenBalanceFsmFrameEvent = __e;
            match self.__compartment.state.as_str() {
                "Start" => self._state_Start(__ev),
                "Normal" => self._state_Normal(__ev),
                "InString" => self._state_InString(__ev),
                "StringEscape" => self._state_StringEscape(__ev),
                "InAtom" => self._state_InAtom(__ev),
                "AtomEscape" => self._state_AtomEscape(__ev),
                "Done" => self._state_Done(__ev),
                _ => {}
            }
        }

        fn __transition(&mut self, next_compartment: ParenBalanceFsmCompartment) {
            self.__next_compartment = Some(next_compartment);
        }

        pub fn scan(&mut self) {
            let __e = alloc::rc::Rc::new(ParenBalanceFsmFrameEvent::Scan {});
            let mut __ctx = ParenBalanceFsmFrameContext::new(alloc::rc::Rc::clone(&__e), None);
            self._context_stack.push(__ctx);
            self.__kernel(&__e);
            self._context_stack.pop();
        }

        fn _state_Start(&mut self, __e: &ParenBalanceFsmFrameEvent) {
            match __e {
                ParenBalanceFsmFrameEvent::Scan { .. } => { self._s_Start_hdl_user_scan(__e); }
                _ => {}
            }
        }

        fn _state_Normal(&mut self, __e: &ParenBalanceFsmFrameEvent) {
            match __e {
                ParenBalanceFsmFrameEvent::FrameEnter { .. } => { self._s_Normal_hdl_frame_enter(__e); }
                _ => {}
            }
        }

        fn _state_InString(&mut self, __e: &ParenBalanceFsmFrameEvent) {
            match __e {
                ParenBalanceFsmFrameEvent::FrameEnter { .. } => { self._s_InString_hdl_frame_enter(__e); }
                _ => {}
            }
        }

        fn _state_StringEscape(&mut self, __e: &ParenBalanceFsmFrameEvent) {
            match __e {
                ParenBalanceFsmFrameEvent::FrameEnter { .. } => { self._s_StringEscape_hdl_frame_enter(__e); }
                _ => {}
            }
        }

        fn _state_InAtom(&mut self, __e: &ParenBalanceFsmFrameEvent) {
            match __e {
                ParenBalanceFsmFrameEvent::FrameEnter { .. } => { self._s_InAtom_hdl_frame_enter(__e); }
                _ => {}
            }
        }

        fn _state_AtomEscape(&mut self, __e: &ParenBalanceFsmFrameEvent) {
            match __e {
                ParenBalanceFsmFrameEvent::FrameEnter { .. } => { self._s_AtomEscape_hdl_frame_enter(__e); }
                _ => {}
            }
        }

        fn _state_Done(&mut self, __e: &ParenBalanceFsmFrameEvent) {
            match __e {
                _ => {}
            }
        }

        fn _s_Start_hdl_user_scan(&mut self, __e: &ParenBalanceFsmFrameEvent) {
            let mut __compartment = self.__prepareEnter("Normal");
            self.__transition(__compartment);
            return;
        }

        fn _s_Normal_hdl_frame_enter(&mut self, __e: &ParenBalanceFsmFrameEvent) {
            if self.pos >= self.end {
                let mut __compartment = self.__prepareEnter("Done");
                self.__transition(__compartment);
                return;
            }
            let c = self.bytes[self.pos];
            if c == b'"' {
                self.pos += 1;
                let mut __compartment = self.__prepareEnter("InString");
                self.__transition(__compartment);
                return;
            }
            if c == b'\'' {
                self.pos += 1;
                let mut __compartment = self.__prepareEnter("InAtom");
                self.__transition(__compartment);
                return;
            }
            if c == b'%' {
                // Line comment — the rest of the line is ignored.
                let mut __compartment = self.__prepareEnter("Done");
                self.__transition(__compartment);
                return;
            }
            if c == b'(' || c == b'[' || c == b'{' {
                self.depth += 1;
            } else if c == b')' || c == b']' || c == b'}' {
                self.depth -= 1;
            }
            self.pos += 1;
            let mut __compartment = self.__prepareEnter("Normal");
            self.__transition(__compartment);
            return;
        }

        fn _s_InString_hdl_frame_enter(&mut self, __e: &ParenBalanceFsmFrameEvent) {
            if self.pos >= self.end {
                let mut __compartment = self.__prepareEnter("Done");
                self.__transition(__compartment);
                return;
            }
            let c = self.bytes[self.pos];
            if c == b'\\' {
                self.pos += 1;
                let mut __compartment = self.__prepareEnter("StringEscape");
                self.__transition(__compartment);
                return;
            }
            if c == b'"' {
                self.pos += 1;
                let mut __compartment = self.__prepareEnter("Normal");
                self.__transition(__compartment);
                return;
            }
            self.pos += 1;
            let mut __compartment = self.__prepareEnter("InString");
            self.__transition(__compartment);
            return;
        }

        fn _s_StringEscape_hdl_frame_enter(&mut self, __e: &ParenBalanceFsmFrameEvent) {
            if self.pos >= self.end {
                let mut __compartment = self.__prepareEnter("Done");
                self.__transition(__compartment);
                return;
            }
            self.pos += 1;
            let mut __compartment = self.__prepareEnter("InString");
            self.__transition(__compartment);
            return;
        }

        fn _s_InAtom_hdl_frame_enter(&mut self, __e: &ParenBalanceFsmFrameEvent) {
            if self.pos >= self.end {
                let mut __compartment = self.__prepareEnter("Done");
                self.__transition(__compartment);
                return;
            }
            let c = self.bytes[self.pos];
            if c == b'\\' {
                self.pos += 1;
                let mut __compartment = self.__prepareEnter("AtomEscape");
                self.__transition(__compartment);
                return;
            }
            if c == b'\'' {
                self.pos += 1;
                let mut __compartment = self.__prepareEnter("Normal");
                self.__transition(__compartment);
                return;
            }
            self.pos += 1;
            let mut __compartment = self.__prepareEnter("InAtom");
            self.__transition(__compartment);
            return;
        }

        fn _s_AtomEscape_hdl_frame_enter(&mut self, __e: &ParenBalanceFsmFrameEvent) {
            if self.pos >= self.end {
                let mut __compartment = self.__prepareEnter("Done");
                self.__transition(__compartment);
                return;
            }
            self.pos += 1;
            let mut __compartment = self.__prepareEnter("InAtom");
            self.__transition(__compartment);
            return;
        }
    }
}
pub use _paren_balance_fsm_framec::*;
