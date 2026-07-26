
// STATE-REACHABILITY analysis, dogfooded as a plain `@@system` GRAPH WALKER (the second
// back-half machine, after HsmCycle). A state that no transition/stack-push/parent path can
// reach from the start state is dead — worth a warning. The graph is an EDGE LIST: `from[e] ->
// to[e]` for each of `edge_count` edges over `node_count` nodes. `seed` is the initial visited
// set (just the start node). framec owns the WALK; the leaf `relax` queries+grows the frontier.
//
// The walk is iterative relaxation (no explicit stack): each $Pass sweeps every edge once and
// marks a `to` node visited when its `from` node already is; it repeats until a $Pass changes
// nothing (or `node_count` passes — the longest simple path — elapse). `visited` then holds
// exactly the nodes reachable from the start. The wrapper drives `step()` a bounded number of
// times and reads `visited`.

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
mod _reachability_framec {
    use super::*;
    extern crate alloc;
    use alloc::{vec, format};
    #[derive(Clone)]
    #[allow(dead_code, non_camel_case_types)]
    enum ReachabilityFrameEvent {
        Step {  },
        FrameEnter {},
        FrameExit {},
    }

    #[derive(Clone)]
    #[allow(dead_code, non_camel_case_types)]
    enum ReachabilityFrameReturn {
        _Lifecycle(alloc::rc::Rc<dyn core::any::Any>),
    }

    #[allow(dead_code)]
    impl ReachabilityFrameEvent {
        fn name(&self) -> &'static str {
            match self {
                ReachabilityFrameEvent::Step { .. } => "step",
                ReachabilityFrameEvent::FrameEnter { .. } => "$>",
                ReachabilityFrameEvent::FrameExit { .. } => "<$",
            }
        }
    }

    #[derive(Clone, Debug)]
    #[allow(dead_code, non_camel_case_types)]
    enum ReachabilityFrameValue {
        Int(i64),
        Float(f64),
        Bool(bool),
        Str(String),
        List(Vec<Self>),
        Dict(alloc::collections::BTreeMap<String, Self>),
    }

    #[allow(dead_code, non_camel_case_types)]
    struct ReachabilityFrameContext {
        event: alloc::rc::Rc<ReachabilityFrameEvent>,
        _return: Option<ReachabilityFrameReturn>,
        _data: alloc::collections::BTreeMap<String, ReachabilityFrameValue>,
        _transitioned: bool,
    }

    impl ReachabilityFrameContext {
        fn new(event: alloc::rc::Rc<ReachabilityFrameEvent>, default_return: Option<ReachabilityFrameReturn>) -> Self {
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
    enum ReachabilityStateContext {
        Pass,
        Scan,
        EndPass,
        Done,
        __NoContext,
    }

    impl Default for ReachabilityStateContext {
        fn default() -> Self {
            ReachabilityStateContext::Pass
        }
    }

    #[allow(dead_code, non_camel_case_types)]
    #[derive(Clone)]
    struct ReachabilityCompartment {
        state: String,
        state_context: ReachabilityStateContext,
        forward_event: Option<ReachabilityFrameEvent>,
        parent_compartment: Option<Box<ReachabilityCompartment>>,
    }

    impl ReachabilityCompartment {
        fn new(state: &str) -> Self {
            let state_context = match state {
                "Pass" => ReachabilityStateContext::Pass,
                "Scan" => ReachabilityStateContext::Scan,
                "EndPass" => ReachabilityStateContext::EndPass,
                "Done" => ReachabilityStateContext::Done,
                _ => ReachabilityStateContext::__NoContext,
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
    pub struct Reachability {
        _state_stack: Vec<ReachabilityCompartment>,
        __compartment: ReachabilityCompartment,
        __next_compartment: Option<ReachabilityCompartment>,
        _context_stack: Vec<ReachabilityFrameContext>,
        pub from: Vec<i32>,
        pub to: Vec<i32>,
        pub edge_count: usize,
        pub node_count: usize,
        pub visited: Vec<bool>,
        pub changed: bool,
        pub p: usize,
        pub e: usize,
    }

    #[allow(non_snake_case)]
    impl Reachability {
        pub fn new(from: Vec<i32>, to: Vec<i32>, edge_count: usize, node_count: usize, seed: Vec<bool>) -> Self {
            Self {
                _state_stack: Vec::new(),
                _context_stack: Vec::new(),
                from: from,
                to: to,
                edge_count: edge_count,
                node_count: node_count,
                visited: seed,
                changed: false,
                p: 0,
                e: 0,
                __compartment: ReachabilityCompartment::new("Pass"),
                __next_compartment: None,
            }
        }

        pub fn __create(from: Vec<i32>, to: Vec<i32>, edge_count: usize, node_count: usize, seed: Vec<bool>) -> Self {
            let mut c = Self::new(from, to, edge_count, node_count, seed);
            c.__compartment = c.__prepareEnter("Pass");
            let __e = alloc::rc::Rc::new(ReachabilityFrameEvent::FrameEnter {});
            let __ctx = ReachabilityFrameContext::new(alloc::rc::Rc::clone(&__e), None);
            c._context_stack.push(__ctx);
            c.__kernel(&__e);
            c._context_stack.pop();
            c
        }

        fn __hsm_chain(&mut self, leaf: &str) -> &'static [&'static str] {
            match leaf {
                "Pass" => &["Pass"],
                "Scan" => &["Scan"],
                "EndPass" => &["EndPass"],
                "Done" => &["Done"],
                _ => &[],
            }
        }

        fn __prepareEnter(&mut self, leaf: &str) -> ReachabilityCompartment {
            let chain = self.__hsm_chain(leaf);
            let mut comp: Option<ReachabilityCompartment> = None;
            for name in chain.iter() {
                let mut new_comp = ReachabilityCompartment::new(name);
                if let Some(parent) = comp.take() {
                    new_comp.parent_compartment = Some(Box::new(parent));
                }
                comp = Some(new_comp);
            }
            comp.expect("chain must contain at least the leaf state")
        }

        fn __kernel(&mut self, __e: &alloc::rc::Rc<ReachabilityFrameEvent>) {
            // Route event to current state.
            self.__router(__e);
            // Drain any transitions queued by the handler.
            while self.__next_compartment.is_some() {
                let next_compartment = self.__next_compartment.take().expect("invariant: while-loop guard checked is_some()");
                // Exit the current (leaf) state. RFC-0025.1: exit args live in the
                // source state's typed ctx (written at the transition site), so the
                // synthesized `<$` event carries no payload.
                let exit_event = alloc::rc::Rc::new(ReachabilityFrameEvent::FrameExit {});
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
                        let enter_event = alloc::rc::Rc::new(ReachabilityFrameEvent::FrameEnter {});
                        self.__router(&enter_event);
                    }
                    Some(fwd) if matches!(fwd, ReachabilityFrameEvent::FrameEnter { .. }) => {
                        // Forwarded event IS $> — dispatch directly so the
                        // destination's $> handler receives the caller's payload.
                        let fwd_rc = alloc::rc::Rc::new(fwd);
                        self.__router(&fwd_rc);
                    }
                    Some(fwd) => {
                        // Forwarded event is not $> — initialize the destination
                        // with a fresh $>, then dispatch the forward.
                        let enter_event = alloc::rc::Rc::new(ReachabilityFrameEvent::FrameEnter {});
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

        fn __router(&mut self, __e: &alloc::rc::Rc<ReachabilityFrameEvent>) {
            let __ev: &ReachabilityFrameEvent = __e;
            match self.__compartment.state.as_str() {
                "Pass" => self._state_Pass(__ev),
                "Scan" => self._state_Scan(__ev),
                "EndPass" => self._state_EndPass(__ev),
                "Done" => self._state_Done(__ev),
                _ => {}
            }
        }

        fn __transition(&mut self, next_compartment: ReachabilityCompartment) {
            self.__next_compartment = Some(next_compartment);
        }

        pub fn step(&mut self) {
            let __e = alloc::rc::Rc::new(ReachabilityFrameEvent::Step {});
            let mut __ctx = ReachabilityFrameContext::new(alloc::rc::Rc::clone(&__e), None);
            self._context_stack.push(__ctx);
            self.__kernel(&__e);
            self._context_stack.pop();
        }

        fn _state_Pass(&mut self, __e: &ReachabilityFrameEvent) {
            match __e {
                ReachabilityFrameEvent::Step { .. } => { self._s_Pass_hdl_user_step(__e); }
                _ => {}
            }
        }

        fn _state_Scan(&mut self, __e: &ReachabilityFrameEvent) {
            match __e {
                ReachabilityFrameEvent::Step { .. } => { self._s_Scan_hdl_user_step(__e); }
                _ => {}
            }
        }

        fn _state_EndPass(&mut self, __e: &ReachabilityFrameEvent) {
            match __e {
                ReachabilityFrameEvent::Step { .. } => { self._s_EndPass_hdl_user_step(__e); }
                _ => {}
            }
        }

        fn _state_Done(&mut self, __e: &ReachabilityFrameEvent) {
            match __e {
                _ => {}
            }
        }

        fn _s_Pass_hdl_user_step(&mut self, __e: &ReachabilityFrameEvent) {
            if self.p >= self.node_count {
                let mut __compartment = self.__prepareEnter("Done");
                self.__transition(__compartment);
                return;
            }
            self.changed = false;
            self.e = 0;
            let mut __compartment = self.__prepareEnter("Scan");
            self.__transition(__compartment);
            return;
        }

        fn _s_Scan_hdl_user_step(&mut self, __e: &ReachabilityFrameEvent) {
            if self.e >= self.edge_count {
                let mut __compartment = self.__prepareEnter("EndPass");
                self.__transition(__compartment);
                return;
            }
            let grew = relax(&mut self.visited, &self.from, &self.to, self.e);
            if grew {
                self.changed = true;
            }
            self.e = self.e + 1;
            let mut __compartment = self.__prepareEnter("Scan");
            self.__transition(__compartment);
            return;
        }

        fn _s_EndPass_hdl_user_step(&mut self, __e: &ReachabilityFrameEvent) {
            if self.changed {
                self.p = self.p + 1;
                let mut __compartment = self.__prepareEnter("Pass");
                self.__transition(__compartment);
                return;
            }
            let mut __compartment = self.__prepareEnter("Done");
            self.__transition(__compartment);
            return;
        }
    }
}
pub use _reachability_framec::*;
