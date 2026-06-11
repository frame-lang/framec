
// ContextParser — FSM for parsing all @@ context constructs.
//
// Dispatches on the character after @@ to parse:
//   @@:return [= expr] → ContextReturn (kind=2)
//   @@:event           → ContextEvent (kind=3)
//   @@:data.key [= e]  → ContextData (kind=4) or ContextDataAssign (kind=5)
//   @@:params.key      → ContextParams (kind=6)
//   @@SystemName()     → SystemInstantiation (kind=7), Factory call
//   @@!SystemName()    → SystemInstantiation (kind=7), NoInitialization (RFC-0015 D7)
//   @@:(expr)          → ContextReturnExpr (kind=8)
//   @@:return(expr)    → ReturnCall (kind=9)
//   @@:self.method()   → ContextSelfCall (kind=10)
//   @@:self[.field]    → ContextSelf (kind=11)
//   @@:self.field.method(args) → ContextSelfFieldCall (kind=15), RFC-0046 embed/field call
//   @@:system.state.name → ContextSystemState (kind=12), current state name
//   @@:system.state    → ContextSystemStateReserved (kind=14), reserved (RFC-0045) → E608
//   other              → no match (has_result=false)
//
// For SystemInstantiation, the FSM sets `result_no_init = true` if the source
// had `@@!SystemName(...)` (the user's no-initialization sigil). The caller
// reads that flag to populate `InstantiationKind::{Factory, NoInitialization}`
// in the segment metadata.
//
// Demonstrates hierarchical composition: $ParseReturn and $ParseData
// create ExprScannerFsm sub-machines when they detect assignment `=`.

include!("expr_scanner.gen.rs");

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
mod _context_parser_fsm_framec {
    use super::*;
    extern crate alloc;
    use alloc::{vec, format};
    #[derive(Clone, Debug)]
    #[allow(dead_code, non_camel_case_types)]
    enum ContextParserFsmFrameEvent {
        DoParse {  },
        FrameEnter {},
        FrameExit {},
    }

    #[derive(Clone)]
    #[allow(dead_code, non_camel_case_types)]
    enum ContextParserFsmFrameReturn {
        _Lifecycle(alloc::rc::Rc<dyn core::any::Any>),
    }

    #[allow(dead_code)]
    impl ContextParserFsmFrameEvent {
        fn name(&self) -> &'static str {
            match self {
                ContextParserFsmFrameEvent::DoParse { .. } => "do_parse",
                ContextParserFsmFrameEvent::FrameEnter { .. } => "$>",
                ContextParserFsmFrameEvent::FrameExit { .. } => "<$",
            }
        }
    }

    #[derive(Clone, Debug)]
    #[allow(dead_code, non_camel_case_types)]
    enum ContextParserFsmFrameValue {
        Int(i64),
        Float(f64),
        Bool(bool),
        Str(String),
        List(Vec<Self>),
        Dict(alloc::collections::BTreeMap<String, Self>),
    }

    #[allow(dead_code, non_camel_case_types)]
    struct ContextParserFsmFrameContext {
        event: alloc::rc::Rc<ContextParserFsmFrameEvent>,
        _return: Option<ContextParserFsmFrameReturn>,
        _data: alloc::collections::BTreeMap<String, ContextParserFsmFrameValue>,
        _transitioned: bool,
    }

    impl ContextParserFsmFrameContext {
        fn new(event: alloc::rc::Rc<ContextParserFsmFrameEvent>, default_return: Option<ContextParserFsmFrameReturn>) -> Self {
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
    enum ContextParserFsmStateContext {
        Init,
        Dispatching,
        DispatchColon,
        ParseReturn,
        ParseContextReturnExpr,
        ParseData,
        ParseParams,
        ParseSelf,
        ParseSystem,
        ParseInstantiation,
        Done,
        __NoContext,
    }

    impl Default for ContextParserFsmStateContext {
        fn default() -> Self {
            ContextParserFsmStateContext::Init
        }
    }

    #[allow(dead_code, non_camel_case_types)]
    #[derive(Clone)]
    struct ContextParserFsmCompartment {
        state: String,
        state_context: ContextParserFsmStateContext,
        forward_event: Option<ContextParserFsmFrameEvent>,
        parent_compartment: Option<Box<ContextParserFsmCompartment>>,
    }

    impl ContextParserFsmCompartment {
        fn new(state: &str) -> Self {
            let state_context = match state {
                "Init" => ContextParserFsmStateContext::Init,
                "Dispatching" => ContextParserFsmStateContext::Dispatching,
                "DispatchColon" => ContextParserFsmStateContext::DispatchColon,
                "ParseReturn" => ContextParserFsmStateContext::ParseReturn,
                "ParseContextReturnExpr" => ContextParserFsmStateContext::ParseContextReturnExpr,
                "ParseData" => ContextParserFsmStateContext::ParseData,
                "ParseParams" => ContextParserFsmStateContext::ParseParams,
                "ParseSelf" => ContextParserFsmStateContext::ParseSelf,
                "ParseSystem" => ContextParserFsmStateContext::ParseSystem,
                "ParseInstantiation" => ContextParserFsmStateContext::ParseInstantiation,
                "Done" => ContextParserFsmStateContext::Done,
                _ => ContextParserFsmStateContext::__NoContext,
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
    pub struct ContextParserFsm {
        _state_stack: Vec<ContextParserFsmCompartment>,
        __compartment: ContextParserFsmCompartment,
        __next_compartment: Option<ContextParserFsmCompartment>,
        _context_stack: Vec<ContextParserFsmFrameContext>,
        pub bytes: Vec<u8>,
        pub pos: usize,
        pub end: usize,
        pub result_end: usize,
        pub result_kind: usize,
        pub has_result: bool,
        pub paren_end: usize,
        // RFC-0015 D7: set true when $Dispatching saw `@@!` and routed to
        // $ParseInstantiation. Caller uses this to map result_kind=7
        // to InstantiationKind::NoInitialization (else Factory).
        pub result_no_init: bool,
    }

    #[allow(non_snake_case)]
    impl ContextParserFsm {
        pub fn new() -> Self {
            Self {
                _state_stack: Vec::new(),
                _context_stack: Vec::new(),
                bytes: Vec::new(),
                pos: 0,
                end: 0,
                result_end: 0,
                result_kind: 0,
                has_result: false,
                paren_end: 0,
                result_no_init: false,
                __compartment: ContextParserFsmCompartment::new("Init"),
                __next_compartment: None,
            }
        }

        pub fn __create() -> Self {
            let mut c = Self::new();
            c.__compartment = c.__prepareEnter("Init");
            let __e = alloc::rc::Rc::new(ContextParserFsmFrameEvent::FrameEnter {});
            let __ctx = ContextParserFsmFrameContext::new(alloc::rc::Rc::clone(&__e), None);
            c._context_stack.push(__ctx);
            c.__kernel(&__e);
            c._context_stack.pop();
            c
        }

        fn __hsm_chain(&mut self, leaf: &str) -> &'static [&'static str] {
            match leaf {
                "Init" => &["Init"],
                "Dispatching" => &["Dispatching"],
                "DispatchColon" => &["DispatchColon"],
                "ParseReturn" => &["ParseReturn"],
                "ParseContextReturnExpr" => &["ParseContextReturnExpr"],
                "ParseData" => &["ParseData"],
                "ParseParams" => &["ParseParams"],
                "ParseSelf" => &["ParseSelf"],
                "ParseSystem" => &["ParseSystem"],
                "ParseInstantiation" => &["ParseInstantiation"],
                "Done" => &["Done"],
                _ => &[],
            }
        }

        fn __prepareEnter(&mut self, leaf: &str) -> ContextParserFsmCompartment {
            let chain = self.__hsm_chain(leaf);
            let mut comp: Option<ContextParserFsmCompartment> = None;
            for name in chain.iter() {
                let mut new_comp = ContextParserFsmCompartment::new(name);
                if let Some(parent) = comp.take() {
                    new_comp.parent_compartment = Some(Box::new(parent));
                }
                comp = Some(new_comp);
            }
            comp.expect("chain must contain at least the leaf state")
        }

        fn __kernel(&mut self, __e: &alloc::rc::Rc<ContextParserFsmFrameEvent>) {
            // Route event to current state.
            self.__router(__e);
            // Drain any transitions queued by the handler.
            while self.__next_compartment.is_some() {
                let next_compartment = self.__next_compartment.take().expect("invariant: while-loop guard checked is_some()");
                // Exit the current (leaf) state. RFC-0025.1: exit args live in the
                // source state's typed ctx (written at the transition site), so the
                // synthesized `<$` event carries no payload.
                let exit_event = alloc::rc::Rc::new(ContextParserFsmFrameEvent::FrameExit {});
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
                        let enter_event = alloc::rc::Rc::new(ContextParserFsmFrameEvent::FrameEnter {});
                        self.__router(&enter_event);
                    }
                    Some(fwd) if matches!(fwd, ContextParserFsmFrameEvent::FrameEnter { .. }) => {
                        // Forwarded event IS $> — dispatch directly so the
                        // destination's $> handler receives the caller's payload.
                        let fwd_rc = alloc::rc::Rc::new(fwd);
                        self.__router(&fwd_rc);
                    }
                    Some(fwd) => {
                        // Forwarded event is not $> — initialize the destination
                        // with a fresh $>, then dispatch the forward.
                        let enter_event = alloc::rc::Rc::new(ContextParserFsmFrameEvent::FrameEnter {});
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

        fn __router(&mut self, __e: &alloc::rc::Rc<ContextParserFsmFrameEvent>) {
            let __ev: &ContextParserFsmFrameEvent = __e;
            match self.__compartment.state.as_str() {
                "Init" => self._state_Init(__ev),
                "Dispatching" => self._state_Dispatching(__ev),
                "DispatchColon" => self._state_DispatchColon(__ev),
                "ParseReturn" => self._state_ParseReturn(__ev),
                "ParseContextReturnExpr" => self._state_ParseContextReturnExpr(__ev),
                "ParseData" => self._state_ParseData(__ev),
                "ParseParams" => self._state_ParseParams(__ev),
                "ParseSelf" => self._state_ParseSelf(__ev),
                "ParseSystem" => self._state_ParseSystem(__ev),
                "ParseInstantiation" => self._state_ParseInstantiation(__ev),
                "Done" => self._state_Done(__ev),
                _ => {}
            }
        }

        fn __transition(&mut self, next_compartment: ContextParserFsmCompartment) {
            self.__next_compartment = Some(next_compartment);
        }

        pub fn do_parse(&mut self) {
            let __e = alloc::rc::Rc::new(ContextParserFsmFrameEvent::DoParse {});
            let mut __ctx = ContextParserFsmFrameContext::new(alloc::rc::Rc::clone(&__e), None);
            self._context_stack.push(__ctx);
            self.__kernel(&__e);
            self._context_stack.pop();
        }

        fn _state_Init(&mut self, __e: &ContextParserFsmFrameEvent) {
            match __e {
                ContextParserFsmFrameEvent::DoParse { .. } => { self._s_Init_hdl_user_do_parse(__e); }
                _ => {}
            }
        }

        fn _state_Dispatching(&mut self, __e: &ContextParserFsmFrameEvent) {
            match __e {
                ContextParserFsmFrameEvent::FrameEnter { .. } => { self._s_Dispatching_hdl_frame_enter(__e); }
                _ => {}
            }
        }

        fn _state_DispatchColon(&mut self, __e: &ContextParserFsmFrameEvent) {
            match __e {
                ContextParserFsmFrameEvent::FrameEnter { .. } => { self._s_DispatchColon_hdl_frame_enter(__e); }
                _ => {}
            }
        }

        fn _state_ParseReturn(&mut self, __e: &ContextParserFsmFrameEvent) {
            match __e {
                ContextParserFsmFrameEvent::FrameEnter { .. } => { self._s_ParseReturn_hdl_frame_enter(__e); }
                _ => {}
            }
        }

        fn _state_ParseContextReturnExpr(&mut self, __e: &ContextParserFsmFrameEvent) {
            match __e {
                ContextParserFsmFrameEvent::FrameEnter { .. } => { self._s_ParseContextReturnExpr_hdl_frame_enter(__e); }
                _ => {}
            }
        }

        fn _state_ParseData(&mut self, __e: &ContextParserFsmFrameEvent) {
            match __e {
                ContextParserFsmFrameEvent::FrameEnter { .. } => { self._s_ParseData_hdl_frame_enter(__e); }
                _ => {}
            }
        }

        fn _state_ParseParams(&mut self, __e: &ContextParserFsmFrameEvent) {
            match __e {
                ContextParserFsmFrameEvent::FrameEnter { .. } => { self._s_ParseParams_hdl_frame_enter(__e); }
                _ => {}
            }
        }

        fn _state_ParseSelf(&mut self, __e: &ContextParserFsmFrameEvent) {
            match __e {
                ContextParserFsmFrameEvent::FrameEnter { .. } => { self._s_ParseSelf_hdl_frame_enter(__e); }
                _ => {}
            }
        }

        fn _state_ParseSystem(&mut self, __e: &ContextParserFsmFrameEvent) {
            match __e {
                ContextParserFsmFrameEvent::FrameEnter { .. } => { self._s_ParseSystem_hdl_frame_enter(__e); }
                _ => {}
            }
        }

        fn _state_ParseInstantiation(&mut self, __e: &ContextParserFsmFrameEvent) {
            match __e {
                ContextParserFsmFrameEvent::FrameEnter { .. } => { self._s_ParseInstantiation_hdl_frame_enter(__e); }
                _ => {}
            }
        }

        fn _state_Done(&mut self, __e: &ContextParserFsmFrameEvent) {
            match __e {
                ContextParserFsmFrameEvent::FrameEnter { .. } => { self._s_Done_hdl_frame_enter(__e); }
                _ => {}
            }
        }

        fn _s_Init_hdl_user_do_parse(&mut self, __e: &ContextParserFsmFrameEvent) {
            let mut __compartment = self.__prepareEnter("Dispatching");
            self.__transition(__compartment);
            return;
        }

        fn _s_Dispatching_hdl_frame_enter(&mut self, __e: &ContextParserFsmFrameEvent) {
            let i = self.pos;
            let end = self.end;
            let bytes = &self.bytes;
            
            if i >= end {
                self.has_result = false;
                let mut __compartment = self.__prepareEnter("Done");
                self.__transition(__compartment);
                return;
            }
            
            let b = bytes[i];
            
            if b == b':' {
                self.pos = i + 1;
                let mut __compartment = self.__prepareEnter("DispatchColon");
                self.__transition(__compartment);
                return;
            } else if b == b'!' {
                // @@! — RFC-0015 D7 no-initialization sigil. Must be
                // followed immediately by an uppercase identifier
                // (the system name). If not, no match — the user
                // wrote something like `@@! foo` which is meaningless.
                let j = i + 1;
                if j < end && bytes[j].is_ascii_uppercase() {
                    self.pos = j;
                    self.result_no_init = true;
                    let mut __compartment = self.__prepareEnter("ParseInstantiation");
                    self.__transition(__compartment);
                    return;
                } else {
                    self.result_end = i;
                    self.has_result = false;
                    let mut __compartment = self.__prepareEnter("Done");
                    self.__transition(__compartment);
                    return;
                }
            } else if b.is_ascii_uppercase() {
                // @@SystemName — pos stays at start of name;
                let mut __compartment = self.__prepareEnter("ParseInstantiation");
                self.__transition(__compartment);
                return;
            } else {
                // Just @@ without . or : or uppercase or !
                self.result_end = i;
                self.has_result = false;
                let mut __compartment = self.__prepareEnter("Done");
                self.__transition(__compartment);
                return;
            }
        }

        fn _s_DispatchColon_hdl_frame_enter(&mut self, __e: &ContextParserFsmFrameEvent) {
            // @@: — dispatch on the keyword after ':'
            let i = self.pos;
            let end = self.end;
            let bytes = &self.bytes;
            
            if i + 5 < end && &bytes[i..i + 6] == b"return" {
                self.pos = i + 6;
                let mut __compartment = self.__prepareEnter("ParseReturn");
                self.__transition(__compartment);
                return;
            } else if i + 4 < end && &bytes[i..i + 5] == b"event" {
                self.result_end = i + 5;
                self.result_kind = 3; // ContextEvent
                self.has_result = true;
                let mut __compartment = self.__prepareEnter("Done");
                self.__transition(__compartment);
                return;
            } else if i + 3 < end && &bytes[i..i + 4] == b"data" {
                self.pos = i + 4;
                let mut __compartment = self.__prepareEnter("ParseData");
                self.__transition(__compartment);
                return;
            } else if i + 5 < end && &bytes[i..i + 6] == b"params" {
                self.pos = i + 6;
                let mut __compartment = self.__prepareEnter("ParseParams");
                self.__transition(__compartment);
                return;
            } else if i + 3 < end && &bytes[i..i + 4] == b"self" {
                self.pos = i + 4;
                let mut __compartment = self.__prepareEnter("ParseSelf");
                self.__transition(__compartment);
                return;
            } else if i + 5 < end && &bytes[i..i + 6] == b"system" {
                self.pos = i + 6;
                let mut __compartment = self.__prepareEnter("ParseSystem");
                self.__transition(__compartment);
                return;
            } else if i < end && bytes[i] == b'(' {
                // @@:(expr) — context return expression
                self.pos = i;
                let mut __compartment = self.__prepareEnter("ParseContextReturnExpr");
                self.__transition(__compartment);
                return;
            } else {
                // Unknown @@: variant
                self.result_end = i;
                self.has_result = false;
                let mut __compartment = self.__prepareEnter("Done");
                self.__transition(__compartment);
                return;
            }
        }

        fn _s_ParseReturn_hdl_frame_enter(&mut self, __e: &ContextParserFsmFrameEvent) {
            // @@:return — check for assignment, call form, or bare read
            let mut i = self.pos;
            let end = self.end;
            let bytes = &self.bytes;
            
            // Skip whitespace
            while i < end && (bytes[i] == b' ' || bytes[i] == b'\t') {
                i += 1;
            }
            
            if i < end && bytes[i] == b'(' {
                // @@:return(expr) — set return value AND exit handler.
                // Scan balanced parens to find matching ')'.
                let mut depth: usize = 1;
                i += 1; // Skip opening '('
                while i < end && depth > 0 {
                    if bytes[i] == b'(' { depth += 1; }
                    if bytes[i] == b')' { depth -= 1; }
                    if depth > 0 { i += 1; }
                }
                if depth == 0 { i += 1; } // Skip closing ')'
                self.result_end = i;
                self.result_kind = 9; // ReturnCall
                self.has_result = true;
                let mut __compartment = self.__prepareEnter("Done");
                self.__transition(__compartment);
                return;
            } else if i < end && bytes[i] == b'=' && (i + 1 >= end || bytes[i + 1] != b'=') {
                // @@:return = <expr> — create ExprScanner sub-machine
                i += 1; // Skip '='
                let mut expr = ExprScannerFsm::new();
                expr.bytes = bytes.to_vec();
                expr.pos = i;
                expr.end = end;
                expr.do_scan();
                i = expr.result_end;
                // expr is destroyed here (state manager pattern)
                self.result_end = i;
                self.result_kind = 2; // ContextReturn
                self.has_result = true;
                let mut __compartment = self.__prepareEnter("Done");
                self.__transition(__compartment);
                return;
            } else {
                // @@:return (bare read) — rvalue access to return slot
                self.result_end = i;
                self.result_kind = 2; // ContextReturn (read mode)
                self.has_result = true;
                let mut __compartment = self.__prepareEnter("Done");
                self.__transition(__compartment);
                return;
            }
        }

        fn _s_ParseContextReturnExpr_hdl_frame_enter(&mut self, __e: &ContextParserFsmFrameEvent) {
            // @@:(expr) — scan balanced parens to find matching ')'
            let mut i = self.pos;
            let end = self.end;
            let bytes = &self.bytes;
            
            if i < end && bytes[i] == b'(' {
                let mut depth: usize = 1;
                i += 1; // Skip opening '('
                while i < end && depth > 0 {
                    let b = bytes[i];
                    if b == b'(' {
                        depth += 1;
                    } else if b == b')' {
                        depth -= 1;
                    } else if b == b'"' || b == b'\'' {
                        // Skip string literals
                        let q = b;
                        i += 1;
                        while i < end {
                            if bytes[i] == b'\\' && i + 1 < end {
                                i += 2;
                                continue;
                            }
                            if bytes[i] == q {
                                break;
                            }
                            i += 1;
                        }
                    }
                    i += 1;
                }
            }
            
            self.result_end = i;
            self.result_kind = 8; // ContextReturnExpr
            self.has_result = true;
            let mut __compartment = self.__prepareEnter("Done");
            self.__transition(__compartment);
            return;
        }

        fn _s_ParseData_hdl_frame_enter(&mut self, __e: &ContextParserFsmFrameEvent) {
            // @@:data.key or @@:data.key = expr
            let mut i = self.pos;
            let end = self.end;
            let bytes = &self.bytes;
            
            // Scan .key (dot + identifier)
            if i < end && bytes[i] == b'.' {
                i += 1; // Skip '.'
                while i < end && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
                    i += 1;
                }
            }
            
            // Check for assignment
            let mut j = i;
            while j < end && (bytes[j] == b' ' || bytes[j] == b'\t') {
                j += 1;
            }
            
            if j < end && bytes[j] == b'=' && (j + 1 >= end || bytes[j + 1] != b'=') {
                // @@:data[key] = expr — create ExprScanner sub-machine
                j += 1; // Skip '='
                let mut expr = ExprScannerFsm::new();
                expr.bytes = bytes.to_vec();
                expr.pos = j;
                expr.end = end;
                expr.do_scan();
                self.result_end = expr.result_end;
                // expr is destroyed here (state manager pattern)
                self.result_kind = 5; // ContextDataAssign
            } else {
                self.result_end = i;
                self.result_kind = 4; // ContextData
            }
            
            self.has_result = true;
            let mut __compartment = self.__prepareEnter("Done");
            self.__transition(__compartment);
            return;
        }

        fn _s_ParseParams_hdl_frame_enter(&mut self, __e: &ContextParserFsmFrameEvent) {
            // @@:params.key — dot-accessor for interface parameter
            let mut i = self.pos;
            let end = self.end;
            let bytes = &self.bytes;
            
            if i < end && bytes[i] == b'.' {
                i += 1; // Skip '.'
                // Scan identifier
                while i < end && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
                    i += 1;
                }
            }
            
            self.result_end = i;
            self.result_kind = 6; // ContextParams
            self.has_result = true;
            let mut __compartment = self.__prepareEnter("Done");
            self.__transition(__compartment);
            return;
        }

        fn _s_ParseSelf_hdl_frame_enter(&mut self, __e: &ContextParserFsmFrameEvent) {
            // @@:self — bare reference or @@:self.method(args) call
            let mut i = self.pos;
            let end = self.end;
            let bytes = &self.bytes;
            
            if i < end && bytes[i] == b'.' {
                i += 1; // Skip '.'
                // Scan identifier (method or property name)
                let name_start = i;
                while i < end && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
                    i += 1;
                }
                if i < end && bytes[i] == b'(' {
                    // @@:self.method(args) — scan balanced parens
                    let mut depth: usize = 1;
                    i += 1; // Skip '('
                    while i < end && depth > 0 {
                        if bytes[i] == b'(' { depth += 1; }
                        if bytes[i] == b')' { depth -= 1; }
                        if bytes[i] == b'"' || bytes[i] == b'\'' {
                            let q = bytes[i];
                            i += 1;
                            while i < end {
                                if bytes[i] == b'\\' && i + 1 < end { i += 2; continue; }
                                if bytes[i] == q { break; }
                                i += 1;
                            }
                        }
                        if depth > 0 { i += 1; }
                    }
                    if depth == 0 { i += 1; } // Skip closing ')'
                    self.result_end = i;
                    self.result_kind = 10; // ContextSelfCall
                    self.has_result = true;
                } else if i + 1 < end && bytes[i] == b'.'
                    && (bytes[i + 1].is_ascii_alphabetic() || bytes[i + 1] == b'_')
                {
                    // Possible @@:self.field.method(args) — chained call
                    // through a self field (RFC-0046). `field` is the first
                    // ident already scanned; look ahead for `.method(`.
                    let field_end = i; // end of the field identifier
                    let mut j = i + 1; // skip the second '.'
                    while j < end && (bytes[j].is_ascii_alphanumeric() || bytes[j] == b'_') {
                        j += 1;
                    }
                    if j < end && bytes[j] == b'(' {
                        // @@:self.field.method(args) — scan balanced parens
                        // (quote-aware, mirrors the method-call arm above).
                        let mut depth: usize = 1;
                        j += 1; // Skip '('
                        while j < end && depth > 0 {
                            if bytes[j] == b'(' { depth += 1; }
                            if bytes[j] == b')' { depth -= 1; }
                            if bytes[j] == b'"' || bytes[j] == b'\'' {
                                let q = bytes[j];
                                j += 1;
                                while j < end {
                                    if bytes[j] == b'\\' && j + 1 < end { j += 2; continue; }
                                    if bytes[j] == q { break; }
                                    j += 1;
                                }
                            }
                            if depth > 0 { j += 1; }
                        }
                        if depth == 0 { j += 1; } // Skip closing ')'
                        self.result_end = j;
                        self.result_kind = 15; // ContextSelfFieldCall
                        self.has_result = true;
                    } else {
                        // `.method` not followed by `(` (e.g. @@:self.a.b.c()):
                        // capture only the first field; the rest is native.
                        self.result_end = field_end;
                        self.result_kind = 11; // ContextSelf (field access)
                        self.has_result = true;
                    }
                } else {
                    // @@:self.field — scalar domain-field accessor
                    self.result_end = i;
                    self.result_kind = 11; // ContextSelf
                    self.has_result = true;
                }
            } else {
                // bare @@:self
                self.result_end = i;
                self.result_kind = 11; // ContextSelf
                self.has_result = true;
            }
            
            let mut __compartment = self.__prepareEnter("Done");
            self.__transition(__compartment);
            return;
        }

        fn _s_ParseSystem_hdl_frame_enter(&mut self, __e: &ContextParserFsmFrameEvent) {
            // @@:system — `.state.name` is the current-state name accessor.
            // Bare `@@:system.state` is RESERVED for future use (RFC-0045)
            // and rejected with E608; anything else is E604.
            let i = self.pos;
            let end = self.end;
            let bytes = &self.bytes;
            
            if i + 5 < end && &bytes[i..i + 6] == b".state"
                && (i + 6 >= end || !(bytes[i + 6].is_ascii_alphanumeric() || bytes[i + 6] == b'_'))
            {
                // Matched `.state` at a word boundary — now require `.name`.
                let j = i + 6;
                if j + 4 < end && &bytes[j..j + 5] == b".name"
                    && (j + 5 >= end || !(bytes[j + 5].is_ascii_alphanumeric() || bytes[j + 5] == b'_'))
                {
                    // @@:system.state.name — read-only state name accessor
                    self.result_end = j + 5;
                    self.result_kind = 12; // ContextSystemState
                    self.has_result = true;
                } else {
                    // @@:system.state (no `.name`) — reserved (RFC-0045) → E608
                    self.result_end = j;
                    self.result_kind = 14; // ContextSystemStateReserved
                    self.has_result = true;
                }
            } else {
                // Bare @@:system or unknown variant — emit for validation (E604)
                self.result_end = i;
                self.result_kind = 13; // ContextSystemBare
                self.has_result = true;
            }
            
            let mut __compartment = self.__prepareEnter("Done");
            self.__transition(__compartment);
            return;
        }

        fn _s_ParseInstantiation_hdl_frame_enter(&mut self, __e: &ContextParserFsmFrameEvent) {
            // @@SystemName() — scan name, find balanced parens
            let mut i = self.pos;
            let end = self.end;
            let bytes = &self.bytes;
            
            // Scan identifier
            while i < end && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
                i += 1;
            }
            
            // Must be followed by (
            if i < end && bytes[i] == b'(' {
                // Use the pre-computed paren_end if available
                if self.paren_end > 0 {
                    i = self.paren_end;
                    self.result_end = i;
                    self.result_kind = 7; // SystemInstantiation
                    self.has_result = true;
                } else {
                    // No paren_end provided — caller must handle
                    self.result_end = i;
                    self.has_result = false;
                }
            } else {
                // @@SomeName without () — treat as native
                self.result_end = i;
                self.has_result = false;
            }
            
            let mut __compartment = self.__prepareEnter("Done");
            self.__transition(__compartment);
            return;
        }

        fn _s_Done_hdl_frame_enter(&mut self, __e: &ContextParserFsmFrameEvent) {
            // Terminal state — results are in domain vars;
        }
    }
}
pub use _context_parser_fsm_framec::*;
