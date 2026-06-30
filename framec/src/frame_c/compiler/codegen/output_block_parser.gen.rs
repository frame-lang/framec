
// Output Block Parser — Frame state machine.
//
// Consumes exhaustive token stream from OutputBlockLexer.
// Every token is either emitted as-is or transformed.
//
// Since the lexer covers every byte, the parser outputs exactly
// the same text if no transformations apply.
//
// Lua mode (mode=1):
//   IF TEXT LBRACE → "if" TEXT "then"
//   RBRACE ELSE LBRACE → "else"
//   RBRACE ELSE LBRACE IF TEXT LBRACE → "elseif" TEXT "then"
//   WHILE TEXT LBRACE → "while" TEXT "do"
//   RBRACE (block close) → "end"
//   RETURN → emit + mark terminal (skip subsequent non-comment tokens)

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
mod _output_block_parser_fsm_framec {
    use super::*;
    extern crate alloc;
    use alloc::{vec, format};
    #[derive(Clone, Debug)]
    #[allow(dead_code, non_camel_case_types)]
    enum OutputBlockParserFsmFrameEvent {
        DoParse {  },
        FrameEnter {},
        FrameExit {},
    }

    #[derive(Clone)]
    #[allow(dead_code, non_camel_case_types)]
    enum OutputBlockParserFsmFrameReturn {
        _Lifecycle(alloc::rc::Rc<dyn core::any::Any>),
    }

    #[allow(dead_code)]
    impl OutputBlockParserFsmFrameEvent {
        fn name(&self) -> &'static str {
            match self {
                OutputBlockParserFsmFrameEvent::DoParse { .. } => "do_parse",
                OutputBlockParserFsmFrameEvent::FrameEnter { .. } => "$>",
                OutputBlockParserFsmFrameEvent::FrameExit { .. } => "<$",
            }
        }
    }

    #[derive(Clone, Debug)]
    #[allow(dead_code, non_camel_case_types)]
    enum OutputBlockParserFsmFrameValue {
        Int(i64),
        Float(f64),
        Bool(bool),
        Str(String),
        List(Vec<Self>),
        Dict(alloc::collections::BTreeMap<String, Self>),
    }

    #[allow(dead_code, non_camel_case_types)]
    struct OutputBlockParserFsmFrameContext {
        event: alloc::rc::Rc<OutputBlockParserFsmFrameEvent>,
        _return: Option<OutputBlockParserFsmFrameReturn>,
        _data: alloc::collections::BTreeMap<String, OutputBlockParserFsmFrameValue>,
        _transitioned: bool,
    }

    impl OutputBlockParserFsmFrameContext {
        fn new(event: alloc::rc::Rc<OutputBlockParserFsmFrameEvent>, default_return: Option<OutputBlockParserFsmFrameReturn>) -> Self {
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
    enum OutputBlockParserFsmStateContext {
        Init,
        Parsing,
        __NoContext,
    }

    impl Default for OutputBlockParserFsmStateContext {
        fn default() -> Self {
            OutputBlockParserFsmStateContext::Init
        }
    }

    #[allow(dead_code, non_camel_case_types)]
    #[derive(Clone)]
    struct OutputBlockParserFsmCompartment {
        state: String,
        state_context: OutputBlockParserFsmStateContext,
        forward_event: Option<OutputBlockParserFsmFrameEvent>,
        parent_compartment: Option<Box<OutputBlockParserFsmCompartment>>,
    }

    impl OutputBlockParserFsmCompartment {
        fn new(state: &str) -> Self {
            let state_context = match state {
                "Init" => OutputBlockParserFsmStateContext::Init,
                "Parsing" => OutputBlockParserFsmStateContext::Parsing,
                _ => OutputBlockParserFsmStateContext::__NoContext,
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
    pub struct OutputBlockParserFsm {
        _state_stack: Vec<OutputBlockParserFsmCompartment>,
        __compartment: OutputBlockParserFsmCompartment,
        __next_compartment: Option<OutputBlockParserFsmCompartment>,
        _context_stack: Vec<OutputBlockParserFsmFrameContext>,
        pub bytes: Vec<u8>,
        pub mode: usize,
        pub token_kinds: Vec<usize>,
        pub token_starts: Vec<usize>,
        pub token_ends: Vec<usize>,
        pub result: String,
    }

    #[allow(non_snake_case)]
    impl OutputBlockParserFsm {
        pub fn new() -> Self {
            Self {
                _state_stack: Vec::new(),
                _context_stack: Vec::new(),
                bytes: Vec::new(),
                mode: 1,
                token_kinds: Vec::new(),
                token_starts: Vec::new(),
                token_ends: Vec::new(),
                result: String::new(),
                __compartment: OutputBlockParserFsmCompartment::new("Init"),
                __next_compartment: None,
            }
        }

        pub fn __create() -> Self {
            let mut c = Self::new();
            c.__compartment = c.__prepareEnter("Init");
            let __e = alloc::rc::Rc::new(OutputBlockParserFsmFrameEvent::FrameEnter {});
            let __ctx = OutputBlockParserFsmFrameContext::new(alloc::rc::Rc::clone(&__e), None);
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

        fn __prepareEnter(&mut self, leaf: &str) -> OutputBlockParserFsmCompartment {
            let chain = self.__hsm_chain(leaf);
            let mut comp: Option<OutputBlockParserFsmCompartment> = None;
            for name in chain.iter() {
                let mut new_comp = OutputBlockParserFsmCompartment::new(name);
                if let Some(parent) = comp.take() {
                    new_comp.parent_compartment = Some(Box::new(parent));
                }
                comp = Some(new_comp);
            }
            comp.expect("chain must contain at least the leaf state")
        }

        fn __kernel(&mut self, __e: &alloc::rc::Rc<OutputBlockParserFsmFrameEvent>) {
            // Route event to current state.
            self.__router(__e);
            // Drain any transitions queued by the handler.
            while self.__next_compartment.is_some() {
                let next_compartment = self.__next_compartment.take().expect("invariant: while-loop guard checked is_some()");
                // Exit the current (leaf) state. RFC-0025.1: exit args live in the
                // source state's typed ctx (written at the transition site), so the
                // synthesized `<$` event carries no payload.
                let exit_event = alloc::rc::Rc::new(OutputBlockParserFsmFrameEvent::FrameExit {});
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
                        let enter_event = alloc::rc::Rc::new(OutputBlockParserFsmFrameEvent::FrameEnter {});
                        self.__router(&enter_event);
                    }
                    Some(fwd) if matches!(fwd, OutputBlockParserFsmFrameEvent::FrameEnter { .. }) => {
                        // Forwarded event IS $> — dispatch directly so the
                        // destination's $> handler receives the caller's payload.
                        let fwd_rc = alloc::rc::Rc::new(fwd);
                        self.__router(&fwd_rc);
                    }
                    Some(fwd) => {
                        // Forwarded event is not $> — initialize the destination
                        // with a fresh $>, then dispatch the forward.
                        let enter_event = alloc::rc::Rc::new(OutputBlockParserFsmFrameEvent::FrameEnter {});
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

        fn __router(&mut self, __e: &alloc::rc::Rc<OutputBlockParserFsmFrameEvent>) {
            let __ev: &OutputBlockParserFsmFrameEvent = __e;
            match self.__compartment.state.as_str() {
                "Init" => self._state_Init(__ev),
                "Parsing" => self._state_Parsing(__ev),
                _ => {}
            }
        }

        fn __transition(&mut self, next_compartment: OutputBlockParserFsmCompartment) {
            self.__next_compartment = Some(next_compartment);
        }

        pub fn do_parse(&mut self) {
            let __e = alloc::rc::Rc::new(OutputBlockParserFsmFrameEvent::DoParse {});
            let mut __ctx = OutputBlockParserFsmFrameContext::new(alloc::rc::Rc::clone(&__e), None);
            self._context_stack.push(__ctx);
            self.__kernel(&__e);
            self._context_stack.pop();
        }

        fn _state_Init(&mut self, __e: &OutputBlockParserFsmFrameEvent) {
            match __e {
                OutputBlockParserFsmFrameEvent::DoParse { .. } => { self._s_Init_hdl_user_do_parse(__e); }
                _ => {}
            }
        }

        fn _state_Parsing(&mut self, __e: &OutputBlockParserFsmFrameEvent) {
            match __e {
                OutputBlockParserFsmFrameEvent::FrameEnter { .. } => { self._s_Parsing_hdl_frame_enter(__e); }
                _ => {}
            }
        }

        fn _s_Init_hdl_user_do_parse(&mut self, __e: &OutputBlockParserFsmFrameEvent) {
            let mut __compartment = self.__prepareEnter("Parsing");
            self.__transition(__compartment);
            return;
        }

        fn _s_Parsing_hdl_frame_enter(&mut self, __e: &OutputBlockParserFsmFrameEvent) {
            let bytes = &self.bytes;
            let n = self.token_kinds.len();
            let mut ti: usize = 0;
            let mut block_depth: i32 = 0;
            let mut table_depth: i32 = 0;
            let mut after_return = false;
            // RBRACE token indices to swallow silently. When an `else if`
            // ladder is collapsed (`} else { if c { ... } }` → `elseif`),
            // the `else` block and the inner `if` are merged into ONE block,
            // so the inner `if`'s `}` becomes the single `end` and the `else`
            // block's own trailing `}` is redundant — record it here and drop
            // it without touching `block_depth`.
            let mut swallow_close: Vec<usize> = Vec::new();
            
            while ti < n {
                let kind = self.token_kinds[ti];
                let start = self.token_starts[ti];
                let end = self.token_ends[ti];
                let text = String::from_utf8_lossy(&bytes[start..end]).to_string();
            
                // ---- Swallow a collapsed `else` block's redundant `}` ----
                // When an `else if` ladder was collapsed, the `else` block's
                // own closing brace was recorded; drop it here without
                // emitting anything or perturbing the depth counters.
                if kind == 7 && swallow_close.contains(&ti) {
                    ti += 1;
                    continue;
                }
            
                // ---- Table-constructor / expression braces (#122) ----
                // Brace matching is a pushdown problem, but it collapses to
                // two counters here: a Lua table constructor `{ ... }` can
                // only contain expressions, never statements, so a
                // control-flow block is never opened inside a table. The
                // PDA stack is therefore always stratified (blocks below,
                // tables on top), and "table_depth > 0 ⟹ innermost open
                // brace is a table" holds exactly. Control-flow openers
                // (if/while/else) consume their own `{`, so any LBRACE that
                // reaches the main loop opens a table constructor; track its
                // depth so the matching RBRACE is emitted verbatim instead
                // of being converted to `end`. `return` exprs are handled in
                // the RETURN arm below. Guarded by `!after_return`: a
                // `return` cannot appear inside a table, so `table_depth` is
                // always 0 when `after_return` is set — no desync — and
                // unreachable-code stripping is left to the existing logic.
                if !after_return {
                    if kind == 6 {
                        table_depth += 1;
                        self.result.push_str(&text);
                        ti += 1;
                        continue;
                    }
                    if kind == 7 && table_depth > 0 {
                        table_depth -= 1;
                        self.result.push_str(&text);
                        ti += 1;
                        continue;
                    }
                }
            
                // ---- RBRACE: check for else/elseif patterns ----
                if kind == 7 && block_depth > 0 {
                    // Look ahead past whitespace/newline for ELSE
                    let mut scan = ti + 1;
                    while scan < n && (self.token_kinds[scan] == 10 || self.token_kinds[scan] == 11) {
                        let st = &bytes[self.token_starts[scan]..self.token_ends[scan]];
                        if st.iter().all(|&b| b == b' ' || b == b'\t' || b == b'\n') {
                            scan += 1;
                        } else {
                            break;
                        }
                    }
            
                    if scan < n && self.token_kinds[scan] == 2 {
                        // RBRACE ... ELSEIF — look for condition then LBRACE
                        let elseif_ti = scan;
                        scan += 1;
                        let mut cond = String::new();
                        while scan < n && self.token_kinds[scan] != 6 && self.token_kinds[scan] != 10 {
                            let s = self.token_starts[scan];
                            let e = self.token_ends[scan];
                            cond.push_str(&String::from_utf8_lossy(&bytes[s..e]));
                            scan += 1;
                        }
                        if scan < n && self.token_kinds[scan] == 6 {
                            if self.mode == 1 {
                                self.result.push_str("elseif");
                                self.result.push_str(cond.trim_end());
                                self.result.push_str(" then");
                            }
                            ti = scan + 1;
                            after_return = false;
                            continue;
                        }
                    }
            
                    if scan < n && self.token_kinds[scan] == 3 {
                        // RBRACE ... ELSE — look for LBRACE after else
                        let else_ti = scan;
                        scan += 1;
                        while scan < n && (self.token_kinds[scan] == 10 || self.token_kinds[scan] == 11) {
                            let st = &bytes[self.token_starts[scan]..self.token_ends[scan]];
                            if st.iter().all(|&b| b == b' ' || b == b'\t' || b == b'\n') { scan += 1; } else { break; }
                        }
            
                        if scan < n && self.token_kinds[scan] == 1 {
                            // RBRACE ELSE IF ... LBRACE — direct `else if`
                            // chain (the `if` keyword follows `else` with no
                            // intervening `{`, distinct from the `else { if }`
                            // form handled below). The closing `}` would have
                            // become `end`, but here it is collapsed with the
                            // `else if` into Lua's single `elseif` keyword.
                            // No `end` is emitted and no new block is opened
                            // (the `if`'s `{` reuses the same level), so
                            // `block_depth` is unchanged — exactly mirroring
                            // the one-word ELSEIF arm above. This generalises
                            // to N-arm chains: each `} else if c {` link is an
                            // independent RBRACE-ELSE-IF match, and the final
                            // arm's `}` lowers to the lone `end`.
                            let if_ti = scan;
                            scan += 1;
                            let mut cond = String::new();
                            while scan < n && self.token_kinds[scan] != 6 && self.token_kinds[scan] != 10 {
                                let s = self.token_starts[scan];
                                let e = self.token_ends[scan];
                                cond.push_str(&String::from_utf8_lossy(&bytes[s..e]));
                                scan += 1;
                            }
                            if scan < n && self.token_kinds[scan] == 6 {
                                if self.mode == 1 {
                                    self.result.push_str("elseif");
                                    self.result.push_str(cond.trim_end());
                                    self.result.push_str(" then");
                                }
                                ti = scan + 1;
                                after_return = false;
                                continue;
                            }
                            // No LBRACE after `else if <cond>` — not our
                            // pattern; fall through to the plain-`else` /
                            // plain-RBRACE handling below.
                        }
            
                        if scan < n && self.token_kinds[scan] == 6 {
                            let lbrace_ti = scan;
                            // Check for IF after LBRACE (elseif pattern)
                            scan += 1;
                            while scan < n && (self.token_kinds[scan] == 10 || self.token_kinds[scan] == 11) {
                                let st = &bytes[self.token_starts[scan]..self.token_ends[scan]];
                                if st.iter().all(|&b| b == b' ' || b == b'\t' || b == b'\n') { scan += 1; } else { break; }
                            }
            
                            if scan < n && self.token_kinds[scan] == 1 {
                                // RBRACE ELSE LBRACE IF ... LBRACE — candidate
                                // `else if` ladder. Collapsing `} else { if c {`
                                // into `elseif c then` is ONLY valid when the
                                // inner `if` is the SOLE content of the `else`
                                // block: its closing `}` must be the `}` that
                                // closes the `else` block, with no inner `else`
                                // and no trailing statements. Otherwise (#135)
                                // the inner `if` is a genuine nested conditional
                                // and must lower as a normal block — collapsing
                                // it drops a block level and leaks a brace.
                                let if_ti = scan;
                                // Collect condition tokens until next LBRACE
                                scan += 1;
                                let mut cond = String::new();
                                while scan < n && self.token_kinds[scan] != 6 {
                                    let s = self.token_starts[scan];
                                    let e = self.token_ends[scan];
                                    cond.push_str(&String::from_utf8_lossy(&bytes[s..e]));
                                    scan += 1;
                                }
                                if scan < n && self.token_kinds[scan] == 6 {
                                    // `scan` is the inner `if`'s LBRACE. Walk
                                    // forward, matching braces, to find the
                                    // brace that closes the inner `if`'s block.
                                    let mut depth: i32 = 1;
                                    let mut walk = scan + 1;
                                    while walk < n && depth > 0 {
                                        let wk = self.token_kinds[walk];
                                        if wk == 6 { depth += 1; }
                                        else if wk == 7 { depth -= 1; }
                                        if depth == 0 { break; }
                                        walk += 1;
                                    }
                                    // `walk` now points at the inner `if`'s
                                    // closing RBRACE (or off the end). Skip
                                    // whitespace/newlines after it and inspect
                                    // the next significant token.
                                    let mut after = walk + 1;
                                    while after < n && (self.token_kinds[after] == 10 || self.token_kinds[after] == 11) {
                                        let st = &bytes[self.token_starts[after]..self.token_ends[after]];
                                        if st.iter().all(|&b| b == b' ' || b == b'\t' || b == b'\n') { after += 1; } else { break; }
                                    }
                                    // Sole-content iff the inner `if` block is
                                    // immediately followed by the `else` block's
                                    // own RBRACE (kind 7). An `else`/`elseif`
                                    // (kind 3 / 2) belonging to the inner `if`,
                                    // or any trailing content, blocks the
                                    // collapse.
                                    let sole_content = walk < n
                                        && self.token_kinds[walk] == 7
                                        && after < n
                                        && self.token_kinds[after] == 7;
                                    if sole_content {
                                        // Found a true `else if` ladder. The
                                        // inner `if` body will emit the single
                                        // `end`; swallow the `else` block's own
                                        // redundant closing `}` (`after`).
                                        swallow_close.push(after);
                                        if self.mode == 1 {
                                            self.result.push_str("elseif");
                                            self.result.push_str(cond.trim_end());
                                            self.result.push_str(" then");
                                        }
                                        ti = scan + 1;
                                        after_return = false;
                                        continue;
                                    }
                                    // Not a ladder — fall through to the plain
                                    // `else` path so the nested `if` lowers as
                                    // an ordinary block.
                                }
                            }
            
                            // RBRACE ELSE LBRACE → else
                            if self.mode == 1 {
                                self.result.push_str("else");
                            }
                            ti = lbrace_ti + 1;
                            after_return = false;
                            continue;
                        }
                    }
            
                    // Plain RBRACE → end
                    block_depth -= 1;
                    if after_return { after_return = false; }
                    if self.mode == 1 {
                        self.result.push_str("end");
                    }
                    ti += 1;
                    continue;
                }
            
                // ---- IF: look for LBRACE pattern ----
                if kind == 1 && !after_return {
                    // Collect tokens until LBRACE or NEWLINE
                    let mut scan = ti + 1;
                    let mut cond = String::new();
                    let mut found_brace = false;
                    while scan < n {
                        let sk = self.token_kinds[scan];
                        if sk == 6 {
                            // IF ... LBRACE
                            if self.mode == 1 {
                                self.result.push_str("if");
                                self.result.push_str(cond.trim_end());
                                self.result.push_str(" then");
                            }
                            block_depth += 1;
                            ti = scan + 1;
                            found_brace = true;
                            break;
                        }
                        if sk == 10 { break; } // Newline before brace — not our pattern
                        let s = self.token_starts[scan];
                        let e = self.token_ends[scan];
                        cond.push_str(&String::from_utf8_lossy(&bytes[s..e]));
                        scan += 1;
                    }
                    if found_brace { continue; }
                    // Not our pattern — emit as-is
                }
            
                // ---- WHILE: look for LBRACE pattern ----
                if kind == 4 && !after_return {
                    let mut scan = ti + 1;
                    let mut cond = String::new();
                    let mut found_brace = false;
                    while scan < n {
                        let sk = self.token_kinds[scan];
                        if sk == 6 {
                            if self.mode == 1 {
                                self.result.push_str("while");
                                self.result.push_str(cond.trim_end());
                                self.result.push_str(" do");
                            }
                            block_depth += 1;
                            ti = scan + 1;
                            found_brace = true;
                            break;
                        }
                        if sk == 10 { break; }
                        let s = self.token_starts[scan];
                        let e = self.token_ends[scan];
                        cond.push_str(&String::from_utf8_lossy(&bytes[s..e]));
                        scan += 1;
                    }
                    if found_brace { continue; }
                }
            
                // ---- RETURN: emit the return keyword, then continue
                // emitting all subsequent tokens on the same line. We mark
                // `after_return` only after the newline so any expression
                // tokens that follow `return` (e.g. `return self.value`)
                // make it through. Without this, `return X` collapsed to
                // bare `return` and the X token was stripped — Lua tests
                // that returned a value silently returned nothing.
                if kind == 8 && !after_return {
                    self.result.push_str(&text);
                    // Walk subsequent tokens until newline or block close.
                    // A returned table constructor (`return { ... }`) keeps
                    // its braces: track table depth so a table's `}` is
                    // emitted inline rather than ending the walk and being
                    // mis-converted to `end` by the outer loop. (#122)
                    let mut look = ti + 1;
                    let mut ret_table: i32 = 0;
                    while look < n {
                        let lk = self.token_kinds[look];
                        if lk == 10 {
                            // Newline — emit it and now mark after_return.
                            self.result.push_str(
                                &String::from_utf8_lossy(
                                    &self.bytes[self.token_starts[look]..self.token_ends[look]],
                                ),
                            );
                            look += 1;
                            break;
                        }
                        if lk == 6 {
                            // LBRACE — open a table constructor.
                            ret_table += 1;
                            self.result.push_str(
                                &String::from_utf8_lossy(
                                    &self.bytes[self.token_starts[look]..self.token_ends[look]],
                                ),
                            );
                            look += 1;
                            continue;
                        }
                        if lk == 7 && ret_table > 0 {
                            // RBRACE closing a table constructor — emit inline.
                            ret_table -= 1;
                            self.result.push_str(
                                &String::from_utf8_lossy(
                                    &self.bytes[self.token_starts[look]..self.token_ends[look]],
                                ),
                            );
                            look += 1;
                            continue;
                        }
                        if lk == 7 || lk == 9 {
                            // RBRACE / END — let the outer loop handle it
                            // so block_depth is decremented correctly.
                            break;
                        }
                        // Anything else (text, whitespace, identifiers) —
                        // emit as-is so the return expression survives.
                        self.result.push_str(
                            &String::from_utf8_lossy(
                                &self.bytes[self.token_starts[look]..self.token_ends[look]],
                            ),
                        );
                        look += 1;
                    }
                    after_return = true;
                    ti = look;
                    continue;
                }
            
                // ---- After return: skip non-comment, non-structural tokens ----
                if after_return {
                    // Allow through: COMMENT, NEWLINE (to preserve formatting),
                    // RBRACE/END (block closers reset terminal state)
                    if kind == 12 {
                        // Comment — emit
                        self.result.push_str(&text);
                        ti += 1;
                        continue;
                    }
                    if kind == 10 {
                        // Newline — emit (keeps line structure)
                        self.result.push_str(&text);
                        ti += 1;
                        continue;
                    }
                    if kind == 7 {
                        // RBRACE — end of block, reset after_return
                        after_return = false;
                        block_depth -= 1;
                        if self.mode == 1 { self.result.push_str("end"); }
                        ti += 1;
                        continue;
                    }
                    if kind == 9 || kind == 2 || kind == 3 {
                        // END/ELSEIF/ELSE — structural boundary, reset
                        after_return = false;
                        self.result.push_str(&text);
                        ti += 1;
                        continue;
                    }
                    // Skip everything else (unreachable code)
                    ti += 1;
                    continue;
                }
            
                // ---- Default: emit token text unchanged ----
                self.result.push_str(&text);
                ti += 1;
            }
        }
    }
}
pub use _output_block_parser_fsm_framec::*;
