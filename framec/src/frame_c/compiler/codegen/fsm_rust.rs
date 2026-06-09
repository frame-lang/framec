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
//! (`>{}`/`@{}`/`${}`/`%{}`/`@eof{}`, §3.5.5/§5.4) — over all three alphabets
//! (`bytes`/`char` as a `Vec<char>`; `token` as a `Vec<String>` mapped to
//! small integer ids), with the `@@:matched` / `to_int` / `to_str` / `len`
//! built-ins, Mode C sub-fsm call-out (`/@Inner/`, §8.3 — constructs the
//! inner fsm over the input at the cursor, exposing it via
//! `$state.label.return_value`), and boundary anchors (a leading `^`/`\A`,
//! a trailing `$`/`\z`, §6.6). This is full parity with the Python reference
//! backend. Not yet handled (clear `Unsupported` error, never a silent
//! miscompile): mid-pattern anchors and `\b`/`\B` (deferred to v0.2 in both
//! backends).

use crate::frame_c::compiler::frame_ast::{
    BinaryOp, EmbeddingOp, Expression, FsmDeclAst, FsmStateAst, FsmTransitionTarget, Literal,
    MatchAst, MatchElement, StageAst, Type, UnaryOp,
};
use crate::frame_c::compiler::fsm_regex::{
    self, size_check::DEFAULT_MAX_DFA_STATES, subset::DfaLabel, Alphabet, CompileError,
    WordBoundary,
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
    /// Leading `^`/`\A`: the match must start at the input start (cursor 0).
    requires_start: bool,
    /// Trailing `$`/`\z`: the match must end at the input end.
    requires_end: bool,
    /// Leading `\b`/`\B` (bytes only): a word boundary must be present
    /// (`Required`) or absent (`Forbidden`) at the match start.
    start_boundary: Option<WordBoundary>,
    /// Trailing `\b`/`\B`: same, at the match end.
    end_boundary: Option<WordBoundary>,
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
    /// `(state label, stage label)` → element index, for stage-ref
    /// re-entry (`-> $State.stage`). Single-match states only.
    stage_entry: std::collections::HashMap<(String, String), usize>,
    /// Token-alphabet only: each token-kind name → a small integer id, so
    /// token transitions reuse the same numeric range matcher as bytes/chars
    /// (the per-element read maps a token to its id; unknown → -1).
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
            token_ids: std::collections::HashMap::new(),
            stage_dfas: Vec::new(),
        };
        g.compile_stage_dfas()?;
        Ok(g)
    }

    /// Compile every stage's DFA across all states and all `|` alternatives,
    /// in traversal order, so the emitted `DFA_<sid>` consts line up with the
    /// `sid` counter the state emitters advance.
    fn compile_stage_dfas(&mut self) -> Result<(), String> {
        // `token_ids` is taken out so `compile_one` can be a borrow-free
        // associated function (it both reads `self.decl` and grows the map).
        let mut token_ids = std::mem::take(&mut self.token_ids);
        for st in &self.decl.states {
            for m in &st.matches {
                for el in &m.elements {
                    if let MatchElement::Stage(stage) = el {
                        if let Some(inner) = mode_c_inner(&stage.regex) {
                            // Mode C: a sub-fsm call-out, no DFA. Push a
                            // placeholder so stage indices stay aligned with
                            // the emit walk.
                            self.stage_dfas.push(StageDfa {
                                states: Vec::new(),
                                start: 0,
                                requires_start: false,
                                requires_end: false,
                                start_boundary: None,
                                end_boundary: None,
                                mode_c: Some(inner.to_string()),
                            });
                            continue;
                        }
                        match Self::compile_one(self.alphabet, &stage.regex, &mut token_ids) {
                            Ok(dfa) => self.stage_dfas.push(dfa),
                            Err(e) => {
                                self.token_ids = token_ids;
                                return Err(e);
                            }
                        }
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
                    requires_start: compiled.requires_start,
                    requires_end: compiled.requires_end,
                    start_boundary: compiled.start_boundary,
                    end_boundary: compiled.end_boundary,
                    mode_c: None,
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

    /// The Rust type of the auto-promoted input field: a token stream is a
    /// `Vec<String>` of token-kind names; a byte/char stream is a `Vec<char>`
    /// (cursor-indexable in O(1)).
    fn input_type(&self) -> &'static str {
        match self.alphabet {
            Alphabet::Token => "Vec<String>",
            _ => "Vec<char>",
        }
    }

    /// The Rust type of `matched` / stage captures: a `Vec<String>` slice of
    /// consumed tokens, else the matched `String`.
    fn matched_type(&self) -> &'static str {
        match self.alphabet {
            Alphabet::Token => "Vec<String>",
            _ => "String",
        }
    }

    /// The empty `matched` initializer for the alphabet.
    fn matched_empty(&self) -> &'static str {
        match self.alphabet {
            Alphabet::Token => "Vec::new()",
            _ => "String::new()",
        }
    }

    /// The expression materializing the matched run `self.<input>[cursor.._r]`
    /// into an owned value of [`Self::matched_type`].
    fn matched_slice(&self) -> String {
        let inp = &self.decl.params[0].name;
        match self.alphabet {
            Alphabet::Token => format!("self.{}[self.cursor..(_r as usize)].to_vec()", inp),
            _ => format!("self.{}[self.cursor..(_r as usize)].iter().collect()", inp),
        }
    }

    /// The per-element read as an `i64`: a byte/char by code point, a token by
    /// its small integer id (`-1` for an unknown token, matching no range).
    /// This is the only point where the matcher differs across alphabets.
    fn element_read(&self) -> String {
        let inp = &self.decl.params[0].name;
        match self.alphabet {
            Alphabet::Token => format!("self.tok_id(&self.{}[pos])", inp),
            _ => format!("self.{}[pos] as i64", inp),
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
        // cursor-indexable stream (`Vec<char>` / `Vec<String>`); other params
        // keep their declared type.
        for (i, p) in self.decl.params.iter().enumerate() {
            if i == 0 {
                writeln!(out, "    pub {}: {},", p.name, self.input_type()).ok();
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
        writeln!(out, "    matched: {},", self.matched_type()).ok();
        // Stage-ref re-entry point (`-> $State.stage` sets it; the dispatch
        // loop consumes it). 0 = enter at the state's first element.
        out.push_str("    enter: usize,\n");
        // One owned field per labeled stage in a labeled state, holding the
        // matched slice for `$state.label` reads.
        for f in self.capture_fields() {
            writeln!(out, "    {}: {},", f, self.matched_type()).ok();
        }
        // Mode C: the inner fsm instance, for `$state.label.<field>` reads.
        for (f, inner) in self.mode_c_inst_fields() {
            writeln!(out, "    {}: Option<{}>,", f, inner).ok();
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

    /// Does any stage match via the shared DFA executor (i.e. a regex stage
    /// with no embedding actions)? When every stage is embedding-aware or a
    /// Mode C call-out, `dfa_match` is never called and must not be emitted
    /// (dead code).
    fn has_plain_stage(&self) -> bool {
        self.decl.states.iter().any(|st| {
            st.matches.iter().any(|m| {
                m.elements.iter().any(|el| {
                    matches!(el, MatchElement::Stage(s)
                        if s.embedding_actions.is_empty() && mode_c_inner(&s.regex).is_none())
                })
            })
        })
    }

    /// `(field name, inner fsm name)` for each labeled Mode C stage in a
    /// labeled state — the `Option<Inner>` instance fields backing
    /// `$state.label.<return_value|accepted|cursor|reject_position>` reads.
    fn mode_c_inst_fields(&self) -> Vec<(String, String)> {
        let mut out = Vec::new();
        for st in &self.decl.states {
            let Some(slabel) = &st.label else { continue };
            for m in &st.matches {
                for el in &m.elements {
                    if let MatchElement::Stage(stage) = el {
                        if let (Some(lbl), Some(inner)) = (&stage.label, mode_c_inner(&stage.regex))
                        {
                            out.push((cap_inst_field(slabel, lbl), inner.to_string()));
                        }
                    }
                }
            }
        }
        out
    }

    fn emit_impl(&self, out: &mut String) -> Result<(), String> {
        writeln!(out, "impl {} {{", self.decl.name).ok();
        self.emit_new(out);
        self.emit_tok_id(out);
        if self.has_plain_stage() {
            self.emit_dfa_matcher(out);
        }
        self.emit_run(out);
        self.emit_state_methods(out)?;
        self.emit_embed_matchers(out)?;
        self.emit_action_methods(out)?;
        if self.uses_word_boundary() {
            self.emit_word_boundary_helper(out);
        }
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
        // Constructor signature: input as the cursor-indexable stream, other
        // params typed.
        let mut sig = String::new();
        for (i, p) in self.decl.params.iter().enumerate() {
            if i > 0 {
                sig.push_str(", ");
            }
            if i == 0 {
                write!(sig, "{}: {}", p.name, self.input_type()).ok();
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
        writeln!(out, "            matched: {},", self.matched_empty()).ok();
        out.push_str("            enter: 0,\n");
        for f in self.capture_fields() {
            writeln!(out, "            {}: {},", f, self.matched_empty()).ok();
        }
        for (f, _) in self.mode_c_inst_fields() {
            writeln!(out, "            {}: None,", f).ok();
        }
        out.push_str("        };\n");
        out.push_str("        _m.run();\n");
        out.push_str("        if _m.accepted { _m.reject_position = 0; }\n");
        writeln!(out, "        _m").ok();
        out.push_str("    }\n\n");
        let _ = input;
    }

    /// Token alphabet: emit the token-kind → id lookup used by the
    /// per-element read (unknown token → -1). Emitted only when at least one
    /// stage matcher exists to call it.
    fn emit_tok_id(&self, out: &mut String) {
        if self.alphabet != Alphabet::Token || self.stage_dfas.is_empty() {
            return;
        }
        let mut entries: Vec<(&String, &u32)> = self.token_ids.iter().collect();
        entries.sort_by_key(|(_, id)| **id);
        out.push_str("    fn tok_id(&self, t: &str) -> i64 {\n        match t {\n");
        for (name, id) in entries {
            writeln!(out, "            {:?} => {},", name, id).ok();
        }
        out.push_str("            _ => -1,\n        }\n    }\n\n");
    }

    /// Greedy longest-match DFA executor (mirrors `_dfa_match` in Python).
    fn emit_dfa_matcher(&self, out: &mut String) {
        let input = &self.decl.params[0].name;
        let read = self.element_read();
        writeln!(
            out,
            "    fn dfa_match(&self, states: &[(&[(u32, u32, usize)], bool)], start: usize) -> i64 {{\n\
             \x20       let mut st = start;\n\
             \x20       let mut pos = self.cursor;\n\
             \x20       let n = self.{input}.len();\n\
             \x20       let mut last: i64 = if states[st].1 {{ pos as i64 }} else {{ -1 }};\n\
             \x20       while pos < n {{\n\
             \x20           let v: i64 = {read};\n\
             \x20           let mut nxt: Option<usize> = None;\n\
             \x20           for &(lo, hi, tgt) in states[st].0 {{\n\
             \x20               if (lo as i64) <= v && v <= (hi as i64) {{ nxt = Some(tgt); break; }}\n\
             \x20           }}\n\
             \x20           match nxt {{\n\
             \x20               Some(t) => {{ st = t; pos += 1; if states[st].1 {{ last = pos as i64; }} }}\n\
             \x20               None => break,\n\
             \x20           }}\n\
             \x20       }}\n\
             \x20       last\n\
             \x20   }}\n\n",
            input = input,
            read = read
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
        // `@eof{}` mid-match guard (the `%{}` leave action is a post-scan event
        // keyed on `last`, not on `prev`); track it only when `@eof{}` is
        // present (else it's a dead variable).
        let leave = self.embed_bodies(stage, EmbeddingOp::LeaveAccept, "            ")?;
        let eof = self.embed_bodies(stage, EmbeddingOp::Eof, "            ")?;
        let needs_prev = !eof.is_empty();
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
             \x20           let v: i64 = {read};\n\
             \x20           let mut nxt: Option<usize> = None;\n\
             \x20           for &(lo, hi, tgt) in DFA_{sid}[st].0 {{\n\
             \x20               if (lo as i64) <= v && v <= (hi as i64) {{ nxt = Some(tgt); break; }}\n\
             \x20           }}\n\
             \x20           let Some(t) = nxt else {{ break; }};\n\
             \x20           st = t;\n\
             \x20           pos += 1;\n\
             \x20           self.cursor = pos;",
            read = self.element_read(),
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
        out.push_str("            if _now { last = pos as i64; }\n");
        if needs_prev {
            out.push_str("            prev = _now;\n");
        }
        out.push_str("        }\n");
        // `%{}` — left the last accepting state: a post-scan event firing once
        // when the longest match stops extending (failing element or EOF), with
        // `@@:cursor` at the end of the matched region (`last`), not the failing
        // element (§5.4 / FSM-TEST-603). `last < 0` ⇒ no accepting state was
        // entered, so there is nothing to leave.
        if !leave.is_empty() {
            out.push_str("        if last >= 0 {\n            self.cursor = last as usize;\n");
            out.push_str(&leave);
            out.push_str("        }\n");
        }
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
                    if self.stage_dfas[my_sid].mode_c.is_some() {
                        return Err(
                            "a Mode C (`/@Fsm/`) stage as a `|` alternative selector is not yet \
                             supported by the Rust backend"
                                .into(),
                        );
                    }
                    let MatchElement::Stage(sel) = &m.elements[fs] else {
                        unreachable!("first_stage indexes a Stage element")
                    };
                    let bind = self.r_binding(my_sid);
                    if sel.embedding_actions.is_empty() {
                        self.emit_dfa_const(out, my_sid, "        ");
                        writeln!(
                            out,
                            "        {} = self.dfa_match(DFA_{}, {});",
                            bind, my_sid, self.stage_dfas[my_sid].start
                        )
                        .ok();
                    } else {
                        writeln!(out, "        {} = self.match_stage_{}();", bind, my_sid).ok();
                    }
                    self.emit_anchor_guards(out, my_sid, "        ");
                    out.push_str("        if _r >= 0 {\n");
                    // Committed: record the selector's capture + advance.
                    writeln!(out, "            self.matched = {};", self.matched_slice()).ok();
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

    /// The `let` binding for a stage's match result `_r`. It is `mut` only
    /// when boundary anchors may rewrite it to `-1` (else `let mut` would
    /// draw an `unused_mut` warning in the generated code).
    fn r_binding(&self, sid: usize) -> &'static str {
        let dfa = &self.stage_dfas[sid];
        if dfa.requires_start
            || dfa.requires_end
            || dfa.start_boundary.is_some()
            || dfa.end_boundary.is_some()
        {
            "let mut _r"
        } else {
            "let _r"
        }
    }

    /// After a stage's match result `_r` is computed, enforce its boundary
    /// anchors (§6.6): a leading `^`/`\A` requires the match to start at the
    /// input start (cursor 0); a trailing `$`/`\z` requires it to end at the
    /// input end. A violated anchor turns the match into a miss (`_r = -1`).
    fn emit_anchor_guards(&self, out: &mut String, sid: usize, ind: &str) {
        let dfa = &self.stage_dfas[sid];
        if dfa.requires_start {
            writeln!(out, "{ind}if self.cursor != 0 {{ _r = -1; }}").ok();
        }
        if dfa.requires_end {
            let input = &self.decl.params[0].name;
            writeln!(
                out,
                "{ind}if _r != self.{input}.len() as i64 {{ _r = -1; }}"
            )
            .ok();
        }
        // Word boundaries (§6.6, bytes only): a `\b`/`\B` at the match start
        // (`self.cursor`) / end (`_r`) is satisfied against the live input via
        // `iswordat`. A boundary is present iff the two sides differ in
        // word-ness; `_r >= 0` keeps a prior miss a miss.
        if let Some(kind) = dfa.start_boundary {
            let op = match kind {
                WordBoundary::Required => "==",
                WordBoundary::Forbidden => "!=",
            };
            writeln!(
                out,
                "{ind}if self.iswordat(self.cursor as i64 - 1) {op} self.iswordat(self.cursor as i64) {{ _r = -1; }}"
            )
            .ok();
        }
        if let Some(kind) = dfa.end_boundary {
            let op = match kind {
                WordBoundary::Required => "==",
                WordBoundary::Forbidden => "!=",
            };
            writeln!(
                out,
                "{ind}if _r >= 0 && self.iswordat(_r - 1) {op} self.iswordat(_r) {{ _r = -1; }}"
            )
            .ok();
        }
    }

    /// Does any stage carry a `\b`/`\B` edge boundary? Gates the `iswordat`
    /// helper so it isn't emitted (and flagged dead) when unused.
    fn uses_word_boundary(&self) -> bool {
        self.stage_dfas
            .iter()
            .any(|d| d.start_boundary.is_some() || d.end_boundary.is_some())
    }

    /// `iswordat(p)` — is the byte at input position `p` a word character
    /// (`[0-9A-Za-z_]`)? Out-of-range positions are non-word, so a boundary at
    /// the input edge resolves correctly. Used by the `\b`/`\B` guards.
    fn emit_word_boundary_helper(&self, out: &mut String) {
        let input = &self.decl.params[0].name;
        writeln!(
            out,
            "    fn iswordat(&self, p: i64) -> bool {{\n\
             \x20       if p < 0 || p as usize >= self.{input}.len() {{ return false; }}\n\
             \x20       let b = self.{input}[p as usize];\n\
             \x20       b.is_ascii_alphanumeric() || b == '_'\n\
             \x20   }}\n"
        )
        .ok();
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
        let ind4 = format!("{}    ", ind);
        match el {
            MatchElement::Stage(stage) => {
                let my_sid = *sid;
                *sid += 1;
                if let Some(inner) = self.stage_dfas[my_sid].mode_c.clone() {
                    self.emit_mode_c(out, &inner, stage, m, state_label, ind, &ind4)?;
                    return Ok(());
                }
                // A stage carrying embedding actions matches via its
                // specialized `match_stage_<sid>` (which fires the actions at
                // their DFA positions); a plain stage uses the shared DFA
                // executor over an inline `DFA_<sid>` const. `_r` is `mut`
                // only when boundary anchors may turn the match into a miss.
                let bind = self.r_binding(my_sid);
                if stage.embedding_actions.is_empty() {
                    self.emit_dfa_const(out, my_sid, ind);
                    writeln!(
                        out,
                        "{}{} = self.dfa_match(DFA_{}, {});",
                        ind, bind, my_sid, self.stage_dfas[my_sid].start
                    )
                    .ok();
                } else {
                    writeln!(out, "{}{} = self.match_stage_{}();", ind, bind, my_sid).ok();
                }
                self.emit_anchor_guards(out, my_sid, ind);
                writeln!(out, "{}if _r < 0 {{", ind).ok();
                self.emit_failure(out, m, &ind4)?;
                writeln!(out, "{}}}", ind).ok();
                writeln!(out, "{}self.matched = {};", ind, self.matched_slice()).ok();
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
        let input = &self.decl.params[0].name;
        writeln!(
            out,
            "{}let _inner = {}::new(self.{}[self.cursor..].to_vec());",
            ind, inner, input
        )
        .ok();
        writeln!(out, "{}if !_inner.accepted {{", ind).ok();
        self.emit_failure(out, m, ind4)?;
        writeln!(out, "{}}}", ind).ok();
        // The inner consumed `_inner.cursor` elements; capture before moving.
        writeln!(out, "{}let _icur = _inner.cursor;", ind).ok();
        let slice = match self.alphabet {
            Alphabet::Token => format!(
                "self.{}[self.cursor..(self.cursor + _icur)].to_vec()",
                input
            ),
            _ => format!(
                "self.{}[self.cursor..(self.cursor + _icur)].iter().collect()",
                input
            ),
        };
        writeln!(out, "{}self.matched = {};", ind, slice).ok();
        if let Some(lbl) = &stage.label {
            if !state_label.is_empty() {
                writeln!(
                    out,
                    "{}self.{} = self.matched.clone();",
                    ind,
                    cap_field(state_label, lbl)
                )
                .ok();
                writeln!(
                    out,
                    "{}self.{} = Some(_inner);",
                    ind,
                    cap_inst_field(state_label, lbl)
                )
                .ok();
            }
        }
        writeln!(out, "{}self.cursor = self.cursor + _icur;", ind).ok();
        writeln!(out, "{}self.accepted = true;", ind).ok();
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
                // Mode C (§8.3): `$state.label.<fsm field>` reads the inner
                // fsm instance recorded for that stage, not the matched slice.
                // (`$state.label` alone is the slice — handled by the Var arm.)
                if let Expression::Var(name) = object.as_ref() {
                    if let Some((state, label)) =
                        name.strip_prefix('$').and_then(|c| c.split_once('.'))
                    {
                        if matches!(
                            field.as_str(),
                            "return_value" | "accepted" | "cursor" | "reject_position"
                        ) {
                            return format!(
                                "self.{}.as_ref().unwrap().{}",
                                cap_inst_field(state, label),
                                field
                            );
                        }
                    }
                }
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

/// Struct field name holding a Mode C inner fsm instance:
/// `$state.label` → `cap_inst_state_label`.
fn cap_inst_field(state: &str, label: &str) -> String {
    format!("cap_inst_{}_{}", state, label)
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

    /// FSM-TEST-603 — `%{...}` fires when the DFA leaves its last accepting
    /// state, capturing the end of the matched region. For `/[0-9]+/` over
    /// "42x" that is `@@:cursor == 2`, not the failing `x` position.
    #[test]
    fn rust_embed_leave_final() {
        let src = "@@fsm M(text: bytes) : int = 0 { \
                   /[0-9]+/ %{ self.end_pos = @@:cursor } self.end_pos \
                   domain: end_pos: int = 0 }";
        let Some((_, ret)) = run(src, "M", "42x", "emb_l") else {
            return;
        };
        assert_eq!(ret, "2");
        assert_eq!(run(src, "M", "abx", "emb_l2").unwrap().1, "0"); // never accepting → no fire
    }

    /// Mode C call-out (RFC-0042 §8.3): `/@Inner/` constructs the inner fsm
    /// over the input at the cursor, advances by what it consumed, and exposes
    /// the inner instance via `$state.label.return_value`.
    #[test]
    fn rust_mode_c_callout() {
        let inner_src = "@@fsm Digits(text: bytes) : int = 0 { /[0-9]+/ to_int(@@:matched) }";
        let outer_src = "@@fsm Outer(text: bytes) : int = 0 { $s: .d/@Digits/ $s.d.return_value }";
        let inner = generate(&parse_fsm_block(inner_src.as_bytes()).expect("inner parses"))
            .expect("inner generates");
        let outer = generate(&parse_fsm_block(outer_src.as_bytes()).expect("outer parses"))
            .expect("outer generates");
        let run_outer = |inp: &str, tag: &str| -> Option<(String, String)> {
            let driver = format!(
                "{inner}\n{outer}\nfn main() {{ let m = Outer::new(\"{inp}\".chars().collect()); println!(\"{{}}\", m.accepted); println!(\"{{}}\", m.return_value); }}\n",
                inner = inner, outer = outer, inp = inp
            );
            let dir = std::env::temp_dir();
            let s = dir.join(format!("framec_rs_{}.rs", tag));
            let b = dir.join(format!("framec_rs_{}", tag));
            std::fs::write(&s, driver).ok()?;
            let c = Command::new("rustc")
                .arg("-O")
                .arg("--edition=2021")
                .arg(&s)
                .arg("-o")
                .arg(&b)
                .output()
                .ok()?;
            assert!(c.status.success(), "{}", String::from_utf8_lossy(&c.stderr));
            let o = Command::new(&b).output().expect("run");
            let text = String::from_utf8_lossy(&o.stdout);
            let lines: Vec<&str> = text.lines().collect();
            Some((lines[0].to_string(), lines[1].to_string()))
        };
        let Some((acc, ret)) = run_outer("42", "modec_a") else {
            return;
        };
        assert_eq!((acc.as_str(), ret.as_str()), ("true", "42"));
        // "x": inner Digits rejects → outer Mode C stage fails → reject.
        assert_eq!(run_outer("x", "modec_b").unwrap().0, "false");
    }

    /// Token alphabet (FSM-TEST-253): the input is a sequence of token
    /// kinds; regex identifiers reference token kinds, not characters.
    #[test]
    fn rust_token_alphabet() {
        let src = "@@fsm M(toks: token) : bool = false { /IDENT LPAREN RPAREN/ true }";
        let decl = parse_fsm_block(src.as_bytes()).expect("parses");
        let code = generate(&decl).expect("generates");
        let run_toks = |toks: &[&str], tag: &str| -> Option<String> {
            let items: Vec<String> = toks
                .iter()
                .map(|t| format!("{:?}.to_string()", t))
                .collect();
            let driver = format!(
                "{code}\nfn main() {{ let m = M::new(vec![{items}]); println!(\"{{}}\", m.accepted); }}\n",
                code = code,
                items = items.join(", ")
            );
            let dir = std::env::temp_dir();
            let s = dir.join(format!("framec_rs_{}.rs", tag));
            let b = dir.join(format!("framec_rs_{}", tag));
            std::fs::write(&s, driver).ok()?;
            let c = Command::new("rustc")
                .arg("-O")
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
        let Some(ok) = run_toks(&["IDENT", "LPAREN", "RPAREN"], "tok_seq") else {
            return;
        };
        assert_eq!(ok, "true");
        // Wrong token sequence → not in the language.
        assert_eq!(run_toks(&["IDENT", "RPAREN"], "tok_bad").unwrap(), "false");
        // An unknown token kind never matches a transition.
        assert_eq!(
            run_toks(&["IDENT", "WAT", "RPAREN"], "tok_unk").unwrap(),
            "false"
        );
    }

    /// FSM-TEST-312 — leading `^` matches only at cursor 0. `/^foo/` accepts
    /// "foo", rejects "xfoo" and "".
    #[test]
    fn rust_start_anchor() {
        let src = "@@fsm M(text: bytes) : bool = false { /^foo/ true }";
        let Some((acc, _)) = run(src, "M", "foo", "anc_a") else {
            return;
        };
        assert_eq!(acc, "true");
        assert_eq!(run(src, "M", "xfoo", "anc_b").unwrap().0, "false");
        assert_eq!(run(src, "M", "", "anc_c").unwrap().0, "false");
    }

    /// Edge word boundaries (§6.6): `/\bcat\b/` matches "cat" (boundaries at
    /// both input edges) but not "cats" (the trailing `\b` fails between the
    /// two word bytes `t` and `s`).
    #[test]
    fn rust_word_boundary() {
        let src = "@@fsm M(text: bytes) : bool = false { /\\bcat\\b/ true }";
        let Some((acc, _)) = run(src, "M", "cat", "wb_a") else {
            return;
        };
        assert_eq!(acc, "true");
        assert_eq!(run(src, "M", "cats", "wb_b").unwrap().0, "false");
    }

    /// A trailing `$` requires the match to reach the end of input.
    #[test]
    fn rust_end_anchor() {
        let src = "@@fsm M(text: bytes) : bool = false { /[0-9]+$/ true }";
        let Some((acc, _)) = run(src, "M", "123", "anc_e") else {
            return;
        };
        assert_eq!(acc, "true");
        // "123x": digits don't reach end-of-input → `$` fails → reject.
        assert_eq!(run(src, "M", "123x", "anc_f").unwrap().0, "false");
    }

    /// A mid-pattern anchor is outside the v0.1 cut and errors clearly.
    #[test]
    fn rust_unsupported_errors() {
        let decl =
            parse_fsm_block(b"@@fsm M(text: bytes) : bool = false { /a$b/ true }").expect("parses");
        let err = generate(&decl).unwrap_err();
        assert!(err.contains("anchor"), "got {err}");
    }
}
