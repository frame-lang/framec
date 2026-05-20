
// W414 reachable-states BFS walker, expressed as a 3-state
// Frame system.
//
// RFC-0035 round 6. Round 5 applied Frame to a graph algorithm
// where each chain walk was an independent FSM instance. Round
// 6 takes the next step: ONE FSM instance for the entire BFS
// walk, with the visited set and queue threaded through domain
// fields that evolve across many events.
//
// States:
//
//   $Initial — no walk started; first call must be seed().
//   $Walking — BFS in progress. enqueue() adds neighbors to
//              the queue (de-duped against visited). next()
//              pops the queue head, or transitions to $Done
//              when the queue is empty.
//   $Done    — terminal. next() returns "DONE" idempotently.
//              unreachable() computes the diff against the
//              caller-supplied "all states" list.
//
// The caller orchestrates the walk:
//
//   fsm.seed(start)
//   loop {
//       let head = fsm.next();
//       if head == "DONE" { break; }
//       for neighbor in state_map[head].transitions + parents {
//           fsm.enqueue(neighbor);
//       }
//   }
//   let unreachable = fsm.unreachable(all_states_csv);
//
// Domain fields carry the algorithmic state:
//   queue   — comma-separated FIFO queue of pending node names
//   visited — comma-separated set of all nodes ever enqueued
//
// Linear-scan lookup on `visited` is fine at HSM graph scale
// (machines typically have ≤50 states). A first-class Frame
// `Set` interface type would make this O(1), but the dogfood
// observation is the same as Round 5.

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
mod _reachable_walker_framec {
    use super::*;
    extern crate alloc;
    use alloc::{vec, format};
    #[derive(Clone, Debug)]
    #[allow(dead_code, non_camel_case_types)]
    enum ReachableWalkerFrameEvent {
        Seed { start: String },
        Enqueue { name: String },
        Next {  },
        Unreachable { all_csv: String },
        FrameEnter { args: Vec<String> },
        FrameExit { args: Vec<String> },
    }

    #[derive(Clone)]
    #[allow(dead_code, non_camel_case_types)]
    enum ReachableWalkerFrameReturn {
        Enqueue(String),
        Next(String),
        Seed(String),
        Unreachable(String),
        _Lifecycle(alloc::rc::Rc<dyn core::any::Any>),
    }

    #[allow(dead_code)]
    impl ReachableWalkerFrameEvent {
        fn name(&self) -> &'static str {
            match self {
                ReachableWalkerFrameEvent::Seed { .. } => "seed",
                ReachableWalkerFrameEvent::Enqueue { .. } => "enqueue",
                ReachableWalkerFrameEvent::Next { .. } => "next",
                ReachableWalkerFrameEvent::Unreachable { .. } => "unreachable",
                ReachableWalkerFrameEvent::FrameEnter { .. } => "$>",
                ReachableWalkerFrameEvent::FrameExit { .. } => "<$",
            }
        }
    }

    #[derive(Clone, Debug)]
    #[allow(dead_code, non_camel_case_types)]
    enum ReachableWalkerFrameValue {
        Int(i64),
        Float(f64),
        Bool(bool),
        Str(String),
        List(Vec<Self>),
        Dict(alloc::collections::BTreeMap<String, Self>),
    }

    #[allow(dead_code, non_camel_case_types)]
    struct ReachableWalkerFrameContext {
        event: alloc::rc::Rc<ReachableWalkerFrameEvent>,
        _return: Option<ReachableWalkerFrameReturn>,
        _data: alloc::collections::BTreeMap<String, ReachableWalkerFrameValue>,
        _transitioned: bool,
    }

    impl ReachableWalkerFrameContext {
        fn new(event: alloc::rc::Rc<ReachableWalkerFrameEvent>, default_return: Option<ReachableWalkerFrameReturn>) -> Self {
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
    enum ReachableWalkerStateContext {
        Initial,
        Walking,
        Done,
        Empty,
    }

    impl Default for ReachableWalkerStateContext {
        fn default() -> Self {
            ReachableWalkerStateContext::Initial
        }
    }

    #[allow(dead_code, non_camel_case_types)]
    #[derive(Clone)]
    struct ReachableWalkerCompartment {
        state: String,
        state_context: ReachableWalkerStateContext,
        enter_args: Vec<String>,
        exit_args: Vec<String>,
        forward_event: Option<ReachableWalkerFrameEvent>,
        parent_compartment: Option<Box<ReachableWalkerCompartment>>,
    }

    impl ReachableWalkerCompartment {
        fn new(state: &str) -> Self {
            let state_context = match state {
                "Initial" => ReachableWalkerStateContext::Initial,
                "Walking" => ReachableWalkerStateContext::Walking,
                "Done" => ReachableWalkerStateContext::Done,
                _ => ReachableWalkerStateContext::Empty,
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
    pub struct ReachableWalker {
        _state_stack: Vec<ReachableWalkerCompartment>,
        __compartment: ReachableWalkerCompartment,
        __next_compartment: Option<ReachableWalkerCompartment>,
        _context_stack: Vec<ReachableWalkerFrameContext>,
        pub queue: String,
        pub visited: String,
    }

    #[allow(non_snake_case)]
    impl ReachableWalker {
        pub fn new() -> Self {
            Self {
                _state_stack: Vec::new(),
                _context_stack: Vec::new(),
                queue: String::new(),
                visited: String::new(),
                __compartment: ReachableWalkerCompartment::new("Initial"),
                __next_compartment: None,
            }
        }

        pub fn __create() -> Self {
            let mut c = Self::new();
            c.__compartment = c.__prepareEnter("Initial", vec![]);
            let __e = alloc::rc::Rc::new(ReachableWalkerFrameEvent::FrameEnter { args: c.__compartment.enter_args.clone() });
            let __ctx = ReachableWalkerFrameContext::new(alloc::rc::Rc::clone(&__e), None);
            c._context_stack.push(__ctx);
            c.__kernel(&__e);
            c._context_stack.pop();
            c
        }

        fn __hsm_chain(&mut self, leaf: &str) -> &'static [&'static str] {
            match leaf {
                "Initial" => &["Initial"],
                "Walking" => &["Walking"],
                "Done" => &["Done"],
                _ => &[],
            }
        }

        fn __prepareEnter(&mut self, leaf: &str, enter_args: Vec<String>) -> ReachableWalkerCompartment {
            let chain = self.__hsm_chain(leaf);
            let mut comp: Option<ReachableWalkerCompartment> = None;
            for name in chain.iter() {
                let mut new_comp = ReachableWalkerCompartment::new(name);
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

        fn __kernel(&mut self, __e: &alloc::rc::Rc<ReachableWalkerFrameEvent>) {
            // Route event to current state.
            self.__router(__e);
            // Drain any transitions queued by the handler.
            while self.__next_compartment.is_some() {
                let next_compartment = self.__next_compartment.take().expect("invariant: while-loop guard checked is_some()");
                // Exit the current (leaf) state.
                let exit_args = self.__compartment.exit_args.clone();
                let exit_event = alloc::rc::Rc::new(ReachableWalkerFrameEvent::FrameExit { args: exit_args });
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
                        let enter_event = alloc::rc::Rc::new(ReachableWalkerFrameEvent::FrameEnter { args: enter_args });
                        self.__router(&enter_event);
                    }
                    Some(fwd) if matches!(fwd, ReachableWalkerFrameEvent::FrameEnter { .. }) => {
                        // Forwarded event IS $> — dispatch directly so the
                        // destination's $> handler receives the caller's payload.
                        let fwd_rc = alloc::rc::Rc::new(fwd);
                        self.__router(&fwd_rc);
                    }
                    Some(fwd) => {
                        // Forwarded event is not $> — initialize the destination
                        // with a fresh $>, then dispatch the forward.
                        let enter_args = self.__compartment.enter_args.clone();
                        let enter_event = alloc::rc::Rc::new(ReachableWalkerFrameEvent::FrameEnter { args: enter_args });
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

        fn __router(&mut self, __e: &alloc::rc::Rc<ReachableWalkerFrameEvent>) {
            let __ev: &ReachableWalkerFrameEvent = __e;
            match self.__compartment.state.as_str() {
                "Initial" => self._state_Initial(__ev),
                "Walking" => self._state_Walking(__ev),
                "Done" => self._state_Done(__ev),
                _ => {}
            }
        }

        fn __transition(&mut self, next_compartment: ReachableWalkerCompartment) {
            self.__next_compartment = Some(next_compartment);
        }

        pub fn seed(&mut self, start: String) -> String {
            let __e = alloc::rc::Rc::new(ReachableWalkerFrameEvent::Seed { start: start.clone() });
            let mut __ctx = ReachableWalkerFrameContext::new(alloc::rc::Rc::clone(&__e), None);
            self._context_stack.push(__ctx);
            self.__kernel(&__e);
            let __ctx = self._context_stack.pop().expect("invariant: handler must have pushed a context before reading return");
            match __ctx._return {
                Some(ReachableWalkerFrameReturn::Seed(v)) => v,
                Some(ReachableWalkerFrameReturn::_Lifecycle(v)) => v.downcast_ref::<String>().cloned().unwrap_or_default(),
                _ => Default::default(),
            }
        }

        pub fn enqueue(&mut self, name: String) -> String {
            let __e = alloc::rc::Rc::new(ReachableWalkerFrameEvent::Enqueue { name: name.clone() });
            let mut __ctx = ReachableWalkerFrameContext::new(alloc::rc::Rc::clone(&__e), None);
            self._context_stack.push(__ctx);
            self.__kernel(&__e);
            let __ctx = self._context_stack.pop().expect("invariant: handler must have pushed a context before reading return");
            match __ctx._return {
                Some(ReachableWalkerFrameReturn::Enqueue(v)) => v,
                Some(ReachableWalkerFrameReturn::_Lifecycle(v)) => v.downcast_ref::<String>().cloned().unwrap_or_default(),
                _ => Default::default(),
            }
        }

        pub fn next(&mut self) -> String {
            let __e = alloc::rc::Rc::new(ReachableWalkerFrameEvent::Next {});
            let mut __ctx = ReachableWalkerFrameContext::new(alloc::rc::Rc::clone(&__e), None);
            self._context_stack.push(__ctx);
            self.__kernel(&__e);
            let __ctx = self._context_stack.pop().expect("invariant: handler must have pushed a context before reading return");
            match __ctx._return {
                Some(ReachableWalkerFrameReturn::Next(v)) => v,
                Some(ReachableWalkerFrameReturn::_Lifecycle(v)) => v.downcast_ref::<String>().cloned().unwrap_or_default(),
                _ => Default::default(),
            }
        }

        pub fn unreachable(&mut self, all_csv: String) -> String {
            let __e = alloc::rc::Rc::new(ReachableWalkerFrameEvent::Unreachable { all_csv: all_csv.clone() });
            let mut __ctx = ReachableWalkerFrameContext::new(alloc::rc::Rc::clone(&__e), None);
            self._context_stack.push(__ctx);
            self.__kernel(&__e);
            let __ctx = self._context_stack.pop().expect("invariant: handler must have pushed a context before reading return");
            match __ctx._return {
                Some(ReachableWalkerFrameReturn::Unreachable(v)) => v,
                Some(ReachableWalkerFrameReturn::_Lifecycle(v)) => v.downcast_ref::<String>().cloned().unwrap_or_default(),
                _ => Default::default(),
            }
        }

        fn _state_Initial(&mut self, __e: &ReachableWalkerFrameEvent) {
            match __e {
                ReachableWalkerFrameEvent::Seed { start, .. } => {
                    self._s_Initial_hdl_user_seed(__e, start.clone());
                }
                _ => {}
            }
        }

        fn _state_Walking(&mut self, __e: &ReachableWalkerFrameEvent) {
            match __e {
                ReachableWalkerFrameEvent::Enqueue { name, .. } => {
                    self._s_Walking_hdl_user_enqueue(__e, name.clone());
                }
                ReachableWalkerFrameEvent::Next { .. } => { self._s_Walking_hdl_user_next(__e); }
                ReachableWalkerFrameEvent::Unreachable { all_csv, .. } => {
                    self._s_Walking_hdl_user_unreachable(__e, all_csv.clone());
                }
                _ => {}
            }
        }

        fn _state_Done(&mut self, __e: &ReachableWalkerFrameEvent) {
            match __e {
                ReachableWalkerFrameEvent::Enqueue { name, .. } => {
                    self._s_Done_hdl_user_enqueue(__e, name.clone());
                }
                ReachableWalkerFrameEvent::Next { .. } => { self._s_Done_hdl_user_next(__e); }
                ReachableWalkerFrameEvent::Unreachable { all_csv, .. } => {
                    self._s_Done_hdl_user_unreachable(__e, all_csv.clone());
                }
                _ => {}
            }
        }

        fn _s_Initial_hdl_user_seed(&mut self, __e: &ReachableWalkerFrameEvent, start: String) {
                            self.queue = start.clone();
                            self.visited = start.clone();
                            let mut __compartment = self.__prepareEnter("Walking", vec![]);
                            self.__transition(__compartment);
            let __return_val = ReachableWalkerFrameReturn::Seed("READY".to_string());
                            if let Some(ctx) = self._context_stack.last_mut() { ctx._return = Some(__return_val); }
                            return;
        }

        fn _s_Walking_hdl_user_enqueue(&mut self, __e: &ReachableWalkerFrameEvent, name: String) {
                            if name.is_empty() {
            let __return_val = ReachableWalkerFrameReturn::Enqueue("SKIP".to_string());
                                if let Some(ctx) = self._context_stack.last_mut() { ctx._return = Some(__return_val); }
                                return
                            }
                            let already = self.visited.split(',').any(|v| v == name.as_str());
                            if already {
            let __return_val = ReachableWalkerFrameReturn::Enqueue("SKIP".to_string());
                                if let Some(ctx) = self._context_stack.last_mut() { ctx._return = Some(__return_val); }
                                return
                            }
                            if !self.visited.is_empty() {
                                self.visited.push(',');
                            }
                            self.visited.push_str(&name);
                            if !self.queue.is_empty() {
                                self.queue.push(',');
                            }
                            self.queue.push_str(&name);
            let __return_val = ReachableWalkerFrameReturn::Enqueue("ENQUEUED".to_string());
                            if let Some(ctx) = self._context_stack.last_mut() { ctx._return = Some(__return_val); }
        }

        fn _s_Walking_hdl_user_next(&mut self, __e: &ReachableWalkerFrameEvent) {
                            if self.queue.is_empty() {
                                let mut __compartment = self.__prepareEnter("Done", vec![]);
                                self.__transition(__compartment);
            let __return_val = ReachableWalkerFrameReturn::Next("DONE".to_string());
                                if let Some(ctx) = self._context_stack.last_mut() { ctx._return = Some(__return_val); }
                                return;
            
                            }
                            let head;
                            let rest;
                            if let Some(comma_pos) = self.queue.find(',') {
                                head = self.queue[..comma_pos].to_string();
                                rest = self.queue[comma_pos + 1..].to_string();
                            } else {
                                head = self.queue.clone();
                                rest = String::new();
                            }
                            self.queue = rest;
            let __return_val = ReachableWalkerFrameReturn::Next(head.clone());
                            if let Some(ctx) = self._context_stack.last_mut() { ctx._return = Some(__return_val); }
        }

        fn _s_Walking_hdl_user_unreachable(&mut self, __e: &ReachableWalkerFrameEvent, all_csv: String) {
                            // Available before walk completes — read-only diff.
                            let mut result = String::new();
                            for name in all_csv.split(',') {
                                if name.is_empty() {
                                    continue;
                                }
                                let was_visited = self.visited.split(',').any(|v| v == name);
                                if !was_visited {
                                    if !result.is_empty() {
                                        result.push(',');
                                    }
                                    result.push_str(name);
                                }
                            }
            let __return_val = ReachableWalkerFrameReturn::Unreachable(result.clone());
                            if let Some(ctx) = self._context_stack.last_mut() { ctx._return = Some(__return_val); }
        }

        fn _s_Done_hdl_user_enqueue(&mut self, __e: &ReachableWalkerFrameEvent, name: String) {
                            // Defensive: a caller that loops past DONE shouldn't
                            // crash. Absorb the enqueue; the walk has already
                            // ended.
            let __return_val = ReachableWalkerFrameReturn::Enqueue("SKIP".to_string());
                            if let Some(ctx) = self._context_stack.last_mut() { ctx._return = Some(__return_val); }
        }

        fn _s_Done_hdl_user_next(&mut self, __e: &ReachableWalkerFrameEvent) {
            let __return_val = ReachableWalkerFrameReturn::Next("DONE".to_string());
                            if let Some(ctx) = self._context_stack.last_mut() { ctx._return = Some(__return_val); }
        }

        fn _s_Done_hdl_user_unreachable(&mut self, __e: &ReachableWalkerFrameEvent, all_csv: String) {
                            let mut result = String::new();
                            for name in all_csv.split(',') {
                                if name.is_empty() {
                                    continue;
                                }
                                let was_visited = self.visited.split(',').any(|v| v == name);
                                if !was_visited {
                                    if !result.is_empty() {
                                        result.push(',');
                                    }
                                    result.push_str(name);
                                }
                            }
            let __return_val = ReachableWalkerFrameReturn::Unreachable(result.clone());
                            if let Some(ctx) = self._context_stack.last_mut() { ctx._return = Some(__return_val); }
        }
    }
}
pub use _reachable_walker_framec::*;

