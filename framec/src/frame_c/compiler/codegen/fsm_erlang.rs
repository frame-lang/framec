//! Erlang backend for `@@fsm` (RFC-0042, Phase 8).
//!
//! Generates a self-contained Erlang module from a validated `FsmDeclAst`,
//! mirroring the Python reference backend ([`super::fsm_python`]) and the
//! Rust backend ([`super::fsm_rust`]) — but *functionally*. Erlang has no
//! mutation, so the recognizer state is a map threaded through the state
//! functions: each `state_<n>(St, Enter)` returns `{NextStateIndex, St2}`,
//! and `run/2` loops until the index is negative. The input is converted to
//! a tuple once (`list_to_tuple/1`) for O(1) indexed access by the cursor.
//!
//! The public entry point is `<module>:recognize/<arity>` (arity = the fsm's
//! parameter count); it returns the final state map, whose `accepted`,
//! `return_value`, `cursor`, and `reject_position` keys are the observable
//! result (§5.1).
//!
//! # v0.1 scope (first cut)
//!
//! Supports single-match states, plain regex stages with `.label` captures,
//! bare-expression returns, action blocks (`self.X = …` assignment /
//! `if`-`else`), declared `actions:` (functional `act_<name>(St, …) ->
//! {RetVal, St2}` helpers; calls are hoisted out of expression position so
//! the state map threads), and static + failure transitions over the
//! `bytes`/`char` alphabets, with the `@@:matched` / `to_int` / `to_str` /
//! `len` built-ins, conditional (`when`) and stage-ref (`-> $S.stage`, via a
//! `fsm_enter` re-entry index a per-state `case Enter of` honors) transition
//! targets, multi-match (`|`) ordered-choice states (commit-on-first-stage,
//! stageless catch-all), and embedding actions (`>{}`/`@{}`/`${}`/`%{}`/
//! `@eof{}`, §3.5.5/§5.4 — a specialized `match_stage_<sid>(St) -> {Last,
//! St2}` that threads the mutated state map through the scan). Alphabets:
//! `bytes`/`char` (a code-point list → tuple) and `token` (an atom list →
//! tuple; token kinds map to small integer ids via a generated `tok_id/1`).
//! Mode C sub-fsm call-out (`/@Inner/`, §8.3) constructs the inner module
//! (`inner:recognize/1`) over the input at the cursor, advancing by what it
//! consumed and exposing it via `$state.label.return_value`. Not yet handled
//! (clear `Unsupported` error, never a silent miscompile): anchors (deferred
//! to the next increment).

use crate::frame_c::compiler::frame_ast::{
    BinaryOp, EmbeddingOp, Expression, FsmDeclAst, FsmStateAst, FsmTransitionTarget, Literal,
    MatchAst, MatchElement, StageAst, Statement, Type, UnaryOp,
};
use crate::frame_c::compiler::fsm_regex::{
    self, size_check::DEFAULT_MAX_DFA_STATES, subset::DfaLabel, Alphabet, CompileError,
};
use std::fmt::Write;

/// Generate Erlang source implementing `decl`, or a reason it is outside the
/// v0.1 Erlang cut.
pub fn generate(decl: &FsmDeclAst) -> Result<String, String> {
    Generator::new(decl)?.emit()
}

/// One stage's compiled DFA, flattened for emission.
struct StageDfa {
    states: Vec<(Vec<(u32, u32, usize)>, bool)>,
    start: usize,
    /// RFC-0042 §8.3 Mode C: when `Some(name)`, this stage is a call-out to
    /// the `@@fsm` `name` rather than a regex DFA match (no DFA; a
    /// placeholder keeps stage indices aligned with the emit walk).
    mode_c: Option<String>,
}

/// A Mode C stage's regex body starts with `@`; the rest names the inner
/// fsm (`/@Digit/` → `Some("Digit")`).
fn mode_c_inner(regex: &str) -> Option<&str> {
    regex.strip_prefix('@')
}

struct Generator<'a> {
    decl: &'a FsmDeclAst,
    alphabet: Alphabet,
    label_to_index: std::collections::HashMap<String, usize>,
    /// `(state index, alternative index, element index)` → stage-DFA index.
    /// Precomputed so a stage's DFA reference is fixed by position, letting a
    /// state body be emitted from multiple `_Enter` entry points (and each
    /// `|` alternative) without a running counter drifting out of sync with
    /// `stage_dfas`.
    stage_sid: std::collections::HashMap<(usize, usize, usize), usize>,
    /// `(state label, stage label)` → element index, for stage-ref re-entry
    /// (`-> $State.stage`).
    stage_entry: std::collections::HashMap<(String, String), usize>,
    /// Token-alphabet only: each token-kind name → a small integer id, so
    /// token transitions reuse the same numeric range matcher (the
    /// per-element read maps a token atom to its id; unknown → -1).
    token_ids: std::collections::HashMap<String, u32>,
    stage_dfas: Vec<StageDfa>,
}

impl<'a> Generator<'a> {
    fn new(decl: &'a FsmDeclAst) -> Result<Self, String> {
        let alphabet = match decl.params.first().map(|p| &p.param_type) {
            Some(Type::Custom(t)) if t == "char" => Alphabet::Char,
            Some(Type::Custom(t)) if t == "token" => Alphabet::Token,
            _ => Alphabet::Bytes,
        };
        let mut label_to_index = std::collections::HashMap::new();
        let mut stage_entry = std::collections::HashMap::new();
        for (i, st) in decl.states.iter().enumerate() {
            if let Some(l) = &st.label {
                label_to_index.insert(l.clone(), i);
                // Stage-ref re-entry is registered only for single-match
                // states; selection in a multi-match state is by first stage.
                if st.matches.len() == 1 {
                    for (ei, el) in st.matches[0].elements.iter().enumerate() {
                        if let MatchElement::Stage(stage) = el {
                            if let Some(sl) = &stage.label {
                                stage_entry.insert((l.clone(), sl.clone()), ei);
                            }
                        }
                    }
                }
            }
        }
        let mut g = Generator {
            decl,
            alphabet,
            label_to_index,
            stage_sid: std::collections::HashMap::new(),
            stage_entry,
            token_ids: std::collections::HashMap::new(),
            stage_dfas: Vec::new(),
        };
        g.compile_stage_dfas()?;
        Ok(g)
    }

    /// Compile every stage's DFA in traversal order, recording each stage
    /// element's `(state index, element index) → dfa index` so the emitted
    /// `dfa_<n>` helpers line up with the references regardless of how many
    /// `_Enter` entry points re-emit a state body.
    fn compile_stage_dfas(&mut self) -> Result<(), String> {
        // `token_ids` is taken out so `compile_one` can be a borrow-free
        // associated function (it both reads the regex and grows the map).
        let mut token_ids = std::mem::take(&mut self.token_ids);
        let mut sid = 0usize;
        for (si, st) in self.decl.states.iter().enumerate() {
            for (ai, m) in st.matches.iter().enumerate() {
                for (ei, el) in m.elements.iter().enumerate() {
                    if let MatchElement::Stage(stage) = el {
                        if let Some(inner) = mode_c_inner(&stage.regex) {
                            // Mode C: a sub-fsm call-out, no DFA. Push a
                            // placeholder to keep stage indices aligned.
                            self.stage_dfas.push(StageDfa {
                                states: Vec::new(),
                                start: 0,
                                mode_c: Some(inner.to_string()),
                            });
                            self.stage_sid.insert((si, ai, ei), sid);
                            sid += 1;
                            continue;
                        }
                        match Self::compile_one(self.alphabet, &stage.regex, &mut token_ids) {
                            Ok(dfa) => self.stage_dfas.push(dfa),
                            Err(e) => {
                                self.token_ids = token_ids;
                                return Err(e);
                            }
                        }
                        self.stage_sid.insert((si, ai, ei), sid);
                        sid += 1;
                    }
                }
            }
        }
        self.token_ids = token_ids;
        Ok(())
    }

    fn compile_one(
        alphabet: Alphabet,
        regex: &str,
        token_ids: &mut std::collections::HashMap<String, u32>,
    ) -> Result<StageDfa, String> {
        match fsm_regex::compile(regex, alphabet, DEFAULT_MAX_DFA_STATES) {
            Ok(compiled) => {
                if compiled.requires_start || compiled.requires_end {
                    return Err(
                        "anchors are not yet supported by the Erlang backend (v0.1 first cut)"
                            .into(),
                    );
                }
                let mut states = Vec::with_capacity(compiled.dfa.states.len());
                for s in &compiled.dfa.states {
                    let mut trans = Vec::new();
                    for t in &s.transitions {
                        let (lo, hi) = match &t.label {
                            DfaLabel::Byte(b) => (*b as u32, *b as u32),
                            DfaLabel::ByteRange { low, high } => (*low as u32, *high as u32),
                            DfaLabel::CodePoint(c) => (*c as u32, *c as u32),
                            DfaLabel::CodePointRange { low, high } => (*low as u32, *high as u32),
                            DfaLabel::Token(name) => {
                                // Map the token kind to a small integer id so
                                // it reuses the numeric range matcher.
                                let next = token_ids.len() as u32;
                                let id = *token_ids.entry(name.clone()).or_insert(next);
                                (id, id)
                            }
                        };
                        trans.push((lo, hi, t.to));
                    }
                    states.push((trans, s.is_accept));
                }
                Ok(StageDfa {
                    states,
                    start: compiled.dfa.start,
                    mode_c: None,
                })
            }
            Err(CompileError::Diagnostics(ds)) => Err(format!(
                "regex `/{}/` failed to compile: {}",
                regex,
                ds.first().map(|d| d.message.as_str()).unwrap_or("")
            )),
            Err(CompileError::UnsupportedAnchors(_)) => Err(format!(
                "regex `/{}/` uses anchors, not yet supported by the Erlang backend",
                regex
            )),
        }
    }

    /// The Erlang module name — the fsm name lowercased (Erlang module atoms
    /// are conventionally lowercase, and must match the on-disk filename).
    fn module_name(&self) -> String {
        self.decl.name.to_lowercase()
    }

    fn emit(&self) -> Result<String, String> {
        let mut out = String::new();
        let arity = self.decl.params.len();
        writeln!(out, "-module({}).", self.module_name()).ok();
        writeln!(out, "-export([recognize/{}]).", arity).ok();
        out.push('\n');
        self.emit_recognize(&mut out);
        self.emit_run(&mut out);
        self.emit_state_dispatch(&mut out);
        self.emit_state_functions(&mut out)?;
        self.emit_embed_matchers(&mut out)?;
        self.emit_action_functions(&mut out)?;
        self.emit_tok_id(&mut out);
        self.emit_dfa_helpers(&mut out);
        self.emit_dfa_runtime(&mut out);
        Ok(out)
    }

    /// The per-element read as an integer: a byte/char by code point, a token
    /// by its small integer id (`-1` for an unknown token, matching no
    /// range). This is the only point where the scan differs across alphabets.
    fn element_read_expr(&self) -> &'static str {
        match self.alphabet {
            Alphabet::Token => "tok_id(element(Pos + 1, Input))",
            _ => "element(Pos + 1, Input)",
        }
    }

    /// Token alphabet: emit the `tok_id/1` atom → id lookup (unknown → -1).
    /// Emitted only when a stage matcher exists to call it.
    fn emit_tok_id(&self, out: &mut String) {
        if self.alphabet != Alphabet::Token || self.stage_dfas.is_empty() {
            return;
        }
        let mut entries: Vec<(&String, &u32)> = self.token_ids.iter().collect();
        entries.sort_by_key(|(_, id)| **id);
        for (name, id) in entries {
            writeln!(out, "tok_id({}) -> {};", erl_atom(name), id).ok();
        }
        out.push_str("tok_id(_) -> -1.\n\n");
    }

    /// Emit a specialized matcher `match_stage_<sid>(St) -> {Last, St2}` for
    /// each stage that carries embedding actions (§3.5.5 / §5.4). Same greedy
    /// longest-match scan as `dfa_match`, but firing the embedding actions at
    /// their DFA positions and threading the (mutated) state map: `>{}` at
    /// scan start, `${}` per consumed element, `@{}` on entering an accepting
    /// state, `%{}` on leaving one, `@eof{}` at end-of-input while mid-match.
    /// `@@:cursor` reflects the scan position during firing; it is restored to
    /// the stage-entry position before returning.
    fn emit_embed_matchers(&self, out: &mut String) -> Result<(), String> {
        let mut sid = 0usize;
        for st in &self.decl.states {
            for m in &st.matches {
                for el in &m.elements {
                    if let MatchElement::Stage(stage) = el {
                        if !stage.embedding_actions.is_empty() {
                            self.emit_one_matcher(out, sid, stage)?;
                        }
                        sid += 1;
                    }
                }
            }
        }
        Ok(())
    }

    /// Statements of `stage`'s embedding actions with op `op`, concatenated.
    fn embed_stmts(&self, stage: &StageAst, op: EmbeddingOp) -> Vec<Statement> {
        let mut out = Vec::new();
        for ea in &stage.embedding_actions {
            if ea.op == op {
                out.extend(ea.body.statements.iter().cloned());
            }
        }
        out
    }

    fn emit_one_matcher(
        &self,
        out: &mut String,
        sid: usize,
        stage: &StageAst,
    ) -> Result<(), String> {
        let mut ctr = 0usize;
        writeln!(out, "match_stage_{}(St) ->", sid).ok();
        writeln!(out, "    {{States, Start}} = dfa_{}(),", sid).ok();
        out.push_str("    Entry = maps:get(cursor, St),\n");
        out.push_str("    Input = maps:get(fsm_input, St),\n");
        out.push_str("    N = maps:get(fsm_n, St),\n");
        out.push_str("    {_, A0} = element(Start + 1, States),\n");
        out.push_str("    Last0 = case A0 of true -> Entry; false -> -1 end,\n");
        out.push_str("    StA = St#{cursor => Entry},\n");
        // `>{}` — begin matching.
        let start_b = self.emit_block(
            out,
            &self.embed_stmts(stage, EmbeddingOp::Start),
            "StA",
            "    ",
            &mut ctr,
        )?;
        // The final position/accepting flag are only needed for `@eof{}`;
        // bind them to `_` otherwise so erlc does not warn.
        let eof = self.embed_stmts(stage, EmbeddingOp::Eof);
        let (pf, prf) = if eof.is_empty() {
            ("_", "_")
        } else {
            ("PosF", "PrevF")
        };
        writeln!(
            out,
            "    {{Last, {}, {}, StZ}} = embed_loop_{}(Input, N, States, Start, Entry, Last0, A0, {}),",
            pf, prf, sid, start_b
        )
        .ok();
        // `@eof{}` — end of input reached while mid-match (non-accepting).
        let final_st = if eof.is_empty() {
            "StZ".to_string()
        } else {
            let v = fresh("St", &mut ctr);
            writeln!(out, "    {} = case (PosF >= N) andalso (not PrevF) of", v).ok();
            out.push_str("        true ->\n");
            let b = self.emit_block(out, &eof, "StZ", "            ", &mut ctr)?;
            writeln!(out, "            {};", b).ok();
            out.push_str("        false -> StZ\n");
            out.push_str("    end,\n");
            v
        };
        writeln!(out, "    {{Last, {}#{{cursor => Entry}}}}.", final_st).ok();
        out.push('\n');

        // The per-step loop.
        writeln!(
            out,
            "embed_loop_{}(Input, N, States, S, Pos, Last, Prev, St) ->",
            sid
        )
        .ok();
        out.push_str("    case Pos < N of\n");
        out.push_str("        false -> {Last, Pos, Prev, St};\n");
        out.push_str("        true ->\n");
        writeln!(out, "            V = {},", self.element_read_expr()).ok();
        out.push_str("            {Trans, _} = element(S + 1, States),\n");
        out.push_str("            case dfa_find(Trans, V) of\n");
        out.push_str("                none -> {Last, Pos, Prev, St};\n");
        out.push_str("                {ok, Tgt} ->\n");
        out.push_str("                    Pos2 = Pos + 1,\n");
        out.push_str("                    StC = St#{cursor => Pos2},\n");
        let ind = "                    ";
        // `${}` — every consumed element.
        let after_every = self.emit_block(
            out,
            &self.embed_stmts(stage, EmbeddingOp::EveryTransition),
            "StC",
            ind,
            &mut ctr,
        )?;
        writeln!(out, "{}{{_, Now}} = element(Tgt + 1, States),", ind).ok();
        // `@{}` — a transition into an accepting state.
        let accept = self.embed_stmts(stage, EmbeddingOp::Accept);
        let after_accept = if accept.is_empty() {
            after_every.clone()
        } else {
            self.emit_gated_body(out, "Now", &accept, &after_every, ind, &mut ctr)?
        };
        // `%{}` — left an accepting state.
        let leave = self.embed_stmts(stage, EmbeddingOp::LeaveAccept);
        let after_leave = if leave.is_empty() {
            after_accept.clone()
        } else {
            self.emit_gated_body(
                out,
                "Prev andalso (not Now)",
                &leave,
                &after_accept,
                ind,
                &mut ctr,
            )?
        };
        writeln!(
            out,
            "{}Last2 = case Now of true -> Pos2; false -> Last end,",
            ind
        )
        .ok();
        writeln!(
            out,
            "{}embed_loop_{}(Input, N, States, Tgt, Pos2, Last2, Now, {})",
            ind, sid, after_leave
        )
        .ok();
        out.push_str("            end\n    end.\n\n");
        Ok(())
    }

    /// Emit `V = case <cond> of true -> <body threaded from st_in> Final;
    /// false -> st_in end,` and return the fresh `V`. Used for the gated
    /// embedding actions (`@{}`/`%{}`).
    fn emit_gated_body(
        &self,
        out: &mut String,
        cond: &str,
        stmts: &[Statement],
        st_in: &str,
        ind: &str,
        ctr: &mut usize,
    ) -> Result<String, String> {
        let v = fresh("St", ctr);
        let inner = format!("{}        ", ind);
        writeln!(out, "{}{} = case {} of", ind, v, cond).ok();
        writeln!(out, "{}    true ->", ind).ok();
        let b = self.emit_block(out, stmts, st_in, &inner, ctr)?;
        writeln!(out, "{}        {};", ind, b).ok();
        writeln!(out, "{}    false -> {}", ind, st_in).ok();
        writeln!(out, "{}end,", ind).ok();
        Ok(v)
    }

    /// Is `name` a declared `actions:` helper? Declared-action calls thread
    /// the state map (`{Value, St2}`), so they are hoisted out of expression
    /// position rather than emitted inline.
    fn is_action(&self, name: &str) -> bool {
        self.decl
            .actions
            .as_ref()
            .map(|b| b.actions.iter().any(|a| a.name == name))
            .unwrap_or(false)
    }

    /// If `e` is a call to a declared action, its `(name, args)`.
    fn as_action_call<'e>(&self, e: &'e Expression) -> Option<(&'e str, &'e [Expression])> {
        if let Expression::Call { func, args } = e {
            if self.is_action(func) {
                return Some((func.as_str(), args));
            }
        }
        None
    }

    /// Emit `{Value, St2} = act_<name>(St, Args...),` and return the fresh
    /// `(value var, threaded-state var)`. Declared actions both read/mutate
    /// the state map and yield a value, so the call threads `St`.
    fn emit_action_call(
        &self,
        out: &mut String,
        name: &str,
        args: &[Expression],
        st: &str,
        ind: &str,
        ctr: &mut usize,
    ) -> (String, String) {
        let v = fresh("V", ctr);
        let st2 = fresh("St", ctr);
        let a: Vec<String> = args.iter().map(|e| self.expr(e, st)).collect();
        let arglist = if a.is_empty() {
            String::new()
        } else {
            format!(", {}", a.join(", "))
        };
        writeln!(
            out,
            "{}{{{}, {}}} = act_{}({}{}),",
            ind, v, st2, name, st, arglist
        )
        .ok();
        (v, st2)
    }

    /// Emit each declared `actions:` helper as `act_<name>(St, Params...) ->
    /// {RetVal, St2}.` The body threads the state map; a trailing bare
    /// expression (when the action has a return type) is the `RetVal`, else
    /// `RetVal` is the atom `ok`.
    fn emit_action_functions(&self, out: &mut String) -> Result<(), String> {
        let Some(block) = &self.decl.actions else {
            return Ok(());
        };
        for act in &block.actions {
            let mut sig = String::from("St");
            for p in &act.params {
                write!(sig, ", {}", erl_var(&p.name)).ok();
            }
            writeln!(out, "act_{}({}) ->", act.name, sig).ok();
            let mut ctr = 0usize;
            let stmts = &act.body.statements;
            let has_ret = act.return_type.is_some();
            // A trailing bare (non-assignment) expression is the return value.
            let (init, tail) = match (has_ret, stmts.last()) {
                (true, Some(crate::frame_c::compiler::frame_ast::Statement::Expression(e)))
                    if !matches!(e.expr, Expression::Assign { .. }) =>
                {
                    (&stmts[..stmts.len() - 1], Some(&e.expr))
                }
                _ => (&stmts[..], None),
            };
            let final_st = self.emit_block(out, init, "St", "    ", &mut ctr)?;
            match tail {
                Some(e) => {
                    // The tail value may itself be a declared-action call.
                    if let Some((name, args)) = self.as_action_call(e) {
                        let (v, st2) =
                            self.emit_action_call(out, name, args, &final_st, "    ", &mut ctr);
                        writeln!(out, "    {{{}, {}}}.", v, st2).ok();
                    } else {
                        writeln!(out, "    {{{}, {}}}.", self.expr(e, &final_st), final_st).ok();
                    }
                }
                None => {
                    writeln!(out, "    {{ok, {}}}.", final_st).ok();
                }
            }
            out.push('\n');
        }
        Ok(())
    }

    /// `recognize/<arity>`: build the initial state map, run the dispatch
    /// loop, and normalize `reject_position` to 0 on accept.
    fn emit_recognize(&self, out: &mut String) {
        let input = &self.decl.params[0].name;
        let params: Vec<String> = self.decl.params.iter().map(|p| erl_var(&p.name)).collect();
        writeln!(out, "recognize({}) ->", params.join(", ")).ok();
        writeln!(out, "    Input = list_to_tuple({}),", erl_var(input)).ok();
        out.push_str("    St0 = #{\n");
        // Internal scratch keys are `fsm_`-prefixed so they never collide
        // with a user domain field (e.g. a field literally named `n`).
        out.push_str("        fsm_input => Input,\n");
        writeln!(out, "        fsm_n => tuple_size(Input),").ok();
        out.push_str("        cursor => 0,\n");
        out.push_str("        accepted => false,\n");
        out.push_str("        reject_position => 0,\n");
        writeln!(
            out,
            "        return_value => {},",
            erl_default(&self.decl.return_type, &self.decl.default_expr)
        )
        .ok();
        writeln!(out, "        fsm_matched => {},", self.matched_empty()).ok();
        out.push_str("        fsm_enter => 0");
        // Auto-promoted params become state keys.
        for p in &self.decl.params {
            write!(
                out,
                ",\n        {} => {}",
                erl_key(&p.name),
                erl_var(&p.name)
            )
            .ok();
        }
        // Explicit domain fields (skip one re-declaring the input param).
        if let Some(domain) = &self.decl.domain {
            for v in &domain.vars {
                if self.decl.params.first().map(|p| &p.name) == Some(&v.name) {
                    continue;
                }
                write!(
                    out,
                    ",\n        {} => {}",
                    erl_key(&v.name),
                    self.expr(&v.default, "St0")
                )
                .ok();
            }
        }
        out.push_str("\n    },\n");
        out.push_str("    St1 = run(St0, 0),\n");
        out.push_str("    case maps:get(accepted, St1) of\n");
        out.push_str("        true -> St1#{reject_position => 0};\n");
        out.push_str("        false -> St1\n");
        out.push_str("    end.\n\n");
    }

    fn emit_run(&self, out: &mut String) {
        out.push_str("run(St, State) when State < 0 -> St;\n");
        out.push_str("run(St, State) ->\n");
        out.push_str("    Enter = maps:get(fsm_enter, St),\n");
        out.push_str("    {Next, St2} = state_dispatch(State, St#{fsm_enter => 0}, Enter),\n");
        out.push_str("    run(St2, Next).\n\n");
    }

    fn emit_state_dispatch(&self, out: &mut String) {
        for i in 0..self.decl.states.len() {
            writeln!(
                out,
                "state_dispatch({}, St, Enter) -> state_{}(St, Enter);",
                i, i
            )
            .ok();
        }
        out.push_str("state_dispatch(_, St, _) -> {-1, St}.\n\n");
    }

    fn emit_state_functions(&self, out: &mut String) -> Result<(), String> {
        for (i, st) in self.decl.states.iter().enumerate() {
            match st.matches.len() {
                0 => {
                    writeln!(out, "state_{}(St, _Enter) -> {{-1, St}}.\n", i).ok();
                }
                1 => self.emit_one_state(out, i, st, &st.matches[0])?,
                _ => self.emit_multi_match(out, i, st)?,
            }
        }
        Ok(())
    }

    /// Emit a multi-match (`|`) state as ordered choice (RFC-0042 §3.4): each
    /// alternative's first stage is tried at the state-entry cursor; the
    /// first that matches commits and runs to its transition (a committed
    /// alternative's later-stage failure follows its own failure branch). A
    /// first-stage miss falls through to the next alternative; a stageless
    /// alternative is an unconditional catch-all. If none matches, the input
    /// is not in the language (§5.6). `_Enter` re-entry is not applied here.
    fn emit_multi_match(
        &self,
        out: &mut String,
        index: usize,
        st: &FsmStateAst,
    ) -> Result<(), String> {
        let state_label = st.label.clone().unwrap_or_default();
        writeln!(out, "state_{}(St, _Enter) ->", index).ok();
        let mut ctr = 0usize;
        self.emit_alt(out, st, index, 0, "St", &state_label, "    ", &mut ctr)?;
        out.push_str(".\n\n");
        Ok(())
    }

    /// Recursively emit one `|` alternative and the fall-through chain. The
    /// alternative's first stage is the selector: on match it commits and
    /// runs the alternative body; on miss it falls through to the next
    /// alternative (cursor unchanged). A stageless alternative is an
    /// unconditional catch-all.
    #[allow(clippy::too_many_arguments)]
    fn emit_alt(
        &self,
        out: &mut String,
        st: &FsmStateAst,
        index: usize,
        ai: usize,
        sv: &str,
        state_label: &str,
        ind: &str,
        ctr: &mut usize,
    ) -> Result<(), String> {
        if ai == st.matches.len() {
            // No alternative matched: not in the language (§5.6).
            writeln!(
                out,
                "{ind}{sv}r = {sv}#{{accepted => false, reject_position => maps:get(cursor, {sv})}},",
                ind = ind,
                sv = sv
            )
            .ok();
            write!(out, "{}{{-1, {}r}}", ind, sv).ok();
            return Ok(());
        }
        let m = &st.matches[ai];
        let fs = m
            .elements
            .iter()
            .position(|e| matches!(e, MatchElement::Stage(_)));
        match fs {
            Some(fs) => {
                if fs > 0 {
                    return Err(
                        "a `|` alternative with elements before its first stage is not yet \
                         supported by the Erlang backend"
                            .into(),
                    );
                }
                let MatchElement::Stage(sel) = &m.elements[fs] else {
                    unreachable!("first_stage indexes a Stage element")
                };
                if mode_c_inner(&sel.regex).is_some() {
                    return Err(
                        "a Mode C (`/@Fsm/`) stage as a `|` alternative selector is not yet \
                         supported by the Erlang backend"
                            .into(),
                    );
                }
                let my_sid = self.stage_sid[&(index, ai, fs)];
                let r = fresh("R", ctr);
                // An embedding selector threads its (mutated) state map.
                let sbase = if sel.embedding_actions.is_empty() {
                    writeln!(out, "{}{} = dfa_match({}, dfa_{}()),", ind, r, sv, my_sid).ok();
                    sv.to_string()
                } else {
                    let se = fresh("St", ctr);
                    writeln!(
                        out,
                        "{}{{{}, {}}} = match_stage_{}({}),",
                        ind, r, se, my_sid, sv
                    )
                    .ok();
                    se
                };
                writeln!(out, "{}case {} >= 0 of", ind, r).ok();
                let ind4 = format!("{}    ", ind);
                let ind8 = format!("{}        ", ind);
                writeln!(out, "{}true ->", ind4).ok();
                // Commit the selector: matched slice + capture + advance.
                let mtch = fresh("M", ctr);
                writeln!(
                    out,
                    "{}{} = lists:sublist(maps:get({}, {}), maps:get(cursor, {}) + 1, {} - maps:get(cursor, {})),",
                    ind8, mtch, erl_key(&self.decl.params[0].name), sbase, sbase, r, sbase
                )
                .ok();
                let cap = match &sel.label {
                    Some(lbl) if !state_label.is_empty() => {
                        format!(", {} => {}", erl_key(&cap_key(state_label, lbl)), mtch)
                    }
                    _ => String::new(),
                };
                let committed = fresh("St", ctr);
                writeln!(
                    out,
                    "{}{} = {}#{{fsm_matched => {}, cursor => {}, accepted => true{}}},",
                    ind8, committed, sbase, mtch, r, cap
                )
                .ok();
                // Remaining elements + this alternative's success transition.
                self.emit_seq(
                    out,
                    &m.elements,
                    fs + 1,
                    &committed,
                    m,
                    index,
                    state_label,
                    &ind8,
                    ctr,
                    ai,
                )?;
                out.push_str(";\n");
                writeln!(out, "{}false ->", ind4).ok();
                // A selector miss falls through to the next alternative; an
                // embedding selector's mutations (cursor restored) carry over.
                self.emit_alt(out, st, index, ai + 1, &sbase, state_label, &ind8, ctr)?;
                writeln!(out).ok();
                write!(out, "{}end", ind).ok();
            }
            None => {
                // Stageless catch-all: matches unconditionally.
                let committed = fresh("St", ctr);
                writeln!(out, "{}{} = {}#{{accepted => true}},", ind, committed, sv).ok();
                self.emit_seq(
                    out,
                    &m.elements,
                    0,
                    &committed,
                    m,
                    index,
                    state_label,
                    ind,
                    ctr,
                    ai,
                )?;
            }
        }
        Ok(())
    }

    fn emit_one_state(
        &self,
        out: &mut String,
        index: usize,
        st: &FsmStateAst,
        m: &MatchAst,
    ) -> Result<(), String> {
        let state_label = st.label.clone().unwrap_or_default();
        // Element indices > 0 that are labeled stages — the possible
        // stage-ref (`-> $State.stage`) re-entry points into this state. When
        // present, the function dispatches on `Enter`; otherwise it always
        // enters at element 0 (and the `Enter` param is unused).
        let entries: Vec<usize> = m
            .elements
            .iter()
            .enumerate()
            .filter(|(i, el)| *i > 0 && matches!(el, MatchElement::Stage(s) if s.label.is_some()))
            .map(|(i, _)| i)
            .collect();
        // The parameter is `St`; fresh threaded maps are `St0`, `St1`, … so a
        // fresh name never collides with (and rebinds) the parameter.
        if entries.is_empty() {
            writeln!(out, "state_{}(St, _Enter) ->", index).ok();
            let mut ctr = 0usize;
            self.emit_seq(
                out,
                &m.elements,
                0,
                "St",
                m,
                index,
                &state_label,
                "    ",
                &mut ctr,
                0,
            )?;
            out.push_str(".\n\n");
            return Ok(());
        }
        let mut ctr = 0usize;
        writeln!(out, "state_{}(St, Enter) ->", index).ok();
        out.push_str("    case Enter of\n");
        for entry in &entries {
            writeln!(out, "        {} ->", entry).ok();
            self.emit_seq(
                out,
                &m.elements,
                *entry,
                "St",
                m,
                index,
                &state_label,
                "            ",
                &mut ctr,
                0,
            )?;
            out.push_str(";\n");
        }
        // Re-entry at element 0 (or any other index) enters at the top.
        out.push_str("        _ ->\n");
        self.emit_seq(
            out,
            &m.elements,
            0,
            "St",
            m,
            index,
            &state_label,
            "            ",
            &mut ctr,
            0,
        )?;
        out.push_str("\n    end.\n\n");
        Ok(())
    }

    /// Emit the element sequence as a chain of Erlang expressions. A stage
    /// wraps the remaining elements inside the `false ->` branch of its
    /// match-result `case` (a stage miss short-circuits to the failure
    /// branch); a bare expression threads a new state map. When the elements
    /// are exhausted, the success transition is the tail expression.
    #[allow(clippy::too_many_arguments)]
    fn emit_seq(
        &self,
        out: &mut String,
        elements: &[MatchElement],
        idx: usize,
        st: &str,
        m: &MatchAst,
        state_idx: usize,
        state_label: &str,
        ind: &str,
        ctr: &mut usize,
        ai: usize,
    ) -> Result<(), String> {
        if idx == elements.len() {
            self.emit_success(out, m, st, ind);
            return Ok(());
        }
        match &elements[idx] {
            MatchElement::Stage(stage) if mode_c_inner(&stage.regex).is_some() => {
                // Mode C (§8.3): construct the inner fsm over the input at the
                // cursor; on accept, advance by what it consumed (its
                // `cursor`); on reject, follow the failure branch (§5.6).
                let inner = mode_c_inner(&stage.regex).unwrap().to_lowercase();
                let input_key = erl_key(&self.decl.params[0].name);
                let iv = fresh("Inner", ctr);
                writeln!(
                    out,
                    "{}{} = {}:recognize(lists:nthtail(maps:get(cursor, {}), maps:get({}, {}))),",
                    ind, iv, inner, st, input_key, st
                )
                .ok();
                writeln!(out, "{}case maps:get(accepted, {}) of", ind, iv).ok();
                let ind4 = format!("{}    ", ind);
                let ind8 = format!("{}        ", ind);
                writeln!(out, "{}false ->", ind4).ok();
                self.emit_failure(out, m, st, &ind8);
                out.push_str(";\n");
                writeln!(out, "{}true ->", ind4).ok();
                let icur = fresh("IC", ctr);
                writeln!(out, "{}{} = maps:get(cursor, {}),", ind8, icur, iv).ok();
                let mtch = fresh("M", ctr);
                writeln!(
                    out,
                    "{}{} = lists:sublist(maps:get({}, {}), maps:get(cursor, {}) + 1, {}),",
                    ind8, mtch, input_key, st, st, icur
                )
                .ok();
                let cap = match &stage.label {
                    Some(lbl) if !state_label.is_empty() => format!(
                        ", {} => {}, {} => {}",
                        erl_key(&cap_key(state_label, lbl)),
                        mtch,
                        erl_key(&cap_inst_key(state_label, lbl)),
                        iv
                    ),
                    _ => String::new(),
                };
                let st2 = fresh("St", ctr);
                writeln!(
                    out,
                    "{}{} = {}#{{fsm_matched => {}, cursor => maps:get(cursor, {}) + {}, accepted => true{}}},",
                    ind8, st2, st, mtch, st, icur, cap
                )
                .ok();
                self.emit_seq(
                    out,
                    elements,
                    idx + 1,
                    &st2,
                    m,
                    state_idx,
                    state_label,
                    &ind8,
                    ctr,
                    ai,
                )?;
                writeln!(out).ok();
                write!(out, "{}end", ind).ok();
            }
            MatchElement::Stage(stage) => {
                let my_sid = self.stage_sid[&(state_idx, ai, idx)];
                let r = fresh("R", ctr);
                // An embedding stage matches via `match_stage_<sid>`, which
                // fires its actions and returns the (mutated) state map; a
                // plain stage uses the shared `dfa_match` executor. `base` is
                // the state to commit / fail from (the threaded map for an
                // embedding stage, else the incoming `st`).
                let base = if stage.embedding_actions.is_empty() {
                    writeln!(out, "{}{} = dfa_match({}, dfa_{}()),", ind, r, st, my_sid).ok();
                    st.to_string()
                } else {
                    let se = fresh("St", ctr);
                    writeln!(
                        out,
                        "{}{{{}, {}}} = match_stage_{}({}),",
                        ind, r, se, my_sid, st
                    )
                    .ok();
                    se
                };
                writeln!(out, "{}case {} < 0 of", ind, r).ok();
                let ind2 = format!("{}    ", ind);
                writeln!(out, "{}true ->", ind2).ok();
                self.emit_failure(out, m, &base, &format!("{}    ", ind2));
                out.push_str(";\n");
                writeln!(out, "{}false ->", ind2).ok();
                let ind3 = format!("{}    ", ind2);
                // Commit: matched slice + cursor advance + accepted, plus the
                // `.label` capture (`$state.label`) for a labeled stage in a
                // labeled state.
                let mtch = fresh("M", ctr);
                writeln!(
                    out,
                    "{}{} = lists:sublist(maps:get({}, {}), maps:get(cursor, {}) + 1, {} - maps:get(cursor, {})),",
                    ind3, mtch, erl_key(&self.decl.params[0].name), base, base, r, base
                )
                .ok();
                let st2 = fresh("St", ctr);
                let cap = match &stage.label {
                    Some(lbl) if !state_label.is_empty() => {
                        format!(", {} => {}", erl_key(&cap_key(state_label, lbl)), mtch)
                    }
                    _ => String::new(),
                };
                writeln!(
                    out,
                    "{}{} = {}#{{fsm_matched => {}, cursor => {}, accepted => true{}}},",
                    ind3, st2, base, mtch, r, cap
                )
                .ok();
                self.emit_seq(
                    out,
                    elements,
                    idx + 1,
                    &st2,
                    m,
                    state_idx,
                    state_label,
                    &ind3,
                    ctr,
                    ai,
                )?;
                writeln!(out).ok();
                write!(out, "{}end", ind).ok();
            }
            MatchElement::BareExpression { expr, .. } => {
                // A declared-action call as the return expression threads the
                // state map first (`{V, StA} = act_…`), then sets the value.
                let (value, base) = match self.as_action_call(expr) {
                    Some((name, args)) => {
                        let (v, sta) = self.emit_action_call(out, name, args, st, ind, ctr);
                        (v, sta)
                    }
                    None => (self.expr(expr, st), st.to_string()),
                };
                let st2 = fresh("St", ctr);
                writeln!(
                    out,
                    "{}{} = {}#{{return_value => {}}},",
                    ind, st2, base, value
                )
                .ok();
                self.emit_seq(
                    out,
                    elements,
                    idx + 1,
                    &st2,
                    m,
                    state_idx,
                    state_label,
                    ind,
                    ctr,
                    ai,
                )?;
            }
            MatchElement::ActionBlock(blk) => {
                let st2 = self.emit_block(out, &blk.statements, st, ind, ctr)?;
                self.emit_seq(
                    out,
                    elements,
                    idx + 1,
                    &st2,
                    m,
                    state_idx,
                    state_label,
                    ind,
                    ctr,
                    ai,
                )?;
            }
        }
        Ok(())
    }

    /// Emit a sequence of action-block statements, threading the state map
    /// from `st` through each; returns the final state-map variable name.
    fn emit_block(
        &self,
        out: &mut String,
        statements: &[crate::frame_c::compiler::frame_ast::Statement],
        st: &str,
        ind: &str,
        ctr: &mut usize,
    ) -> Result<String, String> {
        let mut cur = st.to_string();
        for s in statements {
            cur = self.emit_stmt(out, s, &cur, ind, ctr)?;
        }
        Ok(cur)
    }

    /// Emit one action-block statement, returning the threaded state-map var.
    /// A `self.X = V` assignment updates the map; an `if`/`else` binds the
    /// branch's resulting map via a `case`.
    fn emit_stmt(
        &self,
        out: &mut String,
        s: &crate::frame_c::compiler::frame_ast::Statement,
        st: &str,
        ind: &str,
        ctr: &mut usize,
    ) -> Result<String, String> {
        match s {
            Statement::Expression(e) => match &e.expr {
                Expression::Assign { target, value } => {
                    let field = self.assign_field(target)?;
                    // `self.X = action(...)` hoists the call (which threads
                    // the state map) before the field update.
                    if let Some((name, args)) = self.as_action_call(value) {
                        let (v, sta) = self.emit_action_call(out, name, args, st, ind, ctr);
                        let st2 = fresh("St", ctr);
                        writeln!(
                            out,
                            "{}{} = {}#{{{} => {}}},",
                            ind,
                            st2,
                            sta,
                            erl_key(&field),
                            v
                        )
                        .ok();
                        return Ok(st2);
                    }
                    let st2 = fresh("St", ctr);
                    writeln!(
                        out,
                        "{}{} = {}#{{{} => {}}},",
                        ind,
                        st2,
                        st,
                        erl_key(&field),
                        self.expr(value, st)
                    )
                    .ok();
                    Ok(st2)
                }
                // A bare declared-action call (`tally()`) is invoked for its
                // state-map effect; the returned value is discarded (`_`).
                _ if self.as_action_call(&e.expr).is_some() => {
                    let (name, args) = self.as_action_call(&e.expr).unwrap();
                    let st2 = fresh("St", ctr);
                    let a: Vec<String> = args.iter().map(|x| self.expr(x, st)).collect();
                    let arglist = if a.is_empty() {
                        String::new()
                    } else {
                        format!(", {}", a.join(", "))
                    };
                    writeln!(
                        out,
                        "{}{{_, {}}} = act_{}({}{}),",
                        ind, st2, name, st, arglist
                    )
                    .ok();
                    Ok(st2)
                }
                _ => Err(
                    "only `self.X = ...` assignments and declared-action calls are supported in \
                     @@fsm action blocks by the Erlang backend"
                        .into(),
                ),
            },
            Statement::If(if_ast) => {
                let st2 = fresh("St", ctr);
                writeln!(
                    out,
                    "{}{} = case {} of",
                    ind,
                    st2,
                    self.expr(&if_ast.condition, st)
                )
                .ok();
                let ind2 = format!("{}    ", ind);
                // then-branch
                writeln!(out, "{}true ->", ind2).ok();
                let then_st =
                    self.emit_branch(out, &if_ast.then_branch, st, &format!("{}    ", ind2), ctr)?;
                writeln!(out, "{}    {};", ind2, then_st).ok();
                // else-branch (or pass-through)
                writeln!(out, "{}false ->", ind2).ok();
                match &if_ast.else_branch {
                    Some(else_b) => {
                        let else_st =
                            self.emit_branch(out, else_b, st, &format!("{}    ", ind2), ctr)?;
                        writeln!(out, "{}    {}", ind2, else_st).ok();
                    }
                    None => {
                        writeln!(out, "{}    {}", ind2, st).ok();
                    }
                }
                writeln!(out, "{}end,", ind).ok();
                Ok(st2)
            }
            Statement::Block(blk) => self.emit_block(out, &blk.statements, st, ind, ctr),
            other => Err(format!(
                "statement {:?} not supported in @@fsm action blocks by the Erlang backend",
                std::mem::discriminant(other)
            )),
        }
    }

    /// Emit a branch (the `then`/`else` of an `if`) and return its final
    /// state-map variable (the value the enclosing `case` clause yields).
    fn emit_branch(
        &self,
        out: &mut String,
        s: &crate::frame_c::compiler::frame_ast::Statement,
        st: &str,
        ind: &str,
        ctr: &mut usize,
    ) -> Result<String, String> {
        self.emit_stmt(out, s, st, ind, ctr)
    }

    /// The state-map field name a `self.X` assignment target writes.
    fn assign_field(&self, target: &Expression) -> Result<String, String> {
        if let Expression::Member { object, field } = target {
            if let Expression::Var(name) = object.as_ref() {
                if name == "self" {
                    return Ok(field.clone());
                }
            }
        }
        Err("@@fsm action-block assignment target must be `self.<field>` (Erlang backend)".into())
    }

    /// Emit the success-branch transition tuple `{Target, St}` (the tail of a
    /// state-function body / `false ->` branch).
    fn emit_success(&self, out: &mut String, m: &MatchAst, st: &str, ind: &str) {
        match m.transition.as_ref().and_then(|c| c.success.as_ref()) {
            None => {
                write!(out, "{}{{-1, {}}}", ind, st).ok();
            }
            Some(target) => self.emit_target(out, target, st, ind, m),
        }
    }

    /// Emit the failure-branch resolution: a new state map with `accepted =
    /// false` and the reject position, then the `{Target, St}` tuple.
    fn emit_failure(&self, out: &mut String, m: &MatchAst, st: &str, ind: &str) {
        writeln!(
            out,
            "{ind}{st}f = {st}#{{accepted => false, reject_position => maps:get(cursor, {st})}},",
            ind = ind,
            st = st
        )
        .ok();
        let stf = format!("{}f", st);
        match m.transition.as_ref().and_then(|c| c.failure.as_ref()) {
            None => {
                write!(out, "{}{{-1, {}}}", ind, stf).ok();
            }
            Some(target) => self.emit_target(out, target, &stf, ind, m),
        }
    }

    /// Emit a transition target. A static target is a `{Index, St}` tuple; a
    /// conditional target is an ordered chain of `case <when> of` clauses, the
    /// first satisfied `when` selecting its target, with the match's failure
    /// branch (a reject) as the no-`when`-held fallback (§3.5.4).
    fn emit_target(
        &self,
        out: &mut String,
        target: &FsmTransitionTarget,
        st: &str,
        ind: &str,
        m: &MatchAst,
    ) {
        match target {
            FsmTransitionTarget::Static {
                state, stage: None, ..
            } => {
                let idx = self
                    .label_to_index
                    .get(state)
                    .copied()
                    .unwrap_or(usize::MAX);
                if idx == usize::MAX {
                    // Surfaced at compile time via the unwrap fallback would
                    // be opaque; instead emit a recognizable runtime error.
                    write!(
                        out,
                        "{}erlang:error({{undeclared_state, {}}})",
                        ind,
                        erl_atom(state)
                    )
                    .ok();
                } else {
                    write!(out, "{}{{{}, {}}}", ind, idx, st).ok();
                }
            }
            FsmTransitionTarget::Static {
                state,
                stage: Some(stage),
                ..
            } => {
                // Stage-ref: re-enter `state` at element `entry`, recorded in
                // the state map as `fsm_enter` for the dispatch `case`.
                let idx = self
                    .label_to_index
                    .get(state)
                    .copied()
                    .unwrap_or(usize::MAX);
                let entry = self
                    .stage_entry
                    .get(&(state.clone(), stage.clone()))
                    .copied();
                match (idx, entry) {
                    (usize::MAX, _) | (_, None) => {
                        write!(
                            out,
                            "{}erlang:error({{undeclared_stage, {}, {}}})",
                            ind,
                            erl_atom(state),
                            erl_atom(stage)
                        )
                        .ok();
                    }
                    (idx, Some(entry)) => {
                        write!(out, "{}{{{}, {}#{{fsm_enter => {}}}}}", ind, idx, st, entry).ok();
                    }
                }
            }
            FsmTransitionTarget::Conditional(alts) => {
                self.emit_conditional(out, alts, 0, st, ind, m);
            }
        }
    }

    /// Recursively emit the ordered `when` chain as nested `case`s; when no
    /// `when` holds, the match's failure branch fires (a reject, §3.5.4).
    fn emit_conditional(
        &self,
        out: &mut String,
        alts: &[crate::frame_c::compiler::frame_ast::FsmCondAlt],
        idx: usize,
        st: &str,
        ind: &str,
        m: &MatchAst,
    ) {
        if idx == alts.len() {
            self.emit_failure(out, m, st, ind);
            return;
        }
        let alt = &alts[idx];
        let ind4 = format!("{}    ", ind);
        let ind8 = format!("{}        ", ind);
        writeln!(out, "{}case {} of", ind, self.expr(&alt.condition, st)).ok();
        writeln!(out, "{}true ->", ind4).ok();
        self.emit_target(out, &alt.target, st, &ind8, m);
        out.push_str(";\n");
        writeln!(out, "{}false ->", ind4).ok();
        self.emit_conditional(out, alts, idx + 1, st, &ind8, m);
        writeln!(out).ok();
        write!(out, "{}end", ind).ok();
    }

    /// Per-stage DFA helper: `dfa_<sid>() -> {StatesTuple, Start}.` Mode C
    /// placeholders carry no DFA and emit nothing (no `dfa_match` call).
    fn emit_dfa_helpers(&self, out: &mut String) {
        for (sid, dfa) in self.stage_dfas.iter().enumerate() {
            if dfa.mode_c.is_some() {
                continue;
            }
            let states: Vec<String> = dfa
                .states
                .iter()
                .map(|(trans, acc)| {
                    let ts: Vec<String> = trans
                        .iter()
                        .map(|(lo, hi, tgt)| format!("{{{}, {}, {}}}", lo, hi, tgt))
                        .collect();
                    format!("{{[{}], {}}}", ts.join(", "), acc)
                })
                .collect();
            writeln!(
                out,
                "dfa_{}() -> {{{{{}}}, {}}}.",
                sid,
                states.join(", "),
                dfa.start
            )
            .ok();
        }
        if !self.stage_dfas.is_empty() {
            out.push('\n');
        }
    }

    /// Does any stage match via the shared `dfa_match` executor (a regex
    /// stage with no embedding actions and not a Mode C call-out)? When none
    /// does, `dfa_match`/`dfa_loop` are never called and must not be emitted.
    fn has_plain_stage(&self) -> bool {
        self.any_stage(|s| s.embedding_actions.is_empty() && mode_c_inner(&s.regex).is_none())
    }

    /// Does any stage carry embedding actions? Its specialized matcher is the
    /// only `dfa_find` caller besides the plain `dfa_loop`.
    fn has_embedding_stage(&self) -> bool {
        self.any_stage(|s| !s.embedding_actions.is_empty())
    }

    fn any_stage(&self, pred: impl Fn(&StageAst) -> bool) -> bool {
        self.decl.states.iter().any(|st| {
            st.matches.iter().any(|m| {
                m.elements
                    .iter()
                    .any(|el| matches!(el, MatchElement::Stage(s) if pred(s)))
            })
        })
    }

    /// The shared greedy longest-match DFA executor (identical in every
    /// generated module). Reads the cursor from `St`; returns the end index
    /// of the longest match, or -1 for no match.
    fn emit_dfa_runtime(&self, out: &mut String) {
        if self.has_plain_stage() {
            writeln!(
                out,
                "dfa_match(St, {{States, Start}}) ->\n\
                 \x20   Input = maps:get(fsm_input, St),\n\
                 \x20   N = maps:get(fsm_n, St),\n\
                 \x20   Pos = maps:get(cursor, St),\n\
                 \x20   {{_, Acc}} = element(Start + 1, States),\n\
                 \x20   Last = case Acc of true -> Pos; false -> -1 end,\n\
                 \x20   dfa_loop(Input, N, States, Start, Pos, Last).\n\n\
                 dfa_loop(Input, N, States, S, Pos, Last) ->\n\
                 \x20   case Pos < N of\n\
                 \x20       false -> Last;\n\
                 \x20       true ->\n\
                 \x20           V = {read},\n\
                 \x20           {{Trans, _}} = element(S + 1, States),\n\
                 \x20           case dfa_find(Trans, V) of\n\
                 \x20               none -> Last;\n\
                 \x20               {{ok, Tgt}} ->\n\
                 \x20                   {{_, Acc}} = element(Tgt + 1, States),\n\
                 \x20                   Last2 = case Acc of true -> Pos + 1; false -> Last end,\n\
                 \x20                   dfa_loop(Input, N, States, Tgt, Pos + 1, Last2)\n\
                 \x20           end\n\
                 \x20   end.\n",
                read = self.element_read_expr()
            )
            .ok();
        }
        // `dfa_find` is called by the plain `dfa_loop` and the embedding
        // matcher; emit it only when one of those exists (a Mode-C-only fsm
        // needs neither).
        if self.has_plain_stage() || self.has_embedding_stage() {
            out.push_str(
                "dfa_find([], _) -> none;\n\
                 dfa_find([{Lo, Hi, Tgt} | T], V) ->\n\
                 \x20   case (Lo =< V) andalso (V =< Hi) of\n\
                 \x20       true -> {ok, Tgt};\n\
                 \x20       false -> dfa_find(T, V)\n\
                 \x20   end.\n",
            );
        }
    }

    /// The empty `matched` value for the alphabet (an empty list / string).
    fn matched_empty(&self) -> &'static str {
        "[]"
    }

    /// Translate a Frame expression to an Erlang expression reading the
    /// state map bound to `st`.
    fn expr(&self, e: &Expression, st: &str) -> String {
        match e {
            Expression::Literal(l) => match l {
                Literal::Int(i) => i.to_string(),
                Literal::Float(f) => f.to_string(),
                Literal::Bool(b) => b.to_string(),
                Literal::String(s) => {
                    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
                }
                Literal::Null => "undefined".to_string(),
            },
            Expression::Var(name) => match name.as_str() {
                "@@:matched" => format!("maps:get(fsm_matched, {})", st),
                "@@:cursor" => format!("maps:get(cursor, {})", st),
                "@@:return" => format!("maps:get(return_value, {})", st),
                _ => match name.strip_prefix('$').and_then(|c| c.split_once('.')) {
                    Some((state, label)) => {
                        format!("maps:get({}, {})", erl_key(&cap_key(state, label)), st)
                    }
                    None => erl_var(name),
                },
            },
            Expression::Binary { left, op, right } => format!(
                "({} {} {})",
                self.expr(left, st),
                binop(op),
                self.expr(right, st)
            ),
            Expression::Unary { op, expr } => match op {
                UnaryOp::Not => format!("(not {})", self.expr(expr, st)),
                UnaryOp::Neg => format!("(- {})", self.expr(expr, st)),
                UnaryOp::BitNot => format!("(bnot {})", self.expr(expr, st)),
            },
            Expression::Call { func, args } => self.call(func, args, st),
            Expression::Member { object, field } => {
                if let Expression::Var(name) = object.as_ref() {
                    // Mode C (§8.3): `$state.label.<fsm field>` reads the
                    // inner fsm instance map recorded for that stage.
                    if let Some((state, label)) =
                        name.strip_prefix('$').and_then(|c| c.split_once('.'))
                    {
                        if matches!(
                            field.as_str(),
                            "return_value" | "accepted" | "cursor" | "reject_position"
                        ) {
                            return format!(
                                "maps:get({}, maps:get({}, {}))",
                                erl_key(field),
                                erl_key(&cap_inst_key(state, label)),
                                st
                            );
                        }
                    }
                    // `self.field` reads a state-map key.
                    if name == "self" {
                        return format!("maps:get({}, {})", erl_key(field), st);
                    }
                }
                format!("maps:get({}, {})", erl_key(field), st)
            }
            Expression::Index { object, index } => {
                format!(
                    "lists:nth({} + 1, {})",
                    self.expr(index, st),
                    self.expr(object, st)
                )
            }
            Expression::Assign { .. } => {
                // Assignments thread the state map and are handled in the
                // statement emitter; they do not appear in pure-expr position
                // in the v0.1 first cut.
                "erlang:error(assign_in_expr_unsupported)".to_string()
            }
            Expression::NativeExpr(s) => s.clone(),
        }
    }

    fn call(&self, func: &str, args: &[Expression], st: &str) -> String {
        let a: Vec<String> = args.iter().map(|e| self.expr(e, st)).collect();
        match func {
            "to_int" => format!("list_to_integer({})", a.join(", ")),
            // `matched` is already an Erlang string (list of code points).
            "to_str" => a.join(", "),
            "len" => format!("length({})", a.join(", ")),
            _ => format!("erlang:error({{action_unsupported, {}}})", erl_atom(func)),
        }
    }
}

/// A state-map key atom for `name` (`return_value`, a param, a `cap_*`, …).
fn erl_key(name: &str) -> String {
    erl_atom(name)
}

/// `$state.label` capture key name → `cap_state_label`.
fn cap_key(state: &str, label: &str) -> String {
    format!("cap_{}_{}", state, label)
}

/// Mode C inner-instance key name → `cap_inst_state_label`.
fn cap_inst_key(state: &str, label: &str) -> String {
    format!("cap_inst_{}_{}", state, label)
}

/// An Erlang atom literal: unquoted when it is a syntactically valid bare
/// atom (lowercase-led, alphanumeric/underscore), quoted otherwise.
fn erl_atom(s: &str) -> String {
    let bare = s
        .chars()
        .next()
        .map(|c| c.is_ascii_lowercase())
        .unwrap_or(false)
        && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
    if bare {
        s.to_string()
    } else {
        format!("'{}'", s.replace('\\', "\\\\").replace('\'', "\\'"))
    }
}

/// An Erlang variable name for a Frame identifier — capitalized (Erlang vars
/// must start uppercase). `text` → `V_text` keeps the mapping unambiguous.
fn erl_var(name: &str) -> String {
    format!("V_{}", name)
}

fn binop(op: &BinaryOp) -> &'static str {
    match op {
        BinaryOp::Add => "+",
        BinaryOp::Sub => "-",
        BinaryOp::Mul => "*",
        BinaryOp::Div => "div",
        BinaryOp::Mod => "rem",
        BinaryOp::Eq => "==",
        BinaryOp::Ne => "/=",
        BinaryOp::Lt => "<",
        BinaryOp::Le => "=<",
        BinaryOp::Gt => ">",
        BinaryOp::Ge => ">=",
        BinaryOp::And => "andalso",
        BinaryOp::Or => "orelse",
        BinaryOp::BitAnd => "band",
        BinaryOp::BitOr => "bor",
        BinaryOp::BitXor => "bxor",
    }
}

/// A fresh Erlang variable name `<prefix><n>`, advancing the counter.
fn fresh(prefix: &str, ctr: &mut usize) -> String {
    let v = format!("{}{}", prefix, ctr);
    *ctr += 1;
    v
}

/// Map a raw default-value token to an Erlang term of the field's type.
fn erl_default(ty: &Type, raw: &str) -> String {
    match raw {
        "false" => "false".to_string(),
        "true" => "true".to_string(),
        "" => default_for(ty),
        s if s.starts_with('"') => s.to_string(),
        s => s.to_string(),
    }
}

/// A type-appropriate Erlang default when no initializer token is present.
fn default_for(ty: &Type) -> String {
    let s = match ty {
        Type::Custom(s) => s.as_str(),
        _ => "",
    };
    match s {
        "int" => "0".to_string(),
        "float" => "0.0".to_string(),
        "bool" => "false".to_string(),
        _ => "[]".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frame_c::compiler::fsm_parser::parse_fsm_block;
    use std::process::Command;

    /// Generate Erlang for `src`, write it as `<module>.erl`, compile with
    /// `erlc`, and run `<module>:recognize/1` over `input` via `erl -eval`,
    /// returning `(accepted, return_value)` as printed terms. `None` if the
    /// Erlang toolchain is unavailable.
    fn run(src: &str, module: &str, input: &str, tag: &str) -> Option<(String, String)> {
        let decl = parse_fsm_block(src.as_bytes()).expect("fixture must parse");
        let code = generate(&decl).expect("fixture must generate");
        let dir = std::env::temp_dir().join(format!("framec_erl_{}", tag));
        std::fs::create_dir_all(&dir).ok()?;
        let erl_path = dir.join(format!("{}.erl", module));
        std::fs::write(&erl_path, &code).expect("write erl");
        // Compile the module into `dir`.
        let compile = match Command::new("erlc")
            .arg("-o")
            .arg(&dir)
            .arg(&erl_path)
            .output()
        {
            Ok(o) => o,
            Err(_) => return None, // erlc absent — skip
        };
        assert!(
            compile.status.success(),
            "erlc failed for {:?}:\n{}\n--- source ---\n{}",
            src,
            String::from_utf8_lossy(&compile.stderr),
            code
        );
        // Drive it: print accepted + return_value on separate lines.
        let eval = format!(
            "R = {module}:recognize(\"{input}\"), io:format(\"~p~n~p~n\", [maps:get(accepted, R), maps:get(return_value, R)])",
            module = module,
            input = input
        );
        let out = Command::new("erl")
            .arg("-noshell")
            .arg("-pa")
            .arg(&dir)
            .arg("-eval")
            .arg(&eval)
            .arg("-s")
            .arg("init")
            .arg("stop")
            .output()
            .expect("run erl");
        let text = String::from_utf8_lossy(&out.stdout);
        let lines: Vec<&str> = text.lines().collect();
        assert!(
            lines.len() >= 2,
            "erl produced no result:\nstdout: {}\nstderr: {}\n--- source ---\n{}",
            text,
            String::from_utf8_lossy(&out.stderr),
            code
        );
        Some((lines[0].to_string(), lines[1].to_string()))
    }

    #[test]
    fn erl_smoke_bool() {
        let src = "@@fsm M(text: bytes) : bool = false { /a/ true }";
        let Some((acc, ret)) = run(src, "m", "a", "smoke_a") else {
            return;
        };
        assert_eq!((acc.as_str(), ret.as_str()), ("true", "true"));
        assert_eq!(run(src, "m", "b", "smoke_b").unwrap().0, "false");
    }

    #[test]
    fn erl_matched_to_int() {
        let src = "@@fsm M(text: bytes) : int = 0 { /[0-9]+/ to_int(@@:matched) }";
        let Some((acc, ret)) = run(src, "m", "123", "tok_a") else {
            return;
        };
        assert_eq!((acc.as_str(), ret.as_str()), ("true", "123"));
        assert_eq!(run(src, "m", "x", "tok_b").unwrap().0, "false");
    }

    #[test]
    fn erl_len_self_input() {
        let src = "@@fsm M(text: bytes) : int = 0 { /[0-9]+/ len(self.text) }";
        let Some((_, ret)) = run(src, "m", "123", "len_a") else {
            return;
        };
        assert_eq!(ret, "3");
    }

    /// Static success + failure transitions across labeled states.
    /// `$0` matches a letter then transitions to `$digits`; a non-letter
    /// routes to `$error`.
    #[test]
    fn erl_static_transitions() {
        let src = "@@fsm M(text: bytes) : int = 0 { \
                   $0: /[a-z]/ -> $digits : -> $error \
                   $digits: /[0-9]+/ to_int(@@:matched) \
                   $error: -1 }";
        let Some((acc, ret)) = run(src, "m", "x42", "tr_a") else {
            return;
        };
        assert_eq!((acc.as_str(), ret.as_str()), ("true", "42"));
        // 'X' fails /[a-z]/ → failure branch → $error → -1.
        assert_eq!(run(src, "m", "X", "tr_b").unwrap().1, "-1");
    }

    /// Stage capture: `.n/[0-9]+/` captures the matched slice as `$s.n`.
    #[test]
    fn erl_stage_capture() {
        let src = "@@fsm M(text: bytes) : int = 0 { $s: .n/[0-9]+/ to_int($s.n) }";
        let Some((acc, ret)) = run(src, "m", "42", "cap_a") else {
            return;
        };
        assert_eq!((acc.as_str(), ret.as_str()), ("true", "42"));
    }

    /// Action block mutating a domain field, returned by a bare expression.
    #[test]
    fn erl_action_block() {
        let src = "@@fsm M(text: bytes) : int = 0 { \
                   /[0-9]/ { self.count = self.count + 1 } self.count \
                   domain: count: int = 0 }";
        let Some((_, ret)) = run(src, "m", "5", "act_a") else {
            return;
        };
        assert_eq!(ret, "1");
    }

    /// Action block with an `if`/`else` that threads the state map.
    #[test]
    fn erl_action_if_else() {
        let src = "@@fsm M(text: bytes) : int = 0 { \
                   /[0-9]/ { if self.flag { self.n = 1 } else { self.n = 2 } } self.n \
                   domain: flag: bool = false n: int = 0 }";
        let Some((_, ret)) = run(src, "m", "5", "if_a") else {
            return;
        };
        assert_eq!(ret, "2"); // flag is false → else branch
    }

    /// Declared `actions:` helper, callable from a match (with a return
    /// value) and threading the state map.
    #[test]
    fn erl_declared_action() {
        let src = "@@fsm M(text: bytes) : int = 0 { \
                   /[0-9]+/ parse_int(@@:matched) \
                   actions: parse_int(s: bytes): int { to_int(s) } }";
        let Some((_, ret)) = run(src, "m", "42", "decl_a") else {
            return;
        };
        assert_eq!(ret, "42");
    }

    /// A declared action that mutates state (no return value), invoked for
    /// effect from an action block, then read back.
    #[test]
    fn erl_declared_action_effect() {
        let src = "@@fsm M(text: bytes) : int = 0 { \
                   /[0-9]/ { tally() } self.count \
                   actions: tally() { self.count = self.count + 1 } \
                   domain: count: int = 0 }";
        let Some((_, ret)) = run(src, "m", "7", "decl_b") else {
            return;
        };
        assert_eq!(ret, "1");
    }

    /// Conditional `when` target (FSM-TEST-402): the first satisfied `when`
    /// selects its state; no `when` held → the failure branch (a reject).
    #[test]
    fn erl_conditional_target() {
        let src = "@@fsm M(text: bytes, mode: int) : int = 0 { \
                   /[01]/ -> ( $zero when self.mode == 0, $one when self.mode == 1 ) : -> $error \
                   $zero: 0 \
                   $one: 1 \
                   $error: -1 }";
        let decl = parse_fsm_block(src.as_bytes()).expect("parses");
        let code = generate(&decl).expect("generates");
        let run_mode = |inp: &str, mode: i64, tag: &str| -> Option<String> {
            let dir = std::env::temp_dir().join(format!("framec_erl_{}", tag));
            std::fs::create_dir_all(&dir).ok()?;
            let erl_path = dir.join("m.erl");
            std::fs::write(&erl_path, &code).ok()?;
            let c = Command::new("erlc")
                .arg("-o")
                .arg(&dir)
                .arg(&erl_path)
                .output()
                .ok()?;
            assert!(c.status.success(), "{}", String::from_utf8_lossy(&c.stderr));
            let eval = format!(
                "R = m:recognize(\"{}\", {}), io:format(\"~p~n\", [maps:get(return_value, R)])",
                inp, mode
            );
            let o = Command::new("erl")
                .arg("-noshell")
                .arg("-pa")
                .arg(&dir)
                .arg("-eval")
                .arg(&eval)
                .arg("-s")
                .arg("init")
                .arg("stop")
                .output()
                .expect("run erl");
            Some(String::from_utf8_lossy(&o.stdout).trim().to_string())
        };
        let Some(z) = run_mode("0", 0, "cond_a") else {
            return;
        };
        assert_eq!(z, "0");
        assert_eq!(run_mode("1", 1, "cond_b").unwrap(), "1");
        assert_eq!(run_mode("0", 2, "cond_c").unwrap(), "-1"); // no when → failure
    }

    /// Stage-ref target `-> $State.stage`: re-enter a state at a specific
    /// stage, skipping its earlier elements. `$0` matches `x` then re-enters
    /// `$rest` at the `tail` stage (element 1), bypassing element 0. The
    /// match carries a failure branch so it passes validation (the bypassed
    /// `/y/` stage is only reached when entering `$rest` at element 0).
    #[test]
    fn erl_stage_ref_target() {
        let src = "@@fsm M(text: bytes) : int = 0 { \
                   $0: /x/ -> $rest.tail : -> $err \
                   $rest: /y/ .tail/[0-9]+/ to_int($rest.tail) : -> $err \
                   $err: -1 }";
        let Some((acc, ret)) = run(src, "m", "x42", "sref_a") else {
            return;
        };
        assert_eq!((acc.as_str(), ret.as_str()), ("true", "42"));
    }

    /// Multi-match (`|`) ordered choice: the first alternative whose first
    /// stage matches wins; distinct first stages route to distinct targets.
    #[test]
    fn erl_multi_match_ordered_choice() {
        let src = "@@fsm M(text: bytes) : int = 0 { \
                   /[0-9]/ -> $num | /[a-z]/ -> $word \
                   $num: 1 \
                   $word: 2 }";
        let Some((acc, ret)) = run(src, "m", "5", "mmc_a") else {
            return;
        };
        assert_eq!((acc.as_str(), ret.as_str()), ("true", "1"));
        assert_eq!(run(src, "m", "a", "mmc_b").unwrap().1, "2");
        // Neither alternative's first stage matches → reject.
        assert_eq!(run(src, "m", "!", "mmc_c").unwrap().0, "false");
    }

    /// Selection commits on the first stage: a committed alternative's later
    /// stage failure follows *its* failure branch, no backtracking.
    #[test]
    fn erl_multi_match_commits() {
        let src = "@@fsm M(text: bytes) : int = 0 { \
                   /a/ /b/ -> $ab : -> $err | /a/ /c/ -> $ac \
                   $ab: 1 \
                   $ac: 2 \
                   $err: -1 }";
        let Some((_, ret)) = run(src, "m", "ab", "mmk_a") else {
            return;
        };
        assert_eq!(ret, "1");
        // "ac": alt0 commits on /a/; /b/ fails on 'c' → alt0's failure ($err).
        let (acc, ret) = run(src, "m", "ac", "mmk_b").unwrap();
        assert_eq!((acc.as_str(), ret.as_str()), ("false", "-1"));
    }

    /// A stageless final alternative is an unconditional catch-all.
    #[test]
    fn erl_multi_match_catch_all() {
        let src = "@@fsm M(text: bytes) : int = 0 { \
                   /[0-9]/ -> $num | 99 \
                   $num: 1 }";
        let Some((acc, ret)) = run(src, "m", "5", "mma_a") else {
            return;
        };
        assert_eq!((acc.as_str(), ret.as_str()), ("true", "1"));
        // 'a': digit alternative misses → catch-all matches unconditionally.
        let (acc2, ret2) = run(src, "m", "a", "mma_b").unwrap();
        assert_eq!((acc2.as_str(), ret2.as_str()), ("true", "99"));
    }

    /// `${...}` fires once per consumed element (FSM-TEST-123); a declared
    /// action is callable from inside it, threading the state map.
    #[test]
    fn erl_embed_every_transition() {
        let src = "@@fsm M(text: bytes) : int = 0 { \
                   /[0-9]+/ ${ tally() } \
                   self.count \
                   actions: tally() { self.count = self.count + 1 } \
                   domain: count: int = 0 }";
        let Some((_, ret)) = run(src, "m", "123", "emb_e") else {
            return;
        };
        assert_eq!(ret, "3"); // three digits → ${} fires 3×
    }

    /// `@{...}` fires on each transition into an accepting state; for `/a+/`
    /// over "aaa" that is once per `a`.
    #[test]
    fn erl_embed_accept() {
        let src = "@@fsm M(text: bytes) : int = 0 { \
                   /a+/ @{ self.hits = self.hits + 1 } self.hits \
                   domain: hits: int = 0 }";
        let Some((_, ret)) = run(src, "m", "aaa", "emb_a") else {
            return;
        };
        assert_eq!(ret, "3");
    }

    /// `>{...}` fires once at scan start; `@@:cursor` there is the
    /// stage-entry position (after the prior stage consumed `x`).
    #[test]
    fn erl_embed_start_cursor() {
        let src = "@@fsm M(text: bytes) : int = 0 { \
                   /x/ /[0-9]+/ >{ self.start = @@:cursor } self.start \
                   domain: start: int = -1 }";
        let Some((_, ret)) = run(src, "m", "x42", "emb_s") else {
            return;
        };
        assert_eq!(ret, "1");
    }

    /// Token alphabet (FSM-TEST-253): the input is a list of token-kind
    /// atoms; regex identifiers reference token kinds, not characters.
    #[test]
    fn erl_token_alphabet() {
        let src = "@@fsm M(toks: token) : bool = false { /IDENT LPAREN RPAREN/ true }";
        let decl = parse_fsm_block(src.as_bytes()).expect("parses");
        let code = generate(&decl).expect("generates");
        let run_toks = |toks: &str, tag: &str| -> Option<String> {
            let dir = std::env::temp_dir().join(format!("framec_erl_{}", tag));
            std::fs::create_dir_all(&dir).ok()?;
            let erl_path = dir.join("m.erl");
            std::fs::write(&erl_path, &code).ok()?;
            let c = Command::new("erlc")
                .arg("-o")
                .arg(&dir)
                .arg(&erl_path)
                .output()
                .ok()?;
            assert!(c.status.success(), "{}", String::from_utf8_lossy(&c.stderr));
            let eval = format!(
                "R = m:recognize([{}]), io:format(\"~p~n\", [maps:get(accepted, R)])",
                toks
            );
            let o = Command::new("erl")
                .arg("-noshell")
                .arg("-pa")
                .arg(&dir)
                .arg("-eval")
                .arg(&eval)
                .arg("-s")
                .arg("init")
                .arg("stop")
                .output()
                .expect("run erl");
            Some(String::from_utf8_lossy(&o.stdout).trim().to_string())
        };
        let Some(ok) = run_toks("'IDENT', 'LPAREN', 'RPAREN'", "tok_seq") else {
            return;
        };
        assert_eq!(ok, "true");
        // Wrong token sequence → not in the language.
        assert_eq!(run_toks("'IDENT', 'RPAREN'", "tok_bad").unwrap(), "false");
        // An unknown token kind never matches a transition.
        assert_eq!(
            run_toks("'IDENT', 'WAT', 'RPAREN'", "tok_unk").unwrap(),
            "false"
        );
    }

    /// Mode C call-out (§8.3): `/@Inner/` constructs the inner fsm over the
    /// input at the cursor, advances by what it consumed, and exposes the
    /// inner instance via `$state.label.return_value`. Both modules are
    /// written into one dir and the outer runs `outer:recognize/1`.
    #[test]
    fn erl_mode_c_callout() {
        let inner_src = "@@fsm Digits(text: bytes) : int = 0 { /[0-9]+/ to_int(@@:matched) }";
        let outer_src = "@@fsm Outer(text: bytes) : int = 0 { $s: .d/@Digits/ $s.d.return_value }";
        let inner = generate(&parse_fsm_block(inner_src.as_bytes()).expect("inner parses"))
            .expect("inner generates");
        let outer = generate(&parse_fsm_block(outer_src.as_bytes()).expect("outer parses"))
            .expect("outer generates");
        let run_outer = |inp: &str, tag: &str| -> Option<(String, String)> {
            let dir = std::env::temp_dir().join(format!("framec_erl_{}", tag));
            std::fs::create_dir_all(&dir).ok()?;
            std::fs::write(dir.join("digits.erl"), &inner).ok()?;
            std::fs::write(dir.join("outer.erl"), &outer).ok()?;
            for f in ["digits.erl", "outer.erl"] {
                let c = Command::new("erlc")
                    .arg("-o")
                    .arg(&dir)
                    .arg(dir.join(f))
                    .output()
                    .ok()?;
                assert!(c.status.success(), "{}", String::from_utf8_lossy(&c.stderr));
            }
            let eval = format!(
                "R = outer:recognize(\"{}\"), io:format(\"~p~n~p~n\", [maps:get(accepted, R), maps:get(return_value, R)])",
                inp
            );
            let o = Command::new("erl")
                .arg("-noshell")
                .arg("-pa")
                .arg(&dir)
                .arg("-eval")
                .arg(&eval)
                .arg("-s")
                .arg("init")
                .arg("stop")
                .output()
                .expect("run erl");
            let text = String::from_utf8_lossy(&o.stdout);
            let lines: Vec<&str> = text.lines().collect();
            assert!(lines.len() >= 2, "no result: {}", text);
            Some((lines[0].to_string(), lines[1].to_string()))
        };
        let Some((acc, ret)) = run_outer("42", "modec_a") else {
            return;
        };
        assert_eq!((acc.as_str(), ret.as_str()), ("true", "42"));
        // "x": inner Digits rejects → outer Mode C stage fails → reject.
        assert_eq!(run_outer("x", "modec_b").unwrap().0, "false");
    }

    /// A construct outside the first cut errors clearly. Anchors are deferred
    /// to a later increment.
    #[test]
    fn erl_unsupported_errors() {
        let decl =
            parse_fsm_block(b"@@fsm M(text: bytes) : bool = false { /^a/ true }").expect("parses");
        assert!(generate(&decl).is_err());
    }
}
