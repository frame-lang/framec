
// Erlang Output Block Parser — Frame state machine (dogfood, #123).
//
// Consumes the exhaustive token stream from OutputBlockLexer (the same
// string/comment-safe scanner Lua uses) and emits the Erlang
// `case … of … end` lowering of Frame's C-style `if/else`/`else if` blocks.
//
// Token kinds: 1=IF, 3=ELSE, 6=LBRACE, 7=RBRACE, 10=NEWLINE, 11=TEXT.
//
// A `{`/`}` is a BLOCK brace only at a line-structural position (`if … {`
// ending its line, `}` starting its line); Erlang tuple/map braces
// (`{call, From}`, `#{…}`) appear mid-line and pass through verbatim. Brace
// nesting (incl. else-if chains, with or without a trailing else) is one
// principled stack pass. Emits the intermediate shape the downstream passes
// (`erlang_nest_early_exits`, comma insertion, `erlang_smart_join`) consume;
// those trim, so exact indentation here is cosmetic.

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
mod _erlang_block_parser_fsm_framec {
    use super::*;
    extern crate alloc;
    use alloc::{vec, format};
    #[derive(Clone, Debug)]
    #[allow(dead_code, non_camel_case_types)]
    enum ErlangBlockParserFsmFrameEvent {
        DoParse {  },
        FrameEnter {},
        FrameExit {},
    }

    #[derive(Clone)]
    #[allow(dead_code, non_camel_case_types)]
    enum ErlangBlockParserFsmFrameReturn {
        _Lifecycle(alloc::rc::Rc<dyn core::any::Any>),
    }

    #[allow(dead_code)]
    impl ErlangBlockParserFsmFrameEvent {
        fn name(&self) -> &'static str {
            match self {
                ErlangBlockParserFsmFrameEvent::DoParse { .. } => "do_parse",
                ErlangBlockParserFsmFrameEvent::FrameEnter { .. } => "$>",
                ErlangBlockParserFsmFrameEvent::FrameExit { .. } => "<$",
            }
        }
    }

    #[derive(Clone, Debug)]
    #[allow(dead_code, non_camel_case_types)]
    enum ErlangBlockParserFsmFrameValue {
        Int(i64),
        Float(f64),
        Bool(bool),
        Str(String),
        List(Vec<Self>),
        Dict(alloc::collections::BTreeMap<String, Self>),
    }

    #[allow(dead_code, non_camel_case_types)]
    struct ErlangBlockParserFsmFrameContext {
        event: alloc::rc::Rc<ErlangBlockParserFsmFrameEvent>,
        _return: Option<ErlangBlockParserFsmFrameReturn>,
        _data: alloc::collections::BTreeMap<String, ErlangBlockParserFsmFrameValue>,
        _transitioned: bool,
    }

    impl ErlangBlockParserFsmFrameContext {
        fn new(event: alloc::rc::Rc<ErlangBlockParserFsmFrameEvent>, default_return: Option<ErlangBlockParserFsmFrameReturn>) -> Self {
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
    enum ErlangBlockParserFsmStateContext {
        Init,
        Parsing,
        __NoContext,
    }

    impl Default for ErlangBlockParserFsmStateContext {
        fn default() -> Self {
            ErlangBlockParserFsmStateContext::Init
        }
    }

    #[allow(dead_code, non_camel_case_types)]
    #[derive(Clone)]
    struct ErlangBlockParserFsmCompartment {
        state: String,
        state_context: ErlangBlockParserFsmStateContext,
        forward_event: Option<ErlangBlockParserFsmFrameEvent>,
        parent_compartment: Option<Box<ErlangBlockParserFsmCompartment>>,
    }

    impl ErlangBlockParserFsmCompartment {
        fn new(state: &str) -> Self {
            let state_context = match state {
                "Init" => ErlangBlockParserFsmStateContext::Init,
                "Parsing" => ErlangBlockParserFsmStateContext::Parsing,
                _ => ErlangBlockParserFsmStateContext::__NoContext,
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
    pub struct ErlangBlockParserFsm {
        _state_stack: Vec<ErlangBlockParserFsmCompartment>,
        __compartment: ErlangBlockParserFsmCompartment,
        __next_compartment: Option<ErlangBlockParserFsmCompartment>,
        _context_stack: Vec<ErlangBlockParserFsmFrameContext>,
        pub bytes: Vec<u8>,
        pub token_kinds: Vec<usize>,
        pub token_starts: Vec<usize>,
        pub token_ends: Vec<usize>,
        pub result: String,
    }

    #[allow(non_snake_case)]
    impl ErlangBlockParserFsm {
        pub fn new() -> Self {
            Self {
                _state_stack: Vec::new(),
                _context_stack: Vec::new(),
                bytes: Vec::new(),
                token_kinds: Vec::new(),
                token_starts: Vec::new(),
                token_ends: Vec::new(),
                result: String::new(),
                __compartment: ErlangBlockParserFsmCompartment::new("Init"),
                __next_compartment: None,
            }
        }

        pub fn __create() -> Self {
            let mut c = Self::new();
            c.__compartment = c.__prepareEnter("Init");
            let __e = alloc::rc::Rc::new(ErlangBlockParserFsmFrameEvent::FrameEnter {});
            let __ctx = ErlangBlockParserFsmFrameContext::new(alloc::rc::Rc::clone(&__e), None);
            c._context_stack.push(__ctx);
            c.__kernel(&__e);
            c._context_stack.pop();
            c
        }

        fn __hsm_chain(&mut self, leaf: &str) -> &'static [&'static str] {
            match leaf {
                "Init" => &["Init"],
                "Parsing" => &["Parsing"],
                _ => &[],
            }
        }

        fn __prepareEnter(&mut self, leaf: &str) -> ErlangBlockParserFsmCompartment {
            let chain = self.__hsm_chain(leaf);
            let mut comp: Option<ErlangBlockParserFsmCompartment> = None;
            for name in chain.iter() {
                let mut new_comp = ErlangBlockParserFsmCompartment::new(name);
                if let Some(parent) = comp.take() {
                    new_comp.parent_compartment = Some(Box::new(parent));
                }
                comp = Some(new_comp);
            }
            comp.expect("chain must contain at least the leaf state")
        }

        fn __kernel(&mut self, __e: &alloc::rc::Rc<ErlangBlockParserFsmFrameEvent>) {
            // Route event to current state.
            self.__router(__e);
            // Drain any transitions queued by the handler.
            while self.__next_compartment.is_some() {
                let next_compartment = self.__next_compartment.take().expect("invariant: while-loop guard checked is_some()");
                // Exit the current (leaf) state. RFC-0025.1: exit args live in the
                // source state's typed ctx (written at the transition site), so the
                // synthesized `<$` event carries no payload.
                let exit_event = alloc::rc::Rc::new(ErlangBlockParserFsmFrameEvent::FrameExit {});
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
                        let enter_event = alloc::rc::Rc::new(ErlangBlockParserFsmFrameEvent::FrameEnter {});
                        self.__router(&enter_event);
                    }
                    Some(fwd) if matches!(fwd, ErlangBlockParserFsmFrameEvent::FrameEnter { .. }) => {
                        // Forwarded event IS $> — dispatch directly so the
                        // destination's $> handler receives the caller's payload.
                        let fwd_rc = alloc::rc::Rc::new(fwd);
                        self.__router(&fwd_rc);
                    }
                    Some(fwd) => {
                        // Forwarded event is not $> — initialize the destination
                        // with a fresh $>, then dispatch the forward.
                        let enter_event = alloc::rc::Rc::new(ErlangBlockParserFsmFrameEvent::FrameEnter {});
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

        fn __router(&mut self, __e: &alloc::rc::Rc<ErlangBlockParserFsmFrameEvent>) {
            let __ev: &ErlangBlockParserFsmFrameEvent = __e;
            match self.__compartment.state.as_str() {
                "Init" => self._state_Init(__ev),
                "Parsing" => self._state_Parsing(__ev),
                _ => {}
            }
        }

        fn __transition(&mut self, next_compartment: ErlangBlockParserFsmCompartment) {
            self.__next_compartment = Some(next_compartment);
        }

        pub fn do_parse(&mut self) {
            let __e = alloc::rc::Rc::new(ErlangBlockParserFsmFrameEvent::DoParse {});
            let mut __ctx = ErlangBlockParserFsmFrameContext::new(alloc::rc::Rc::clone(&__e), None);
            self._context_stack.push(__ctx);
            self.__kernel(&__e);
            self._context_stack.pop();
        }

        fn _state_Init(&mut self, __e: &ErlangBlockParserFsmFrameEvent) {
            match __e {
                ErlangBlockParserFsmFrameEvent::DoParse { .. } => { self._s_Init_hdl_user_do_parse(__e); }
                _ => {}
            }
        }

        fn _state_Parsing(&mut self, __e: &ErlangBlockParserFsmFrameEvent) {
            match __e {
                ErlangBlockParserFsmFrameEvent::FrameEnter { .. } => { self._s_Parsing_hdl_frame_enter(__e); }
                _ => {}
            }
        }

        fn _s_Init_hdl_user_do_parse(&mut self, __e: &ErlangBlockParserFsmFrameEvent) {
            let mut __compartment = self.__prepareEnter("Parsing");
            self.__transition(__compartment);
            return;
        }

        fn _s_Parsing_hdl_frame_enter(&mut self, __e: &ErlangBlockParserFsmFrameEvent) {
            let kinds = self.token_kinds.clone();
            let starts = self.token_starts.clone();
            let ends = self.token_ends.clone();
            let bytes = self.bytes.clone();
            let n = kinds.len();
            
            // Whitespace-only TEXT token.
            let hws = |k: usize| {
                kinds[k] == 11
                    && bytes[starts[k]..ends[k]]
                        .iter()
                        .all(|&b| b == b' ' || b == b'\t')
            };
            // Is `ti` the first meaningful token on its line? (block form,
            // no early `return` — keeps framec's W415 handler-return check
            // from mis-firing on a closure.)
            let at_line_start = |ti: usize| -> bool {
                let mut k = ti;
                let mut is_start = true;
                let mut done = false;
                while k > 0 && !done {
                    k -= 1;
                    if kinds[k] == 10 {
                        done = true;
                    } else if !hws(k) {
                        is_start = false;
                        done = true;
                    }
                }
                is_start
            };
            // Index of the NEWLINE ending `ti`'s line (or `n`).
            let line_end = |ti: usize| -> usize {
                let mut k = ti;
                while k < n && kinds[k] != 10 {
                    k += 1;
                }
                k
            };
            // Last meaningful token on line `(ti, le)` (or `ti`).
            let last_on_line = |ti: usize, le: usize| -> usize {
                let mut k = le;
                let mut found = ti;
                let mut done = false;
                while k > ti && !done {
                    k -= 1;
                    if !hws(k) {
                        found = k;
                        done = true;
                    }
                }
                found
            };
            // First meaningful token at/after `from` within the line.
            let next_on_line = |from: usize, le: usize| -> usize {
                let mut k = from;
                while k < le && hws(k) {
                    k += 1;
                }
                k
            };
            
            // After the `}` at `ti`, is there meaningful content before the
            // enclosing scope closes (a line-start `}`) or the handler ends?
            // If so, a no-else `if` is an EARLY EXIT: its condition, when
            // true, returns; otherwise the trailing code runs. That trailing
            // code must land in the `case`'s false arm, so we emit `; false ->`
            // (not `; false -> ok end`) and DEFER the `end` until this scope
            // closes (tracked by `owed`). Replaces the hand-rolled
            // `erlang_nest_early_exits` post-pass — and fixes its mis-nesting
            // of trailing code into the wrong arm.
            let has_trailing = |ti: usize| -> bool {
                let mut k = ti + 1;
                let mut found = false;
                let mut done = false;
                while k < n && !done {
                    if kinds[k] == 10 || hws(k) {
                        k += 1;
                    } else if kinds[k] == 7 && at_line_start(k) {
                        done = true; // enclosing close — no trailing
                    } else {
                        found = true;
                        done = true;
                    }
                }
                found
            };
            
            // Stack of open cases: (kind, has_else, saved_owed). kind 0 =
            // `if`, 1 = `elif`. `saved_owed` restores `owed` when this case
            // closes; `owed` = deferred early-exit `end`s in the current arm.
            let mut stack: Vec<(u8, bool, usize)> = Vec::new();
            let mut owed: usize = 0;
            let mut ti: usize = 0;
            while ti < n {
                // `if <cond> {` — IF at line start, block `{` ending the line.
                if kinds[ti] == 1 && at_line_start(ti) {
                    let le = line_end(ti);
                    let k = last_on_line(ti + 1, le);
                    if k > ti && kinds[k] == 6 {
                        let cond = String::from_utf8_lossy(&bytes[ends[ti]..starts[k]]);
                        let cond = cond.trim();
                        self.result
                            .push_str(&format!("case ({}) of\n    true ->\n", cond));
                        stack.push((0, false, owed));
                        owed = 0;
                        ti = k + 1;
                        continue;
                    }
                }
            
                // `}` at line start → close / else / else-if.
                if kinds[ti] == 7 && at_line_start(ti) && !stack.is_empty() {
                    let le = line_end(ti);
                    let a = next_on_line(ti + 1, le);
                    // The arm ending at this `}` flushes its deferred
                    // early-exit `end`s before this brace's own handling.
                    while owed > 0 {
                        self.result.push_str("end\n");
                        owed -= 1;
                    }
                    if a < le && kinds[a] == 3 {
                        let k = last_on_line(a + 1, le);
                        let b = next_on_line(a + 1, le);
                        if b < le && kinds[b] == 1 && k > b && kinds[k] == 6 {
                            // `} else if <cond> {`
                            let cond = String::from_utf8_lossy(&bytes[ends[b]..starts[k]]);
                            let cond = cond.trim();
                            let (_, _, saved) = stack.pop().unwrap();
                            self.result.push_str(&format!(
                                "    ; false ->\n        case ({}) of\n            true ->\n",
                                cond
                            ));
                            stack.push((1, false, saved));
                            stack.push((0, false, 0));
                            owed = 0;
                            ti = k + 1;
                            continue;
                        }
                        if kinds[k] == 6 {
                            // `} else {`
                            if let Some(last) = stack.last_mut() {
                                last.1 = true;
                            }
                            self.result.push_str("    ; false ->\n");
                            ti = k + 1;
                            continue;
                        }
                    } else if a >= le {
                        // `}` alone on its line → close the case (+ elifs).
                        let (ctx, has_else, saved) = stack.pop().unwrap();
                        // Early exit: a no-else `if` followed by trailing code
                        // — emit `; false ->` and defer this case's `end` to
                        // the enclosing close (the trailing IS the false arm).
                        if ctx == 0 && !has_else && has_trailing(ti) {
                            self.result.push_str("    ; false ->\n");
                            owed = saved + 1;
                            ti += 1;
                            continue;
                        }
                        owed = saved;
                        if !has_else && ctx == 0 {
                            self.result.push_str("    ; false -> ok\n");
                        }
                        if self.result.trim_end().ends_with("->") {
                            self.result.push_str("    ok\n");
                        }
                        self.result.push_str("end");
                        if ctx == 1 {
                            if !stack.is_empty() {
                                let (_, _, s2) = stack.pop().unwrap();
                                owed = s2;
                                self.result.push_str("\nend");
                            }
                        } else {
                            while let Some(&(octx, _, _)) = stack.last() {
                                if octx != 1 {
                                    break;
                                }
                                let (_, _, s2) = stack.pop().unwrap();
                                owed = s2;
                                if let Some(&(c, _, _)) = stack.last() {
                                    if c == 0 {
                                        let (_, _, s3) = stack.pop().unwrap();
                                        owed = s3;
                                    }
                                }
                                self.result.push_str("\nend");
                            }
                        }
                        self.result.push('\n');
                        ti += 1;
                        continue;
                    }
                    // Otherwise (content after `}` that isn't `else`): a
                    // tuple/expression brace — fall through to verbatim.
                }
            
                // Default: emit the token's bytes verbatim.
                self.result
                    .push_str(&String::from_utf8_lossy(&bytes[starts[ti]..ends[ti]]));
                ti += 1;
            }
            
            // Flush any early-exit `end`s deferred to the handler's end.
            while owed > 0 {
                self.result.push_str("\nend");
                owed -= 1;
            }
        }
    }
}
pub use _erlang_block_parser_fsm_framec::*;
