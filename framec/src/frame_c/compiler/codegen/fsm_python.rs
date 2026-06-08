//! Python reference backend for `@@fsm` (RFC-0042).
//!
//! This is the first target that actually *runs* an `@@fsm`. It is a
//! self-contained generator — the `@@fsm` runtime model (§5) is distinct
//! from `@@system`'s, so it does not reuse the `CodegenNode` pipeline.
//!
//! Each stage's `/regex/` is compiled to a minimal DFA by
//! [`crate::frame_c::compiler::fsm_regex`]; the emitted class carries the
//! DFA tables as data and drives recognition with a per-state dispatch
//! loop implementing §5.2 (construction), §5.3 (execution), §5.5
//! (transitions), and §5.6 (failure).
//!
//! Acceptance follows the RE2 recognizer model (§5.3): the fsm accepts
//! iff the input is in the recognized language — i.e. recognition halts
//! on a *successful match completion* at a terminal state. A stage
//! failure (or a conditional transition matching no `when`) that routes
//! to a terminal rejects; a failure branch to a non-terminal continues
//! and may still accept (the classifier idiom). `emit_failure` records
//! the rejection (`accepted = False`); a later stage success flips it
//! back. `reject_position` is normalized to 0 on acceptance.
//!
//! # v0.1 scope
//!
//! Supports single-match states, match stages (with `.label` captures),
//! bare-expression returns, action blocks (assignment / `if`-`else`
//! statements), declared `actions:` helpers (emitted as methods), and
//! static, conditional (`when`), and stage-ref (`-> $S.stage`) success/
//! failure transitions (including failure-only clauses `: -> $Err`), and
//! multi-match (`|`) ordered-choice states, embedding actions
//! (`>{}`/`@{}`/`${}`/`%{}`/`@eof{}`, §3.5.5/§5.4), boundary anchors
//! (leading `^`/`\A`, trailing `$`/`\z`, §6.6 — matcher position guards),
//! and Mode C sub-fsm call-out (`/@Inner/`, §8.3 — constructs the inner
//! fsm at the cursor, advances by its cursor, exposes the matched slice as
//! `$state.label` and the inner instance as `$state.label.return_value`),
//! over all three alphabets — `bytes`, `char`, `token` (token kinds map to
//! integer ids so they share the numeric range matcher). Not yet handled
//! (clear `Unsupported` error, never a silent miscompile): mid-pattern
//! anchors and `\b`/`\B`, a `|` alternative with elements before its first
//! stage, and a Mode C stage as a `|` selector.

use crate::frame_c::compiler::frame_ast::{
    BinaryOp, BlockAst, EmbeddingOp, Expression, FsmDeclAst, FsmStateAst, FsmTransitionTarget,
    Literal, MatchAst, MatchElement, StageAst, Statement, Type, UnaryOp,
};
use crate::frame_c::compiler::fsm_regex::{
    self, size_check::DEFAULT_MAX_DFA_STATES, subset::DfaLabel, Alphabet, CompileError,
};
use std::collections::HashMap;
use std::fmt::Write;

/// Generate a Python module source implementing `decl`. Returns the
/// source on success, or a human-readable reason the construct is not yet
/// supported by this backend.
pub fn generate(decl: &FsmDeclAst) -> Result<String, String> {
    Generator::new(decl)?.emit()
}

/// Per-stage compiled DFA, flattened to the data the emitter needs.
struct StageDfa {
    /// One entry per DFA state: `(transitions, is_accept)` where each
    /// transition is `(low, high, target)`. Empty for a Mode C stage.
    states: Vec<(Vec<(u32, u32, usize)>, bool)>,
    start: usize,
    /// Leading `^`/`\A`: the match must start at the input start.
    requires_start: bool,
    /// Trailing `$`/`\z`: the match must end at the input end.
    requires_end: bool,
    /// RFC-0042 §8.3 Mode C: when `Some(name)`, this stage is a call-out
    /// to the `@@fsm` `name` rather than a regex DFA match.
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
    /// State label → dispatch index. Unlabeled start state has no entry
    /// but is index 0.
    label_to_index: HashMap<String, usize>,
    /// `(state label, stage label)` → element index within that state, so
    /// a stage-ref target `-> $State.stage` re-enters at the right element.
    stage_entry: HashMap<(String, String), usize>,
    /// Token-alphabet only: each token-kind name → a small integer id, so
    /// token transitions reuse the same numeric range matcher as bytes/
    /// chars (the per-element read maps a token to its id; unknown → -1).
    token_ids: HashMap<String, u32>,
    /// Compiled DFAs, one per stage, in traversal order; the state code
    /// references them by index as `self._DFA_<n>`.
    stage_dfas: Vec<StageDfa>,
}

impl<'a> Generator<'a> {
    fn new(decl: &'a FsmDeclAst) -> Result<Self, String> {
        let alphabet = match decl.params.first().map(|p| &p.param_type) {
            Some(Type::Custom(t)) if t == "char" => Alphabet::Char,
            Some(Type::Custom(t)) if t == "token" => Alphabet::Token,
            _ => Alphabet::Bytes,
        };

        // Map state labels to dispatch indices (declaration order), and
        // each labeled stage to its element index within its state. Stage
        // re-entry (`_enter`) is only meaningful for single-match states;
        // multi-match states select by first stage, so their stage labels
        // are not registered as re-entry points.
        let mut label_to_index = HashMap::new();
        let mut stage_entry = HashMap::new();
        for (i, st) in decl.states.iter().enumerate() {
            if let Some(l) = &st.label {
                label_to_index.insert(l.clone(), i);
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
            stage_entry,
            token_ids: HashMap::new(),
            stage_dfas: Vec::new(),
        };
        g.compile_stage_dfas()?;
        Ok(g)
    }

    /// Compile every stage regex up-front so codegen can reference DFA
    /// tables by index and reject unsupported constructs early. Stages are
    /// numbered across all states and all `|` alternatives in traversal
    /// order; `emit` walks the same order so `_DFA_<n>` indices line up.
    fn compile_stage_dfas(&mut self) -> Result<(), String> {
        // `token_ids` is taken out so `compile_one` can be a borrow-free
        // associated fn (the loop borrows `self.decl` immutably).
        let mut token_ids = std::mem::take(&mut self.token_ids);
        let alphabet = self.alphabet;
        let mut dfas = Vec::new();
        for st in &self.decl.states {
            for m in &st.matches {
                for el in &m.elements {
                    if let MatchElement::Stage(stage) = el {
                        if let Some(inner) = mode_c_inner(&stage.regex) {
                            // Mode C: a sub-fsm call-out, no DFA. Push a
                            // placeholder so stage-DFA indices stay aligned
                            // with the emit walk.
                            dfas.push(StageDfa {
                                states: Vec::new(),
                                start: 0,
                                requires_start: false,
                                requires_end: false,
                                mode_c: Some(inner.to_string()),
                            });
                        } else {
                            dfas.push(Self::compile_one(alphabet, &stage.regex, &mut token_ids)?);
                        }
                    }
                }
            }
        }
        self.stage_dfas = dfas;
        self.token_ids = token_ids;
        Ok(())
    }

    fn compile_one(
        alphabet: Alphabet,
        regex: &str,
        token_ids: &mut HashMap<String, u32>,
    ) -> Result<StageDfa, String> {
        match fsm_regex::compile(regex, alphabet, DEFAULT_MAX_DFA_STATES) {
            Ok(compiled) => {
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
                                // it shares the numeric range matcher.
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
                    requires_start: compiled.requires_start,
                    requires_end: compiled.requires_end,
                    mode_c: None,
                })
            }
            Err(CompileError::Diagnostics(ds)) => Err(format!(
                "regex `/{}/` failed to compile: {}",
                regex,
                ds.first().map(|d| d.message.as_str()).unwrap_or("")
            )),
            Err(CompileError::UnsupportedAnchors(_)) => Err(format!(
                "regex `/{}/` uses a mid-pattern or word-boundary anchor; only a leading \
                 `^`/`\\A` or trailing `$`/`\\z` is supported in v0.1",
                regex
            )),
        }
    }

    fn emit(&self) -> Result<String, String> {
        let mut out = String::new();
        out.push_str("# Generated by framec — RFC-0042 @@fsm (Python reference backend).\n\n");
        self.emit_preamble(&mut out);
        self.emit_class(&mut out)?;
        Ok(out)
    }

    /// Shared runtime helpers, emitted once.
    fn emit_preamble(&self, out: &mut String) {
        out.push_str(
            "def _frame_to_int(s):\n    return int(s)\n\n\
             def _frame_to_str(s):\n    return str(s)\n\n\
             def _frame_len(s):\n    return len(s)\n\n",
        );
    }

    fn emit_class(&self, out: &mut String) -> Result<(), String> {
        let name = &self.decl.name;
        writeln!(out, "class {}:", name).ok();

        self.emit_dfa_tables(out);
        self.emit_token_table(out);
        self.emit_ctor(out);
        self.emit_dfa_matcher(out);
        self.emit_embed_matchers(out)?;
        self.emit_run(out);
        self.emit_state_methods(out)?;
        self.emit_action_methods(out)?;
        Ok(())
    }

    /// Emit each declared `actions:` helper as a method on the recognizer.
    /// Actions read/write domain fields via `self.<name>` (they share the
    /// instance); a trailing bare expression is the action's return value.
    fn emit_action_methods(&self, out: &mut String) -> Result<(), String> {
        let Some(block) = &self.decl.actions else {
            return Ok(());
        };
        for act in &block.actions {
            let mut sig = String::from("self");
            for p in &act.params {
                write!(sig, ", {}", p.name).ok();
            }
            writeln!(out, "    def {}({}):", act.name, sig).ok();
            self.emit_action_body(out, &act.body, act.return_type.is_some())?;
            out.push('\n');
        }
        Ok(())
    }

    /// Emit an action body. When the action declares a return type and its
    /// final statement is a (non-assignment) bare expression, that
    /// expression is the action's return value (§3.7 implicit tail).
    fn emit_action_body(
        &self,
        out: &mut String,
        body: &BlockAst,
        has_return: bool,
    ) -> Result<(), String> {
        if body.statements.is_empty() {
            out.push_str("        pass\n");
            return Ok(());
        }
        let last = body.statements.len() - 1;
        for (i, st) in body.statements.iter().enumerate() {
            if i == last && has_return {
                if let Statement::Expression(e) = st {
                    if !matches!(e.expr, Expression::Assign { .. }) {
                        writeln!(out, "        return {}", expr_to_py(&e.expr)).ok();
                        continue;
                    }
                }
            }
            out.push_str(&stmt_to_py(st, "        ")?);
        }
        Ok(())
    }

    fn emit_dfa_tables(&self, out: &mut String) {
        for (i, dfa) in self.stage_dfas.iter().enumerate() {
            let states: Vec<String> = dfa
                .states
                .iter()
                .map(|(trans, acc)| {
                    let ts: Vec<String> = trans
                        .iter()
                        .map(|(lo, hi, tgt)| format!("({}, {}, {})", lo, hi, tgt))
                        .collect();
                    format!("([{}], {})", ts.join(", "), py_bool(*acc))
                })
                .collect();
            writeln!(
                out,
                "    _DFA_{} = ([{}], {})",
                i,
                states.join(", "),
                dfa.start
            )
            .ok();
        }
        out.push('\n');
    }

    /// Token alphabet: emit the token-kind → id map and the `_tok_id`
    /// lookup used by the per-element read (unknown token → -1).
    fn emit_token_table(&self, out: &mut String) {
        if self.alphabet != Alphabet::Token {
            return;
        }
        let mut entries: Vec<(&String, &u32)> = self.token_ids.iter().collect();
        entries.sort_by_key(|(_, id)| **id);
        let items: Vec<String> = entries
            .iter()
            .map(|(name, id)| format!("{:?}: {}", name, id))
            .collect();
        writeln!(out, "    _TOK_IDS = {{{}}}", items.join(", ")).ok();
        out.push_str("    def _tok_id(self, t):\n        return self._TOK_IDS.get(t, -1)\n\n");
    }

    fn emit_ctor(&self, out: &mut String) {
        // __init__ signature: input param positional, others with defaults.
        let mut sig = String::from("self");
        for (i, p) in self.decl.params.iter().enumerate() {
            if i == 0 {
                write!(sig, ", {}", p.name).ok();
            } else {
                match &p.default {
                    Some(d) => write!(sig, ", {}={}", p.name, py_default(d)).ok(),
                    None => write!(sig, ", {}", p.name).ok(),
                };
            }
        }
        writeln!(out, "    def __init__({}):", sig).ok();

        // §5.2: auto-promote each parameter to a domain field.
        for p in &self.decl.params {
            writeln!(out, "        self.{} = {}", p.name, p.name).ok();
        }
        // Explicit domain fields (auto fields are already in scope).
        if let Some(domain) = &self.decl.domain {
            for v in &domain.vars {
                // An explicit field re-declaring a parameter keeps the
                // parameter binding (§5.2 step 4 overrides only when given
                // a distinct default); emit its initializer.
                writeln!(out, "        self.{} = {}", v.name, expr_to_py(&v.default)).ok();
            }
        }

        // Observable fields (§5.1) + recognition scratch.
        writeln!(
            out,
            "        self.return_value = {}",
            py_default(&self.decl.default_expr)
        )
        .ok();
        // `@@:matched` is the empty slice of the alphabet's collection
        // type before any stage completes: `[]` for tokens, `""` otherwise.
        let empty_match = if self.alphabet == Alphabet::Token {
            "[]"
        } else {
            "\"\""
        };
        writeln!(
            out,
            "        self.accepted = False\n\
             \x20       self.reject_position = 0\n\
             \x20       self.cursor = 0\n\
             \x20       self._matched = {}\n\
             \x20       self._cap = {{}}\n\
             \x20       self._cap_inst = {{}}\n\
             \x20       self._enter = 0\n\
             \x20       self._run()\n\
             \x20       if self.accepted:\n\
             \x20           self.reject_position = 0\n",
            empty_match
        )
        .ok();
    }

    /// The input domain field — the first parameter's name (auto-promoted
    /// per §3.2). `self.<input>` is the buffer the matcher scans.
    fn input_field(&self) -> &str {
        self.decl
            .params
            .first()
            .map(|p| p.name.as_str())
            .unwrap_or("text")
    }

    /// The per-element read: a byte/char maps to its code via `ord`; a
    /// token maps to its small integer id (`-1` for an unknown token, which
    /// matches no transition). This is the only point where the matcher
    /// differs across alphabets — the range comparison is shared.
    fn element_read(&self) -> String {
        let inp = self.input_field();
        match self.alphabet {
            Alphabet::Token => format!("self._tok_id(self.{}[pos])", inp),
            _ => format!("ord(self.{}[pos])", inp),
        }
    }

    /// Greedy longest-match DFA executor over `self.text` from the cursor.
    /// Returns the end position of the longest match, or -1 if the stage
    /// does not match (not even the empty string).
    fn emit_dfa_matcher(&self, out: &mut String) {
        writeln!(
            out,
            "    def _dfa_match(self, dfa):\n\
             \x20       states, start = dfa\n\
             \x20       st = start\n\
             \x20       pos = self.cursor\n\
             \x20       n = len(self.{})\n\
             \x20       last = pos if states[st][1] else -1\n\
             \x20       while pos < n:\n\
             \x20           v = {}\n\
             \x20           nxt = None\n\
             \x20           for lo, hi, tgt in states[st][0]:\n\
             \x20               if lo <= v <= hi:\n\
             \x20                   nxt = tgt\n\
             \x20                   break\n\
             \x20           if nxt is None:\n\
             \x20               break\n\
             \x20           st = nxt\n\
             \x20           pos += 1\n\
             \x20           if states[st][1]:\n\
             \x20               last = pos\n\
             \x20       return last\n",
            self.input_field(),
            self.element_read()
        )
        .ok();
    }

    /// Emit a specialized matcher `_match_stage_<sid>` for each stage that
    /// carries embedding actions (§3.5.5 / §5.4). Walks states → matches →
    /// elements in the same order as `compile_stage_dfas`, so sids align.
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

    /// Emit one stage's embedding-aware matcher. Same greedy longest-match
    /// scan as `_dfa_match`, but firing the embedding actions at their DFA
    /// positions: `>{}` once at scan start, `${}` per consumed element,
    /// `@{}` on entering an accepting state, `%{}` on leaving one, and
    /// `@eof{}` at end-of-input while mid-match (non-accepting). `@@:cursor`
    /// reflects the scan position during firing; it is restored to the
    /// stage-entry position on return so the caller's slice/advance hold.
    fn emit_one_matcher(
        &self,
        out: &mut String,
        sid: usize,
        stage: &StageAst,
    ) -> Result<(), String> {
        writeln!(out, "    def _match_stage_{}(self):", sid).ok();
        writeln!(out, "        states, start = self._DFA_{}", sid).ok();
        writeln!(
            out,
            "        _entry = self.cursor\n\
             \x20       st = start\n\
             \x20       pos = _entry\n\
             \x20       n = len(self.{})\n\
             \x20       last = pos if states[st][1] else -1\n\
             \x20       self.cursor = pos",
            self.input_field()
        )
        .ok();
        // `>{}` — begin matching.
        out.push_str(&self.embed_bodies(stage, EmbeddingOp::Start, "        ")?);
        writeln!(
            out,
            "        prev = states[st][1]\n\
             \x20       while pos < n:\n\
             \x20           v = {}\n\
             \x20           nxt = None\n\
             \x20           for lo, hi, tgt in states[st][0]:\n\
             \x20               if lo <= v <= hi:\n\
             \x20                   nxt = tgt\n\
             \x20                   break\n\
             \x20           if nxt is None:\n\
             \x20               break\n\
             \x20           st = nxt\n\
             \x20           pos += 1\n\
             \x20           self.cursor = pos",
            self.element_read()
        )
        .ok();
        // `${}` — every consumed element.
        out.push_str(&self.embed_bodies(stage, EmbeddingOp::EveryTransition, "            ")?);
        out.push_str("            _now = states[st][1]\n");
        // `@{}` — a transition into an accepting state (§3.5.5: every
        // transition whose destination is final, not only the first entry).
        let accept = self.embed_bodies(stage, EmbeddingOp::Accept, "                ")?;
        if !accept.is_empty() {
            out.push_str("            if _now:\n");
            out.push_str(&accept);
        }
        // `%{}` — left an accepting state.
        let leave = self.embed_bodies(stage, EmbeddingOp::LeaveAccept, "                ")?;
        if !leave.is_empty() {
            out.push_str("            if prev and not _now:\n");
            out.push_str(&leave);
        }
        out.push_str(
            "            if _now:\n\
             \x20               last = pos\n\
             \x20           prev = _now\n",
        );
        // `@eof{}` — end of input reached while mid-match (non-accepting).
        let eof = self.embed_bodies(stage, EmbeddingOp::Eof, "            ")?;
        if !eof.is_empty() {
            out.push_str("        if pos >= n and not prev:\n");
            out.push_str(&eof);
        }
        out.push_str("        self.cursor = _entry\n        return last\n\n");
        Ok(())
    }

    /// Concatenated Python for every embedding-action body of `op` on
    /// `stage`, each statement prefixed with `ind`. Empty when the stage
    /// has no `op` action (so the caller can skip an empty guard block).
    fn embed_bodies(&self, stage: &StageAst, op: EmbeddingOp, ind: &str) -> Result<String, String> {
        let mut s = String::new();
        for ea in &stage.embedding_actions {
            if ea.op == op {
                for st in &ea.body.statements {
                    s.push_str(&stmt_to_py(st, ind)?);
                }
            }
        }
        Ok(s)
    }

    fn emit_run(&self, out: &mut String) {
        // `_enter` carries the element index a stage-ref transition
        // (`-> $State.stage`) re-enters at; it is consumed (reset to 0)
        // each step so plain transitions start at the state's first element.
        out.push_str(
            "    def _run(self):\n        state = 0\n        while state >= 0:\n\
             \x20           _enter = self._enter\n\
             \x20           self._enter = 0\n",
        );
        for i in 0..self.decl.states.len() {
            let kw = if i == 0 { "if" } else { "elif" };
            writeln!(
                out,
                "            {} state == {}:\n                state = self._state_{}(_enter)",
                kw, i, i
            )
            .ok();
        }
        // A target index out of range should never occur (validator E731),
        // but guard defensively.
        out.push_str("            else:\n                return\n\n");
    }

    fn emit_state_methods(&self, out: &mut String) -> Result<(), String> {
        let mut sid = 0usize; // running global stage-DFA index
        for (i, st) in self.decl.states.iter().enumerate() {
            match st.matches.len() {
                0 => {
                    // A stateless state just halts (terminal, no change to
                    // accepted). Rare; emit a no-op method.
                    writeln!(
                        out,
                        "    def _state_{}(self, _enter):\n        return -1\n",
                        i
                    )
                    .ok();
                }
                1 => self.emit_one_state(out, i, st, &st.matches[0], &mut sid)?,
                _ => self.emit_multi_match(out, i, st, &mut sid)?,
            }
        }
        Ok(())
    }

    /// Emit a multi-match (`|`) state as ordered choice (RFC-0042 §3.4).
    /// Each alternative's first stage is tried at the state-entry cursor;
    /// the first that matches commits and runs to its transition. A
    /// first-stage miss falls through to the next alternative (cursor
    /// unchanged); a *committed* alternative's later-stage failure follows
    /// that alternative's failure branch. A stageless alternative is an
    /// unconditional catch-all. If no alternative matches, the input is not
    /// in the language and recognition rejects (§5.6).
    ///
    /// `_enter` re-entry is not applied here (selection is by first stage);
    /// stage-ref targets into a multi-match state are not registered.
    fn emit_multi_match(
        &self,
        out: &mut String,
        index: usize,
        st: &FsmStateAst,
        sid: &mut usize,
    ) -> Result<(), String> {
        let state_label = st.label.clone().unwrap_or_default();
        writeln!(out, "    def _state_{}(self, _enter):", index).ok();

        for m in &st.matches {
            let first_stage = m
                .elements
                .iter()
                .position(|e| matches!(e, MatchElement::Stage(_)));
            match first_stage {
                Some(fs) => {
                    if fs > 0 {
                        // Elements before the first stage would run during
                        // selection (before this alternative is chosen),
                        // which has ambiguous side-effect semantics. Reject
                        // rather than silently drop or misorder them.
                        return Err(
                            "a `|` alternative with elements before its first stage is not yet \
                             supported by the Python backend"
                                .into(),
                        );
                    }
                    // Selector: try the first stage at the current cursor.
                    let my_sid = *sid;
                    *sid += 1;
                    if self.stage_dfas[my_sid].mode_c.is_some() {
                        return Err(
                            "a Mode C (`/@Fsm/`) stage as a `|` alternative selector is not yet \
                             supported by the Python backend"
                                .into(),
                        );
                    }
                    let MatchElement::Stage(sel) = &m.elements[fs] else {
                        unreachable!("first_stage indexes a Stage element")
                    };
                    writeln!(out, "        _r = {}", stage_call(sel, my_sid)).ok();
                    self.emit_anchor_guards(out, my_sid, "        ");
                    out.push_str("        if _r >= 0:\n");
                    // Committed: record the first stage's capture + advance.
                    writeln!(
                        out,
                        "            self._matched = self.{}[self.cursor:_r]",
                        self.input_field()
                    )
                    .ok();
                    if let MatchElement::Stage(stage) = &m.elements[fs] {
                        if let Some(slabel) = &stage.label {
                            writeln!(
                                out,
                                "            self._cap[{:?}] = self._matched",
                                format!("{}.{}", state_label, slabel)
                            )
                            .ok();
                        }
                    }
                    out.push_str("            self.cursor = _r\n");
                    out.push_str("            self.accepted = True\n");
                    // Remaining elements run inside the commit; a later stage
                    // failure follows this alternative's failure branch.
                    for el in &m.elements[fs + 1..] {
                        self.emit_element(out, el, m, &state_label, "            ", sid)?;
                    }
                    self.emit_success(out, m, "            ")?;
                    // First-stage miss: fall through to the next alternative.
                }
                None => {
                    // Stageless alternative: an unconditional catch-all. It
                    // matches with no stage, so it accepts (a prior selector
                    // miss leaves `accepted` untouched, unlike single-match).
                    out.push_str("        self.accepted = True\n");
                    for el in &m.elements {
                        self.emit_element(out, el, m, &state_label, "        ", sid)?;
                    }
                    self.emit_success(out, m, "        ")?;
                }
            }
        }

        // No alternative's first stage matched: not in the language (§5.6).
        out.push_str("        self.accepted = False\n");
        out.push_str("        self.reject_position = self.cursor\n");
        out.push_str("        return -1\n\n");
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
        let state_label = st.label.clone().unwrap_or_default();
        writeln!(out, "    def _state_{}(self, _enter):", index).ok();

        // Each element is guarded by `if _enter <= <idx>:` so a stage-ref
        // re-entry (`-> $State.stage`) skips the leading elements. A plain
        // entry has `_enter == 0`, so every guard passes.
        for (idx, el) in m.elements.iter().enumerate() {
            writeln!(out, "        if _enter <= {}:", idx).ok();
            self.emit_element(out, el, m, &state_label, "            ", sid)?;
        }

        // All elements succeeded: follow the success branch (or halt at a
        // terminal). `accepted` already reflects the last stage.
        self.emit_success(out, m, "        ")?;
        out.push('\n');
        Ok(())
    }

    /// After a stage's match result `_r` is computed, enforce its boundary
    /// anchors (§6.6): a leading `^`/`\A` requires the match to start at the
    /// input start (cursor 0); a trailing `$`/`\z` requires it to end at the
    /// input end. A violated anchor turns the match into a miss (`_r = -1`).
    fn emit_anchor_guards(&self, out: &mut String, sid: usize, ind: &str) {
        let dfa = &self.stage_dfas[sid];
        if dfa.requires_start {
            writeln!(out, "{ind}if self.cursor != 0:\n{ind}    _r = -1").ok();
        }
        if dfa.requires_end {
            writeln!(
                out,
                "{ind}if _r != len(self.{}):\n{ind}    _r = -1",
                self.input_field()
            )
            .ok();
        }
    }

    /// Emit one match element at `ind`. A stage runs its DFA, routes to the
    /// failure branch on no-match, and records the capture / advances the
    /// cursor on success. `ind4` is `ind` + 4 spaces (the failure block).
    fn emit_element(
        &self,
        out: &mut String,
        el: &MatchElement,
        m: &MatchAst,
        state_label: &str,
        ind: &str,
        sid: &mut usize,
    ) -> Result<(), String> {
        let ind4 = format!("{}    ", ind);
        match el {
            MatchElement::Stage(stage) => {
                let my_sid = *sid;
                *sid += 1;
                if let Some(inner) = self.stage_dfas[my_sid].mode_c.clone() {
                    self.emit_mode_c(out, &inner, stage, m, state_label, ind, &ind4)?;
                    return Ok(());
                }
                writeln!(out, "{}_r = {}", ind, stage_call(stage, my_sid)).ok();
                self.emit_anchor_guards(out, my_sid, ind);
                writeln!(out, "{}if _r < 0:", ind).ok();
                // The stage failed: follow the failure branch (or §5.6).
                // emit_failure records the rejection (accepted=False).
                self.emit_failure(out, m, &ind4)?;
                writeln!(
                    out,
                    "{}self._matched = self.{}[self.cursor:_r]",
                    ind,
                    self.input_field()
                )
                .ok();
                if let Some(slabel) = &stage.label {
                    writeln!(
                        out,
                        "{}self._cap[{:?}] = self._matched",
                        ind,
                        format!("{}.{}", state_label, slabel)
                    )
                    .ok();
                }
                writeln!(out, "{}self.cursor = _r", ind).ok();
                writeln!(out, "{}self.accepted = True", ind).ok();
            }
            MatchElement::BareExpression { expr, .. } => {
                writeln!(out, "{}self.return_value = {}", ind, expr_to_py(expr)).ok();
            }
            MatchElement::ActionBlock(blk) => {
                // Action blocks consume no input and (in v0.1) cannot fail.
                for st in &blk.statements {
                    out.push_str(&stmt_to_py(st, ind)?);
                }
            }
        }
        Ok(())
    }

    /// Emit a Mode C stage (RFC-0042 §8.3 `/@Inner/`): construct the inner
    /// fsm over the input at the cursor, run it to completion, and on accept
    /// advance the outer cursor by the inner's. The stage fails (failure
    /// branch / §5.6) when the inner rejects. A labeled stage records both
    /// the matched slice (`$state.label`) and the inner instance
    /// (`$state.label.return_value`).
    #[allow(clippy::too_many_arguments)]
    fn emit_mode_c(
        &self,
        out: &mut String,
        inner: &str,
        stage: &StageAst,
        m: &MatchAst,
        state_label: &str,
        ind: &str,
        ind4: &str,
    ) -> Result<(), String> {
        let input = self.input_field();
        writeln!(
            out,
            "{}_inner = {}(self.{}[self.cursor:])",
            ind, inner, input
        )
        .ok();
        writeln!(out, "{}if not _inner.accepted:", ind).ok();
        self.emit_failure(out, m, ind4)?;
        writeln!(
            out,
            "{}self._matched = self.{}[self.cursor:self.cursor + _inner.cursor]",
            ind, input
        )
        .ok();
        if let Some(slabel) = &stage.label {
            let key = format!("{}.{}", state_label, slabel);
            writeln!(out, "{}self._cap[{:?}] = self._matched", ind, key).ok();
            writeln!(out, "{}self._cap_inst[{:?}] = _inner", ind, key).ok();
        }
        writeln!(out, "{}self.cursor = self.cursor + _inner.cursor", ind).ok();
        writeln!(out, "{}self.accepted = True", ind).ok();
        Ok(())
    }

    /// Emit the success-branch transition after a match completes. A
    /// static target returns its index; a conditional target evaluates
    /// each `when` in order and, if none holds, the failure branch fires
    /// (FSM-TEST-402). No transition halts (terminal, accepted stands).
    fn emit_success(&self, out: &mut String, m: &MatchAst, indent: &str) -> Result<(), String> {
        match m.transition.as_ref().and_then(|c| c.success.as_ref()) {
            // No transition, or a failure-only clause: the success path is
            // the implicit-terminal match — halt (accepted stands).
            None => {
                writeln!(out, "{}return -1", indent).ok();
                Ok(())
            }
            Some(success) => self.emit_target(out, success, indent, &|out, indent| {
                // No success condition held → the failure branch fires.
                self.emit_failure(out, m, indent)
            }),
        }
    }

    /// Emit the failure-branch resolution: the failure target's transition
    /// if present, else §5.6 (halt). Used both on a stage failure and as
    /// the fallback when a conditional success matches no `when`.
    ///
    /// Per the RE2 recognizer model (§5.3), reaching here is a rejection
    /// event: it records `accepted = False` and the reject position. If the
    /// failure branch routes to a non-terminal state that later completes a
    /// match, a subsequent stage success flips `accepted` back to True.
    fn emit_failure(&self, out: &mut String, m: &MatchAst, indent: &str) -> Result<(), String> {
        writeln!(out, "{}self.accepted = False", indent).ok();
        writeln!(out, "{}self.reject_position = self.cursor", indent).ok();
        match m.transition.as_ref().and_then(|c| c.failure.as_ref()) {
            None => {
                writeln!(out, "{}return -1", indent).ok();
                Ok(())
            }
            Some(target) => self.emit_target(out, target, indent, &|out, indent| {
                writeln!(out, "{}return -1", indent).ok();
                Ok(())
            }),
        }
    }

    /// Emit `return <index>` for a target. A conditional target emits an
    /// ordered `if <when>: return <idx>` chain, then `on_none` for the
    /// no-match case.
    fn emit_target(
        &self,
        out: &mut String,
        target: &FsmTransitionTarget,
        indent: &str,
        on_none: &dyn Fn(&mut String, &str) -> Result<(), String>,
    ) -> Result<(), String> {
        match target {
            FsmTransitionTarget::Static { .. } => self.emit_goto(out, target, indent),
            FsmTransitionTarget::Conditional(alts) => {
                let inner = format!("{}    ", indent);
                for alt in alts {
                    writeln!(out, "{}if {}:", indent, expr_to_py(&alt.condition)).ok();
                    self.emit_goto(out, &alt.target, &inner)?;
                }
                on_none(out, indent)
            }
        }
    }

    /// Emit the dispatch jump for a static target: `return <state_index>`.
    /// A stage-ref target (`$State.stage`) also sets `self._enter` to the
    /// stage's element index so the state re-enters mid-match.
    fn emit_goto(
        &self,
        out: &mut String,
        t: &FsmTransitionTarget,
        indent: &str,
    ) -> Result<(), String> {
        match t {
            FsmTransitionTarget::Static {
                state, stage: None, ..
            } => {
                let idx = self
                    .label_to_index
                    .get(state)
                    .copied()
                    .ok_or_else(|| format!("transition to undeclared state `${}`", state))?;
                writeln!(out, "{}return {}", indent, idx).ok();
                Ok(())
            }
            FsmTransitionTarget::Static {
                state,
                stage: Some(s),
                ..
            } => {
                let idx = self
                    .label_to_index
                    .get(state)
                    .copied()
                    .ok_or_else(|| format!("transition to undeclared state `${}`", state))?;
                let entry = self
                    .stage_entry
                    .get(&(state.clone(), s.clone()))
                    .copied()
                    .ok_or_else(|| format!("transition to undeclared stage `${}.{}`", state, s))?;
                writeln!(out, "{}self._enter = {}", indent, entry).ok();
                writeln!(out, "{}return {}", indent, idx).ok();
                Ok(())
            }
            FsmTransitionTarget::Conditional(_) => {
                Err("a conditional target may not nest another conditional target".into())
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Statement translation (action-block bodies, RFC-0043)
// ---------------------------------------------------------------------------

/// Render a statement as Python source lines, each prefixed with `indent`.
/// Action-block statements are expression statements (incl. assignments)
/// and `if/else`; transitions are not valid here (E712, caught earlier).
fn stmt_to_py(stmt: &Statement, indent: &str) -> Result<String, String> {
    match stmt {
        Statement::Expression(e) => Ok(format!("{}{}\n", indent, expr_to_py(&e.expr))),
        Statement::If(if_ast) => {
            let inner = format!("{}    ", indent);
            let mut s = format!("{}if {}:\n", indent, expr_to_py(&if_ast.condition));
            s.push_str(&stmt_to_py(&if_ast.then_branch, &inner)?);
            if let Some(else_b) = &if_ast.else_branch {
                s.push_str(&format!("{}else:\n", indent));
                s.push_str(&stmt_to_py(else_b, &inner)?);
            }
            Ok(s)
        }
        Statement::Block(blk) => {
            if blk.statements.is_empty() {
                return Ok(format!("{}pass\n", indent));
            }
            let mut s = String::new();
            for st in &blk.statements {
                s.push_str(&stmt_to_py(st, indent)?);
            }
            Ok(s)
        }
        other => Err(format!(
            "statement form {:?} is not supported in @@fsm action blocks by the Python backend",
            std::mem::discriminant(other)
        )),
    }
}

// ---------------------------------------------------------------------------
// Expression translation
// ---------------------------------------------------------------------------

/// Translate a Frame expression to a Python expression string.
fn expr_to_py(e: &Expression) -> String {
    match e {
        Expression::Literal(l) => literal_to_py(l),
        Expression::Var(name) => var_to_py(name),
        Expression::Binary { left, op, right } => {
            format!(
                "({} {} {})",
                expr_to_py(left),
                binop_to_py(op),
                expr_to_py(right)
            )
        }
        Expression::Unary { op, expr } => match op {
            UnaryOp::Not => format!("(not {})", expr_to_py(expr)),
            UnaryOp::Neg => format!("(-{})", expr_to_py(expr)),
            UnaryOp::BitNot => format!("(~{})", expr_to_py(expr)),
        },
        Expression::Call { func, args } => call_to_py(func, args),
        Expression::Member { object, field } => {
            // Mode C (§8.3): `$state.label.<fsm field>` reads the inner fsm
            // instance recorded for that stage, not the matched slice.
            // (`$state.label` alone is the slice — handled by `var_to_py`.)
            if let Expression::Var(name) = object.as_ref() {
                if let Some(cap) = name.strip_prefix('$') {
                    if matches!(
                        field.as_str(),
                        "return_value" | "accepted" | "cursor" | "reject_position"
                    ) {
                        return format!("self._cap_inst[{:?}].{}", cap, field);
                    }
                }
            }
            // `self.field` stays `self.field`; nested members chain.
            format!("{}.{}", expr_to_py(object), field)
        }
        Expression::Index { object, index } => {
            format!("{}[{}]", expr_to_py(object), expr_to_py(index))
        }
        Expression::Assign { target, value } => {
            format!("{} = {}", expr_to_py(target), expr_to_py(value))
        }
        Expression::NativeExpr(s) => s.clone(),
    }
}

fn literal_to_py(l: &Literal) -> String {
    match l {
        Literal::Int(i) => i.to_string(),
        Literal::Float(f) => f.to_string(),
        Literal::String(s) => format!("{:?}", s),
        Literal::Bool(b) => py_bool(*b).to_string(),
        Literal::Null => "None".to_string(),
    }
}

/// Variable references include the `@@:` context probes and `$state.stage`
/// captures, which map to recognition scratch.
fn var_to_py(name: &str) -> String {
    match name {
        "@@:matched" => "self._matched".to_string(),
        "@@:cursor" => "self.cursor".to_string(),
        "@@:return" => "self.return_value".to_string(),
        _ => {
            if let Some(cap) = name.strip_prefix('$') {
                // `$state.stage` capture reference.
                format!("self._cap[{:?}]", cap)
            } else {
                name.to_string()
            }
        }
    }
}

fn call_to_py(func: &str, args: &[Expression]) -> String {
    let rendered: Vec<String> = args.iter().map(expr_to_py).collect();
    match func {
        // RFC-0042 built-ins.
        "to_int" => format!("_frame_to_int({})", rendered.join(", ")),
        "to_str" => format!("_frame_to_str({})", rendered.join(", ")),
        "len" => format!("_frame_len({})", rendered.join(", ")),
        // Any other call names a declared `actions:` helper, emitted as a
        // method on the recognizer instance.
        _ => format!("self.{}({})", func, rendered.join(", ")),
    }
}

fn binop_to_py(op: &BinaryOp) -> &'static str {
    match op {
        BinaryOp::Add => "+",
        BinaryOp::Sub => "-",
        BinaryOp::Mul => "*",
        BinaryOp::Div => "//",
        BinaryOp::Mod => "%",
        BinaryOp::Eq => "==",
        BinaryOp::Ne => "!=",
        BinaryOp::Lt => "<",
        BinaryOp::Le => "<=",
        BinaryOp::Gt => ">",
        BinaryOp::Ge => ">=",
        BinaryOp::And => "and",
        BinaryOp::Or => "or",
        BinaryOp::BitAnd => "&",
        BinaryOp::BitOr => "|",
        BinaryOp::BitXor => "^",
    }
}

fn py_bool(b: bool) -> &'static str {
    if b {
        "True"
    } else {
        "False"
    }
}

/// The matcher invocation for a stage: the shared `_dfa_match` for a
/// plain stage, or its specialized `_match_stage_<sid>` when the stage
/// carries embedding actions that must fire during the scan.
fn stage_call(stage: &StageAst, sid: usize) -> String {
    if stage.embedding_actions.is_empty() {
        format!("self._dfa_match(self._DFA_{})", sid)
    } else {
        format!("self._match_stage_{}()", sid)
    }
}

/// Map a raw default-value token (from the parser) to Python source.
fn py_default(raw: &str) -> String {
    match raw {
        "false" => "False".to_string(),
        "true" => "True".to_string(),
        "null" | "nil" | "None" => "None".to_string(),
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frame_c::compiler::fsm_parser::parse_fsm_block;
    use std::process::Command;

    /// One observed run of a generated fsm: the four observable fields,
    /// as their Python `repr`/`str` text.
    struct Run {
        accepted: String,
        return_value: String,
        cursor: String,
        reject: String,
    }

    /// Generate Python for `src`, run it on `input` via `python3`, and
    /// return the observed fields. Returns `None` if `python3` is absent
    /// (so the test self-skips in environments without it).
    fn run(src: &str, input: &str, tag: &str) -> Option<Run> {
        let decl = parse_fsm_block(src.as_bytes()).expect("fixture must parse");
        let code = generate(&decl).expect("fixture must generate");
        let driver = format!(
            "{code}\nimport sys\nm = {name}(sys.argv[1])\n\
             print(m.accepted)\nprint(repr(m.return_value))\n\
             print(m.cursor)\nprint(m.reject_position)\n",
            code = code,
            name = decl.name
        );
        let path = std::env::temp_dir().join(format!("framec_fsm_{}.py", tag));
        std::fs::write(&path, driver).expect("write temp py");

        let out = match Command::new("python3").arg(&path).arg(input).output() {
            Ok(o) => o,
            Err(_) => return None, // python3 not available — skip
        };
        assert!(
            out.status.success(),
            "python3 failed for {:?} on {:?}: {}",
            src,
            input,
            String::from_utf8_lossy(&out.stderr)
        );
        let text = String::from_utf8_lossy(&out.stdout);
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 4, "unexpected output: {:?}", text);
        Some(Run {
            accepted: lines[0].to_string(),
            return_value: lines[1].to_string(),
            cursor: lines[2].to_string(),
            reject: lines[3].to_string(),
        })
    }

    /// Generate + run `src` on `input`, returning the `repr` of each
    /// requested instance expression (e.g. `"m.return_value"`, `"m.flag"`).
    /// `None` if python3 is unavailable.
    fn eval_py(src: &str, input: &str, exprs: &[&str], tag: &str) -> Option<Vec<String>> {
        let decl = parse_fsm_block(src.as_bytes()).expect("fixture must parse");
        let code = generate(&decl).expect("fixture must generate");
        let prints: String = exprs
            .iter()
            .map(|e| format!("print(repr({}))\n", e))
            .collect();
        let driver = format!(
            "{code}\nimport sys\nm = {name}(sys.argv[1])\n{prints}",
            code = code,
            name = decl.name,
            prints = prints
        );
        let path = std::env::temp_dir().join(format!("framec_fsm_{}.py", tag));
        std::fs::write(&path, driver).expect("write temp py");
        let out = match Command::new("python3").arg(&path).arg(input).output() {
            Ok(o) => o,
            Err(_) => return None,
        };
        assert!(
            out.status.success(),
            "python3 failed for {:?}: {}",
            src,
            String::from_utf8_lossy(&out.stderr)
        );
        Some(
            String::from_utf8_lossy(&out.stdout)
                .lines()
                .map(|l| l.to_string())
                .collect(),
        )
    }

    /// FSM-TEST-030 — multi-statement action block (`;`-separated),
    /// mutating domain fields.
    #[test]
    fn fsm_test_030_action_block_semicolons() {
        let src = "@@fsm M(text: bytes) : int = 0 { \
                   /[0-9]/ { self.count = self.count + 1; self.flag = true } \
                   self.count \
                   domain: count: int = 0  flag: bool = false }";
        let Some(v) = eval_py(src, "5", &["m.return_value", "m.flag", "m.count"], "t030") else {
            return;
        };
        assert_eq!(v, vec!["1", "True", "1"]);
    }

    /// FSM-TEST-031 — same logic, whitespace-separated statements.
    #[test]
    fn fsm_test_031_action_block_whitespace() {
        let src = "@@fsm M(text: bytes) : int = 0 { \
                   /[0-9]/ { self.count = self.count + 1  self.flag = true } \
                   self.count \
                   domain: count: int = 0  flag: bool = false }";
        let Some(v) = eval_py(src, "5", &["m.return_value", "m.flag", "m.count"], "t031") else {
            return;
        };
        assert_eq!(v, vec!["1", "True", "1"]);
    }

    /// FSM-TEST-032 — if/else in an action block.
    #[test]
    fn fsm_test_032_if_else() {
        let src = "@@fsm M(text: bytes) : int = 0 { \
                   /[0-9]/ { if to_int(@@:matched) > 5 { self.flag = true } else { self.flag = false } } \
                   to_int(@@:matched) \
                   domain: flag: bool = false }";
        let Some(seven) = eval_py(src, "7", &["m.return_value", "m.flag"], "t032a") else {
            return;
        };
        assert_eq!(seven, vec!["7", "True"]);
        let three = eval_py(src, "3", &["m.return_value", "m.flag"], "t032b").unwrap();
        assert_eq!(three, vec!["3", "False"]);
    }

    /// FSM-TEST-253 — token alphabet: the input is a sequence of token
    /// kinds; regex identifiers reference token kinds, not characters.
    #[test]
    fn fsm_test_253_token_alphabet() {
        let src = "@@fsm M(toks: token) : bool = false { /IDENT LPAREN RPAREN/ true }";
        let decl = parse_fsm_block(src.as_bytes()).expect("parses");
        let code = generate(&decl).expect("generates");
        let run_toks = |toks: &str, tag: &str| -> Option<String> {
            let driver = format!(
                "{code}\nm = M([{toks}])\nprint(repr(m.accepted))\n",
                code = code
            );
            let path = std::env::temp_dir().join(format!("framec_fsm_{}.py", tag));
            std::fs::write(&path, driver).ok()?;
            let out = Command::new("python3").arg(&path).output().ok()?;
            assert!(
                out.status.success(),
                "{}",
                String::from_utf8_lossy(&out.stderr)
            );
            Some(String::from_utf8_lossy(&out.stdout).trim().to_string())
        };
        let Some(ok) = run_toks("\"IDENT\", \"LPAREN\", \"RPAREN\"", "t253a") else {
            return;
        };
        assert_eq!(ok, "True");
        // Wrong token sequence → not in the language.
        assert_eq!(run_toks("\"IDENT\", \"IDENT\"", "t253b").unwrap(), "False");
        // An unknown token kind never matches a transition.
        assert_eq!(
            run_toks("\"WAT\", \"LPAREN\", \"RPAREN\"", "t253c").unwrap(),
            "False"
        );
    }

    /// FSM-TEST-120 — declared action callable from a match, with params
    /// and a return value propagated to `@@:return`.
    #[test]
    fn fsm_test_120_action_callable() {
        let src = "@@fsm M(text: bytes) : int = 0 { \
                   /[0-9]+/ parse_int(@@:matched) \
                   actions: parse_int(s: bytes): int { to_int(s) } }";
        let Some(v) = eval_py(src, "42", &["m.return_value"], "t120") else {
            return;
        };
        assert_eq!(v, vec!["42"]);
    }

    /// FSM-TEST-121 — action reads/writes a domain field; its side effect
    /// persists into the bare-expression return.
    #[test]
    fn fsm_test_121_action_domain_access() {
        let src = "@@fsm M(text: bytes) : int = 0 { \
                   /[0-9]/ { increment() } \
                   self.count \
                   actions: increment() { self.count = self.count + 1 } \
                   domain: count: int = 0 }";
        let Some(v) = eval_py(src, "5", &["m.return_value", "m.count"], "t121") else {
            return;
        };
        assert_eq!(v, vec!["1", "1"]);
    }

    /// FSM-TEST-001 — the smoke test. Also exercises FSM-TEST-1000 (empty
    /// input rejects at position 0), FSM-TEST-1001 (input exactly matching:
    /// accepts, cursor at end), and FSM-TEST-203 (a bare-expression match with
    /// no transition is an implicit-terminal accepting match).
    #[test]
    fn fsm_test_001_minimal() {
        let src = "@@fsm M(text: bytes) : bool = false { /a/ true }";
        let Some(a) = run(src, "a", "t001a") else {
            return;
        };
        assert_eq!(a.accepted, "True");
        assert_eq!(a.return_value, "True");
        assert_eq!(a.cursor, "1");

        let b = run(src, "b", "t001b").unwrap();
        assert_eq!(b.accepted, "False");
        assert_eq!(b.return_value, "False");
        assert_eq!(b.reject, "0");

        let empty = run(src, "", "t001e").unwrap();
        assert_eq!(empty.accepted, "False");
    }

    /// FSM-TEST-002 — single-digit, `to_int(@@:matched)`. Also FSM-TEST-501 —
    /// construction with no match (`"a"` against `/[0-9]/`) yields
    /// `accepted == false` with `reject_position == 0`.
    #[test]
    fn fsm_test_002_matched_builtin() {
        let src = "@@fsm M(text: bytes) : int = 0 { /[0-9]/ to_int(@@:matched) }";
        let seven = run(src, "7", "t002a").unwrap_or_else(no_py);
        if seven.accepted.is_empty() {
            return;
        }
        assert_eq!(seven.accepted, "True");
        assert_eq!(seven.return_value, "7");
        let a = run(src, "a", "t002b").unwrap();
        assert_eq!(a.accepted, "False");
        assert_eq!(a.reject, "0");
    }

    /// FSM-TEST-005 — `len(self.text)` is the full input length. Also
    /// FSM-TEST-504 — the auto-promoted input parameter is accessible as
    /// `self.<name>` (distinct from `@@:matched`).
    #[test]
    fn fsm_test_005_self_text() {
        let src = "@@fsm M(text: bytes) : int = 0 { /[0-9]+/ len(self.text) }";
        let Some(r) = run(src, "123", "t005a") else {
            return;
        };
        assert_eq!(r.return_value, "3");
        assert_eq!(r.accepted, "True");
        let r2 = run(src, "123456789", "t005b").unwrap();
        assert_eq!(r2.return_value, "9");
    }

    /// FSM-TEST-006 — labeled states, success + failure transitions.
    /// Reaching `$error` via a failure branch is `accepted == false`. Also
    /// FSM-TEST-500 — construction with a full match populates `accepted` and
    /// `return_value` (here `42` from a two-state match).
    #[test]
    fn fsm_test_006_transitions() {
        let src = "@@fsm M(text: bytes) : int = 0 { \
                   $0: /[a-z]/ -> $digits : -> $error \
                   $digits: .n/[0-9]+/ to_int($digits.n) \
                   $error: -1 }";
        let Some(ok) = run(src, "x42", "t006a") else {
            return;
        };
        assert_eq!(ok.accepted, "True");
        assert_eq!(ok.return_value, "42");

        let big = run(src, "X", "t006b").unwrap();
        assert_eq!(
            big.accepted, "False",
            "failure-branch terminal is not accepted"
        );
        assert_eq!(big.return_value, "-1");

        let dig = run(src, "3", "t006c").unwrap();
        assert_eq!(dig.accepted, "False");
        assert_eq!(dig.return_value, "-1");
    }

    /// FSM-TEST-007 — stage label capture, anchored-prefix match. Also
    /// FSM-TEST-503 (the labeled stage's capture holds the matched bytes,
    /// `'123'`), FSM-TEST-1002 (input longer than the match: trailing `abc`
    /// left unconsumed), and FSM-TEST-502 (the cursor advances to 3, the number
    /// of consumed bytes).
    #[test]
    fn fsm_test_007_capture() {
        let src = "@@fsm M(text: bytes) : bytes = \"\" { $main: .x/[0-9]+/ $main.x }";
        let Some(r) = run(src, "123", "t007a") else {
            return;
        };
        assert_eq!(r.return_value, "'123'");
        let r2 = run(src, "123abc", "t007b").unwrap();
        assert_eq!(r2.return_value, "'123'");
        assert_eq!(r2.cursor, "3");
        let r3 = run(src, "abc", "t007c").unwrap();
        assert_eq!(r3.accepted, "False");
    }

    /// FSM-TEST-402 — conditional transition target; first true `when`
    /// wins, falling through all conditions fires the failure branch.
    #[test]
    fn fsm_test_402_conditional_target() {
        let src = "@@fsm M(text: bytes, mode: int) : int = 0 { \
                   /[01]/ -> ( $zero when self.mode == 0, $one when self.mode == 1 ) : -> $error \
                   $zero: 0 \
                   $one: 1 \
                   $error: -1 }";
        // M takes two args; the driver passes mode as a second arg.
        let decl = parse_fsm_block(src.as_bytes()).expect("parses");
        let code = generate(&decl).expect("generates");
        let run_mode = |inp: &str, mode: &str, tag: &str| -> Option<String> {
            let driver = format!(
                "{code}\nimport sys\nm = M(sys.argv[1], int(sys.argv[2]))\nprint(repr(m.return_value))\n",
                code = code
            );
            let path = std::env::temp_dir().join(format!("framec_fsm_{}.py", tag));
            std::fs::write(&path, driver).ok()?;
            let out = Command::new("python3")
                .arg(&path)
                .arg(inp)
                .arg(mode)
                .output()
                .ok()?;
            assert!(
                out.status.success(),
                "{}",
                String::from_utf8_lossy(&out.stderr)
            );
            Some(String::from_utf8_lossy(&out.stdout).trim().to_string())
        };
        let Some(zero) = run_mode("0", "0", "t402a") else {
            return;
        };
        assert_eq!(zero, "0");
        assert_eq!(run_mode("1", "1", "t402b").unwrap(), "1");
        assert_eq!(run_mode("0", "2", "t402c").unwrap(), "-1"); // no when matches → failure
    }

    /// FSM-TEST-400 — static transitions across multiple states, success +
    /// failure branches on both initial and intermediate states. The
    /// intermediate state uses a failure-only clause (`/b/ true : -> $error`).
    /// Also FSM-TEST-202 — an explicit failure branch routes a failed stage
    /// (`"ax"`) to the declared `$error` target.
    #[test]
    fn fsm_test_400_static_transitions() {
        let src = "@@fsm M(text: bytes) : bool = false { \
                   /a/ -> $next : -> $error \
                   $next: /b/ true : -> $error \
                   $error: false }";
        let Some(ab) = run(src, "ab", "t400a") else {
            return;
        };
        assert_eq!(ab.accepted, "True");
        assert_eq!(ab.return_value, "True");
        let ax = run(src, "ax", "t400b").unwrap();
        assert_eq!(ax.accepted, "False");
        assert_eq!(ax.return_value, "False");
        assert_eq!(run(src, "x", "t400c").unwrap().accepted, "False");
    }

    /// FSM-TEST-401 — stage-address transition target: a transition names
    /// a labeled stage within a state and re-enters there.
    ///
    /// NOTE: the RFC fixture asserts `"xabc"` reaches `$other`, but `$0` is
    /// the start state (§3.4) and nothing transitions *to* `$other`, so it
    /// is unreachable — `"xabc"` starts at `$0`, fails `/a/` on `x`, and
    /// routes to `$error`. We assert the correct semantics; the stage-ref
    /// re-entry mechanism is exercised by `stage_ref_skips_leading_stage`.
    #[test]
    fn fsm_test_401_stage_ref_target() {
        let src = "@@fsm M(text: bytes) : bool = false { \
                   $0: .start/a/ /b/ /c/ true : -> $error \
                   $other: /x/ -> $0.start : -> $error \
                   $error: false }";
        let Some(abc) = run(src, "abc", "t401a") else {
            return;
        };
        assert_eq!(abc.accepted, "True");
        assert_eq!(abc.return_value, "True");
        // `$0` is the start state; `$other` is unreachable.
        let xabc = run(src, "xabc", "t401b").unwrap();
        assert_eq!(xabc.accepted, "False");
    }

    /// Stage-ref re-entry that genuinely skips a leading stage: entering
    /// `$s.rest` runs only the `.rest` element, not the prior one.
    #[test]
    fn stage_ref_skips_leading_stage() {
        let src = "@@fsm M(text: bytes) : bool = false { \
                   /x/ -> $s.rest : -> $err \
                   $s: /a/ .rest/b/ true : -> $err \
                   $err: false }";
        // "xb": $0 matches x -> $s.rest, which runs only /b/ (skipping /a/).
        let Some(xb) = run(src, "xb", "tsr_a") else {
            return;
        };
        assert_eq!(xb.accepted, "True");
        // "xab" would NOT match: re-entry at .rest expects /b/ at the cursor
        // after x, but sees 'a'.
        assert_eq!(run(src, "xab", "tsr_b").unwrap().accepted, "False");
    }

    /// FSM-TEST-123 — `${...}` embedding action fires once per consumed
    /// element; a declared action is callable from inside it.
    #[test]
    fn fsm_test_123_every_transition_embed() {
        let src = "@@fsm M(text: bytes) : int = 0 { \
                   /[0-9]+/ ${ tally() } \
                   self.count \
                   actions: tally() { self.count = self.count + 1 } \
                   domain: count: int = 0 }";
        let Some(v) = eval_py(src, "123", &["m.return_value"], "t123") else {
            return;
        };
        assert_eq!(v, vec!["3"]); // three digits → ${} fires 3×
    }

    /// FSM-TEST-600 — entry and per-element actions. `>{...}` fires once at the
    /// start of the stage's scan; `@@:cursor` there is the stage-entry position.
    /// (The per-element `${...}` half is FSM-TEST-123.)
    #[test]
    fn embed_start_captures_entry_cursor() {
        let src = "@@fsm M(text: bytes) : int = 0 { \
                   /x/ /[0-9]+/ >{ self.start = @@:cursor } self.start \
                   domain: start: int = -1 }";
        // "x42": first stage /x/ consumes x (cursor->1); second stage's
        // `>{}` fires at scan start, capturing cursor == 1.
        let Some(v) = eval_py(src, "x42", &["m.return_value"], "tes_a") else {
            return;
        };
        assert_eq!(v, vec!["1"]);
    }

    /// FSM-TEST-601 — final-state action. `@{...}` fires when the DFA enters an
    /// accepting state; for `/a+/` over "aaa" that is once per `a` (each prefix
    /// is accepting).
    #[test]
    fn embed_accept_fires_on_accepting_states() {
        let src = "@@fsm M(text: bytes) : int = 0 { \
                   /a+/ @{ self.hits = self.hits + 1 } self.hits \
                   domain: hits: int = 0 }";
        let Some(v) = eval_py(src, "aaa", &["m.return_value"], "tea_a") else {
            return;
        };
        // Each of the three `a` transitions lands in the accepting state,
        // so `@{}` (transition into a final state, §3.5.5) fires 3 times.
        assert_eq!(v, vec!["3"]);
    }

    /// FSM-TEST-602 — EOF action. `@eof{...}` fires when end-of-input is
    /// reached while a stage is mid-match (non-accepting). "fo" ends inside
    /// `/foo/`; the full "foo" completes without firing it.
    #[test]
    fn fsm_test_602_eof_action() {
        let src = "@@fsm M(text: bytes) : int = 0 { \
                   /foo/ @eof{ self.eofhit = self.eofhit + 1 } : -> $reject \
                   self.eofhit \
                   $reject: self.eofhit \
                   domain: eofhit: int = 0 }";
        let Some(partial) = eval_py(src, "fo", &["m.eofhit"], "t602a") else {
            return;
        };
        assert_eq!(
            partial,
            vec!["1"],
            "@eof fires once when input ends mid-match"
        );
        let full = eval_py(src, "foo", &["m.eofhit"], "t602b").unwrap();
        assert_eq!(
            full,
            vec!["0"],
            "@eof does not fire when the stage completes"
        );
    }

    /// FSM-TEST-1003 — an input that is a strict prefix of the required match
    /// is rejected (the match cannot complete). "a" against `/ab/`.
    #[test]
    fn fsm_test_1003_prefix_of_match_rejected() {
        let src = "@@fsm M(text: bytes) : bool = false { /ab/ true }";
        let Some(a) = run(src, "a", "t1003a") else {
            return;
        };
        assert_eq!(a.accepted, "False");
        assert_eq!(run(src, "ab", "t1003b").unwrap().accepted, "True");
    }

    /// FSM-TEST-1005 — a zero-length match (`/a*/` against "bbb") succeeds by
    /// matching the empty string: accepted, cursor stays at 0.
    #[test]
    fn fsm_test_1005_zero_length_match() {
        let src = "@@fsm M(text: bytes) : bool = true { /a*/ true }";
        let Some(r) = run(src, "bbb", "t1005a") else {
            return;
        };
        assert_eq!(r.accepted, "True");
        assert_eq!(r.cursor, "0");
    }

    /// FSM-TEST-1006 — `@@:matched` before any stage has completed in the
    /// current match is the empty slice.
    #[test]
    fn fsm_test_1006_matched_before_stage() {
        let src = "@@fsm M(text: bytes) : bytes = \"\" { @@:matched }";
        let Some(v) = eval_py(src, "abc", &["m.return_value"], "t1006a") else {
            return;
        };
        assert_eq!(v, vec!["''"]); // empty matched slice — no stage has matched yet
    }

    /// FSM-TEST-304 — alternation is the loosest operator: `foo|bar baz`
    /// parses as `foo | (bar baz)`, so "foo" matches and leaves the cursor at
    /// 3 (the trailing input unconsumed).
    #[test]
    fn fsm_test_304_alternation_precedence() {
        let src = "@@fsm M(text: bytes) : bytes = \"\" { /foo|bar baz/ @@:matched }";
        let Some(foo) = run(src, "foo baz", "t304a") else {
            return;
        };
        assert_eq!(foo.return_value, "'foo'");
        assert_eq!(foo.cursor, "3");
        assert_eq!(
            run(src, "bar baz", "t304b").unwrap().return_value,
            "'bar baz'"
        );
    }

    /// Multi-match (`|`) ordered choice: the first alternative whose first
    /// stage matches wins; distinct first stages route to distinct targets.
    #[test]
    fn multi_match_ordered_choice() {
        let src = "@@fsm M(text: bytes) : int = 0 { \
                   /[0-9]/ -> $num | /[a-z]/ -> $word \
                   $num: 1 \
                   $word: 2 }";
        let Some(d) = run(src, "5", "tmm_a") else {
            return;
        };
        assert_eq!(d.accepted, "True");
        assert_eq!(d.return_value, "1");
        let w = run(src, "a", "tmm_b").unwrap();
        assert_eq!(w.accepted, "True");
        assert_eq!(w.return_value, "2");
        // Neither alternative's first stage matches → reject.
        let bang = run(src, "!", "tmm_c").unwrap();
        assert_eq!(bang.accepted, "False");
        assert_eq!(bang.return_value, "0");
    }

    /// Selection commits on the first stage: when two alternatives share a
    /// first stage, the earlier one wins even if a later stage then fails —
    /// no backtracking to the other alternative.
    #[test]
    fn multi_match_commits_on_first_stage() {
        let src = "@@fsm M(text: bytes) : int = 0 { \
                   /a/ /b/ -> $ab : -> $err | /a/ /c/ -> $ac \
                   $ab: 1 \
                   $ac: 2 \
                   $err: -1 }";
        // "ab": commit alt0, /b/ matches -> $ab.
        let Some(ab) = run(src, "ab", "tmc_a") else {
            return;
        };
        assert_eq!(ab.return_value, "1");
        // "ac": commit alt0 (first stage /a/ matched); /b/ fails on 'c' →
        // alt0's failure branch ($err), NOT alt1. Demonstrates commit.
        let ac = run(src, "ac", "tmc_b").unwrap();
        assert_eq!(ac.return_value, "-1");
        assert_eq!(ac.accepted, "False");
        // "xy": neither alternative's first stage (/a/) matches → reject.
        assert_eq!(run(src, "xy", "tmc_c").unwrap().accepted, "False");
    }

    /// A stageless final alternative is an unconditional catch-all.
    #[test]
    fn multi_match_catch_all() {
        let src = "@@fsm M(text: bytes) : int = 0 { \
                   /[0-9]/ -> $num | 99 \
                   $num: 1 }";
        let Some(num) = run(src, "5", "tca_a") else {
            return;
        };
        assert_eq!(num.return_value, "1");
        assert_eq!(num.accepted, "True");
        // "a": digit alternative misses → catch-all matches unconditionally.
        let other = run(src, "a", "tca_b").unwrap();
        assert_eq!(other.return_value, "99");
        assert_eq!(other.accepted, "True");
    }

    /// A standalone failure-only clause on an implicit-terminal match:
    /// `/a/ true : -> $err` accepts on match, routes to `$err` on failure.
    #[test]
    fn failure_only_clause() {
        let src = "@@fsm M(text: bytes) : bool = false { \
                   /a/ true : -> $err \
                   $err: false }";
        let Some(a) = run(src, "a", "tfo_a") else {
            return;
        };
        assert_eq!(a.accepted, "True");
        assert_eq!(a.return_value, "True");
        let b = run(src, "b", "tfo_b").unwrap();
        assert_eq!(b.accepted, "False");
        assert_eq!(b.return_value, "False");
    }

    /// FSM-TEST-312 — the start-of-input anchor `^` matches only at cursor
    /// 0. `/^foo/` accepts "foo", rejects "xfoo" (reject at 0) and "". Also
    /// FSM-TEST-1004 — an anchored match with a leading non-match byte
    /// (`"xfoo"`) is rejected at position 0.
    #[test]
    fn fsm_test_312_start_anchor() {
        let src = "@@fsm M(text: bytes) : bool = false { /^foo/ true }";
        let Some(foo) = run(src, "foo", "t312a") else {
            return;
        };
        assert_eq!(foo.accepted, "True");
        assert_eq!(foo.return_value, "True");
        let xfoo = run(src, "xfoo", "t312b").unwrap();
        assert_eq!(xfoo.accepted, "False");
        assert_eq!(xfoo.reject, "0");
        assert_eq!(run(src, "", "t312c").unwrap().accepted, "False");
    }

    /// A trailing `$` requires the match to reach the end of input.
    #[test]
    fn end_anchor() {
        let src = "@@fsm M(text: bytes) : bool = false { /[0-9]+$/ true }";
        let Some(ok) = run(src, "123", "tea_end_a") else {
            return;
        };
        assert_eq!(ok.accepted, "True");
        // "123x": digits don't reach end-of-input → `$` fails → reject.
        assert_eq!(run(src, "123x", "tea_end_b").unwrap().accepted, "False");
    }

    /// A mid-pattern anchor is outside the v0.1 cut and errors clearly.
    #[test]
    fn unsupported_mid_anchor_errors() {
        let decl =
            parse_fsm_block(b"@@fsm M(text: bytes) : bool = false { /a$b/ true }").expect("parses");
        let err = generate(&decl).unwrap_err();
        assert!(err.contains("anchor"), "got {err}");
    }

    /// Sentinel for the python3-absent skip path in tests that can't use
    /// `let-else` for the first run.
    fn no_py() -> Run {
        Run {
            accepted: String::new(),
            return_value: String::new(),
            cursor: String::new(),
            reject: String::new(),
        }
    }
}
