
// Attribute scanner — Frame-generated state machine.
//
// Source:    attribute_scanner.frs (Frame specification)
// Generated: attribute_scanner.gen.rs (via framec --target rust)
//
// Recognizes Frame's two pragma surface forms:
//
//   1. Bracket form (RFC-0013): `@@[name]` or `@@[name(args)]`
//      Examples: `@@[persist]`, `@@[target("python_3")]`, `@@[main]`,
//                `@@[save]`, `@@[load]`, `@@[no_persist]`,
//                `@@[migrate(from=1, to=2)]`
//
//   2. Bare form (legacy + still-supported keywords): `@@<name> <value?>`
//      Examples: `@@run-expect 42`, `@@skip-if windows`
//      RFC-0013 hard-cut: bare `@@persist` and `@@target` now error
//      (E803 / E804). RFC-0032 hard-cut: `@@codegen { ... }` now errors
//      (E824). (`@@import` was removed by RFC-0024 but reinstated by
//      RFC-0040 as an analysis-only directive — it is accepted again and
//      handled in `do_parse`, not an error here.)
//      The bare error-forms pass through this scanner as Other and the
//      pipeline reports the migration error downstream.
//
// The scanner returns: name span (`name_start`/`name_end`), optional
// args span (bracket form only — `args_start`/`args_end`), optional
// value span (bare form only — `value_start`/`value_end` post-trim),
// and `is_bracket_form`. Translation from name string to `PragmaKind`
// happens in the wrapper module — keeps the FSM decoupled from the
// growing list of recognized attribute names.
//
// State machine flow:
//
//   $Init.scan(start) → $AfterAt
//                          ├→ $BracketName → $BracketAfterName
//                          │                    ├→ $BracketArgs → $BracketBeforeClose → done
//                          │                    └→ $BracketBeforeClose → done
//                          └→ $BareName → $BareValue → done
//
// Why FSM: bounded, single-pass byte walk over a small lookahead
// window. Same shape as `body_closer/*.frs` and `multisys_assembler.frs`.
// Replacing the existing Rust `identify_pragma` puts attribute parsing
// on the same Frame-native footing as the rest of the scanner work.

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
mod _attribute_scanner_fsm_framec {
    use super::*;
    extern crate alloc;
    use alloc::{vec, format};
    #[derive(Clone, Debug)]
    #[allow(dead_code, non_camel_case_types)]
    enum AttributeScannerFsmFrameEvent {
        Scan { start: usize },
        FrameEnter {},
        FrameExit {},
    }

    #[derive(Clone)]
    #[allow(dead_code, non_camel_case_types)]
    enum AttributeScannerFsmFrameReturn {
        _Lifecycle(alloc::rc::Rc<dyn core::any::Any>),
    }

    #[allow(dead_code)]
    impl AttributeScannerFsmFrameEvent {
        fn name(&self) -> &'static str {
            match self {
                AttributeScannerFsmFrameEvent::Scan { .. } => "scan",
                AttributeScannerFsmFrameEvent::FrameEnter { .. } => "$>",
                AttributeScannerFsmFrameEvent::FrameExit { .. } => "<$",
            }
        }
    }

    #[derive(Clone, Debug)]
    #[allow(dead_code, non_camel_case_types)]
    enum AttributeScannerFsmFrameValue {
        Int(i64),
        Float(f64),
        Bool(bool),
        Str(String),
        List(Vec<Self>),
        Dict(alloc::collections::BTreeMap<String, Self>),
    }

    #[allow(dead_code, non_camel_case_types)]
    struct AttributeScannerFsmFrameContext {
        event: alloc::rc::Rc<AttributeScannerFsmFrameEvent>,
        _return: Option<AttributeScannerFsmFrameReturn>,
        _data: alloc::collections::BTreeMap<String, AttributeScannerFsmFrameValue>,
        _transitioned: bool,
    }

    impl AttributeScannerFsmFrameContext {
        fn new(event: alloc::rc::Rc<AttributeScannerFsmFrameEvent>, default_return: Option<AttributeScannerFsmFrameReturn>) -> Self {
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
    enum AttributeScannerFsmStateContext {
        Init,
        BracketName,
        BracketAfterName,
        BracketArgs,
        BracketBeforeClose,
        BareName,
        BareValue,
        __NoContext,
    }

    impl Default for AttributeScannerFsmStateContext {
        fn default() -> Self {
            AttributeScannerFsmStateContext::Init
        }
    }

    #[allow(dead_code, non_camel_case_types)]
    #[derive(Clone)]
    struct AttributeScannerFsmCompartment {
        state: String,
        state_context: AttributeScannerFsmStateContext,
        forward_event: Option<AttributeScannerFsmFrameEvent>,
        parent_compartment: Option<Box<AttributeScannerFsmCompartment>>,
    }

    impl AttributeScannerFsmCompartment {
        fn new(state: &str) -> Self {
            let state_context = match state {
                "Init" => AttributeScannerFsmStateContext::Init,
                "BracketName" => AttributeScannerFsmStateContext::BracketName,
                "BracketAfterName" => AttributeScannerFsmStateContext::BracketAfterName,
                "BracketArgs" => AttributeScannerFsmStateContext::BracketArgs,
                "BracketBeforeClose" => AttributeScannerFsmStateContext::BracketBeforeClose,
                "BareName" => AttributeScannerFsmStateContext::BareName,
                "BareValue" => AttributeScannerFsmStateContext::BareValue,
                _ => AttributeScannerFsmStateContext::__NoContext,
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
    pub struct AttributeScannerFsm {
        _state_stack: Vec<AttributeScannerFsmCompartment>,
        __compartment: AttributeScannerFsmCompartment,
        __next_compartment: Option<AttributeScannerFsmCompartment>,
        _context_stack: Vec<AttributeScannerFsmFrameContext>,
        // Input: the source bytes; the scan starting position is
        // passed via the `scan(start)` interface call.
        pub bytes: Vec<u8>,
        // Cursor — advances through the bytes during the walk;
        // accessible after the FSM finishes via `pos()`.
        pub pos: usize,
        // Surface form: true for `@@[name]`, false for bare `@@name`.
        pub is_bracket_form: bool,
        // RFC-0052 §4 broadcast prefix: true when the bracket form had a
        // leading `*` (`@@[*persist]`) — the attribute applies to ALL
        // systems in the module, not just the next one. Always false for
        // the bare form.
        pub is_broadcast: bool,
        // Name span — always set after a successful scan.
        pub name_start: usize,
        pub name_end: usize,
        // Args span — only valid when `has_args` is true (bracket
        // form with `(...)`). The span covers the parens and their
        // contents, e.g. for `@@[target("python_3")]` it points at
        // `("python_3")` inclusive.
        pub has_args: bool,
        pub args_start: usize,
        pub args_end: usize,
        pub paren_depth: i32,
        // Value span — only valid when `has_value` is true (bare
        // form with non-empty trailing text). Points at the trimmed
        // rest-of-line.
        pub has_value: bool,
        pub value_start: usize,
        pub value_end: usize,
    }

    #[allow(non_snake_case)]
    impl AttributeScannerFsm {
        pub fn new() -> Self {
            Self {
                _state_stack: Vec::new(),
                _context_stack: Vec::new(),
                bytes: Vec::new(),
                pos: 0,
                is_bracket_form: false,
                is_broadcast: false,
                name_start: 0,
                name_end: 0,
                has_args: false,
                args_start: 0,
                args_end: 0,
                paren_depth: 0,
                has_value: false,
                value_start: 0,
                value_end: 0,
                __compartment: AttributeScannerFsmCompartment::new("Init"),
                __next_compartment: None,
            }
        }

        pub fn __create() -> Self {
            let mut c = Self::new();
            c.__compartment = c.__prepareEnter("Init");
            let __e = alloc::rc::Rc::new(AttributeScannerFsmFrameEvent::FrameEnter {});
            let __ctx = AttributeScannerFsmFrameContext::new(alloc::rc::Rc::clone(&__e), None);
            c._context_stack.push(__ctx);
            c.__kernel(&__e);
            c._context_stack.pop();
            c
        }

        fn __hsm_chain(&mut self, leaf: &str) -> &'static [&'static str] {
            match leaf {
                "Init" => &["Init"],
                "BracketName" => &["BracketName"],
                "BracketAfterName" => &["BracketAfterName"],
                "BracketArgs" => &["BracketArgs"],
                "BracketBeforeClose" => &["BracketBeforeClose"],
                "BareName" => &["BareName"],
                "BareValue" => &["BareValue"],
                _ => &[],
            }
        }

        fn __prepareEnter(&mut self, leaf: &str) -> AttributeScannerFsmCompartment {
            let chain = self.__hsm_chain(leaf);
            let mut comp: Option<AttributeScannerFsmCompartment> = None;
            for name in chain.iter() {
                let mut new_comp = AttributeScannerFsmCompartment::new(name);
                if let Some(parent) = comp.take() {
                    new_comp.parent_compartment = Some(Box::new(parent));
                }
                comp = Some(new_comp);
            }
            comp.expect("chain must contain at least the leaf state")
        }

        fn __kernel(&mut self, __e: &alloc::rc::Rc<AttributeScannerFsmFrameEvent>) {
            // Route event to current state.
            self.__router(__e);
            // Drain any transitions queued by the handler.
            while self.__next_compartment.is_some() {
                let next_compartment = self.__next_compartment.take().expect("invariant: while-loop guard checked is_some()");
                // Exit the current (leaf) state. RFC-0025.1: exit args live in the
                // source state's typed ctx (written at the transition site), so the
                // synthesized `<$` event carries no payload.
                let exit_event = alloc::rc::Rc::new(AttributeScannerFsmFrameEvent::FrameExit {});
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
                        let enter_event = alloc::rc::Rc::new(AttributeScannerFsmFrameEvent::FrameEnter {});
                        self.__router(&enter_event);
                    }
                    Some(fwd) if matches!(fwd, AttributeScannerFsmFrameEvent::FrameEnter { .. }) => {
                        // Forwarded event IS $> — dispatch directly so the
                        // destination's $> handler receives the caller's payload.
                        let fwd_rc = alloc::rc::Rc::new(fwd);
                        self.__router(&fwd_rc);
                    }
                    Some(fwd) => {
                        // Forwarded event is not $> — initialize the destination
                        // with a fresh $>, then dispatch the forward.
                        let enter_event = alloc::rc::Rc::new(AttributeScannerFsmFrameEvent::FrameEnter {});
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

        fn __router(&mut self, __e: &alloc::rc::Rc<AttributeScannerFsmFrameEvent>) {
            let __ev: &AttributeScannerFsmFrameEvent = __e;
            match self.__compartment.state.as_str() {
                "Init" => self._state_Init(__ev),
                "BracketName" => self._state_BracketName(__ev),
                "BracketAfterName" => self._state_BracketAfterName(__ev),
                "BracketArgs" => self._state_BracketArgs(__ev),
                "BracketBeforeClose" => self._state_BracketBeforeClose(__ev),
                "BareName" => self._state_BareName(__ev),
                "BareValue" => self._state_BareValue(__ev),
                _ => {}
            }
        }

        fn __transition(&mut self, next_compartment: AttributeScannerFsmCompartment) {
            self.__next_compartment = Some(next_compartment);
        }

        pub fn scan(&mut self, start: usize) {
            let __e = alloc::rc::Rc::new(AttributeScannerFsmFrameEvent::Scan { start: start.clone() });
            let mut __ctx = AttributeScannerFsmFrameContext::new(alloc::rc::Rc::clone(&__e), None);
            self._context_stack.push(__ctx);
            self.__kernel(&__e);
            self._context_stack.pop();
        }

        fn _state_Init(&mut self, __e: &AttributeScannerFsmFrameEvent) {
            match __e {
                AttributeScannerFsmFrameEvent::Scan { start, .. } => {
                    self._s_Init_hdl_user_scan(__e, *start);
                }
                _ => {}
            }
        }

        // --- Bracket form: @@[name] / @@[name(args)] ----------------
        // Read the attribute name. Name chars: alphanumeric, `_`, `-`.
        // Stop at the first non-name char (`(`, `]`, ws).
        //
        // RFC-0052 §4: a leading `*` inside the brackets — `@@[*persist]` —
        // is the *broadcast* prefix ("spread across all systems in the
        // module"). It is consumed here (the name span excludes it) and
        // recorded in `is_broadcast`; the wrapper maps the name normally
        // and the pipeline reads the flag to decide single-vs-broadcast
        // affinity. `*` is not a valid identifier start, so this is
        // unambiguous and leaves every existing `@@[name]` form unchanged.
        fn _state_BracketName(&mut self, __e: &AttributeScannerFsmFrameEvent) {
            match __e {
                AttributeScannerFsmFrameEvent::FrameEnter { .. } => { self._s_BracketName_hdl_frame_enter(__e); }
                _ => {}
            }
        }

        // After the name: either `(args)` follows, or the closing `]`
        // (with possible intervening whitespace).
        fn _state_BracketAfterName(&mut self, __e: &AttributeScannerFsmFrameEvent) {
            match __e {
                AttributeScannerFsmFrameEvent::FrameEnter { .. } => { self._s_BracketAfterName_hdl_frame_enter(__e); }
                _ => {}
            }
        }

        // Inside `(args)` — track paren depth so nested calls within
        // arg expressions don't terminate the args block early.
        // E.g. `@@[migrate(from=foo(1), to=2)]` has nested parens that
        // depth-tracking keeps as a single args span.
        fn _state_BracketArgs(&mut self, __e: &AttributeScannerFsmFrameEvent) {
            match __e {
                AttributeScannerFsmFrameEvent::FrameEnter { .. } => { self._s_BracketArgs_hdl_frame_enter(__e); }
                _ => {}
            }
        }

        // Skip whitespace, expect closing `]`. Tolerates whitespace
        // between `)` and `]` (`@@[name(args) ]`) for source-style
        // flexibility.
        fn _state_BracketBeforeClose(&mut self, __e: &AttributeScannerFsmFrameEvent) {
            match __e {
                AttributeScannerFsmFrameEvent::FrameEnter { .. } => { self._s_BracketBeforeClose_hdl_frame_enter(__e); }
                _ => {}
            }
        }

        // --- Bare form: @@<name> <value?> ---------------------------
        // Read the bare keyword name (same char class as bracket).
        fn _state_BareName(&mut self, __e: &AttributeScannerFsmFrameEvent) {
            match __e {
                AttributeScannerFsmFrameEvent::FrameEnter { .. } => { self._s_BareName_hdl_frame_enter(__e); }
                _ => {}
            }
        }

        // Skip whitespace after the keyword, then read the rest of
        // the line as the value. Trim trailing ws / `\r` so CRLF
        // sources round-trip clean.
        fn _state_BareValue(&mut self, __e: &AttributeScannerFsmFrameEvent) {
            match __e {
                AttributeScannerFsmFrameEvent::FrameEnter { .. } => { self._s_BareValue_hdl_frame_enter(__e); }
                _ => {}
            }
        }

        fn _s_Init_hdl_user_scan(&mut self, __e: &AttributeScannerFsmFrameEvent, start: usize) {
            self.pos = start + 2;  // skip @@
            if self.pos < self.bytes.len() && self.bytes[self.pos] == 0x5B {
                self.is_bracket_form = true;
                self.pos = self.pos + 1;
                let mut __compartment = self.__prepareEnter("BracketName");
                self.__transition(__compartment);
                return;
            } else {
                self.is_bracket_form = false;
                let mut __compartment = self.__prepareEnter("BareName");
                self.__transition(__compartment);
                return;
            }
        }

        fn _s_BracketName_hdl_frame_enter(&mut self, __e: &AttributeScannerFsmFrameEvent) {
            let n = self.bytes.len();
            if self.pos < n && self.bytes[self.pos] == 0x2A {
                self.is_broadcast = true;
                self.pos = self.pos + 1;
            }
            self.name_start = self.pos;
            while self.pos < n && is_attr_name_char(self.bytes[self.pos]) {
                self.pos = self.pos + 1;
            }
            self.name_end = self.pos;
            let mut __compartment = self.__prepareEnter("BracketAfterName");
            self.__transition(__compartment);
            return;
        }

        fn _s_BracketAfterName_hdl_frame_enter(&mut self, __e: &AttributeScannerFsmFrameEvent) {
            let n = self.bytes.len();
            if self.pos < n && self.bytes[self.pos] == 0x28 {
                self.has_args = true;
                self.args_start = self.pos;
                self.paren_depth = 1;
                self.pos = self.pos + 1;
                let mut __compartment = self.__prepareEnter("BracketArgs");
                self.__transition(__compartment);
                return;
            } else {
                let mut __compartment = self.__prepareEnter("BracketBeforeClose");
                self.__transition(__compartment);
                return;
            }
        }

        fn _s_BracketArgs_hdl_frame_enter(&mut self, __e: &AttributeScannerFsmFrameEvent) {
            let n = self.bytes.len();
            while self.pos < n && self.paren_depth > 0 {
                let b = self.bytes[self.pos];
                if b == 0x28 {
                    self.paren_depth = self.paren_depth + 1;
                } else if b == 0x29 {
                    self.paren_depth = self.paren_depth - 1;
                }
                self.pos = self.pos + 1;
            }
            self.args_end = self.pos;
            let mut __compartment = self.__prepareEnter("BracketBeforeClose");
            self.__transition(__compartment);
            return;
        }

        fn _s_BracketBeforeClose_hdl_frame_enter(&mut self, __e: &AttributeScannerFsmFrameEvent) {
            let n = self.bytes.len();
            while self.pos < n && (self.bytes[self.pos] == 0x20 || self.bytes[self.pos] == 0x09) {
                self.pos = self.pos + 1;
            }
            if self.pos < n && self.bytes[self.pos] == 0x5D {
                self.pos = self.pos + 1;
            }
        }

        fn _s_BareName_hdl_frame_enter(&mut self, __e: &AttributeScannerFsmFrameEvent) {
            self.name_start = self.pos;
            let n = self.bytes.len();
            while self.pos < n && is_attr_name_char(self.bytes[self.pos]) {
                self.pos = self.pos + 1;
            }
            self.name_end = self.pos;
            let mut __compartment = self.__prepareEnter("BareValue");
            self.__transition(__compartment);
            return;
        }

        fn _s_BareValue_hdl_frame_enter(&mut self, __e: &AttributeScannerFsmFrameEvent) {
            let n = self.bytes.len();
            while self.pos < n && (self.bytes[self.pos] == 0x20 || self.bytes[self.pos] == 0x09) {
                self.pos = self.pos + 1;
            }
            let vs = self.pos;
            while self.pos < n && self.bytes[self.pos] != 0x0A {
                self.pos = self.pos + 1;
            }
            let mut ve = self.pos;
            while ve > vs && (
                self.bytes[ve - 1] == 0x20 ||
                self.bytes[ve - 1] == 0x09 ||
                self.bytes[ve - 1] == 0x0D
            ) {
                ve = ve - 1;
            }
            if ve > vs {
                self.has_value = true;
                self.value_start = vs;
                self.value_end = ve;
            }
        }
    }
}
pub use _attribute_scanner_fsm_framec::*;
