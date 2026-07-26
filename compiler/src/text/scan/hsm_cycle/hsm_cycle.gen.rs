
// HSM parent-chain CYCLE detector, dogfooded as a plain `@@system` GRAPH WALKER (not a byte
// scanner) — the first back-half machine, cracking the non-byte drive pattern. A cycle in the
// parent chain (`$A => $B => $A`) would infinite-loop the HSM handler dispatch, so it must be
// caught. The graph is the `parents` array (parent[i] = parent index, or -1 for a root),
// passed via `new(parents, count)`. framec owns the WALK ($Next picks a start node, $Follow
// chases parents); the leaf `parent_of` queries the graph. A node whose chain exceeds `count`
// hops is in a cycle (pigeonhole). The wrapper drives `step()` a bounded number of times.
//
// cyclic ends true iff any parent chain cycles.

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
mod _hsm_cycle_framec {
    use super::*;
    extern crate alloc;
    use alloc::{vec, format};
    #[derive(Clone)]
    #[allow(dead_code, non_camel_case_types)]
    enum HsmCycleFrameEvent {
        Step {  },
        FrameEnter {},
        FrameExit {},
    }

    #[derive(Clone)]
    #[allow(dead_code, non_camel_case_types)]
    enum HsmCycleFrameReturn {
        _Lifecycle(alloc::rc::Rc<dyn core::any::Any>),
    }

    #[allow(dead_code)]
    impl HsmCycleFrameEvent {
        fn name(&self) -> &'static str {
            match self {
                HsmCycleFrameEvent::Step { .. } => "step",
                HsmCycleFrameEvent::FrameEnter { .. } => "$>",
                HsmCycleFrameEvent::FrameExit { .. } => "<$",
            }
        }
    }

    #[derive(Clone, Debug)]
    #[allow(dead_code, non_camel_case_types)]
    enum HsmCycleFrameValue {
        Int(i64),
        Float(f64),
        Bool(bool),
        Str(String),
        List(Vec<Self>),
        Dict(alloc::collections::BTreeMap<String, Self>),
    }

    #[allow(dead_code, non_camel_case_types)]
    struct HsmCycleFrameContext {
        event: alloc::rc::Rc<HsmCycleFrameEvent>,
        _return: Option<HsmCycleFrameReturn>,
        _data: alloc::collections::BTreeMap<String, HsmCycleFrameValue>,
        _transitioned: bool,
    }

    impl HsmCycleFrameContext {
        fn new(event: alloc::rc::Rc<HsmCycleFrameEvent>, default_return: Option<HsmCycleFrameReturn>) -> Self {
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
    enum HsmCycleStateContext {
        Next,
        Follow,
        Done,
        __NoContext,
    }

    impl Default for HsmCycleStateContext {
        fn default() -> Self {
            HsmCycleStateContext::Next
        }
    }

    #[allow(dead_code, non_camel_case_types)]
    #[derive(Clone)]
    struct HsmCycleCompartment {
        state: String,
        state_context: HsmCycleStateContext,
        forward_event: Option<HsmCycleFrameEvent>,
        parent_compartment: Option<Box<HsmCycleCompartment>>,
    }

    impl HsmCycleCompartment {
        fn new(state: &str) -> Self {
            let state_context = match state {
                "Next" => HsmCycleStateContext::Next,
                "Follow" => HsmCycleStateContext::Follow,
                "Done" => HsmCycleStateContext::Done,
                _ => HsmCycleStateContext::__NoContext,
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
    pub struct HsmCycle {
        _state_stack: Vec<HsmCycleCompartment>,
        __compartment: HsmCycleCompartment,
        __next_compartment: Option<HsmCycleCompartment>,
        _context_stack: Vec<HsmCycleFrameContext>,
        pub parents: Vec<i32>,
        pub count: usize,
        pub k: usize,
        pub cur: i32,
        pub steps: usize,
        pub cyclic: bool,
    }

    #[allow(non_snake_case)]
    impl HsmCycle {
        pub fn new(parents: Vec<i32>, count: usize) -> Self {
            Self {
                _state_stack: Vec::new(),
                _context_stack: Vec::new(),
                parents: parents,
                count: count,
                k: 0,
                cur: 0,
                steps: 0,
                cyclic: false,
                __compartment: HsmCycleCompartment::new("Next"),
                __next_compartment: None,
            }
        }

        pub fn __create(parents: Vec<i32>, count: usize) -> Self {
            let mut c = Self::new(parents, count);
            c.__compartment = c.__prepareEnter("Next");
            let __e = alloc::rc::Rc::new(HsmCycleFrameEvent::FrameEnter {});
            let __ctx = HsmCycleFrameContext::new(alloc::rc::Rc::clone(&__e), None);
            c._context_stack.push(__ctx);
            c.__kernel(&__e);
            c._context_stack.pop();
            c
        }

        fn __hsm_chain(&mut self, leaf: &str) -> &'static [&'static str] {
            match leaf {
                "Next" => &["Next"],
                "Follow" => &["Follow"],
                "Done" => &["Done"],
                _ => &[],
            }
        }

        fn __prepareEnter(&mut self, leaf: &str) -> HsmCycleCompartment {
            let chain = self.__hsm_chain(leaf);
            let mut comp: Option<HsmCycleCompartment> = None;
            for name in chain.iter() {
                let mut new_comp = HsmCycleCompartment::new(name);
                if let Some(parent) = comp.take() {
                    new_comp.parent_compartment = Some(Box::new(parent));
                }
                comp = Some(new_comp);
            }
            comp.expect("chain must contain at least the leaf state")
        }

        fn __kernel(&mut self, __e: &alloc::rc::Rc<HsmCycleFrameEvent>) {
            // Route event to current state.
            self.__router(__e);
            // Drain any transitions queued by the handler.
            while self.__next_compartment.is_some() {
                let next_compartment = self.__next_compartment.take().expect("invariant: while-loop guard checked is_some()");
                // Exit the current (leaf) state. RFC-0025.1: exit args live in the
                // source state's typed ctx (written at the transition site), so the
                // synthesized `<$` event carries no payload.
                let exit_event = alloc::rc::Rc::new(HsmCycleFrameEvent::FrameExit {});
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
                        let enter_event = alloc::rc::Rc::new(HsmCycleFrameEvent::FrameEnter {});
                        self.__router(&enter_event);
                    }
                    Some(fwd) if matches!(fwd, HsmCycleFrameEvent::FrameEnter { .. }) => {
                        // Forwarded event IS $> — dispatch directly so the
                        // destination's $> handler receives the caller's payload.
                        let fwd_rc = alloc::rc::Rc::new(fwd);
                        self.__router(&fwd_rc);
                    }
                    Some(fwd) => {
                        // Forwarded event is not $> — initialize the destination
                        // with a fresh $>, then dispatch the forward.
                        let enter_event = alloc::rc::Rc::new(HsmCycleFrameEvent::FrameEnter {});
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

        fn __router(&mut self, __e: &alloc::rc::Rc<HsmCycleFrameEvent>) {
            let __ev: &HsmCycleFrameEvent = __e;
            match self.__compartment.state.as_str() {
                "Next" => self._state_Next(__ev),
                "Follow" => self._state_Follow(__ev),
                "Done" => self._state_Done(__ev),
                _ => {}
            }
        }

        fn __transition(&mut self, next_compartment: HsmCycleCompartment) {
            self.__next_compartment = Some(next_compartment);
        }

        pub fn step(&mut self) {
            let __e = alloc::rc::Rc::new(HsmCycleFrameEvent::Step {});
            let mut __ctx = HsmCycleFrameContext::new(alloc::rc::Rc::clone(&__e), None);
            self._context_stack.push(__ctx);
            self.__kernel(&__e);
            self._context_stack.pop();
        }

        fn _state_Next(&mut self, __e: &HsmCycleFrameEvent) {
            match __e {
                HsmCycleFrameEvent::Step { .. } => { self._s_Next_hdl_user_step(__e); }
                _ => {}
            }
        }

        fn _state_Follow(&mut self, __e: &HsmCycleFrameEvent) {
            match __e {
                HsmCycleFrameEvent::Step { .. } => { self._s_Follow_hdl_user_step(__e); }
                _ => {}
            }
        }

        fn _state_Done(&mut self, __e: &HsmCycleFrameEvent) {
            match __e {
                _ => {}
            }
        }

        fn _s_Next_hdl_user_step(&mut self, __e: &HsmCycleFrameEvent) {
            if self.k >= self.count {
                let mut __compartment = self.__prepareEnter("Done");
                self.__transition(__compartment);
                return;
            }
            self.cur = self.k as i32;
            self.steps = 0;
            let mut __compartment = self.__prepareEnter("Follow");
            self.__transition(__compartment);
            return;
        }

        fn _s_Follow_hdl_user_step(&mut self, __e: &HsmCycleFrameEvent) {
            let p = parent_of(&self.parents, self.cur);
            if p < 0 {
                self.k = self.k + 1;
                let mut __compartment = self.__prepareEnter("Next");
                self.__transition(__compartment);
                return;
            }
            if self.steps > self.count {
                self.cyclic = true;
                let mut __compartment = self.__prepareEnter("Done");
                self.__transition(__compartment);
                return;
            }
            self.cur = p;
            self.steps = self.steps + 1;
            let mut __compartment = self.__prepareEnter("Follow");
            self.__transition(__compartment);
            return;
        }
    }
}
pub use _hsm_cycle_framec::*;
