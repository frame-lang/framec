//! Rust backend for `@@fsm` (RFC-0042, Phase 8).
//!
//! Generates a Rust `struct` + `impl` from a validated `FsmDeclAst`,
//! mirroring the Python reference backend ([`super::fsm_python`]) but with
//! static types. The recognition model is identical (per-stage minimal
//! DFAs + a per-state dispatch loop); the novelties are Rust types
//! (Frame's abstract `int`/`bool`/`bytes` → `i64`/`bool`/`String`) and
//! ownership of the matched slice (the input is held as `Vec<char>`, the
//! matched run materialized into an owned `String`).
//!
//! # v0.1 scope
//!
//! Supports single-match states, match stages with `.label` captures,
//! bare-expression returns, action blocks (assignment / `if`-`else`),
//! declared `actions:` helpers, and all transition forms — static,
//! conditional (`when`), stage-ref (`-> $S.stage`, via a `self.enter`
//! re-entry index), and failure-only — plus multi-match (`|`) ordered-choice
//! states (commit-on-first-stage, stageless catch-all) and embedding actions
//! (`>{}`/`@{}`/`${}`/`%{}`/`@eof{}`, §3.5.5/§5.4) — over the `bytes`/`char`
//! alphabets, with the `@@:matched` / `to_int` / `to_str` / `len` built-ins.
//! Not yet handled (clear `Unsupported` error, never a silent miscompile):
//! Mode C call-out, the token alphabet, and anchors.

use crate::frame_c::compiler::frame_ast::{
    BinaryOp, EmbeddingOp, Expression, FsmDeclAst, FsmStateAst, FsmTransitionTarget, Literal,
    MatchAst, MatchElement, StageAst, Type, UnaryOp,
};
use crate::frame_c::compiler::fsm_regex::{
    self, size_check::DEFAULT_MAX_DFA_STATES, subset::DfaLabel, Alphabet, CompileError,
};
use std::fmt::Write;

/// Generate Rust source implementing `decl`, or a reason it is outside the
/// v0.1 Rust cut.
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
    /// `(state label, stage label)` → element index, for stage-ref
    /// re-entry (`-> $State.stage`). Single-match states only.
    stage_entry: std::collections::HashMap<(String, String), usize>,
    stage_dfas: Vec<StageDfa>,
}

impl<'a> Generator<'a> {
    fn new(decl: &'a FsmDeclAst) -> Result<Self, String> {
        let alphabet = match decl.params.first().map(|p| &p.param_type) {
            Some(Type::Custom(t)) if t == "char" => Alphabet::Char,
            Some(Type::Custom(t)) if t == "token" => {
                return Err("the token alphabet is not yet supported by the Rust backend".into())
            }
            _ => Alphabet::Bytes,
        };
        let mut label_to_index = std::collections::HashMap::new();
        let mut stage_entry = std::collections::HashMap::new();
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
            stage_dfas: Vec::new(),
        };
        g.compile_stage_dfas()?;
        Ok(g)
    }

    /// Compile every stage's DFA across all states and all `|` alternatives,
    /// in traversal order, so the emitted `DFA_<sid>` consts line up with the
    /// `sid` counter the state emitters advance.
    fn compile_stage_dfas(&mut self) -> Result<(), String> {
        for st in &self.decl.states {
            for m in &st.matches {
                for el in &m.elements {
                    if let MatchElement::Stage(stage) = el {
                        if stage.regex.starts_with('@') {
                            return Err(
                                "Mode C (`/@Fsm/`) is not yet supported by the Rust backend".into(),
                            );
                        }
                        self.stage_dfas.push(self.compile_one(&stage.regex)?);
                    }
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
                        "anchors are not yet supported by the Rust backend (v0.1 first cut)".into(),
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
                "regex `/{}/` uses anchors, not yet supported by the Rust backend",
                regex
            )),
        }
    }

    fn emit(&self) -> Result<String, String> {
        let mut out = String::new();
        out.push_str("// Generated by framec — RFC-0042 @@fsm (Rust backend).\n\n");
        self.emit_struct(&mut out);
        self.emit_impl(&mut out)?;
        Ok(out)
    }

    /// The Rust type for a Frame type string (shared with the @@system
    /// Rust backend's conventions: `int`→`i64`, `bool`→`bool`, …).
    fn rust_type(t: &Type) -> String {
        let s = match t {
            Type::Custom(s) => s.as_str(),
            _ => "()",
        };
        match s {
            "int" => "i64".to_string(),
            "float" => "f64".to_string(),
            "bool" => "bool".to_string(),
            "str" | "string" | "String" | "bytes" => "String".to_string(),
            other => other.to_string(),
        }
    }

    fn emit_struct(&self, out: &mut String) {
        writeln!(out, "pub struct {} {{", self.decl.name).ok();
        out.push_str("    pub accepted: bool,\n");
        out.push_str("    pub reject_position: usize,\n");
        out.push_str("    pub cursor: usize,\n");
        writeln!(
            out,
            "    pub return_value: {},",
            Self::rust_type(&self.decl.return_type)
        )
        .ok();
        // Auto-promoted parameters become fields. The input parameter is a
        // `Vec<char>` so the cursor indexes it in O(1); other params keep
        // their declared type.
        for (i, p) in self.decl.params.iter().enumerate() {
            if i == 0 {
                writeln!(out, "    pub {}: Vec<char>,", p.name).ok();
            } else {
                writeln!(
                    out,
                    "    pub {}: {},",
                    p.name,
                    Self::rust_type(&p.param_type)
                )
                .ok();
            }
        }
        if let Some(domain) = &self.decl.domain {
            for v in &domain.vars {
                // Skip a domain field re-declaring the input parameter (it
                // is already the `Vec<char>` field above).
                if self.decl.params.first().map(|p| &p.name) == Some(&v.name) {
                    continue;
                }
                writeln!(out, "    pub {}: {},", v.name, Self::rust_type(&v.var_type)).ok();
            }
        }
        out.push_str("    matched: String,\n");
        // Stage-ref re-entry point (`-> $State.stage` sets it; the dispatch
        // loop consumes it). 0 = enter at the state's first element.
        out.push_str("    enter: usize,\n");
        // One owned field per labeled stage in a labeled state, holding the
        // matched slice for `$state.label` reads.
        for f in self.capture_fields() {
            writeln!(out, "    {}: String,", f).ok();
        }
        out.push_str("}\n\n");
    }

    /// Field names for every labeled stage in a labeled state (the targets
    /// of `$state.label` capture reads).
    fn capture_fields(&self) -> Vec<String> {
        let mut out = Vec::new();
        for st in &self.decl.states {
            let Some(slabel) = &st.label else { continue };
            for m in &st.matches {
                for el in &m.elements {
                    if let MatchElement::Stage(stage) = el {
                        if let Some(lbl) = &stage.label {
                            out.push(cap_field(slabel, lbl));
                        }
                    }
                }
            }
        }
        out
    }

    /// Does any stage match via the shared DFA executor (i.e. carries no
    /// embedding actions)? When every stage is embedding-aware, `dfa_match`
    /// is never called and must not be emitted (dead code).
    fn has_plain_stage(&self) -> bool {
        self.decl.states.iter().any(|st| {
            st.matches.iter().any(|m| {
                m.elements.iter().any(
                    |el| matches!(el, MatchElement::Stage(s) if s.embedding_actions.is_empty()),
                )
            })
        })
    }

    fn emit_impl(&self, out: &mut String) -> Result<(), String> {
        writeln!(out, "impl {} {{", self.decl.name).ok();
        self.emit_new(out);
        if self.has_plain_stage() {
            self.emit_dfa_matcher(out);
        }
        self.emit_run(out);
        self.emit_state_methods(out)?;
        self.emit_embed_matchers(out)?;
        self.emit_action_methods(out)?;
        out.push_str("}\n");
        Ok(())
    }

    /// Emit declared `actions:` helpers as `&mut self` methods. A trailing
    /// bare expression is the action's return value (§3.7 implicit tail).
    fn emit_action_methods(&self, out: &mut String) -> Result<(), String> {
        let Some(block) = &self.decl.actions else {
            return Ok(());
        };
        for act in &block.actions {
            let mut sig = String::from("&mut self");
            for p in &act.params {
                write!(sig, ", {}: {}", p.name, Self::rust_type(&p.param_type)).ok();
            }
            let ret = match &act.return_type {
                Some(t) => format!(" -> {}", Self::rust_type(t)),
                None => String::new(),
            };
            writeln!(out, "    fn {}({}){} {{", act.name, sig, ret).ok();
            self.emit_action_body(out, &act.body, act.return_type.is_some())?;
            out.push_str("    }\n\n");
        }
        Ok(())
    }

    fn emit_action_body(
        &self,
        out: &mut String,
        body: &crate::frame_c::compiler::frame_ast::BlockAst,
        has_return: bool,
    ) -> Result<(), String> {
        use crate::frame_c::compiler::frame_ast::Statement;
        let n = body.statements.len();
        for (i, s) in body.statements.iter().enumerate() {
            // A trailing bare (non-assignment) expression is the Rust tail
            // expression (the action's return value).
            if i + 1 == n && has_return {
                if let Statement::Expression(e) = s {
                    if !matches!(e.expr, Expression::Assign { .. }) {
                        writeln!(out, "        {}", self.expr_top(&e.expr)).ok();
                        continue;
                    }
                }
            }
            out.push_str(&self.stmt(s, "        ")?);
        }
        Ok(())
    }

    fn emit_new(&self, out: &mut String) {
        let input = &self.decl.params[0].name;
        // Constructor signature: input as Vec<char>, other params typed.
        let mut sig = String::new();
        for (i, p) in self.decl.params.iter().enumerate() {
            if i > 0 {
                sig.push_str(", ");
            }
            if i == 0 {
                write!(sig, "{}: Vec<char>", p.name).ok();
            } else {
                write!(sig, "{}: {}", p.name, Self::rust_type(&p.param_type)).ok();
            }
        }
        writeln!(out, "    pub fn new({}) -> Self {{", sig).ok();
        out.push_str("        let mut _m = Self {\n");
        out.push_str("            accepted: false,\n");
        out.push_str("            reject_position: 0,\n");
        out.push_str("            cursor: 0,\n");
        writeln!(
            out,
            "            return_value: {},",
            Self::rust_default(&self.decl.return_type, &self.decl.default_expr)
        )
        .ok();
        for p in &self.decl.params {
            writeln!(out, "            {}: {},", p.name, p.name).ok();
        }
        if let Some(domain) = &self.decl.domain {
            for v in &domain.vars {
                if self.decl.params.first().map(|p| &p.name) == Some(&v.name) {
                    continue;
                }
                writeln!(out, "            {}: {},", v.name, self.expr(&v.default)).ok();
            }
        }
        out.push_str("            matched: String::new(),\n");
        out.push_str("            enter: 0,\n");
        for f in self.capture_fields() {
            writeln!(out, "            {}: String::new(),", f).ok();
        }
        out.push_str("        };\n");
        out.push_str("        _m.run();\n");
        out.push_str("        if _m.accepted { _m.reject_position = 0; }\n");
        writeln!(out, "        _m").ok();
        out.push_str("    }\n\n");
        let _ = input;
    }

    /// Greedy longest-match DFA executor (mirrors `_dfa_match` in Python).
    fn emit_dfa_matcher(&self, out: &mut String) {
        let input = &self.decl.params[0].name;
        writeln!(
            out,
            "    fn dfa_match(&self, states: &[(&[(u32, u32, usize)], bool)], start: usize) -> i64 {{\n\
             \x20       let mut st = start;\n\
             \x20       let mut pos = self.cursor;\n\
             \x20       let n = self.{input}.len();\n\
             \x20       let mut last: i64 = if states[st].1 {{ pos as i64 }} else {{ -1 }};\n\
             \x20       while pos < n {{\n\
             \x20           let v = self.{input}[pos] as u32;\n\
             \x20           let mut nxt: Option<usize> = None;\n\
             \x20           for &(lo, hi, tgt) in states[st].0 {{\n\
             \x20               if lo <= v && v <= hi {{ nxt = Some(tgt); break; }}\n\
             \x20           }}\n\
             \x20           match nxt {{\n\
             \x20               Some(t) => {{ st = t; pos += 1; if states[st].1 {{ last = pos as i64; }} }}\n\
             \x20               None => break,\n\
             \x20           }}\n\
             \x20       }}\n\
             \x20       last\n\
             \x20   }}\n\n",
            input = input
        )
        .ok();
    }

    /// Emit a specialized matcher `match_stage_<sid>` for each stage that
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
    /// scan as `dfa_match`, but firing the embedding actions at their DFA
    /// positions: `>{}` once at scan start, `${}` per consumed element, `@{}`
    /// on entering an accepting state, `%{}` on leaving one, and `@eof{}` at
    /// end-of-input while mid-match (non-accepting). `@@:cursor` reflects the
    /// scan position during firing; it is restored to the stage-entry
    /// position on return so the caller's slice/advance hold.
    fn emit_one_matcher(
        &self,
        out: &mut String,
        sid: usize,
        stage: &StageAst,
    ) -> Result<(), String> {
        let input = &self.decl.params[0].name;
        // `prev` (the previous step's accepting flag) is only consumed by the
        // `%{}` leave guard and the `@eof{}` mid-match guard; track it only
        // when one of those is present (else it's a dead variable).
        let leave = self.embed_bodies(stage, EmbeddingOp::LeaveAccept, "                ")?;
        let eof = self.embed_bodies(stage, EmbeddingOp::Eof, "            ")?;
        let needs_prev = !leave.is_empty() || !eof.is_empty();
        writeln!(out, "    fn match_stage_{}(&mut self) -> i64 {{", sid).ok();
        self.emit_dfa_const(out, sid, "        ");
        writeln!(
            out,
            "        let _entry = self.cursor;\n\
             \x20       let mut st = {start};\n\
             \x20       let mut pos = _entry;\n\
             \x20       let n = self.{input}.len();\n\
             \x20       let mut last: i64 = if DFA_{sid}[st].1 {{ pos as i64 }} else {{ -1 }};\n\
             \x20       self.cursor = pos;",
            start = self.stage_dfas[sid].start,
            input = input,
            sid = sid
        )
        .ok();
        // `>{}` — begin matching.
        out.push_str(&self.embed_bodies(stage, EmbeddingOp::Start, "        ")?);
        if needs_prev {
            writeln!(out, "        let mut prev = DFA_{}[st].1;", sid).ok();
        }
        writeln!(
            out,
            "        while pos < n {{\n\
             \x20           let v = self.{input}[pos] as u32;\n\
             \x20           let mut nxt: Option<usize> = None;\n\
             \x20           for &(lo, hi, tgt) in DFA_{sid}[st].0 {{\n\
             \x20               if lo <= v && v <= hi {{ nxt = Some(tgt); break; }}\n\
             \x20           }}\n\
             \x20           let Some(t) = nxt else {{ break; }};\n\
             \x20           st = t;\n\
             \x20           pos += 1;\n\
             \x20           self.cursor = pos;",
            input = input,
            sid = sid
        )
        .ok();
        // `${}` — every consumed element.
        out.push_str(&self.embed_bodies(stage, EmbeddingOp::EveryTransition, "            ")?);
        writeln!(out, "            let _now = DFA_{}[st].1;", sid).ok();
        // `@{}` — a transition into an accepting state (§3.5.5: every
        // transition whose destination is final, not only the first entry).
        let accept = self.embed_bodies(stage, EmbeddingOp::Accept, "                ")?;
        if !accept.is_empty() {
            out.push_str("            if _now {\n");
            out.push_str(&accept);
            out.push_str("            }\n");
        }
        // `%{}` — left an accepting state.
        if !leave.is_empty() {
            out.push_str("            if prev && !_now {\n");
            out.push_str(&leave);
            out.push_str("            }\n");
        }
        out.push_str("            if _now { last = pos as i64; }\n");
        if needs_prev {
            out.push_str("            prev = _now;\n");
        }
        out.push_str("        }\n");
        // `@eof{}` — end of input reached while mid-match (non-accepting).
        if !eof.is_empty() {
            out.push_str("        if pos >= n && !prev {\n");
            out.push_str(&eof);
            out.push_str("        }\n");
        }
        out.push_str("        self.cursor = _entry;\n        last\n    }\n\n");
        Ok(())
    }

    /// Concatenated Rust for every embedding-action body of `op` on `stage`,
    /// each statement at indent `ind`. Empty when the stage has no `op`
    /// action (so the caller can skip an empty guard block).
    fn embed_bodies(&self, stage: &StageAst, op: EmbeddingOp, ind: &str) -> Result<String, String> {
        let mut s = String::new();
        for ea in &stage.embedding_actions {
            if ea.op == op {
                for st in &ea.body.statements {
                    s.push_str(&self.stmt(st, ind)?);
                }
            }
        }
        Ok(s)
    }

    fn emit_run(&self, out: &mut String) {
        out.push_str("    fn run(&mut self) {\n        let mut state: i64 = 0;\n");
        out.push_str("        while state >= 0 {\n");
        out.push_str("            let _enter = self.enter;\n            self.enter = 0;\n");
        out.push_str("            state = match state {\n");
        for i in 0..self.decl.states.len() {
            writeln!(out, "                {} => self.state_{}(_enter),", i, i).ok();
        }
        out.push_str("                _ => return,\n            };\n        }\n    }\n\n");
    }

    fn emit_state_methods(&self, out: &mut String) -> Result<(), String> {
        let mut sid = 0usize;
        for (i, st) in self.decl.states.iter().enumerate() {
            match st.matches.len() {
                0 => {
                    writeln!(
                        out,
                        "    fn state_{}(&mut self, _enter: usize) -> i64 {{ -1 }}\n",
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

    fn emit_one_state(
        &self,
        out: &mut String,
        index: usize,
        st: &FsmStateAst,
        m: &MatchAst,
        sid: &mut usize,
    ) -> Result<(), String> {
        let state_label = st.label.clone().unwrap_or_default();
        writeln!(
            out,
            "    fn state_{}(&mut self, _enter: usize) -> i64 {{",
            index
        )
        .ok();
        // Each element is guarded by `if _enter <= <idx>` so a stage-ref
        // re-entry skips leading elements (plain entry has `_enter == 0`).
        for (idx, el) in m.elements.iter().enumerate() {
            writeln!(out, "        if _enter <= {} {{", idx).ok();
            self.emit_element(out, el, m, &state_label, "            ", sid)?;
            out.push_str("        }\n");
        }
        self.emit_success(out, m, "        ", /*tail*/ true)?;
        out.push_str("    }\n\n");
        Ok(())
    }

    /// Emit a multi-match (`|`) state as ordered choice (RFC-0042 §3.4).
    /// Each alternative's first stage is tried at the state-entry cursor; the
    /// first that matches commits and runs to its transition (a committed
    /// alternative's later-stage failure follows its own failure branch). A
    /// first-stage miss falls through to the next alternative with the cursor
    /// unchanged. A stageless alternative is an unconditional catch-all. If no
    /// alternative matches, the input is not in the language (§5.6).
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
        let input = &self.decl.params[0].name;
        writeln!(
            out,
            "    fn state_{}(&mut self, _enter: usize) -> i64 {{",
            index
        )
        .ok();

        // A stageless catch-all returns unconditionally; once one is emitted
        // the §5.6 fallback (and any later alternative) is unreachable.
        let mut catch_all = false;
        for m in &st.matches {
            let first_stage = m
                .elements
                .iter()
                .position(|e| matches!(e, MatchElement::Stage(_)));
            match first_stage {
                Some(fs) => {
                    if fs > 0 {
                        // Elements before the first stage would run during
                        // selection (before this alternative is chosen), which
                        // has ambiguous side-effect semantics. Reject rather
                        // than silently drop or misorder them.
                        return Err(
                            "a `|` alternative with elements before its first stage is not yet \
                             supported by the Rust backend"
                                .into(),
                        );
                    }
                    let my_sid = *sid;
                    *sid += 1;
                    let MatchElement::Stage(sel) = &m.elements[fs] else {
                        unreachable!("first_stage indexes a Stage element")
                    };
                    if sel.embedding_actions.is_empty() {
                        self.emit_dfa_const(out, my_sid, "        ");
                        writeln!(
                            out,
                            "        let _r = self.dfa_match(DFA_{}, {});",
                            my_sid, self.stage_dfas[my_sid].start
                        )
                        .ok();
                    } else {
                        writeln!(out, "        let _r = self.match_stage_{}();", my_sid).ok();
                    }
                    out.push_str("        if _r >= 0 {\n");
                    // Committed: record the selector's capture + advance.
                    writeln!(
                        out,
                        "            self.matched = self.{}[self.cursor..(_r as usize)].iter().collect();",
                        input
                    )
                    .ok();
                    if let Some(slabel) = &sel.label {
                        if !state_label.is_empty() {
                            writeln!(
                                out,
                                "            self.{} = self.matched.clone();",
                                cap_field(&state_label, slabel)
                            )
                            .ok();
                        }
                    }
                    out.push_str("            self.cursor = _r as usize;\n");
                    out.push_str("            self.accepted = true;\n");
                    // Remaining elements run inside the commit; a later stage
                    // failure follows this alternative's failure branch.
                    for el in &m.elements[fs + 1..] {
                        self.emit_element(out, el, m, &state_label, "            ", sid)?;
                    }
                    self.emit_success(out, m, "            ", /*tail*/ false)?;
                    out.push_str("        }\n");
                    // First-stage miss: fall through to the next alternative.
                }
                None => {
                    // Stageless alternative: an unconditional catch-all.
                    out.push_str("        self.accepted = true;\n");
                    for el in &m.elements {
                        self.emit_element(out, el, m, &state_label, "        ", sid)?;
                    }
                    self.emit_success(out, m, "        ", /*tail*/ false)?;
                    catch_all = true;
                    break;
                }
            }
        }

        if !catch_all {
            // No alternative's first stage matched: not in the language (§5.6).
            out.push_str("        self.accepted = false;\n");
            out.push_str("        self.reject_position = self.cursor;\n");
            out.push_str("        -1\n");
        }
        out.push_str("    }\n\n");
        Ok(())
    }

    fn emit_element(
        &self,
        out: &mut String,
        el: &MatchElement,
        m: &MatchAst,
        state_label: &str,
        ind: &str,
        sid: &mut usize,
    ) -> Result<(), String> {
        let input = &self.decl.params[0].name;
        let ind4 = format!("{}    ", ind);
        match el {
            MatchElement::Stage(stage) => {
                let my_sid = *sid;
                *sid += 1;
                // A stage carrying embedding actions matches via its
                // specialized `match_stage_<sid>` (which fires the actions at
                // their DFA positions); a plain stage uses the shared DFA
                // executor over an inline `DFA_<sid>` const.
                if stage.embedding_actions.is_empty() {
                    self.emit_dfa_const(out, my_sid, ind);
                    writeln!(
                        out,
                        "{}let _r = self.dfa_match(DFA_{}, {});",
                        ind, my_sid, self.stage_dfas[my_sid].start
                    )
                    .ok();
                } else {
                    writeln!(out, "{}let _r = self.match_stage_{}();", ind, my_sid).ok();
                }
                writeln!(out, "{}if _r < 0 {{", ind).ok();
                self.emit_failure(out, m, &ind4)?;
                writeln!(out, "{}}}", ind).ok();
                writeln!(
                    out,
                    "{}self.matched = self.{}[self.cursor..(_r as usize)].iter().collect();",
                    ind, input
                )
                .ok();
                if let Some(lbl) = &stage.label {
                    if !state_label.is_empty() {
                        writeln!(
                            out,
                            "{}self.{} = self.matched.clone();",
                            ind,
                            cap_field(state_label, lbl)
                        )
                        .ok();
                    }
                }
                writeln!(out, "{}self.cursor = _r as usize;", ind).ok();
                writeln!(out, "{}self.accepted = true;", ind).ok();
            }
            MatchElement::BareExpression { expr, .. } => {
                writeln!(out, "{}self.return_value = {};", ind, self.expr_top(expr)).ok();
            }
            MatchElement::ActionBlock(blk) => {
                for s in &blk.statements {
                    out.push_str(&self.stmt(s, ind)?);
                }
            }
        }
        Ok(())
    }

    /// Emit the success-branch transition after a match completes. `tail`
    /// selects the dispatch-loop tail form (bare `idx`) versus a `return idx;`
    /// — single-match states emit in tail position, a committed `|`
    /// alternative emits inside an `if` and must `return`.
    fn emit_success(
        &self,
        out: &mut String,
        m: &MatchAst,
        ind: &str,
        tail: bool,
    ) -> Result<(), String> {
        match m.transition.as_ref().and_then(|c| c.success.as_ref()) {
            None => {
                if tail {
                    writeln!(out, "{}-1", ind).ok();
                } else {
                    writeln!(out, "{}return -1;", ind).ok();
                }
                Ok(())
            }
            Some(success) => self.emit_target(out, success, ind, m, tail),
        }
    }

    /// Emit the failure-branch resolution (sets `accepted = false` and the
    /// reject position, then routes to the failure target or §5.6 halt).
    fn emit_failure(&self, out: &mut String, m: &MatchAst, ind: &str) -> Result<(), String> {
        writeln!(out, "{}self.accepted = false;", ind).ok();
        writeln!(out, "{}self.reject_position = self.cursor;", ind).ok();
        match m.transition.as_ref().and_then(|c| c.failure.as_ref()) {
            None => {
                writeln!(out, "{}return -1;", ind).ok();
                Ok(())
            }
            Some(target) => self.emit_target(out, target, ind, m, /*tail*/ false),
        }
    }

    /// Emit a target. A static target is a single goto; a conditional emits
    /// an ordered `if <when> {{ goto }}` chain, then the failure branch as
    /// the no-match fallback. `tail` selects the success (bare `expr`) vs
    /// failure (`return expr;`) form for the dispatch loop.
    fn emit_target(
        &self,
        out: &mut String,
        target: &FsmTransitionTarget,
        ind: &str,
        m: &MatchAst,
        tail: bool,
    ) -> Result<(), String> {
        match target {
            FsmTransitionTarget::Static { .. } => self.emit_goto(out, target, ind, tail),
            FsmTransitionTarget::Conditional(alts) => {
                let inner = format!("{}    ", ind);
                for alt in alts {
                    writeln!(out, "{}if {} {{", ind, self.expr_top(&alt.condition)).ok();
                    // Inside an `if`, a goto is always a `return`.
                    self.emit_goto(out, &alt.target, &inner, false)?;
                    writeln!(out, "{}}}", ind).ok();
                }
                // No `when` held → the failure branch fires.
                self.emit_failure(out, m, ind)
            }
        }
    }

    /// Emit the dispatch jump for a static target. In `tail` position (the
    /// state method's last expression) it's a bare `idx`; otherwise a
    /// `return idx;`. A stage-ref sets `self.enter` first.
    fn emit_goto(
        &self,
        out: &mut String,
        t: &FsmTransitionTarget,
        ind: &str,
        tail: bool,
    ) -> Result<(), String> {
        let (state, stage) = match t {
            FsmTransitionTarget::Static { state, stage, .. } => (state, stage),
            FsmTransitionTarget::Conditional(_) => {
                return Err("a conditional target may not nest another conditional".into())
            }
        };
        let idx = self
            .label_to_index
            .get(state)
            .copied()
            .ok_or_else(|| format!("transition to undeclared state `${}`", state))?;
        if let Some(s) = stage {
            let entry = self
                .stage_entry
                .get(&(state.clone(), s.clone()))
                .copied()
                .ok_or_else(|| format!("transition to undeclared stage `${}.{}`", state, s))?;
            writeln!(out, "{}self.enter = {};", ind, entry).ok();
        }
        if tail {
            writeln!(out, "{}{}", ind, idx).ok();
        } else {
            writeln!(out, "{}return {};", ind, idx).ok();
        }
        Ok(())
    }

    /// Translate an action-block statement to Rust source lines.
    fn stmt(
        &self,
        s: &crate::frame_c::compiler::frame_ast::Statement,
        ind: &str,
    ) -> Result<String, String> {
        use crate::frame_c::compiler::frame_ast::Statement;
        match s {
            Statement::Expression(e) => Ok(format!("{}{};\n", ind, self.expr(&e.expr))),
            Statement::If(if_ast) => {
                let inner = format!("{}    ", ind);
                let mut out = format!("{}if {} {{\n", ind, self.expr_top(&if_ast.condition));
                out.push_str(&self.stmt(&if_ast.then_branch, &inner)?);
                out.push_str(&format!("{}}}", ind));
                if let Some(else_b) = &if_ast.else_branch {
                    out.push_str(" else {\n");
                    out.push_str(&self.stmt(else_b, &inner)?);
                    out.push_str(&format!("{}}}\n", ind));
                } else {
                    out.push('\n');
                }
                Ok(out)
            }
            Statement::Block(blk) => {
                let mut out = String::new();
                for st in &blk.statements {
                    out.push_str(&self.stmt(st, ind)?);
                }
                Ok(out)
            }
            other => Err(format!(
                "statement {:?} not supported in @@fsm action blocks by the Rust backend",
                std::mem::discriminant(other)
            )),
        }
    }

    /// Emit the per-stage DFA as a `const DFA_<sid>` (sid-suffixed so a
    /// multi-stage state has distinct consts), at indent `ind`.
    fn emit_dfa_const(&self, out: &mut String, sid: usize, ind: &str) {
        let dfa = &self.stage_dfas[sid];
        let states: Vec<String> = dfa
            .states
            .iter()
            .map(|(trans, acc)| {
                let ts: Vec<String> = trans
                    .iter()
                    .map(|(lo, hi, tgt)| format!("({}, {}, {})", lo, hi, tgt))
                    .collect();
                format!("(&[{}], {})", ts.join(", "), acc)
            })
            .collect();
        writeln!(
            out,
            "{}const DFA_{}: &[(&[(u32, u32, usize)], bool)] = &[{}];",
            ind,
            sid,
            states.join(", ")
        )
        .ok();
    }

    /// Map a raw default token to a Rust expression of the field's type.
    fn rust_default(ty: &Type, raw: &str) -> String {
        match raw {
            "false" => "false".to_string(),
            "true" => "true".to_string(),
            "" => default_for(ty),
            s if s.starts_with('"') => format!("{}.to_string()", s),
            s => s.to_string(),
        }
    }

    /// Translate a Frame expression to a Rust expression.
    fn expr(&self, e: &Expression) -> String {
        match e {
            Expression::Literal(l) => match l {
                Literal::Int(i) => i.to_string(),
                Literal::Float(f) => f.to_string(),
                Literal::Bool(b) => b.to_string(),
                Literal::String(s) => format!("{:?}.to_string()", s),
                Literal::Null => "Default::default()".to_string(),
            },
            Expression::Var(name) => match name.as_str() {
                "@@:matched" => "self.matched.clone()".to_string(),
                "@@:cursor" => "(self.cursor as i64)".to_string(),
                "@@:return" => "self.return_value".to_string(),
                // `$state.label` reads a stage capture (the matched slice).
                _ => match name.strip_prefix('$').and_then(|c| c.split_once('.')) {
                    Some((state, label)) => format!("self.{}.clone()", cap_field(state, label)),
                    None => name.clone(),
                },
            },
            Expression::Binary { left, op, right } => {
                format!("({} {} {})", self.expr(left), binop(op), self.expr(right))
            }
            Expression::Unary { op, expr } => match op {
                UnaryOp::Not => format!("(!{})", self.expr(expr)),
                UnaryOp::Neg => format!("(-{})", self.expr(expr)),
                UnaryOp::BitNot => format!("(!{})", self.expr(expr)),
            },
            Expression::Call { func, args } => self.call(func, args),
            Expression::Member { object, field } => {
                format!("{}.{}", self.expr(object), field)
            }
            Expression::Index { object, index } => {
                format!("{}[{}]", self.expr(object), self.expr(index))
            }
            Expression::Assign { target, value } => {
                format!("{} = {}", self.expr(target), self.expr_top(value))
            }
            Expression::NativeExpr(s) => s.clone(),
        }
    }

    /// Like [`Self::expr`], but for an expression in statement / condition /
    /// assignment-value position, where Rust needs no enclosing parentheses
    /// and `rustc`'s default `unused_parens` lint would flag them. Strips the
    /// single outer layer for `Binary`/`Unary`; inner operands keep their
    /// precedence-preserving parens via [`Self::expr`].
    fn expr_top(&self, e: &Expression) -> String {
        match e {
            Expression::Binary { left, op, right } => {
                format!("{} {} {}", self.expr(left), binop(op), self.expr(right))
            }
            Expression::Unary { op, expr } => match op {
                UnaryOp::Not | UnaryOp::BitNot => format!("!{}", self.expr(expr)),
                UnaryOp::Neg => format!("-{}", self.expr(expr)),
            },
            _ => self.expr(e),
        }
    }

    fn call(&self, func: &str, args: &[Expression]) -> String {
        let a: Vec<String> = args.iter().map(|e| self.expr(e)).collect();
        match func {
            // `to_int` parses the matched slice; `len` is the element count.
            "to_int" => format!("({}).parse::<i64>().unwrap_or(0)", a.join(", ")),
            "to_str" => format!("({}).to_string()", a.join(", ")),
            "len" => format!("({}.len() as i64)", a.join(", ")),
            _ => format!("self.{}({})", func, a.join(", ")),
        }
    }
}

/// Struct field name holding a stage capture: `$state.label` → `cap_state_label`.
fn cap_field(state: &str, label: &str) -> String {
    format!("cap_{}_{}", state, label)
}

fn binop(op: &BinaryOp) -> &'static str {
    match op {
        BinaryOp::Add => "+",
        BinaryOp::Sub => "-",
        BinaryOp::Mul => "*",
        BinaryOp::Div => "/",
        BinaryOp::Mod => "%",
        BinaryOp::Eq => "==",
        BinaryOp::Ne => "!=",
        BinaryOp::Lt => "<",
        BinaryOp::Le => "<=",
        BinaryOp::Gt => ">",
        BinaryOp::Ge => ">=",
        BinaryOp::And => "&&",
        BinaryOp::Or => "||",
        BinaryOp::BitAnd => "&",
        BinaryOp::BitOr => "|",
        BinaryOp::BitXor => "^",
    }
}

/// A type-appropriate Rust default when no initializer token is present.
fn default_for(ty: &Type) -> String {
    let s = match ty {
        Type::Custom(s) => s.as_str(),
        _ => "",
    };
    match s {
        "int" => "0".to_string(),
        "float" => "0.0".to_string(),
        "bool" => "false".to_string(),
        "str" | "string" | "String" | "bytes" => "String::new()".to_string(),
        _ => "Default::default()".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frame_c::compiler::fsm_parser::parse_fsm_block;
    use std::process::Command;

    /// Generate Rust for `src`, append a `main` that constructs the fsm
    /// over `input` and prints `accepted` + `return_value`, compile with
    /// `rustc`, run, and return `(accepted, return_value)`. `None` if
    /// `rustc` is unavailable.
    fn run(src: &str, class: &str, input: &str, tag: &str) -> Option<(String, String)> {
        let decl = parse_fsm_block(src.as_bytes()).expect("fixture must parse");
        let code = generate(&decl).expect("fixture must generate");
        let chars: String = input
            .chars()
            .map(|c| format!("'{}'", c))
            .collect::<Vec<_>>()
            .join(", ");
        let driver = format!(
            "{code}\nfn main() {{\n    let m = {class}::new(vec![{chars}]);\n    println!(\"{{}}\", m.accepted);\n    println!(\"{{:?}}\", m.return_value);\n}}\n",
            code = code,
            class = class,
            chars = chars
        );
        let dir = std::env::temp_dir();
        let src_path = dir.join(format!("framec_rs_{}.rs", tag));
        let bin_path = dir.join(format!("framec_rs_{}", tag));
        std::fs::write(&src_path, driver).expect("write rs");
        let compile = match Command::new("rustc")
            .arg("-O")
            .arg("--edition=2021")
            .arg(&src_path)
            .arg("-o")
            .arg(&bin_path)
            .output()
        {
            Ok(o) => o,
            Err(_) => return None, // rustc absent — skip
        };
        assert!(
            compile.status.success(),
            "rustc failed for {:?}:\n{}",
            src,
            String::from_utf8_lossy(&compile.stderr)
        );
        let out = Command::new(&bin_path).output().expect("run binary");
        let text = String::from_utf8_lossy(&out.stdout);
        let lines: Vec<&str> = text.lines().collect();
        Some((lines[0].to_string(), lines[1].to_string()))
    }

    #[test]
    fn rust_smoke_bool() {
        let src = "@@fsm M(text: bytes) : bool = false { /a/ true }";
        let Some((acc, ret)) = run(src, "M", "a", "smoke_a") else {
            return;
        };
        assert_eq!((acc.as_str(), ret.as_str()), ("true", "true"));
        let (acc2, _) = run(src, "M", "b", "smoke_b").unwrap();
        assert_eq!(acc2, "false");
    }

    #[test]
    fn rust_matched_to_int() {
        let src = "@@fsm M(text: bytes) : int = 0 { /[0-9]+/ to_int(@@:matched) }";
        let Some((acc, ret)) = run(src, "M", "123", "tok_a") else {
            return;
        };
        assert_eq!((acc.as_str(), ret.as_str()), ("true", "123"));
        assert_eq!(run(src, "M", "x", "tok_b").unwrap().0, "false");
    }

    #[test]
    fn rust_len_self_input() {
        let src = "@@fsm M(text: bytes) : int = 0 { /[0-9]+/ len(self.text) }";
        let Some((_, ret)) = run(src, "M", "123", "len_a") else {
            return;
        };
        assert_eq!(ret, "3");
    }

    /// Stage capture: `.n/[0-9]+/` captures the matched slice as `$s.n`.
    #[test]
    fn rust_stage_capture() {
        let src = "@@fsm M(text: bytes) : int = 0 { $s: .n/[0-9]+/ to_int($s.n) }";
        let Some((acc, ret)) = run(src, "M", "42", "cap_a") else {
            return;
        };
        assert_eq!((acc.as_str(), ret.as_str()), ("true", "42"));
    }

    /// Action block mutating a domain field, returned by a bare expression.
    #[test]
    fn rust_action_block() {
        let src = "@@fsm M(text: bytes) : int = 0 { \
                   /[0-9]/ { self.count = self.count + 1 } self.count \
                   domain: count: int = 0 }";
        let Some((_, ret)) = run(src, "M", "5", "act_a") else {
            return;
        };
        assert_eq!(ret, "1");
    }

    /// Declared `actions:` helper callable from a match, with a return value.
    #[test]
    fn rust_declared_action() {
        let src = "@@fsm M(text: bytes) : int = 0 { \
                   /[0-9]+/ parse_int(@@:matched) \
                   actions: parse_int(s: bytes): int { to_int(s) } }";
        let Some((_, ret)) = run(src, "M", "42", "decl_a") else {
            return;
        };
        assert_eq!(ret, "42");
    }

    /// FSM-TEST-006 in Rust: labeled states, static success + failure
    /// transitions, capture read across states.
    #[test]
    fn rust_transitions_and_capture() {
        let src = "@@fsm M(text: bytes) : int = 0 { \
                   $0: /[a-z]/ -> $digits : -> $error \
                   $digits: .n/[0-9]+/ to_int($digits.n) \
                   $error: -1 }";
        let Some((acc, ret)) = run(src, "M", "x42", "tr_a") else {
            return;
        };
        assert_eq!((acc.as_str(), ret.as_str()), ("true", "42"));
        // 'X' fails /[a-z]/ → failure branch → $error → -1.
        assert_eq!(run(src, "M", "X", "tr_b").unwrap().1, "-1");
    }

    /// Conditional `when` target (FSM-TEST-402) in Rust.
    #[test]
    fn rust_conditional_target() {
        let src = "@@fsm M(text: bytes, mode: int) : int = 0 { \
                   /[01]/ -> ( $zero when self.mode == 0, $one when self.mode == 1 ) : -> $error \
                   $zero: 0 \
                   $one: 1 \
                   $error: -1 }";
        let decl = parse_fsm_block(src.as_bytes()).expect("parses");
        let code = generate(&decl).expect("generates");
        let run_mode = |inp: &str, mode: i64, tag: &str| -> Option<String> {
            let driver = format!(
                "{code}\nfn main() {{ let m = M::new(\"{inp}\".chars().collect(), {mode}); println!(\"{{}}\", m.return_value); }}\n",
                code = code, inp = inp, mode = mode
            );
            let dir = std::env::temp_dir();
            let s = dir.join(format!("framec_rs_{}.rs", tag));
            let b = dir.join(format!("framec_rs_{}", tag));
            std::fs::write(&s, driver).ok()?;
            let c = Command::new("rustc")
                .arg("--edition=2021")
                .arg(&s)
                .arg("-o")
                .arg(&b)
                .output()
                .ok()?;
            assert!(c.status.success(), "{}", String::from_utf8_lossy(&c.stderr));
            let o = Command::new(&b).output().expect("run");
            Some(String::from_utf8_lossy(&o.stdout).trim().to_string())
        };
        let Some(z) = run_mode("0", 0, "cond_a") else {
            return;
        };
        assert_eq!(z, "0");
        assert_eq!(run_mode("1", 1, "cond_b").unwrap(), "1");
        assert_eq!(run_mode("0", 2, "cond_c").unwrap(), "-1"); // no when → failure
    }

    /// Multi-match (`|`) ordered choice: the first alternative whose first
    /// stage matches wins; distinct first stages route to distinct targets.
    #[test]
    fn rust_multi_match_ordered_choice() {
        let src = "@@fsm M(text: bytes) : int = 0 { \
                   /[0-9]/ -> $num | /[a-z]/ -> $word \
                   $num: 1 \
                   $word: 2 }";
        let Some((acc, ret)) = run(src, "M", "5", "mmc_a") else {
            return;
        };
        assert_eq!((acc.as_str(), ret.as_str()), ("true", "1"));
        assert_eq!(run(src, "M", "a", "mmc_b").unwrap().1, "2");
        // Neither alternative's first stage matches → reject.
        assert_eq!(run(src, "M", "!", "mmc_c").unwrap().0, "false");
    }

    /// Selection commits on the first stage: a committed alternative's later
    /// stage failure follows *its* failure branch, no backtracking.
    #[test]
    fn rust_multi_match_commits_on_first_stage() {
        let src = "@@fsm M(text: bytes) : int = 0 { \
                   /a/ /b/ -> $ab : -> $err | /a/ /c/ -> $ac \
                   $ab: 1 \
                   $ac: 2 \
                   $err: -1 }";
        let Some((_, ret)) = run(src, "M", "ab", "mmk_a") else {
            return;
        };
        assert_eq!(ret, "1");
        // "ac": alt0 commits on /a/; /b/ fails on 'c' → alt0's failure ($err).
        let (acc, ret) = run(src, "M", "ac", "mmk_b").unwrap();
        assert_eq!((acc.as_str(), ret.as_str()), ("false", "-1"));
        assert_eq!(run(src, "M", "xy", "mmk_c").unwrap().0, "false");
    }

    /// A stageless final alternative is an unconditional catch-all.
    #[test]
    fn rust_multi_match_catch_all() {
        let src = "@@fsm M(text: bytes) : int = 0 { \
                   /[0-9]/ -> $num | 99 \
                   $num: 1 }";
        let Some((acc, ret)) = run(src, "M", "5", "mma_a") else {
            return;
        };
        assert_eq!((acc.as_str(), ret.as_str()), ("true", "1"));
        // 'a': digit alternative misses → catch-all matches unconditionally.
        let (acc2, ret2) = run(src, "M", "a", "mma_b").unwrap();
        assert_eq!((acc2.as_str(), ret2.as_str()), ("true", "99"));
    }

    /// `${...}` fires once per consumed element (FSM-TEST-123); a declared
    /// action is callable from inside it.
    #[test]
    fn rust_embed_every_transition() {
        let src = "@@fsm M(text: bytes) : int = 0 { \
                   /[0-9]+/ ${ tally() } \
                   self.count \
                   actions: tally() { self.count = self.count + 1 } \
                   domain: count: int = 0 }";
        let Some((_, ret)) = run(src, "M", "123", "emb_e") else {
            return;
        };
        assert_eq!(ret, "3"); // three digits → ${} fires 3×
    }

    /// `>{...}` fires once at scan start; `@@:cursor` there is the
    /// stage-entry position (after the prior stage consumed `x`).
    #[test]
    fn rust_embed_start_captures_cursor() {
        let src = "@@fsm M(text: bytes) : int = 0 { \
                   /x/ /[0-9]+/ >{ self.start = @@:cursor } self.start \
                   domain: start: int = -1 }";
        let Some((_, ret)) = run(src, "M", "x42", "emb_s") else {
            return;
        };
        assert_eq!(ret, "1");
    }

    /// `@{...}` fires on each transition into an accepting state; for `/a+/`
    /// over "aaa" that is once per `a`.
    #[test]
    fn rust_embed_accept() {
        let src = "@@fsm M(text: bytes) : int = 0 { \
                   /a+/ @{ self.hits = self.hits + 1 } self.hits \
                   domain: hits: int = 0 }";
        let Some((_, ret)) = run(src, "M", "aaa", "emb_a") else {
            return;
        };
        assert_eq!(ret, "3");
    }

    /// A construct outside the v0.1 Rust cut errors clearly.
    #[test]
    fn rust_unsupported_errors() {
        let decl =
            parse_fsm_block(b"@@fsm M(toks: token) : bool = false { /A/ true }").expect("parses");
        assert!(generate(&decl).is_err());
    }
}
