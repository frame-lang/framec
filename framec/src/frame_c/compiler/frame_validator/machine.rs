//! Machine-walk validation: HSM cycle detection, top-level machine walk
//! (state-by-state, handler-by-handler), and reachability from the start
//! state.
//!
//! `validate_machine` is the entry point — it iterates over states and
//! delegates per-state checks to [`super::transitions::FrameValidator::validate_state`].
//! `validate_hsm_cycles` catches E413 (cyclic parent chain); `validate_reachable_states`
//! emits W414 for any state that can't be reached from the start state.

use super::{FrameValidator, ValidationError};
use crate::frame_c::compiler::frame_ast::*;
use std::collections::{HashMap, HashSet};

impl FrameValidator {
    /// E413: Detect circular parent chains in HSM hierarchy.
    ///
    /// RFC-0035 round 5: Frame-implemented in
    /// `compiler/hsm_cycle_validator/`. The FSM walks each
    /// state's parent chain as a 4-state graph walker:
    /// $Initial → $Walking → ($CycleFound | $ChainRoot).
    pub(super) fn validate_hsm_cycles(
        &mut self,
        _system: &SystemAst,
        state_map: &HashMap<String, &StateAst>,
    ) {
        let parents: Vec<(String, Option<String>)> = state_map
            .iter()
            .map(|(name, state)| (name.clone(), state.parent.clone()))
            .collect();
        let cycles = crate::frame_c::compiler::hsm_cycle_validator::validate_hsm_cycles(&parents);
        for (state_name, cycle_at) in cycles {
            // Every cycle reported by the FSM has state_name in
            // state_map by construction (we built `parents` from
            // state_map.iter()).
            let span = state_map[&state_name].span.clone();
            self.errors.push(
                ValidationError::new(
                    "E413",
                    format!(
                        "HSM cycle detected: state '{}' has circular parent chain through '{}'",
                        state_name, cycle_at
                    ),
                )
                .with_span(span),
            );
        }
    }

    /// Build a set of action names
    pub(super) fn build_action_set(&self, system: &SystemAst) -> HashSet<String> {
        system.actions.iter().map(|a| a.name.clone()).collect()
    }

    /// Build a set of operation names
    pub(super) fn build_operation_set(&self, system: &SystemAst) -> HashSet<String> {
        system.operations.iter().map(|o| o.name.clone()).collect()
    }

    /// Validate a machine
    pub(super) fn validate_machine(
        &mut self,
        machine: &MachineAst,
        state_map: &HashMap<String, &StateAst>,
        interface_methods: &HashMap<String, &InterfaceMethod>,
        _actions: &HashSet<String>,
        _operations: &HashSet<String>,
        system_name: &str,
    ) {
        for state in &machine.states {
            self.validate_state(state, state_map, interface_methods, _actions, _operations);
        }
        self.validate_reachable_states(system_name, machine, state_map);
    }

    /// W414: warn when a state is not reachable from the start state via
    /// any direct `-> $State` transition in any handler / enter / exit
    /// body. BFS from machine.states[0] (Frame's start-state convention)
    /// over Transition statements only — `pop$` returns are treated as
    /// non-transitions (the destination is wherever the runtime stack
    /// last held, not a static target). HSM parents of reachable states
    /// are also considered reachable: the runtime visits a parent on
    /// every enter/exit cascade through its child even though no direct
    /// `-> $Parent` transition exists. States only reached through
    /// stack pop/push from outside the BFS frontier are best-effort
    /// flagged; the warning is advisory, not a build error.
    pub(super) fn validate_reachable_states(
        &mut self,
        system_name: &str,
        machine: &MachineAst,
        state_map: &HashMap<String, &StateAst>,
    ) {
        if machine.states.is_empty() {
            return;
        }
        let start_state = &machine.states[0].name;

        // Build the edge map: for each state, the union of its
        // transition targets (from handlers + enter + exit) and
        // its full ancestor chain. The reachable_validator FSM
        // owns the BFS; we own the AST shape that produces edges.
        // `pop$` targets are filtered here — they're dynamic
        // (resolved at runtime by the stack), not statically
        // reachable.
        let mut edges: HashMap<String, Vec<String>> = HashMap::new();
        let collect_body = |body: &HandlerBody, out: &mut Vec<String>| {
            for stmt in &body.statements {
                match stmt {
                    Statement::Transition(trans) if trans.target != "pop$" => {
                        out.push(trans.target.clone());
                    }
                    // `push$ -> $State` reaches $State just like a normal
                    // transition (the push only saves the current compartment).
                    Statement::StackPush(push) => {
                        if let Some(target) = &push.transition_target {
                            out.push(target.clone());
                        }
                    }
                    _ => {}
                }
            }
        };
        for state in &machine.states {
            let mut neighbors: Vec<String> = Vec::new();
            for handler in &state.handlers {
                collect_body(&handler.body, &mut neighbors);
            }
            if let Some(enter) = &state.enter {
                collect_body(&enter.body, &mut neighbors);
            }
            if let Some(exit) = &state.exit {
                collect_body(&exit.body, &mut neighbors);
            }
            // HSM: every ancestor is reachable via enter/exit cascade.
            // A cyclic parent chain (caught separately by E413) would loop
            // forever here — terminate when we revisit a name we've
            // already added for this state.
            let mut local_visited: HashSet<String> = HashSet::new();
            local_visited.insert(state.name.clone());
            let mut ancestor = state.parent.clone();
            while let Some(parent_name) = ancestor {
                if !local_visited.insert(parent_name.clone()) {
                    break;
                }
                neighbors.push(parent_name.clone());
                ancestor = state_map.get(&parent_name).and_then(|s| s.parent.clone());
            }
            edges.insert(state.name.clone(), neighbors);
        }

        let all_states: Vec<String> = machine.states.iter().map(|s| s.name.clone()).collect();
        let unreachable = crate::frame_c::compiler::reachable_validator::validate_reachable_states(
            start_state,
            &edges,
            &all_states,
        );

        for name in unreachable {
            if let Some(state) = state_map.get(&name) {
                self.warnings.push(
                    ValidationError::new(
                        "W414",
                        format!(
                            "State '{}' is not reachable from start state '{}' in system '{}'",
                            name, start_state, system_name
                        ),
                    )
                    .with_span(state.span.clone()),
                );
            }
        }
    }
}
