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
//! `if`-`else`), and static + failure transitions over the `bytes`/`char`
//! alphabets, with the `@@:matched` / `to_int` / `to_str` / `len` built-ins.
//! Not yet handled (clear `Unsupported` error, never a silent miscompile):
//! declared `actions:`, conditional / stage-ref transition targets,
//! multi-match (`|`) states, embedding actions, Mode C call-out, the token
//! alphabet, and anchors. These land in later increments, matching the Rust
//! backend's build-out.

use crate::frame_c::compiler::frame_ast::{
    BinaryOp, Expression, FsmDeclAst, FsmStateAst, FsmTransitionTarget, Literal, MatchAst,
    MatchElement, Type, UnaryOp,
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
}

struct Generator<'a> {
    decl: &'a FsmDeclAst,
    alphabet: Alphabet,
    label_to_index: std::collections::HashMap<String, usize>,
    stage_dfas: Vec<StageDfa>,
}

impl<'a> Generator<'a> {
    fn new(decl: &'a FsmDeclAst) -> Result<Self, String> {
        let alphabet = match decl.params.first().map(|p| &p.param_type) {
            Some(Type::Custom(t)) if t == "char" => Alphabet::Char,
            Some(Type::Custom(t)) if t == "token" => {
                return Err("the token alphabet is not yet supported by the Erlang backend".into())
            }
            _ => Alphabet::Bytes,
        };
        let mut label_to_index = std::collections::HashMap::new();
        for (i, st) in decl.states.iter().enumerate() {
            if let Some(l) = &st.label {
                label_to_index.insert(l.clone(), i);
            }
        }
        let mut g = Generator {
            decl,
            alphabet,
            label_to_index,
            stage_dfas: Vec::new(),
        };
        g.compile_stage_dfas()?;
        Ok(g)
    }

    /// Compile every stage's DFA in traversal order, so the emitted `dfa_<n>`
    /// helpers line up with the index the state emitters advance.
    fn compile_stage_dfas(&mut self) -> Result<(), String> {
        for st in &self.decl.states {
            if st.matches.len() > 1 {
                return Err(
                    "multi-match (`|`) states are not yet supported by the Erlang backend".into(),
                );
            }
            let Some(m) = st.matches.first() else {
                continue;
            };
            for el in &m.elements {
                if let MatchElement::Stage(stage) = el {
                    if !stage.embedding_actions.is_empty() {
                        return Err(
                            "embedding actions are not yet supported by the Erlang backend".into(),
                        );
                    }
                    if stage.regex.starts_with('@') {
                        return Err(
                            "Mode C (`/@Fsm/`) is not yet supported by the Erlang backend".into(),
                        );
                    }
                    self.stage_dfas.push(self.compile_one(&stage.regex)?);
                }
            }
        }
        Ok(())
    }

    fn compile_one(&self, regex: &str) -> Result<StageDfa, String> {
        match fsm_regex::compile(regex, self.alphabet, DEFAULT_MAX_DFA_STATES) {
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
                            DfaLabel::Token(_) => {
                                return Err("token-alphabet DFA is not supported here".into())
                            }
                        };
                        trans.push((lo, hi, t.to));
                    }
                    states.push((trans, s.is_accept));
                }
                Ok(StageDfa {
                    states,
                    start: compiled.dfa.start,
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
        self.emit_dfa_helpers(&mut out);
        self.emit_dfa_runtime(&mut out);
        Ok(out)
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
        let mut sid = 0usize;
        for (i, st) in self.decl.states.iter().enumerate() {
            match st.matches.first() {
                None => {
                    writeln!(out, "state_{}(St, _Enter) -> {{-1, St}}.\n", i).ok();
                }
                Some(m) => self.emit_one_state(out, i, st, m, &mut sid)?,
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
        sid: &mut usize,
    ) -> Result<(), String> {
        // The parameter is `St`; fresh threaded maps are `St0`, `St1`, … so a
        // fresh name never collides with (and rebinds) the parameter.
        writeln!(out, "state_{}(St, _Enter) ->", index).ok();
        let mut ctr = 0usize;
        let state_label = st.label.clone().unwrap_or_default();
        self.emit_seq(
            out,
            &m.elements,
            0,
            "St",
            m,
            &state_label,
            "    ",
            sid,
            &mut ctr,
        )?;
        out.push_str(".\n\n");
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
        state_label: &str,
        ind: &str,
        sid: &mut usize,
        ctr: &mut usize,
    ) -> Result<(), String> {
        if idx == elements.len() {
            self.emit_success(out, m, st, ind);
            return Ok(());
        }
        match &elements[idx] {
            MatchElement::Stage(stage) => {
                let my_sid = *sid;
                *sid += 1;
                let r = fresh("R", ctr);
                writeln!(out, "{}{} = dfa_match({}, dfa_{}()),", ind, r, st, my_sid).ok();
                writeln!(out, "{}case {} < 0 of", ind, r).ok();
                let ind2 = format!("{}    ", ind);
                writeln!(out, "{}true ->", ind2).ok();
                self.emit_failure(out, m, st, &format!("{}    ", ind2));
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
                    ind3, mtch, erl_key(&self.decl.params[0].name), st, st, r, st
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
                    ind3, st2, st, mtch, r, cap
                )
                .ok();
                self.emit_seq(
                    out,
                    elements,
                    idx + 1,
                    &st2,
                    m,
                    state_label,
                    &ind3,
                    sid,
                    ctr,
                )?;
                writeln!(out).ok();
                write!(out, "{}end", ind).ok();
            }
            MatchElement::BareExpression { expr, .. } => {
                let st2 = fresh("St", ctr);
                writeln!(
                    out,
                    "{}{} = {}#{{return_value => {}}},",
                    ind,
                    st2,
                    st,
                    self.expr(expr, st)
                )
                .ok();
                self.emit_seq(out, elements, idx + 1, &st2, m, state_label, ind, sid, ctr)?;
            }
            MatchElement::ActionBlock(blk) => {
                let st2 = self.emit_block(out, &blk.statements, st, ind, ctr)?;
                self.emit_seq(out, elements, idx + 1, &st2, m, state_label, ind, sid, ctr)?;
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
        use crate::frame_c::compiler::frame_ast::Statement;
        match s {
            Statement::Expression(e) => match &e.expr {
                Expression::Assign { target, value } => {
                    let field = self.assign_field(target)?;
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
                _ => Err(
                    "only `self.X = ...` assignments are supported in @@fsm action blocks by the \
                     Erlang backend"
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
            Some(target) => self.emit_target(out, target, st, ind),
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
            Some(target) => self.emit_target(out, target, &stf, ind),
        }
    }

    /// Emit a static transition target as a `{Index, St}` tuple.
    fn emit_target(&self, out: &mut String, target: &FsmTransitionTarget, st: &str, ind: &str) {
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
            FsmTransitionTarget::Static { .. } => {
                write!(out, "{}erlang:error(stage_ref_target_unsupported)", ind).ok();
            }
            FsmTransitionTarget::Conditional(_) => {
                write!(out, "{}erlang:error(conditional_target_unsupported)", ind).ok();
            }
        }
    }

    /// Per-stage DFA helper: `dfa_<sid>() -> {StatesTuple, Start}.`
    fn emit_dfa_helpers(&self, out: &mut String) {
        for (sid, dfa) in self.stage_dfas.iter().enumerate() {
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

    /// The shared greedy longest-match DFA executor (identical in every
    /// generated module). Reads the cursor from `St`; returns the end index
    /// of the longest match, or -1 for no match.
    fn emit_dfa_runtime(&self, out: &mut String) {
        out.push_str(
            "dfa_match(St, {States, Start}) ->\n\
             \x20   Input = maps:get(fsm_input, St),\n\
             \x20   N = maps:get(fsm_n, St),\n\
             \x20   Pos = maps:get(cursor, St),\n\
             \x20   {_, Acc} = element(Start + 1, States),\n\
             \x20   Last = case Acc of true -> Pos; false -> -1 end,\n\
             \x20   dfa_loop(Input, N, States, Start, Pos, Last).\n\n\
             dfa_loop(Input, N, States, S, Pos, Last) ->\n\
             \x20   case Pos < N of\n\
             \x20       false -> Last;\n\
             \x20       true ->\n\
             \x20           V = element(Pos + 1, Input),\n\
             \x20           {Trans, _} = element(S + 1, States),\n\
             \x20           case dfa_find(Trans, V) of\n\
             \x20               none -> Last;\n\
             \x20               {ok, Tgt} ->\n\
             \x20                   {_, Acc} = element(Tgt + 1, States),\n\
             \x20                   Last2 = case Acc of true -> Pos + 1; false -> Last end,\n\
             \x20                   dfa_loop(Input, N, States, Tgt, Pos + 1, Last2)\n\
             \x20           end\n\
             \x20   end.\n\n\
             dfa_find([], _) -> none;\n\
             dfa_find([{Lo, Hi, Tgt} | T], V) ->\n\
             \x20   case (Lo =< V) andalso (V =< Hi) of\n\
             \x20       true -> {ok, Tgt};\n\
             \x20       false -> dfa_find(T, V)\n\
             \x20   end.\n",
        );
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
                // `self.field` reads a state-map key; other members are not
                // yet supported (Mode C inner-instance reads land later).
                if let Expression::Var(name) = object.as_ref() {
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

    /// A construct outside the first cut errors clearly.
    #[test]
    fn erl_unsupported_errors() {
        let decl =
            parse_fsm_block(b"@@fsm M(toks: token) : bool = false { /A/ true }").expect("parses");
        assert!(generate(&decl).is_err());
    }
}
