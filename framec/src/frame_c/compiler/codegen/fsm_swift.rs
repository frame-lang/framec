//! Swift backend for `@@fsm` (RFC-0042, Phase 8).
//!
//! Generates a self-contained Swift `class` from a validated `FsmDeclAst`,
//! following the same recognition model as the Java backend
//! ([`super::fsm_java`]): a per-state dispatch loop (`switch`) over mutable
//! properties. Frame's abstract types map as `int` → `Int`, `float` →
//! `Double`, `bool` → `Bool`, `str`/`bytes` → `String`. The constructor is
//! `<Name>(...)`; the observable result (§5.1) is the instance's `accepted`,
//! `return_value`, `cursor`, `reject_position`.
//!
//! Swift's gotcha: a `String` is not integer-subscriptable. The `bytes`/
//! `char` input is therefore held as an `[Character]` (`Array(text)`) so the
//! cursor indexes it directly; a code point is `text[pos].unicodeScalars
//! .first!.value`, and the matched run is `String(text[cursor..<end])`. The
//! `token` input is a `[String]` mapped to small integer ids; `.count`
//! unifies the length of both.
//!
//! # v0.1 scope
//!
//! Full parity with the Python reference backend: single-match and
//! multi-match (`|`) ordered-choice states, captures, bare-expression
//! returns, action blocks, declared `actions:` methods, all transition
//! forms, embedding actions, Mode C sub-fsm call-out, all three alphabets,
//! and boundary anchors. Not yet handled (clear `Unsupported` error):
//! mid-pattern anchors and `\b`/`\B`, a Mode C stage as a `|` selector, and a
//! `|` alternative with elements before its first stage.

use crate::frame_c::compiler::frame_ast::{
    BinaryOp, EmbeddingOp, Expression, FsmDeclAst, FsmStateAst, FsmTransitionTarget, Literal,
    MatchAst, MatchElement, StageAst, Type, UnaryOp,
};
use crate::frame_c::compiler::fsm_regex::{
    self, size_check::DEFAULT_MAX_DFA_STATES, subset::DfaLabel, Alphabet, CompileError,
};
use std::fmt::Write;

/// Generate Swift source implementing `decl`, or a reason it is outside the
/// v0.1 Swift cut.
pub fn generate(decl: &FsmDeclAst) -> Result<String, String> {
    Generator::new(decl)?.emit()
}

/// One stage's compiled DFA, flattened for emission.
struct StageDfa {
    states: Vec<(Vec<(u32, u32, usize)>, bool)>,
    start: usize,
    requires_start: bool,
    requires_end: bool,
    mode_c: Option<String>,
}

/// A Mode C stage's regex body starts with `@`; the rest names the inner fsm.
fn mode_c_inner(regex: &str) -> Option<&str> {
    regex.strip_prefix('@')
}

struct Generator<'a> {
    decl: &'a FsmDeclAst,
    alphabet: Alphabet,
    label_to_index: std::collections::HashMap<String, usize>,
    stage_entry: std::collections::HashMap<(String, String), usize>,
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

    fn compile_stage_dfas(&mut self) -> Result<(), String> {
        let mut token_ids = std::mem::take(&mut self.token_ids);
        for st in &self.decl.states {
            for m in &st.matches {
                for el in &m.elements {
                    if let MatchElement::Stage(stage) = el {
                        if let Some(inner) = mode_c_inner(&stage.regex) {
                            self.stage_dfas.push(StageDfa {
                                states: Vec::new(),
                                start: 0,
                                requires_start: false,
                                requires_end: false,
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
                "regex `/{}/` uses a mid-pattern or word-boundary anchor, not yet supported by the \
                 Swift backend",
                regex
            )),
        }
    }

    fn sw_type(t: &Type) -> String {
        let s = match t {
            Type::Custom(s) => s.as_str(),
            _ => "Any",
        };
        match s {
            "int" => "Int".to_string(),
            "float" => "Double".to_string(),
            "bool" => "Bool".to_string(),
            "str" | "string" | "String" | "bytes" => "String".to_string(),
            other => other.to_string(),
        }
    }

    /// The input field type: `[Character]` (byte/char, integer-indexable) or
    /// `[String]` (token).
    fn input_field_type(&self) -> &'static str {
        match self.alphabet {
            Alphabet::Token => "[String]",
            _ => "[Character]",
        }
    }

    /// The input constructor-parameter type (before any materialization).
    fn input_ctor_type(&self) -> &'static str {
        match self.alphabet {
            Alphabet::Token => "[String]",
            _ => "String",
        }
    }

    fn matched_type(&self) -> &'static str {
        match self.alphabet {
            Alphabet::Token => "[String]",
            _ => "String",
        }
    }

    fn matched_empty(&self) -> &'static str {
        match self.alphabet {
            Alphabet::Token => "[]",
            _ => "\"\"",
        }
    }

    fn element_read(&self) -> String {
        let inp = &self.decl.params[0].name;
        match self.alphabet {
            Alphabet::Token => format!("tokId({}[pos])", inp),
            _ => format!("Int({}[pos].unicodeScalars.first!.value)", inp),
        }
    }

    /// The matched run `[cursor, end)`: a `String` (byte/char) or `[String]`
    /// (token) materialized from the `[Character]`/`[String]` slice.
    fn matched_slice(&self, end: &str) -> String {
        let inp = &self.decl.params[0].name;
        match self.alphabet {
            Alphabet::Token => format!("Array({}[cursor..<({})])", inp, end),
            _ => format!("String({}[cursor..<({})])", inp, end),
        }
    }

    fn emit(&self) -> Result<String, String> {
        let mut out = String::new();
        out.push_str("// Generated by framec — RFC-0042 @@fsm (Swift backend).\n\n");
        writeln!(out, "class {} {{", self.decl.name).ok();
        self.emit_properties(&mut out);
        self.emit_ctor(&mut out);
        self.emit_tok_id(&mut out);
        self.emit_dfa_matcher(&mut out);
        self.emit_run(&mut out);
        self.emit_state_methods(&mut out)?;
        self.emit_embed_matchers(&mut out)?;
        self.emit_action_methods(&mut out)?;
        out.push_str("}\n");
        Ok(out)
    }

    /// All stored properties carry a default at declaration (so the
    /// constructor only assigns the parameters + runs); the input is set from
    /// its converted parameter in `init`.
    fn emit_properties(&self, out: &mut String) {
        out.push_str("  var accepted: Bool = false\n");
        out.push_str("  var reject_position: Int = 0\n");
        out.push_str("  var cursor: Int = 0\n");
        writeln!(
            out,
            "  var return_value: {} = {}",
            Self::sw_type(&self.decl.return_type),
            sw_default(&self.decl.return_type, &self.decl.default_expr)
        )
        .ok();
        let mut seen = std::collections::HashSet::new();
        for (i, p) in self.decl.params.iter().enumerate() {
            seen.insert(p.name.clone());
            if i == 0 {
                writeln!(out, "  var {}: {} = []", p.name, self.input_field_type()).ok();
            } else {
                writeln!(
                    out,
                    "  var {}: {} = {}",
                    p.name,
                    Self::sw_type(&p.param_type),
                    default_for(&p.param_type)
                )
                .ok();
            }
        }
        if let Some(domain) = &self.decl.domain {
            for v in &domain.vars {
                if !seen.insert(v.name.clone()) {
                    continue;
                }
                writeln!(
                    out,
                    "  var {}: {} = {}",
                    v.name,
                    Self::sw_type(&v.var_type),
                    self.expr(&v.default)
                )
                .ok();
            }
        }
        writeln!(
            out,
            "  var matched: {} = {}",
            self.matched_type(),
            self.matched_empty()
        )
        .ok();
        out.push_str("  var enter: Int = 0\n");
        for f in self.capture_fields() {
            writeln!(
                out,
                "  var {}: {} = {}",
                f,
                self.matched_type(),
                self.matched_empty()
            )
            .ok();
        }
        for (f, inner) in self.mode_c_inst_fields() {
            writeln!(out, "  var {}: {}? = nil", f, inner).ok();
        }
    }

    fn emit_ctor(&self, out: &mut String) {
        let input = &self.decl.params[0].name;
        let sig: Vec<String> = self
            .decl
            .params
            .iter()
            .enumerate()
            .map(|(i, p)| {
                let ty = if i == 0 {
                    self.input_ctor_type().to_string()
                } else {
                    Self::sw_type(&p.param_type)
                };
                format!("_ {}: {}", p.name, ty)
            })
            .collect();
        writeln!(out, "  init({}) {{", sig.join(", ")).ok();
        for (i, p) in self.decl.params.iter().enumerate() {
            if i == 0 && self.alphabet != Alphabet::Token {
                // Materialize the byte/char input into an `[Character]`.
                writeln!(out, "    self.{} = Array({})", p.name, p.name).ok();
            } else {
                writeln!(out, "    self.{} = {}", p.name, p.name).ok();
            }
        }
        if let Some(domain) = &self.decl.domain {
            for v in &domain.vars {
                if &v.name == input {
                    continue;
                }
                writeln!(out, "    self.{} = {}", v.name, self.expr(&v.default)).ok();
            }
        }
        out.push_str("    run()\n");
        out.push_str("    if accepted { reject_position = 0 }\n");
        out.push_str("  }\n\n");
    }

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

    fn emit_tok_id(&self, out: &mut String) {
        if self.alphabet != Alphabet::Token {
            return;
        }
        let mut entries: Vec<(&String, &u32)> = self.token_ids.iter().collect();
        entries.sort_by_key(|(_, id)| **id);
        out.push_str("  func tokId(_ t: String) -> Int {\n    switch t {\n");
        for (name, id) in entries {
            writeln!(out, "    case {:?}: return {}", name, id).ok();
        }
        out.push_str("    default: return -1\n    }\n  }\n\n");
    }

    fn emit_dfa_matcher(&self, out: &mut String) {
        let inp = &self.decl.params[0].name;
        let read = self.element_read();
        writeln!(
            out,
            "  func dfaMatch(_ trans: [[[Int]]], _ accept: [Bool], _ start: Int) -> Int {{\n\
             \x20   var st = start\n\
             \x20   var pos = cursor\n\
             \x20   let n = {inp}.count\n\
             \x20   var last = accept[st] ? pos : -1\n\
             \x20   while pos < n {{\n\
             \x20     let v = {read}\n\
             \x20     var nxt = -1\n\
             \x20     for tr in trans[st] {{ if tr[0] <= v && v <= tr[1] {{ nxt = tr[2]; break }} }}\n\
             \x20     if nxt < 0 {{ break }}\n\
             \x20     st = nxt; pos += 1\n\
             \x20     if accept[st] {{ last = pos }}\n\
             \x20   }}\n\
             \x20   return last\n\
             \x20 }}\n\n",
            inp = inp,
            read = read
        )
        .ok();
    }

    fn emit_run(&self, out: &mut String) {
        out.push_str("  func run() {\n    var state = 0\n");
        out.push_str("    while state >= 0 {\n");
        out.push_str("      let _enter = enter\n      enter = 0\n");
        out.push_str("      switch state {\n");
        for i in 0..self.decl.states.len() {
            writeln!(out, "      case {}: state = state{}(_enter)", i, i).ok();
        }
        out.push_str("      default: return\n      }\n    }\n  }\n\n");
    }

    fn emit_state_methods(&self, out: &mut String) -> Result<(), String> {
        let mut sid = 0usize;
        for (i, st) in self.decl.states.iter().enumerate() {
            match st.matches.len() {
                0 => {
                    writeln!(
                        out,
                        "  func state{}(_ _enter: Int) -> Int {{ return -1 }}\n",
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
        writeln!(out, "  func state{}(_ _enter: Int) -> Int {{", index).ok();
        for (idx, el) in m.elements.iter().enumerate() {
            writeln!(out, "    if _enter <= {} {{", idx).ok();
            self.emit_element(out, el, m, &state_label, "      ", sid)?;
            out.push_str("    }\n");
        }
        self.emit_success(out, m, "    ");
        out.push_str("  }\n\n");
        Ok(())
    }

    fn emit_multi_match(
        &self,
        out: &mut String,
        index: usize,
        st: &FsmStateAst,
        sid: &mut usize,
    ) -> Result<(), String> {
        let state_label = st.label.clone().unwrap_or_default();
        writeln!(out, "  func state{}(_ _enter: Int) -> Int {{", index).ok();
        let mut catch_all = false;
        for m in &st.matches {
            let first_stage = m
                .elements
                .iter()
                .position(|e| matches!(e, MatchElement::Stage(_)));
            match first_stage {
                Some(fs) => {
                    if fs > 0 {
                        return Err(
                            "a `|` alternative with elements before its first stage is not yet \
                             supported by the Swift backend"
                                .into(),
                        );
                    }
                    let my_sid = *sid;
                    *sid += 1;
                    if self.stage_dfas[my_sid].mode_c.is_some() {
                        return Err(
                            "a Mode C (`/@Fsm/`) stage as a `|` alternative selector is not yet \
                             supported by the Swift backend"
                                .into(),
                        );
                    }
                    let MatchElement::Stage(sel) = &m.elements[fs] else {
                        unreachable!("first_stage indexes a Stage element")
                    };
                    self.emit_dfa_decls(out, my_sid, "    ");
                    writeln!(
                        out,
                        "    {} _r{} = dfaMatch(t{}, a{}, {})",
                        self.r_kw(my_sid),
                        my_sid,
                        my_sid,
                        my_sid,
                        self.stage_dfas[my_sid].start
                    )
                    .ok();
                    self.emit_anchor_guards(out, my_sid, "    ");
                    writeln!(out, "    if _r{} >= 0 {{", my_sid).ok();
                    writeln!(
                        out,
                        "      matched = {}",
                        self.matched_slice(&format!("_r{}", my_sid))
                    )
                    .ok();
                    if let Some(lbl) = &sel.label {
                        if !state_label.is_empty() {
                            writeln!(out, "      {} = matched", cap_field(&state_label, lbl)).ok();
                        }
                    }
                    writeln!(out, "      cursor = _r{}", my_sid).ok();
                    out.push_str("      accepted = true\n");
                    for el in &m.elements[fs + 1..] {
                        self.emit_element(out, el, m, &state_label, "      ", sid)?;
                    }
                    self.emit_success(out, m, "      ");
                    out.push_str("    }\n");
                }
                None => {
                    out.push_str("    accepted = true\n");
                    for el in &m.elements {
                        self.emit_element(out, el, m, &state_label, "    ", sid)?;
                    }
                    self.emit_success(out, m, "    ");
                    catch_all = true;
                    break;
                }
            }
        }
        if !catch_all {
            out.push_str("    accepted = false\n");
            out.push_str("    reject_position = cursor\n");
            out.push_str("    return -1\n");
        }
        out.push_str("  }\n\n");
        Ok(())
    }

    /// `let`/`var` for a stage's match-result, depending on whether boundary
    /// anchors reassign it.
    fn r_kw(&self, sid: usize) -> &'static str {
        let d = &self.stage_dfas[sid];
        if d.requires_start || d.requires_end {
            "var"
        } else {
            "let"
        }
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
        let ind2 = format!("{}  ", ind);
        match el {
            MatchElement::Stage(stage) => {
                let my_sid = *sid;
                *sid += 1;
                if let Some(inner) = self.stage_dfas[my_sid].mode_c.clone() {
                    self.emit_mode_c(out, &inner, stage, m, state_label, my_sid, ind, &ind2);
                    return Ok(());
                }
                if stage.embedding_actions.is_empty() {
                    self.emit_dfa_decls(out, my_sid, ind);
                    writeln!(
                        out,
                        "{}{} _r{} = dfaMatch(t{}, a{}, {})",
                        ind,
                        self.r_kw(my_sid),
                        my_sid,
                        my_sid,
                        my_sid,
                        self.stage_dfas[my_sid].start
                    )
                    .ok();
                } else {
                    writeln!(
                        out,
                        "{}{} _r{} = matchStage{}()",
                        ind,
                        self.r_kw(my_sid),
                        my_sid,
                        my_sid
                    )
                    .ok();
                }
                self.emit_anchor_guards(out, my_sid, ind);
                writeln!(out, "{}if _r{} < 0 {{", ind, my_sid).ok();
                self.emit_failure(out, m, &ind2);
                writeln!(out, "{}}}", ind).ok();
                writeln!(
                    out,
                    "{}matched = {}",
                    ind,
                    self.matched_slice(&format!("_r{}", my_sid))
                )
                .ok();
                if let Some(lbl) = &stage.label {
                    if !state_label.is_empty() {
                        writeln!(out, "{}{} = matched", ind, cap_field(state_label, lbl)).ok();
                    }
                }
                writeln!(out, "{}cursor = _r{}", ind, my_sid).ok();
                writeln!(out, "{}accepted = true", ind).ok();
            }
            MatchElement::BareExpression { expr, .. } => {
                writeln!(out, "{}return_value = {}", ind, self.expr(expr)).ok();
            }
            MatchElement::ActionBlock(blk) => {
                for s in &blk.statements {
                    out.push_str(&self.stmt(s, ind)?);
                }
            }
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn emit_mode_c(
        &self,
        out: &mut String,
        inner: &str,
        stage: &StageAst,
        m: &MatchAst,
        state_label: &str,
        my_sid: usize,
        ind: &str,
        ind2: &str,
    ) {
        let input = &self.decl.params[0].name;
        let iv = format!("inner{}", my_sid);
        let sub = match self.alphabet {
            Alphabet::Token => format!("Array({}[cursor...])", input),
            _ => format!("String({}[cursor...])", input),
        };
        writeln!(out, "{}let {} = {}({})", ind, iv, inner, sub).ok();
        writeln!(out, "{}if !{}.accepted {{", ind, iv).ok();
        self.emit_failure(out, m, ind2);
        writeln!(out, "{}}}", ind).ok();
        let end = format!("cursor + {}.cursor", iv);
        writeln!(out, "{}matched = {}", ind, self.matched_slice(&end)).ok();
        if let Some(lbl) = &stage.label {
            if !state_label.is_empty() {
                writeln!(out, "{}{} = matched", ind, cap_field(state_label, lbl)).ok();
                writeln!(out, "{}{} = {}", ind, cap_inst_field(state_label, lbl), iv).ok();
            }
        }
        writeln!(out, "{}cursor = cursor + {}.cursor", ind, iv).ok();
        writeln!(out, "{}accepted = true", ind).ok();
    }

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

    fn emit_one_matcher(
        &self,
        out: &mut String,
        sid: usize,
        stage: &StageAst,
    ) -> Result<(), String> {
        let inp = &self.decl.params[0].name;
        let read = self.element_read();
        writeln!(out, "  func matchStage{}() -> Int {{", sid).ok();
        self.emit_dfa_decls(out, sid, "    ");
        writeln!(
            out,
            "    let entry = cursor\n\
             \x20   var st = {start}\n\
             \x20   var pos = entry\n\
             \x20   let n = {inp}.count\n\
             \x20   var last = a{sid}[st] ? pos : -1\n\
             \x20   cursor = pos",
            start = self.stage_dfas[sid].start,
            inp = inp,
            sid = sid
        )
        .ok();
        out.push_str(&self.embed_body(stage, EmbeddingOp::Start, "    ")?);
        writeln!(out, "    var prev = a{}[st]", sid).ok();
        writeln!(
            out,
            "    while pos < n {{\n\
             \x20     let v = {read}\n\
             \x20     var nxt = -1\n\
             \x20     for tr in t{sid}[st] {{ if tr[0] <= v && v <= tr[1] {{ nxt = tr[2]; break }} }}\n\
             \x20     if nxt < 0 {{ break }}\n\
             \x20     st = nxt; pos += 1\n\
             \x20     cursor = pos",
            read = read,
            sid = sid
        )
        .ok();
        out.push_str(&self.embed_body(stage, EmbeddingOp::EveryTransition, "      ")?);
        writeln!(out, "      let now = a{}[st]", sid).ok();
        let accept = self.embed_body(stage, EmbeddingOp::Accept, "        ")?;
        if !accept.is_empty() {
            out.push_str("      if now {\n");
            out.push_str(&accept);
            out.push_str("      }\n");
        }
        out.push_str("      if now { last = pos }\n      prev = now\n");
        out.push_str("    }\n");
        // `%{}` — left the last accepting state: a post-scan event firing once
        // when the longest match stops extending (failing element or EOF), with
        // `@@:cursor` at the end of the matched region (`last`), not the failing
        // element (§5.4 / FSM-TEST-603). `last < 0` ⇒ no accepting state was
        // entered, so there is nothing to leave.
        let leave = self.embed_body(stage, EmbeddingOp::LeaveAccept, "      ")?;
        if !leave.is_empty() {
            out.push_str("    if last >= 0 {\n      cursor = last\n");
            out.push_str(&leave);
            out.push_str("    }\n");
        }
        let eof = self.embed_body(stage, EmbeddingOp::Eof, "      ")?;
        if !eof.is_empty() {
            out.push_str("    if pos >= n && !prev {\n");
            out.push_str(&eof);
            out.push_str("    }\n");
        } else {
            // `prev` is otherwise only read by the `%{}` guard; reference it
            // so Swift does not warn about a never-read variable.
            out.push_str("    _ = prev\n");
        }
        out.push_str("    cursor = entry\n    return last\n  }\n\n");
        Ok(())
    }

    fn embed_body(&self, stage: &StageAst, op: EmbeddingOp, ind: &str) -> Result<String, String> {
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

    fn emit_anchor_guards(&self, out: &mut String, sid: usize, ind: &str) {
        let dfa = &self.stage_dfas[sid];
        let inp = &self.decl.params[0].name;
        if dfa.requires_start {
            writeln!(out, "{}if cursor != 0 {{ _r{} = -1 }}", ind, sid).ok();
        }
        if dfa.requires_end {
            writeln!(
                out,
                "{}if _r{} != {}.count {{ _r{} = -1 }}",
                ind, sid, inp, sid
            )
            .ok();
        }
    }

    fn emit_success(&self, out: &mut String, m: &MatchAst, ind: &str) {
        match m.transition.as_ref().and_then(|c| c.success.as_ref()) {
            None => {
                writeln!(out, "{}return -1", ind).ok();
            }
            Some(target) => self.emit_target(out, target, ind, m),
        }
    }

    fn emit_failure(&self, out: &mut String, m: &MatchAst, ind: &str) {
        writeln!(out, "{}accepted = false", ind).ok();
        writeln!(out, "{}reject_position = cursor", ind).ok();
        match m.transition.as_ref().and_then(|c| c.failure.as_ref()) {
            None => {
                writeln!(out, "{}return -1", ind).ok();
            }
            Some(target) => self.emit_target(out, target, ind, m),
        }
    }

    fn emit_target(&self, out: &mut String, target: &FsmTransitionTarget, ind: &str, m: &MatchAst) {
        match target {
            FsmTransitionTarget::Static { state, stage, .. } => {
                self.emit_goto(out, state, stage, ind);
            }
            FsmTransitionTarget::Conditional(alts) => {
                for alt in alts {
                    writeln!(out, "{}if {} {{", ind, self.expr(&alt.condition)).ok();
                    if let FsmTransitionTarget::Static { state, stage, .. } = &alt.target {
                        self.emit_goto(out, state, stage, &format!("{}  ", ind));
                    }
                    writeln!(out, "{}}}", ind).ok();
                }
                self.emit_failure(out, m, ind);
            }
        }
    }

    fn emit_goto(&self, out: &mut String, state: &str, stage: &Option<String>, ind: &str) {
        let idx = self
            .label_to_index
            .get(state)
            .copied()
            .unwrap_or(usize::MAX);
        if idx == usize::MAX {
            writeln!(
                out,
                "{}fatalError(\"transition to undeclared state ${}\")",
                ind, state
            )
            .ok();
            return;
        }
        if let Some(s) = stage {
            match self
                .stage_entry
                .get(&(state.to_string(), s.clone()))
                .copied()
            {
                Some(entry) => {
                    writeln!(out, "{}enter = {}", ind, entry).ok();
                }
                None => {
                    writeln!(
                        out,
                        "{}fatalError(\"transition to undeclared stage ${}.{}\")",
                        ind, state, s
                    )
                    .ok();
                    return;
                }
            }
        }
        writeln!(out, "{}return {}", ind, idx).ok();
    }

    fn stmt(
        &self,
        s: &crate::frame_c::compiler::frame_ast::Statement,
        ind: &str,
    ) -> Result<String, String> {
        use crate::frame_c::compiler::frame_ast::Statement;
        match s {
            Statement::Expression(e) => Ok(format!("{}{}\n", ind, self.expr(&e.expr))),
            Statement::If(if_ast) => {
                let inner = format!("{}  ", ind);
                let mut out = format!("{}if {} {{\n", ind, self.expr(&if_ast.condition));
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
                "statement {:?} not supported in @@fsm action blocks by the Swift backend",
                std::mem::discriminant(other)
            )),
        }
    }

    fn emit_action_methods(&self, out: &mut String) -> Result<(), String> {
        let Some(block) = &self.decl.actions else {
            return Ok(());
        };
        for act in &block.actions {
            let sig: Vec<String> = act
                .params
                .iter()
                .map(|p| format!("_ {}: {}", p.name, Self::sw_type(&p.param_type)))
                .collect();
            let ret = match &act.return_type {
                Some(t) => format!(" -> {}", Self::sw_type(t)),
                None => String::new(),
            };
            writeln!(out, "  func {}({}){} {{", act.name, sig.join(", "), ret).ok();
            let n = act.body.statements.len();
            let has_return = act.return_type.is_some();
            for (i, s) in act.body.statements.iter().enumerate() {
                use crate::frame_c::compiler::frame_ast::Statement;
                if i + 1 == n && has_return {
                    if let Statement::Expression(e) = s {
                        if !matches!(e.expr, Expression::Assign { .. }) {
                            writeln!(out, "    return {}", self.expr(&e.expr)).ok();
                            continue;
                        }
                    }
                }
                out.push_str(&self.stmt(s, "    ")?);
            }
            out.push_str("  }\n\n");
        }
        Ok(())
    }

    /// Emit the per-stage DFA as two parallel locals: `t<sid>: [[[Int]]]`
    /// (transitions) and `a<sid>: [Bool]` (accept).
    fn emit_dfa_decls(&self, out: &mut String, sid: usize, ind: &str) {
        let dfa = &self.stage_dfas[sid];
        let trans: Vec<String> = dfa
            .states
            .iter()
            .map(|(t, _)| {
                let ts: Vec<String> = t
                    .iter()
                    .map(|(lo, hi, tgt)| format!("[{}, {}, {}]", lo, hi, tgt))
                    .collect();
                format!("[{}]", ts.join(", "))
            })
            .collect();
        let accept: Vec<String> = dfa.states.iter().map(|(_, a)| a.to_string()).collect();
        writeln!(
            out,
            "{}let t{}: [[[Int]]] = [{}]",
            ind,
            sid,
            trans.join(", ")
        )
        .ok();
        writeln!(out, "{}let a{}: [Bool] = [{}]", ind, sid, accept.join(", ")).ok();
    }

    fn expr(&self, e: &Expression) -> String {
        match e {
            Expression::Literal(l) => match l {
                Literal::Int(i) => i.to_string(),
                Literal::Float(f) => f.to_string(),
                Literal::Bool(b) => b.to_string(),
                Literal::String(s) => format!("{:?}", s),
                Literal::Null => "nil".to_string(),
            },
            Expression::Var(name) => match name.as_str() {
                "@@:matched" => "matched".to_string(),
                "@@:cursor" => "cursor".to_string(),
                "@@:return" => "return_value".to_string(),
                _ => match name.strip_prefix('$').and_then(|c| c.split_once('.')) {
                    Some((state, label)) => cap_field(state, label),
                    None => name.clone(),
                },
            },
            Expression::Binary { left, op, right } => {
                format!("({} {} {})", self.expr(left), binop(op), self.expr(right))
            }
            Expression::Unary { op, expr } => match op {
                UnaryOp::Not => format!("(!{})", self.expr(expr)),
                UnaryOp::Neg => format!("(-{})", self.expr(expr)),
                UnaryOp::BitNot => format!("(~{})", self.expr(expr)),
            },
            Expression::Call { func, args } => self.call(func, args),
            Expression::Member { object, field } => {
                if let Expression::Var(name) = object.as_ref() {
                    // Mode C (§8.3): `$state.label.<fsm field>` reads the
                    // inner fsm instance recorded for that stage.
                    if let Some((state, label)) =
                        name.strip_prefix('$').and_then(|c| c.split_once('.'))
                    {
                        if matches!(
                            field.as_str(),
                            "return_value" | "accepted" | "cursor" | "reject_position"
                        ) {
                            return format!("{}!.{}", cap_inst_field(state, label), field);
                        }
                    }
                    if name == "self" {
                        return field.to_string();
                    }
                }
                format!("{}.{}", self.expr(object), field)
            }
            Expression::Index { object, index } => {
                format!("{}[{}]", self.expr(object), self.expr(index))
            }
            Expression::Assign { target, value } => {
                format!("{} = {}", self.expr(target), self.expr(value))
            }
            Expression::NativeExpr(s) => s.clone(),
        }
    }

    fn call(&self, func: &str, args: &[Expression]) -> String {
        let a: Vec<String> = args.iter().map(|e| self.expr(e)).collect();
        match func {
            "to_int" => format!("(Int({}) ?? 0)", a.join(", ")),
            "to_str" => format!("String({})", a.join(", ")),
            "len" => format!("({}).count", a.join(", ")),
            _ => format!("{}({})", func, a.join(", ")),
        }
    }
}

fn cap_field(state: &str, label: &str) -> String {
    format!("cap_{}_{}", state, label)
}

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

fn sw_default(ty: &Type, raw: &str) -> String {
    match raw {
        "false" => "false".to_string(),
        "true" => "true".to_string(),
        "" => default_for(ty),
        s if s.starts_with('"') => s.to_string(),
        s => s.to_string(),
    }
}

fn default_for(ty: &Type) -> String {
    let s = match ty {
        Type::Custom(s) => s.as_str(),
        _ => "",
    };
    match s {
        "int" => "0".to_string(),
        "float" => "0.0".to_string(),
        "bool" => "false".to_string(),
        "str" | "string" | "String" | "bytes" => "\"\"".to_string(),
        _ => "0".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frame_c::compiler::fsm_parser::parse_fsm_block;
    use std::process::Command;

    /// Run a Swift program (generated classes + top-level driver) via
    /// `swift prog.swift`. Returns stdout lines. `None` if Swift is absent.
    fn sw_run(code: &str, driver: &str, tag: &str) -> Option<Vec<String>> {
        let prog = format!("{}\n{}\n", code, driver);
        let dir = std::env::temp_dir().join(format!("framec_sw_{}", tag));
        std::fs::create_dir_all(&dir).ok()?;
        let src = dir.join("prog.swift");
        std::fs::write(&src, prog).ok()?;
        let out = match Command::new("swift").arg(&src).output() {
            Ok(o) => o,
            Err(_) => return None,
        };
        assert!(
            out.status.success(),
            "swift failed for {:?}:\n{}",
            tag,
            String::from_utf8_lossy(&out.stderr)
        );
        Some(
            String::from_utf8_lossy(&out.stdout)
                .lines()
                .map(|s| s.to_string())
                .collect(),
        )
    }

    fn gen(src: &str) -> String {
        let decl = parse_fsm_block(src.as_bytes()).expect("fixture must parse");
        generate(&decl).expect("fixture must generate")
    }

    /// The scalar core, all built + run in one `swift` invocation.
    #[test]
    fn swift_core() {
        let cases: &[(&str, &str, &str, &str)] = &[
            ("@@fsm A(text: bytes) : bool = false { /a/ true }", "A(\"a\")", "true", "true"),
            ("@@fsm B(text: bytes) : bool = false { /a/ true }", "B(\"b\")", "false", "false"),
            (
                "@@fsm C(text: bytes) : int = 0 { /[0-9]+/ to_int(@@:matched) }",
                "C(\"123\")",
                "true",
                "123",
            ),
            (
                "@@fsm D(text: bytes) : int = 0 { /[0-9]+/ len(self.text) }",
                "D(\"123\")",
                "true",
                "3",
            ),
            (
                "@@fsm E(text: bytes) : int = 0 { $s: .n/[0-9]+/ to_int($s.n) }",
                "E(\"42\")",
                "true",
                "42",
            ),
            (
                "@@fsm F(text: bytes) : int = 0 { /[0-9]/ { self.count = self.count + 1 } self.count domain: count: int = 0 }",
                "F(\"5\")",
                "true",
                "1",
            ),
            (
                "@@fsm G(text: bytes) : int = 0 { /[0-9]+/ parse_int(@@:matched) actions: parse_int(s: bytes): int { to_int(s) } }",
                "G(\"42\")",
                "true",
                "42",
            ),
            (
                "@@fsm H(text: bytes) : int = 0 { $0: /[a-z]/ -> $digits : -> $error $digits: .n/[0-9]+/ to_int($digits.n) $error: -1 }",
                "H(\"x42\")",
                "true",
                "42",
            ),
        ];
        let code = cases
            .iter()
            .map(|(s, ..)| gen(s))
            .collect::<Vec<_>>()
            .join("\n");
        let driver = cases
            .iter()
            .map(|(_, ctor, ..)| {
                format!("do {{ let m = {ctor}; print(m.accepted); print(m.return_value) }}")
            })
            .collect::<Vec<_>>()
            .join("\n");
        let Some(lines) = sw_run(&code, &driver, "core") else {
            return;
        };
        let mut i = 0;
        for (idx, (_, _, exp_acc, exp_ret)) in cases.iter().enumerate() {
            assert_eq!(&lines[i], exp_acc, "accepted case {idx}");
            assert_eq!(&lines[i + 1], exp_ret, "return case {idx}");
            i += 2;
        }
    }

    #[test]
    fn swift_conditional_target() {
        let code = gen("@@fsm M(text: bytes, mode: int) : int = 0 { \
             /[01]/ -> ( $zero when self.mode == 0, $one when self.mode == 1 ) : -> $error \
             $zero: 0 $one: 1 $error: -1 }");
        let driver = "for md in [0, 1, 2] { print(M(\"0\", md).return_value) }";
        let Some(lines) = sw_run(&code, driver, "cond") else {
            return;
        };
        assert_eq!(lines, vec!["0", "1", "-1"]);
    }

    #[test]
    fn swift_multi_match() {
        let code = gen("@@fsm M(text: bytes) : int = 0 { /[0-9]/ -> $num | 99 $num: 1 }");
        let driver = "for s in [\"5\", \"a\"] { print(M(s).return_value) }";
        let Some(lines) = sw_run(&code, driver, "mm") else {
            return;
        };
        assert_eq!(lines, vec!["1", "99"]);
    }

    #[test]
    fn swift_embed_every_transition() {
        let code = gen(
            "@@fsm M(text: bytes) : int = 0 { /[0-9]+/ ${ tally() } self.count \
             actions: tally() { self.count = self.count + 1 } domain: count: int = 0 }",
        );
        let driver = "print(M(\"123\").return_value)";
        let Some(lines) = sw_run(&code, driver, "emb") else {
            return;
        };
        assert_eq!(lines[0], "3");
    }

    /// FSM-TEST-603 — `%{...}` fires when the DFA leaves its last accepting
    /// state, capturing the end of the matched region.
    #[test]
    fn swift_embed_leave_final() {
        let code = gen(
            "@@fsm M(text: bytes) : int = 0 { /[0-9]+/ %{ self.end_pos = @@:cursor } self.end_pos \
             domain: end_pos: int = 0 }",
        );
        let driver = "for s in [\"42x\", \"abx\"] { print(M(s).return_value) }";
        let Some(lines) = sw_run(&code, driver, "leave") else {
            return;
        };
        assert_eq!(lines, vec!["2", "0"]);
    }

    #[test]
    fn swift_token_alphabet() {
        let code = gen("@@fsm M(toks: token) : bool = false { /IDENT LPAREN RPAREN/ true }");
        let driver = "for t in [[\"IDENT\",\"LPAREN\",\"RPAREN\"], [\"IDENT\",\"RPAREN\"], [\"IDENT\",\"WAT\"]] { print(M(t).accepted) }";
        let Some(lines) = sw_run(&code, driver, "tok") else {
            return;
        };
        assert_eq!(lines, vec!["true", "false", "false"]);
    }

    #[test]
    fn swift_mode_c_callout() {
        let inner = gen("@@fsm Digits(text: bytes) : int = 0 { /[0-9]+/ to_int(@@:matched) }");
        let outer = gen("@@fsm Outer(text: bytes) : int = 0 { $s: .d/@Digits/ $s.d.return_value }");
        let code = format!("{}\n{}", inner, outer);
        let driver = "for s in [\"42\", \"x\"] { let m = Outer(s); print(\"\\(m.accepted) \\(m.return_value)\") }";
        let Some(lines) = sw_run(&code, driver, "modec") else {
            return;
        };
        assert_eq!(lines, vec!["true 42", "false 0"]);
    }

    #[test]
    fn swift_anchors() {
        let start = gen("@@fsm M(text: bytes) : bool = false { /^foo/ true }");
        let end = gen("@@fsm N(text: bytes) : bool = false { /[0-9]+$/ true }");
        let code = format!("{}\n{}", start, end);
        let driver = "for s in [\"foo\", \"xfoo\"] { print(M(s).accepted) }\nfor s in [\"123\", \"123x\"] { print(N(s).accepted) }";
        let Some(lines) = sw_run(&code, driver, "anc") else {
            return;
        };
        assert_eq!(lines, vec!["true", "false", "true", "false"]);
    }

    #[test]
    fn swift_unsupported_errors() {
        let decl =
            parse_fsm_block(b"@@fsm M(text: bytes) : bool = false { /a$b/ true }").expect("parses");
        let err = generate(&decl).unwrap_err();
        assert!(err.contains("anchor"), "got {err}");
    }
}
