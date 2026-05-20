
// Erlang per-line native rewriter / classifier, expressed as a
// single-state Frame system.
//
// RFC-0035 round 3 — the awkward-fit case the round set out to
// document. The classifier:
//
//   1. Returns an *enum* with 7 variants, several carrying
//      multi-field structs. Frame interface return types are
//      `String` only; the variant + payload have to be
//      serialized into a tagged string and parsed back out by
//      the glue Rust function.
//
//   2. Takes two slice parameters (`action_names`,
//      `interface_names`) that are joined into comma-separated
//      strings at the glue boundary and re-split inside the
//      handler body.
//
//   3. The body is 180+ lines of pattern-matching logic that
//      could equally well live as a free Rust function. The
//      Frame system here is essentially a function-definition
//      wrapper.
//
// Also: "multi-state Frame" turns out to be a misnomer for line
// classifiers. The natural shape is single $Classifying state
// that transitions to one of 7 terminal states (one per
// variant), with the destination state's $> entry handler
// emitting the encoded result. BUT Frame semantics: classify()
// returns BEFORE $> fires on the destination state. The entry
// handler can't set the classify() return value. The result
// HAS to be computed in classify()'s body before transitioning.
// So the multi-state framing adds no information; we drop it
// and accept that classifiers are single-state with a branching
// body.
//
// Round 3 ALSO surfaced a framec body_closer bug: rust_lang.frs
// treats `'label:` apostrophes as the start of a char literal
// and miscounts braces, dumping the framec module's `pub use`
// in the middle of the user handler body. We work around it
// here by replacing labeled-break with an immediately-invoked
// closure. The framec bug should be tracked separately and
// fixed in rust_lang.frs.
//
// Output encoding: pipe-delimited tagged variant.
//
//   InterfaceCallWithBind|field=<F>|method=<M>|args=<A>
//   InterfaceCall|method=<M>|args=<A>|result_var=<R>
//   ActionCallWithBind|field=<F>|call=<C>
//   ActionCall|<text>
//   RecordUpdate|field=<F>|value=<V>
//   Plain|<text>

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
mod _erlang_line_classifier_framec {
    use super::*;
    extern crate alloc;
    use alloc::{vec, format};
    #[derive(Clone, Debug)]
    #[allow(dead_code, non_camel_case_types)]
    enum ErlangLineClassifierFrameEvent {
        Classify { line: String, actions_csv: String, interfaces_csv: String, data_var: String },
        FrameEnter { args: Vec<String> },
        FrameExit { args: Vec<String> },
    }

    #[derive(Clone)]
    #[allow(dead_code, non_camel_case_types)]
    enum ErlangLineClassifierFrameReturn {
        Classify(String),
        _Lifecycle(alloc::rc::Rc<dyn core::any::Any>),
    }

    #[allow(dead_code)]
    impl ErlangLineClassifierFrameEvent {
        fn name(&self) -> &'static str {
            match self {
                ErlangLineClassifierFrameEvent::Classify { .. } => "classify",
                ErlangLineClassifierFrameEvent::FrameEnter { .. } => "$>",
                ErlangLineClassifierFrameEvent::FrameExit { .. } => "<$",
            }
        }
    }

    #[derive(Clone, Debug)]
    #[allow(dead_code, non_camel_case_types)]
    enum ErlangLineClassifierFrameValue {
        Int(i64),
        Float(f64),
        Bool(bool),
        Str(String),
        List(Vec<Self>),
        Dict(alloc::collections::BTreeMap<String, Self>),
    }

    #[allow(dead_code, non_camel_case_types)]
    struct ErlangLineClassifierFrameContext {
        event: alloc::rc::Rc<ErlangLineClassifierFrameEvent>,
        _return: Option<ErlangLineClassifierFrameReturn>,
        _data: alloc::collections::BTreeMap<String, ErlangLineClassifierFrameValue>,
        _transitioned: bool,
    }

    impl ErlangLineClassifierFrameContext {
        fn new(event: alloc::rc::Rc<ErlangLineClassifierFrameEvent>, default_return: Option<ErlangLineClassifierFrameReturn>) -> Self {
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
    enum ErlangLineClassifierStateContext {
        Active,
        Empty,
    }

    impl Default for ErlangLineClassifierStateContext {
        fn default() -> Self {
            ErlangLineClassifierStateContext::Active
        }
    }

    #[allow(dead_code, non_camel_case_types)]
    #[derive(Clone)]
    struct ErlangLineClassifierCompartment {
        state: String,
        state_context: ErlangLineClassifierStateContext,
        enter_args: Vec<String>,
        exit_args: Vec<String>,
        forward_event: Option<ErlangLineClassifierFrameEvent>,
        parent_compartment: Option<Box<ErlangLineClassifierCompartment>>,
    }

    impl ErlangLineClassifierCompartment {
        fn new(state: &str) -> Self {
            let state_context = match state {
                "Active" => ErlangLineClassifierStateContext::Active,
                _ => ErlangLineClassifierStateContext::Empty,
            };
            Self {
                state: state.to_string(),
                state_context,
                enter_args: Vec::new(),
                exit_args: Vec::new(),
                forward_event: None,
                parent_compartment: None,
            }
        }
    }

    #[allow(dead_code)]
    pub struct ErlangLineClassifier {
        _state_stack: Vec<ErlangLineClassifierCompartment>,
        __compartment: ErlangLineClassifierCompartment,
        __next_compartment: Option<ErlangLineClassifierCompartment>,
        _context_stack: Vec<ErlangLineClassifierFrameContext>,
    }

    #[allow(non_snake_case)]
    impl ErlangLineClassifier {
        pub fn new() -> Self {
            Self {
                _state_stack: Vec::new(),
                _context_stack: Vec::new(),
                __compartment: ErlangLineClassifierCompartment::new("Active"),
                __next_compartment: None,
            }
        }

        pub fn __create() -> Self {
            let mut c = Self::new();
            c.__compartment = c.__prepareEnter("Active", vec![]);
            let __e = alloc::rc::Rc::new(ErlangLineClassifierFrameEvent::FrameEnter { args: c.__compartment.enter_args.clone() });
            let __ctx = ErlangLineClassifierFrameContext::new(alloc::rc::Rc::clone(&__e), None);
            c._context_stack.push(__ctx);
            c.__kernel(&__e);
            c._context_stack.pop();
            c
        }

        fn __hsm_chain(&mut self, leaf: &str) -> &'static [&'static str] {
            match leaf {
                "Active" => &["Active"],
                _ => &[],
            }
        }

        fn __prepareEnter(&mut self, leaf: &str, enter_args: Vec<String>) -> ErlangLineClassifierCompartment {
            let chain = self.__hsm_chain(leaf);
            let mut comp: Option<ErlangLineClassifierCompartment> = None;
            for name in chain.iter() {
                let mut new_comp = ErlangLineClassifierCompartment::new(name);
                new_comp.enter_args = enter_args.clone();
                if let Some(parent) = comp.take() {
                    new_comp.parent_compartment = Some(Box::new(parent));
                }
                comp = Some(new_comp);
            }
            comp.expect("chain must contain at least the leaf state")
        }

        fn __prepareExit(&mut self, exit_args: Vec<String>) {
            self.__compartment.exit_args = exit_args.clone();
            let mut cursor = self.__compartment.parent_compartment.as_deref_mut();
            while let Some(c) = cursor {
                c.exit_args = exit_args.clone();
                cursor = c.parent_compartment.as_deref_mut();
            }
        }

        fn __kernel(&mut self, __e: &alloc::rc::Rc<ErlangLineClassifierFrameEvent>) {
            // Route event to current state.
            self.__router(__e);
            // Drain any transitions queued by the handler.
            while self.__next_compartment.is_some() {
                let next_compartment = self.__next_compartment.take().expect("invariant: while-loop guard checked is_some()");
                // Exit the current (leaf) state.
                let exit_args = self.__compartment.exit_args.clone();
                let exit_event = alloc::rc::Rc::new(ErlangLineClassifierFrameEvent::FrameExit { args: exit_args });
                self.__router(&exit_event);
                // Switch to the new compartment.
                self.__compartment = next_compartment;
                // Three-branch forward-event handling (RFC-0025 Track B.1: forward
                // event is matched on enum variant; $> recognition is now a
                // structural match, not a string compare).
                match self.__compartment.forward_event.take() {
                    None => {
                        // No forwarded event — synthesize a fresh $>.
                        let enter_args = self.__compartment.enter_args.clone();
                        let enter_event = alloc::rc::Rc::new(ErlangLineClassifierFrameEvent::FrameEnter { args: enter_args });
                        self.__router(&enter_event);
                    }
                    Some(fwd) if matches!(fwd, ErlangLineClassifierFrameEvent::FrameEnter { .. }) => {
                        // Forwarded event IS $> — dispatch directly so the
                        // destination's $> handler receives the caller's payload.
                        let fwd_rc = alloc::rc::Rc::new(fwd);
                        self.__router(&fwd_rc);
                    }
                    Some(fwd) => {
                        // Forwarded event is not $> — initialize the destination
                        // with a fresh $>, then dispatch the forward.
                        let enter_args = self.__compartment.enter_args.clone();
                        let enter_event = alloc::rc::Rc::new(ErlangLineClassifierFrameEvent::FrameEnter { args: enter_args });
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

        fn __router(&mut self, __e: &alloc::rc::Rc<ErlangLineClassifierFrameEvent>) {
            let __ev: &ErlangLineClassifierFrameEvent = __e;
            match self.__compartment.state.as_str() {
                "Active" => self._state_Active(__ev),
                _ => {}
            }
        }

        fn __transition(&mut self, next_compartment: ErlangLineClassifierCompartment) {
            self.__next_compartment = Some(next_compartment);
        }

        pub fn classify(&mut self, line: String, actions_csv: String, interfaces_csv: String, data_var: String) -> String {
            let __e = alloc::rc::Rc::new(ErlangLineClassifierFrameEvent::Classify { line: line.clone(), actions_csv: actions_csv.clone(), interfaces_csv: interfaces_csv.clone(), data_var: data_var.clone() });
            let mut __ctx = ErlangLineClassifierFrameContext::new(alloc::rc::Rc::clone(&__e), None);
            self._context_stack.push(__ctx);
            self.__kernel(&__e);
            let __ctx = self._context_stack.pop().expect("invariant: handler must have pushed a context before reading return");
            match __ctx._return {
                Some(ErlangLineClassifierFrameReturn::Classify(v)) => v,
                Some(ErlangLineClassifierFrameReturn::_Lifecycle(v)) => v.downcast_ref::<String>().cloned().unwrap_or_default(),
                _ => Default::default(),
            }
        }

        fn _state_Active(&mut self, __e: &ErlangLineClassifierFrameEvent) {
            match __e {
                ErlangLineClassifierFrameEvent::Classify { line, actions_csv, interfaces_csv, data_var, .. } => {
                    self._s_Active_hdl_user_classify(__e, line.clone(), actions_csv.clone(), interfaces_csv.clone(), data_var.clone());
                }
                _ => {}
            }
        }

        fn _s_Active_hdl_user_classify(&mut self, __e: &ErlangLineClassifierFrameEvent, line: String, actions_csv: String, interfaces_csv: String, data_var: String) {
                            let l = line.trim();
                            let action_names: Vec<&str> = if actions_csv.is_empty() {
                                Vec::new()
                            } else {
                                actions_csv.split(',').collect()
                            };
                            let interface_names: Vec<&str> = if interfaces_csv.is_empty() {
                                Vec::new()
                            } else {
                                interfaces_csv.split(',').collect()
                            };
            
                            fn classify_one(l: &str, action_names: &[&str], interface_names: &[&str], data_var: &str) -> String {
                                // `self.<field> = self.<iface>(args)` — domain write whose
                                // RHS is an interface call. Must be checked before the
                                // bare InterfaceCall branch.
                                for iface in interface_names {
                                    let call_pat = format!("self.{}(", iface);
                                    if l.starts_with("self.") && l.contains(" = ") && l.contains(&call_pat) {
                                        if let Some(eq_pos) = l.find(" = ") {
                                            let lhs = l[..eq_pos].trim();
                                            let rhs = l[eq_pos + 3..].trim().trim_end_matches(';').trim();
                                            let lhs_field = lhs.strip_prefix("self.").unwrap_or("");
                                            let lhs_is_simple_field = !lhs_field.is_empty()
                                                && !lhs_field.contains('.')
                                                && !lhs_field.contains('(');
                                            if lhs_is_simple_field && rhs.starts_with(&call_pat) {
                                                let inner_start = call_pat.len();
                                                let inner_end = rhs.rfind(')').unwrap_or(rhs.len());
                                                let args = rhs[inner_start..inner_end].trim().to_string();
                                                let method = crate::frame_c::compiler::codegen::codegen_utils::to_snake_case(iface);
                                                return format!(
                                                    "InterfaceCallWithBind|field={}|method={}|args={}",
                                                    lhs_field, method, args
                                                );
                                            }
                                        }
                                    }
                                }
            
                                // self.method(args) → interface dispatch. Pick the
                                // OUTERMOST interface call when multiple are present.
                                let mut best: Option<(&str, usize)> = None;
                                for iface in interface_names {
                                    let pattern = format!("self.{}(", iface);
                                    if let Some(pos) = l.find(&pattern) {
                                        match best {
                                            None => best = Some((iface, pos)),
                                            Some((_, cur_pos)) if pos < cur_pos => best = Some((iface, pos)),
                                            _ => {}
                                        }
                                    }
                                }
                                if let Some((iface, open_pos)) = best {
                                    let pattern = format!("self.{}(", iface);
                                    let call_start = open_pos + pattern.len();
                                    let open_paren_idx = call_start - 1;
                                    let bytes = l.as_bytes();
                                    let mut depth = 0i32;
                                    let mut call_end = l.len();
                                    for i in open_paren_idx..bytes.len() {
                                        match bytes[i] {
                                            b'(' => depth += 1,
                                            b')' => {
                                                depth -= 1;
                                                if depth == 0 {
                                                    call_end = i;
                                                    break;
                                                }
                                            }
                                            _ => {}
                                        }
                                    }
                                    let method_snake = crate::frame_c::compiler::codegen::codegen_utils::to_snake_case(iface);
                                    let args = l[call_start..call_end].trim().to_string();
                                    if let Some(eq_pos) = l.find('=') {
                                        if eq_pos < open_pos {
                                            let result_var = l[..eq_pos].trim().to_string();
                                            return format!(
                                                "InterfaceCall|method={}|args={}|result_var={}",
                                                method_snake, args, result_var
                                            );
                                        }
                                    }
                                    return format!(
                                        "InterfaceCall|method={}|args={}|result_var=_",
                                        method_snake, args
                                    );
                                }
            
                                // self.field = self.method(args) — record-update whose
                                // RHS is an action/op call.
                                for action in action_names {
                                    let call_pattern = format!("self.{}(", action);
                                    if l.starts_with("self.") && l.contains('=') && l.contains(&call_pattern) {
                                        if let Some(eq_pos) = l[5..].find('=') {
                                            let field = l[5..5 + eq_pos].trim().to_string();
                                            let rhs = l[5 + eq_pos + 1..].trim().trim_end_matches(';').trim();
                                            if rhs.contains(&call_pattern) {
                                                let action_lc = crate::frame_c::compiler::codegen::erlang_system::lexical::erlang_op_name(action);
                                                let rewritten_call = rhs
                                                    .replace(&call_pattern, &format!("{}({}, ", action_lc, data_var))
                                                    .replace(
                                                        &format!("({}, )", data_var),
                                                        &format!("({})", data_var),
                                                    );
                                                return format!(
                                                    "ActionCallWithBind|field={}|call={}",
                                                    field, rewritten_call
                                                );
                                            }
                                        }
                                    }
                                }
            
                                // self.method(args) → action call that modifies Data.
                                for action in action_names {
                                    let pattern = format!("self.{}(", action);
                                    if l.contains(&pattern) {
                                        let action_lc = crate::frame_c::compiler::codegen::erlang_system::lexical::erlang_op_name(action);
                                        let replaced = l.replace(&pattern, &format!("{}({}, ", action_lc, data_var));
                                        let fixed = replaced
                                            .replace(&format!("({}, )", data_var), &format!("({})", data_var))
                                            .trim_end_matches(';')
                                            .trim()
                                            .to_string();
                                        return format!("ActionCall|{}", fixed);
                                    }
                                }
            
                                // self.field = expr → record update. String-aware on RHS.
                                if l.starts_with("self.") && l.contains('=') {
                                    let rest = &l[5..];
                                    if let Some(eq_pos) = rest.find('=') {
                                        let field = rest[..eq_pos].trim().to_string();
                                        let rhs = rest[eq_pos + 1..].trim().trim_end_matches(';').trim();
                                        let replacement = format!("{}#data.", data_var);
                                        let value = crate::frame_c::compiler::codegen::codegen_utils::replace_outside_strings_and_comments(
                                            rhs,
                                            crate::frame_c::visitors::TargetLanguage::Erlang,
                                            &[("self.", replacement.as_str())],
                                        );
                                        return format!("RecordUpdate|field={}|value={}", field, value);
                                    }
                                }
            
                                // self.field → DataVar#data.field (access). String-aware.
                                let replacement = format!("{}#data.", data_var);
                                let plain = crate::frame_c::compiler::codegen::codegen_utils::replace_outside_strings_and_comments(
                                    l,
                                    crate::frame_c::visitors::TargetLanguage::Erlang,
                                    &[("self.", replacement.as_str())],
                                );
                                format!("Plain|{}", plain)
                            }
            
                            let result = classify_one(l, &action_names, &interface_names, &data_var);
            let __return_val = ErlangLineClassifierFrameReturn::Classify(result.clone());
                            if let Some(ctx) = self._context_stack.last_mut() { ctx._return = Some(__return_val); }
        }
    }
}
pub use _erlang_line_classifier_framec::*;

