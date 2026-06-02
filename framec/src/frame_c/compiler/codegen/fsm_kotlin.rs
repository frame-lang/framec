//! Kotlin backend for `@@fsm` (RFC-0042, Phase 8).
//!
//! Generates a self-contained Kotlin `class` from a validated `FsmDeclAst`,
//! following the same recognition model as the Java backend
//! ([`super::fsm_java`]) in Kotlin syntax: a primary constructor whose
//! parameters become properties, an `init { run() }` block, `var` instance
//! properties, and a `when`-expression state dispatch. Frame's abstract
//! types map as `int` → `Int`, `float` → `Double`, `bool` → `Boolean`,
//! `str`/`bytes` → `String`. The constructor is `<Name>(...)`; the observable
//! result (§5.1) is the instance's `accepted`, `return_value`, `cursor`,
//! `reject_position`.
//!
//! The `bytes`/`char` input is the source `String` (`text[pos].code` is the
//! code point); the `token` input is an `Array<String>` mapped to small
//! integer ids. A per-stage DFA is two parallel structures —
//! `Array<Array<IntArray>>` transitions and a `BooleanArray` of accept-flags.
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

/// Generate Kotlin source implementing `decl`, or a reason it is outside the
/// v0.1 Kotlin cut.
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
                 Kotlin backend",
                regex
            )),
        }
    }

    fn kt_type(t: &Type) -> String {
        let s = match t {
            Type::Custom(s) => s.as_str(),
            _ => "Any",
        };
        match s {
            "int" => "Int".to_string(),
            "float" => "Double".to_string(),
            "bool" => "Boolean".to_string(),
            "str" | "string" | "String" | "bytes" => "String".to_string(),
            other => other.to_string(),
        }
    }

    fn input_type(&self) -> &'static str {
        match self.alphabet {
            Alphabet::Token => "Array<String>",
            _ => "String",
        }
    }

    fn matched_type(&self) -> &'static str {
        match self.alphabet {
            Alphabet::Token => "Array<String>",
            _ => "String",
        }
    }

    fn matched_empty(&self) -> &'static str {
        match self.alphabet {
            Alphabet::Token => "arrayOf<String>()",
            _ => "\"\"",
        }
    }

    /// The input length (`.length` for a `String`, `.size` for an array).
    fn input_len(&self) -> String {
        let inp = &self.decl.params[0].name;
        match self.alphabet {
            Alphabet::Token => format!("{}.size", inp),
            _ => format!("{}.length", inp),
        }
    }

    fn element_read(&self) -> String {
        let inp = &self.decl.params[0].name;
        match self.alphabet {
            Alphabet::Token => format!("tokId({}[pos])", inp),
            _ => format!("{}[pos].code", inp),
        }
    }

    fn matched_slice(&self, end: &str) -> String {
        let inp = &self.decl.params[0].name;
        match self.alphabet {
            Alphabet::Token => format!("{}.copyOfRange(cursor, {})", inp, end),
            _ => format!("{}.substring(cursor, {})", inp, end),
        }
    }

    fn emit(&self) -> Result<String, String> {
        let mut out = String::new();
        out.push_str("// Generated by framec — RFC-0042 @@fsm (Kotlin backend).\n\n");
        // Primary-constructor parameters (the input first) become properties.
        let ctor: Vec<String> = self
            .decl
            .params
            .iter()
            .enumerate()
            .map(|(i, p)| {
                let ty = if i == 0 {
                    self.input_type().to_string()
                } else {
                    Self::kt_type(&p.param_type)
                };
                format!("val {}: {}", p.name, ty)
            })
            .collect();
        writeln!(out, "class {}({}) {{", self.decl.name, ctor.join(", ")).ok();
        self.emit_properties(&mut out);
        out.push_str("  init {\n    run()\n    if (accepted) reject_position = 0\n  }\n\n");
        self.emit_tok_id(&mut out);
        self.emit_dfa_matcher(&mut out);
        self.emit_run(&mut out);
        self.emit_state_methods(&mut out)?;
        self.emit_embed_matchers(&mut out)?;
        self.emit_action_methods(&mut out)?;
        out.push_str("}\n");
        Ok(out)
    }

    fn emit_properties(&self, out: &mut String) {
        out.push_str("  var accepted = false\n");
        out.push_str("  var reject_position = 0\n");
        out.push_str("  var cursor = 0\n");
        writeln!(
            out,
            "  var return_value: {} = {}",
            Self::kt_type(&self.decl.return_type),
            kt_default(&self.decl.return_type, &self.decl.default_expr)
        )
        .ok();
        let input = &self.decl.params[0].name;
        if let Some(domain) = &self.decl.domain {
            for v in &domain.vars {
                if &v.name == input {
                    continue;
                }
                writeln!(
                    out,
                    "  var {}: {} = {}",
                    v.name,
                    Self::kt_type(&v.var_type),
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
        out.push_str("  var enter = 0\n");
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
            writeln!(out, "  var {}: {}? = null", f, inner).ok();
        }
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
        out.push_str("  fun tokId(t: String): Int {\n    return when (t) {\n");
        for (name, id) in entries {
            writeln!(out, "      {:?} -> {}", name, id).ok();
        }
        out.push_str("      else -> -1\n    }\n  }\n\n");
    }

    fn emit_dfa_matcher(&self, out: &mut String) {
        let read = self.element_read();
        writeln!(
            out,
            "  fun dfaMatch(trans: Array<Array<IntArray>>, accept: BooleanArray, start: Int): Int {{\n\
             \x20   var st = start\n\
             \x20   var pos = cursor\n\
             \x20   val n = {len}\n\
             \x20   var last = if (accept[st]) pos else -1\n\
             \x20   while (pos < n) {{\n\
             \x20     val v = {read}\n\
             \x20     var nxt = -1\n\
             \x20     for (tr in trans[st]) {{ if (tr[0] <= v && v <= tr[1]) {{ nxt = tr[2]; break }} }}\n\
             \x20     if (nxt < 0) break\n\
             \x20     st = nxt; pos++\n\
             \x20     if (accept[st]) last = pos\n\
             \x20   }}\n\
             \x20   return last\n\
             \x20 }}\n\n",
            len = self.input_len(),
            read = read
        )
        .ok();
    }

    fn emit_run(&self, out: &mut String) {
        out.push_str("  fun run() {\n    var state = 0\n");
        out.push_str("    while (state >= 0) {\n");
        out.push_str("      val _enter = enter\n      enter = 0\n");
        out.push_str("      state = when (state) {\n");
        for i in 0..self.decl.states.len() {
            writeln!(out, "        {} -> state{}(_enter)", i, i).ok();
        }
        out.push_str("        else -> return\n      }\n    }\n  }\n\n");
    }

    fn emit_state_methods(&self, out: &mut String) -> Result<(), String> {
        let mut sid = 0usize;
        for (i, st) in self.decl.states.iter().enumerate() {
            match st.matches.len() {
                0 => {
                    writeln!(out, "  fun state{}(_enter: Int): Int {{ return -1 }}\n", i).ok();
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
        writeln!(out, "  fun state{}(_enter: Int): Int {{", index).ok();
        for (idx, el) in m.elements.iter().enumerate() {
            writeln!(out, "    if (_enter <= {}) {{", idx).ok();
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
        writeln!(out, "  fun state{}(_enter: Int): Int {{", index).ok();
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
                             supported by the Kotlin backend"
                                .into(),
                        );
                    }
                    let my_sid = *sid;
                    *sid += 1;
                    if self.stage_dfas[my_sid].mode_c.is_some() {
                        return Err(
                            "a Mode C (`/@Fsm/`) stage as a `|` alternative selector is not yet \
                             supported by the Kotlin backend"
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
                    writeln!(out, "    if (_r{} >= 0) {{", my_sid).ok();
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

    /// `val`/`var` for a stage's match-result, depending on whether boundary
    /// anchors reassign it.
    fn r_kw(&self, sid: usize) -> &'static str {
        let d = &self.stage_dfas[sid];
        if d.requires_start || d.requires_end {
            "var"
        } else {
            "val"
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
                writeln!(out, "{}if (_r{} < 0) {{", ind, my_sid).ok();
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
            Alphabet::Token => format!("{}.copyOfRange(cursor, {}.size)", input, input),
            _ => format!("{}.substring(cursor)", input),
        };
        writeln!(out, "{}val {} = {}({})", ind, iv, inner, sub).ok();
        writeln!(out, "{}if (!{}.accepted) {{", ind, iv).ok();
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
        let read = self.element_read();
        writeln!(out, "  fun matchStage{}(): Int {{", sid).ok();
        self.emit_dfa_decls(out, sid, "    ");
        writeln!(
            out,
            "    val entry = cursor\n\
             \x20   var st = {start}\n\
             \x20   var pos = entry\n\
             \x20   val n = {len}\n\
             \x20   var last = if (a{sid}[st]) pos else -1\n\
             \x20   cursor = pos",
            start = self.stage_dfas[sid].start,
            len = self.input_len(),
            sid = sid
        )
        .ok();
        out.push_str(&self.embed_body(stage, EmbeddingOp::Start, "    ")?);
        writeln!(out, "    var prev = a{}[st]", sid).ok();
        writeln!(
            out,
            "    while (pos < n) {{\n\
             \x20     val v = {read}\n\
             \x20     var nxt = -1\n\
             \x20     for (tr in t{sid}[st]) {{ if (tr[0] <= v && v <= tr[1]) {{ nxt = tr[2]; break }} }}\n\
             \x20     if (nxt < 0) break\n\
             \x20     st = nxt; pos++\n\
             \x20     cursor = pos",
            read = read,
            sid = sid
        )
        .ok();
        out.push_str(&self.embed_body(stage, EmbeddingOp::EveryTransition, "      ")?);
        writeln!(out, "      val now = a{}[st]", sid).ok();
        let accept = self.embed_body(stage, EmbeddingOp::Accept, "        ")?;
        if !accept.is_empty() {
            out.push_str("      if (now) {\n");
            out.push_str(&accept);
            out.push_str("      }\n");
        }
        let leave = self.embed_body(stage, EmbeddingOp::LeaveAccept, "        ")?;
        if !leave.is_empty() {
            out.push_str("      if (prev && !now) {\n");
            out.push_str(&leave);
            out.push_str("      }\n");
        }
        out.push_str("      if (now) last = pos\n      prev = now\n");
        out.push_str("    }\n");
        let eof = self.embed_body(stage, EmbeddingOp::Eof, "      ")?;
        if !eof.is_empty() {
            out.push_str("    if (pos >= n && !prev) {\n");
            out.push_str(&eof);
            out.push_str("    }\n");
        } else {
            // `prev` is otherwise only read by the `%{}` guard; reference it
            // so Kotlin does not warn about an unused assignment.
            out.push_str("    prev\n");
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
        if dfa.requires_start {
            writeln!(out, "{}if (cursor != 0) _r{} = -1", ind, sid).ok();
        }
        if dfa.requires_end {
            writeln!(
                out,
                "{}if (_r{} != {}) _r{} = -1",
                ind,
                sid,
                self.input_len(),
                sid
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
                    writeln!(out, "{}if ({}) {{", ind, self.expr(&alt.condition)).ok();
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
                "{}throw RuntimeException(\"transition to undeclared state ${}\")",
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
                        "{}throw RuntimeException(\"transition to undeclared stage ${}.{}\")",
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
                let mut out = format!("{}if ({}) {{\n", ind, self.expr(&if_ast.condition));
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
                "statement {:?} not supported in @@fsm action blocks by the Kotlin backend",
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
                .map(|p| format!("{}: {}", p.name, Self::kt_type(&p.param_type)))
                .collect();
            let ret = match &act.return_type {
                Some(t) => format!(": {}", Self::kt_type(t)),
                None => String::new(),
            };
            writeln!(out, "  fun {}({}){} {{", act.name, sig.join(", "), ret).ok();
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

    /// Emit the per-stage DFA as two parallel local declarations:
    /// `t<sid>: Array<Array<IntArray>>` (transitions) and `a<sid>:
    /// BooleanArray` (accept).
    fn emit_dfa_decls(&self, out: &mut String, sid: usize, ind: &str) {
        let dfa = &self.stage_dfas[sid];
        let trans: Vec<String> = dfa
            .states
            .iter()
            .map(|(t, _)| {
                let ts: Vec<String> = t
                    .iter()
                    .map(|(lo, hi, tgt)| format!("intArrayOf({}, {}, {})", lo, hi, tgt))
                    .collect();
                format!("arrayOf<IntArray>({})", ts.join(", "))
            })
            .collect();
        let accept: Vec<String> = dfa.states.iter().map(|(_, a)| a.to_string()).collect();
        writeln!(out, "{}val t{} = arrayOf({})", ind, sid, trans.join(", ")).ok();
        writeln!(
            out,
            "{}val a{} = booleanArrayOf({})",
            ind,
            sid,
            accept.join(", ")
        )
        .ok();
    }

    fn expr(&self, e: &Expression) -> String {
        match e {
            Expression::Literal(l) => match l {
                Literal::Int(i) => i.to_string(),
                Literal::Float(f) => f.to_string(),
                Literal::Bool(b) => b.to_string(),
                Literal::String(s) => format!("{:?}", s),
                Literal::Null => "null".to_string(),
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
                UnaryOp::BitNot => format!("({}).inv()", self.expr(expr)),
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
                            return format!("{}!!.{}", cap_inst_field(state, label), field);
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
            "to_int" => format!("({}).toInt()", a.join(", ")),
            "to_str" => format!("({}).toString()", a.join(", ")),
            "len" => match self.alphabet {
                Alphabet::Token => format!("({}).size", a.join(", ")),
                _ => format!("({}).length", a.join(", ")),
            },
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
        // Kotlin spells the integer bitwise operators as infix functions.
        BinaryOp::BitAnd => "and",
        BinaryOp::BitOr => "or",
        BinaryOp::BitXor => "xor",
    }
}

fn kt_default(ty: &Type, raw: &str) -> String {
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
        _ => "null".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frame_c::compiler::fsm_parser::parse_fsm_block;
    use std::process::Command;

    /// Compile (`kotlinc`) + run (`kotlin ProgKt`) a program: the generated
    /// classes plus a top-level `main`. Returns stdout lines. `None` if the
    /// Kotlin toolchain is unavailable. (kotlinc is slow, so each test does
    /// all its cases in one driver.)
    fn kt_run(code: &str, body: &str, tag: &str) -> Option<Vec<String>> {
        let prog = format!("{}\nfun main() {{\n{}\n}}\n", code, body);
        let dir = std::env::temp_dir().join(format!("framec_kt_{}", tag));
        std::fs::create_dir_all(&dir).ok()?;
        let src = dir.join("Prog.kt");
        std::fs::write(&src, prog).ok()?;
        let out_dir = dir.join("out");
        let compile = match Command::new("kotlinc")
            .arg(&src)
            .arg("-d")
            .arg(&out_dir)
            .output()
        {
            Ok(o) => o,
            Err(_) => return None,
        };
        assert!(
            compile.status.success(),
            "kotlinc failed for {:?}:\n{}",
            tag,
            String::from_utf8_lossy(&compile.stderr)
        );
        let out = Command::new("kotlin")
            .arg("-cp")
            .arg(&out_dir)
            .arg("ProgKt")
            .output()
            .expect("kotlin");
        assert!(
            out.status.success(),
            "kotlin failed:\n{}",
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

    /// The scalar core, all type-checked + run in one kotlinc invocation:
    /// each case is a distinct fsm class, instantiated and printed in `main`.
    #[test]
    fn kotlin_core() {
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
        let body = cases
            .iter()
            .map(|(_, ctor, ..)| {
                format!("  run {{ val m = {ctor}; println(m.accepted); println(m.return_value) }}")
            })
            .collect::<Vec<_>>()
            .join("\n");
        let Some(lines) = kt_run(&code, &body, "core") else {
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
    fn kotlin_conditional_target() {
        let code = gen("@@fsm M(text: bytes, mode: int) : int = 0 { \
             /[01]/ -> ( $zero when self.mode == 0, $one when self.mode == 1 ) : -> $error \
             $zero: 0 $one: 1 $error: -1 }");
        let body = "  for (md in intArrayOf(0, 1, 2)) { println(M(\"0\", md).return_value) }";
        let Some(lines) = kt_run(&code, body, "cond") else {
            return;
        };
        assert_eq!(lines, vec!["0", "1", "-1"]);
    }

    #[test]
    fn kotlin_multi_match() {
        let code = gen("@@fsm M(text: bytes) : int = 0 { /[0-9]/ -> $num | 99 $num: 1 }");
        let body = "  for (s in arrayOf(\"5\", \"a\")) { println(M(s).return_value) }";
        let Some(lines) = kt_run(&code, body, "mm") else {
            return;
        };
        assert_eq!(lines, vec!["1", "99"]);
    }

    #[test]
    fn kotlin_embed_every_transition() {
        let code = gen(
            "@@fsm M(text: bytes) : int = 0 { /[0-9]+/ ${ tally() } self.count \
             actions: tally() { self.count = self.count + 1 } domain: count: int = 0 }",
        );
        let body = "  println(M(\"123\").return_value)";
        let Some(lines) = kt_run(&code, body, "emb") else {
            return;
        };
        assert_eq!(lines[0], "3");
    }

    #[test]
    fn kotlin_token_alphabet() {
        let code = gen("@@fsm M(toks: token) : bool = false { /IDENT LPAREN RPAREN/ true }");
        let body = "  for (t in arrayOf(arrayOf(\"IDENT\",\"LPAREN\",\"RPAREN\"), arrayOf(\"IDENT\",\"RPAREN\"), arrayOf(\"IDENT\",\"WAT\"))) { println(M(t).accepted) }";
        let Some(lines) = kt_run(&code, body, "tok") else {
            return;
        };
        assert_eq!(lines, vec!["true", "false", "false"]);
    }

    #[test]
    fn kotlin_mode_c_callout() {
        let inner = gen("@@fsm Digits(text: bytes) : int = 0 { /[0-9]+/ to_int(@@:matched) }");
        let outer = gen("@@fsm Outer(text: bytes) : int = 0 { $s: .d/@Digits/ $s.d.return_value }");
        let code = format!("{}\n{}", inner, outer);
        let body = "  for (s in arrayOf(\"42\", \"x\")) { val m = Outer(s); println(\"${m.accepted} ${m.return_value}\") }";
        let Some(lines) = kt_run(&code, body, "modec") else {
            return;
        };
        assert_eq!(lines, vec!["true 42", "false 0"]);
    }

    #[test]
    fn kotlin_anchors() {
        let start = gen("@@fsm M(text: bytes) : bool = false { /^foo/ true }");
        let end = gen("@@fsm N(text: bytes) : bool = false { /[0-9]+$/ true }");
        let code = format!("{}\n{}", start, end);
        let body = "  for (s in arrayOf(\"foo\", \"xfoo\")) { println(M(s).accepted) }\n  for (s in arrayOf(\"123\", \"123x\")) { println(N(s).accepted) }";
        let Some(lines) = kt_run(&code, body, "anc") else {
            return;
        };
        assert_eq!(lines, vec!["true", "false", "true", "false"]);
    }

    #[test]
    fn kotlin_unsupported_errors() {
        let decl =
            parse_fsm_block(b"@@fsm M(text: bytes) : bool = false { /a$b/ true }").expect("parses");
        let err = generate(&decl).unwrap_err();
        assert!(err.contains("anchor"), "got {err}");
    }
}
