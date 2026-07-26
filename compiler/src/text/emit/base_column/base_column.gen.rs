
// The handler/action body BASE-COLUMN min-fold, dogfooded as a plain `@@system` — the emit-side
// twin of StmtWalk (same READ-ONLY BORROWED DOMAIN: the statement slice is a shared borrow
// threaded through one lifetime `'a`; the cursor, the running minimum, and the seen bit are the
// OWNED domain). It reifies the `base` computation `emit_body` fed to StmtWalk: the SHALLOWEST
// logical column across the body's statements — the reindent baseline everything else is measured
// against, so the user's nesting is reproduced without framec knowing what an `if` is.
//
// framec owns the WALK (the cursor `i`, the `min`/`seen` registers, the halt at `len`); the 8-way
// per-Stmt column extraction is a per-item function surfaced as the leaf `col_at`, which returns
// the statement's column or -1 for a Trivia (or an out-of-bounds index) — exactly the arms of the
// original `.filter_map(...)`. `$Scan` cycles: at end-of-slice (`i >= len`) it halts to `$Done`; a
// -1 column is skipped (the `filter_map` None); the first real column seeds `min`+`seen`; a later
// column shrinks `min` when smaller. The wrapper reads `min` (or 0 when nothing was recorded — the
// original `.unwrap_or(0)`).
//
// Regen: framec-ng -l rust --emit base_column.frs | grep -v '^#!\[allow' > base_column.gen.rs

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
mod _base_column_framec {
    use super::*;
    extern crate alloc;
    use alloc::{vec, format};
    #[derive(Clone)]
    #[allow(dead_code, non_camel_case_types)]
    enum BaseColumnFrameEvent {
        Step {  },
        FrameEnter {},
        FrameExit {},
    }

    #[derive(Clone)]
    #[allow(dead_code, non_camel_case_types)]
    enum BaseColumnFrameReturn {
        _Lifecycle(alloc::rc::Rc<dyn core::any::Any>),
    }

    #[allow(dead_code)]
    impl BaseColumnFrameEvent {
        fn name(&self) -> &'static str {
            match self {
                BaseColumnFrameEvent::Step { .. } => "step",
                BaseColumnFrameEvent::FrameEnter { .. } => "$>",
                BaseColumnFrameEvent::FrameExit { .. } => "<$",
            }
        }
    }

    #[derive(Clone, Debug)]
    #[allow(dead_code, non_camel_case_types)]
    enum BaseColumnFrameValue {
        Int(i64),
        Float(f64),
        Bool(bool),
        Str(String),
        List(Vec<Self>),
        Dict(alloc::collections::BTreeMap<String, Self>),
    }

    #[allow(dead_code, non_camel_case_types)]
    struct BaseColumnFrameContext {
        event: alloc::rc::Rc<BaseColumnFrameEvent>,
        _return: Option<BaseColumnFrameReturn>,
        _data: alloc::collections::BTreeMap<String, BaseColumnFrameValue>,
        _transitioned: bool,
    }

    impl BaseColumnFrameContext {
        fn new(event: alloc::rc::Rc<BaseColumnFrameEvent>, default_return: Option<BaseColumnFrameReturn>) -> Self {
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
    enum BaseColumnStateContext {
        Scan,
        Done,
        __NoContext,
    }

    impl Default for BaseColumnStateContext {
        fn default() -> Self {
            BaseColumnStateContext::Scan
        }
    }

    #[allow(dead_code, non_camel_case_types)]
    #[derive(Clone)]
    struct BaseColumnCompartment {
        state: String,
        state_context: BaseColumnStateContext,
        forward_event: Option<BaseColumnFrameEvent>,
        parent_compartment: Option<Box<BaseColumnCompartment>>,
    }

    impl BaseColumnCompartment {
        fn new(state: &str) -> Self {
            let state_context = match state {
                "Scan" => BaseColumnStateContext::Scan,
                "Done" => BaseColumnStateContext::Done,
                _ => BaseColumnStateContext::__NoContext,
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
    pub struct BaseColumn<'a> {
        _state_stack: Vec<BaseColumnCompartment>,
        __compartment: BaseColumnCompartment,
        __next_compartment: Option<BaseColumnCompartment>,
        _context_stack: Vec<BaseColumnFrameContext>,
        pub stmts: &'a [Stmt],
        pub len: usize,
        pub min: u32,
        pub seen: bool,
        pub i: usize,
    }

    #[allow(non_snake_case)]
    impl<'a> BaseColumn<'a> {
        pub fn new(stmts: &'a [Stmt], len: usize) -> Self {
            Self {
                _state_stack: Vec::new(),
                _context_stack: Vec::new(),
                stmts: stmts,
                len: len,
                min: 0,
                seen: false,
                i: 0,
                __compartment: BaseColumnCompartment::new("Scan"),
                __next_compartment: None,
            }
        }

        pub fn __create(stmts: &'a [Stmt], len: usize) -> Self {
            let mut c = Self::new(stmts, len);
            c.__compartment = c.__prepareEnter("Scan");
            let __e = alloc::rc::Rc::new(BaseColumnFrameEvent::FrameEnter {});
            let __ctx = BaseColumnFrameContext::new(alloc::rc::Rc::clone(&__e), None);
            c._context_stack.push(__ctx);
            c.__kernel(&__e);
            c._context_stack.pop();
            c
        }

        fn __hsm_chain(&mut self, leaf: &str) -> &'static [&'static str] {
            match leaf {
                "Scan" => &["Scan"],
                "Done" => &["Done"],
                _ => &[],
            }
        }

        fn __prepareEnter(&mut self, leaf: &str) -> BaseColumnCompartment {
            let chain = self.__hsm_chain(leaf);
            let mut comp: Option<BaseColumnCompartment> = None;
            for name in chain.iter() {
                let mut new_comp = BaseColumnCompartment::new(name);
                if let Some(parent) = comp.take() {
                    new_comp.parent_compartment = Some(Box::new(parent));
                }
                comp = Some(new_comp);
            }
            comp.expect("chain must contain at least the leaf state")
        }

        fn __kernel(&mut self, __e: &alloc::rc::Rc<BaseColumnFrameEvent>) {
            // Route event to current state.
            self.__router(__e);
            // Drain any transitions queued by the handler.
            while self.__next_compartment.is_some() {
                let next_compartment = self.__next_compartment.take().expect("invariant: while-loop guard checked is_some()");
                // Exit the current (leaf) state. RFC-0025.1: exit args live in the
                // source state's typed ctx (written at the transition site), so the
                // synthesized `<$` event carries no payload.
                let exit_event = alloc::rc::Rc::new(BaseColumnFrameEvent::FrameExit {});
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
                        let enter_event = alloc::rc::Rc::new(BaseColumnFrameEvent::FrameEnter {});
                        self.__router(&enter_event);
                    }
                    Some(fwd) if matches!(fwd, BaseColumnFrameEvent::FrameEnter { .. }) => {
                        // Forwarded event IS $> — dispatch directly so the
                        // destination's $> handler receives the caller's payload.
                        let fwd_rc = alloc::rc::Rc::new(fwd);
                        self.__router(&fwd_rc);
                    }
                    Some(fwd) => {
                        // Forwarded event is not $> — initialize the destination
                        // with a fresh $>, then dispatch the forward.
                        let enter_event = alloc::rc::Rc::new(BaseColumnFrameEvent::FrameEnter {});
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

        fn __router(&mut self, __e: &alloc::rc::Rc<BaseColumnFrameEvent>) {
            let __ev: &BaseColumnFrameEvent = __e;
            match self.__compartment.state.as_str() {
                "Scan" => self._state_Scan(__ev),
                "Done" => self._state_Done(__ev),
                _ => {}
            }
        }

        fn __transition(&mut self, next_compartment: BaseColumnCompartment) {
            self.__next_compartment = Some(next_compartment);
        }

        pub fn step(&mut self) {
            let __e = alloc::rc::Rc::new(BaseColumnFrameEvent::Step {});
            let mut __ctx = BaseColumnFrameContext::new(alloc::rc::Rc::clone(&__e), None);
            self._context_stack.push(__ctx);
            self.__kernel(&__e);
            self._context_stack.pop();
        }

        fn _state_Scan(&mut self, __e: &BaseColumnFrameEvent) {
            match __e {
                BaseColumnFrameEvent::Step { .. } => { self._s_Scan_hdl_user_step(__e); }
                _ => {}
            }
        }

        fn _state_Done(&mut self, __e: &BaseColumnFrameEvent) {
            match __e {
                _ => {}
            }
        }

        fn _s_Scan_hdl_user_step(&mut self, __e: &BaseColumnFrameEvent) {
            if self.i >= self.len {
                let mut __compartment = self.__prepareEnter("Done");
                self.__transition(__compartment);
                return;
            }
            let c = col_at(self.stmts, self.i);
            if c < 0 {
                self.i = self.i + 1;
                let mut __compartment = self.__prepareEnter("Scan");
                self.__transition(__compartment);
                return;
            }
            let cu = c as u32;
            if self.seen == false {
                self.min = cu;
                self.seen = true;
                self.i = self.i + 1;
                let mut __compartment = self.__prepareEnter("Scan");
                self.__transition(__compartment);
                return;
            }
            if cu < self.min {
                self.min = cu;
            }
            self.i = self.i + 1;
            let mut __compartment = self.__prepareEnter("Scan");
            self.__transition(__compartment);
            return;
        }
    }
}
pub use _base_column_framec::*;
