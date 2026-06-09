//! C++ backend for `@@fsm` (RFC-0042, Phase 8).
//!
//! Generates a self-contained C++ `class` from a validated `FsmDeclAst`,
//! following the same recognition model as the Java backend
//! ([`super::fsm_java`]): a per-state dispatch loop (`switch`) over mutable
//! members. Frame's abstract types map as `int` → `int`, `float` →
//! `double`, `bool` → `bool`, `str`/`bytes` → `std::string`. The constructor
//! is `<Name>(...)`; the observable result (§5.1) is the instance's public
//! `accepted`, `return_value`, `cursor`, `reject_position`.
//!
//! The generated class assumes `<string>` and `<vector>` are included by the
//! enclosing translation unit; it uses default member initializers so the
//! constructor only binds the parameters and runs. A per-stage DFA is two
//! parallel `std::vector`s — `std::vector<std::vector<std::vector<int>>>`
//! transitions and `std::vector<bool>` accept. The `token` input is a
//! `std::vector<std::string>`; `.size()` unifies the length of both.
//! Mode C inner instances are held by raw pointer (`new`) — the inner class
//! has no default constructor — so inner fsms must be emitted before the
//! outer (inner-first ordering).
//!
//! # v0.1 scope
//!
//! Full parity with the Python reference backend: single-match and
//! multi-match (`|`) ordered-choice states, captures, bare-expression
//! returns, action blocks, declared `actions:` methods, all transition
//! forms, embedding actions, Mode C sub-fsm call-out, all three alphabets,
//! and boundary anchors — `^`/`$` plus edge word boundaries `\b`/`\B`
//! (bytes only). Not yet handled (clear `Unsupported` error): mid-pattern
//! anchors, a Mode C stage as a `|` selector, and a `|` alternative with
//! elements before its first stage.

use crate::frame_c::compiler::frame_ast::{
    BinaryOp, EmbeddingOp, Expression, FsmDeclAst, FsmStateAst, FsmTransitionTarget, Literal,
    MatchAst, MatchElement, StageAst, Type, UnaryOp,
};
use crate::frame_c::compiler::fsm_regex::{
    self, size_check::DEFAULT_MAX_DFA_STATES, subset::DfaLabel, Alphabet, CompileError,
    WordBoundary,
};
use std::fmt::Write;

/// The inline C++ type of a DFA transitions table.
const TRANS_TYPE: &str = "std::vector<std::vector<std::vector<int>>>";

/// Generate C++ source implementing `decl`, or a reason it is outside the
/// v0.1 C++ cut.
pub fn generate(decl: &FsmDeclAst) -> Result<String, String> {
    Generator::new(decl)?.emit()
}

/// One stage's compiled DFA, flattened for emission.
struct StageDfa {
    states: Vec<(Vec<(u32, u32, usize)>, bool)>,
    start: usize,
    requires_start: bool,
    requires_end: bool,
    start_boundary: Option<WordBoundary>,
    end_boundary: Option<WordBoundary>,
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
                "regex `/{}/` uses a mid-pattern or word-boundary anchor, not yet supported by the \
                 C++ backend",
                regex
            )),
        }
    }

    fn cpp_type(t: &Type) -> String {
        let s = match t {
            Type::Custom(s) => s.as_str(),
            _ => "int",
        };
        match s {
            "int" => "int".to_string(),
            "float" => "double".to_string(),
            "bool" => "bool".to_string(),
            "str" | "string" | "String" | "bytes" => "std::string".to_string(),
            other => other.to_string(),
        }
    }

    fn input_type(&self) -> &'static str {
        match self.alphabet {
            Alphabet::Token => "std::vector<std::string>",
            _ => "std::string",
        }
    }

    fn matched_type(&self) -> &'static str {
        match self.alphabet {
            Alphabet::Token => "std::vector<std::string>",
            _ => "std::string",
        }
    }

    fn element_read(&self) -> String {
        let inp = &self.decl.params[0].name;
        match self.alphabet {
            Alphabet::Token => format!("tokId({}[pos])", inp),
            _ => format!("(int){}[pos]", inp),
        }
    }

    /// The matched run `[cursor, end)`: `substr` for a string, a range
    /// constructor for the token vector.
    fn matched_slice(&self, end: &str) -> String {
        let inp = &self.decl.params[0].name;
        match self.alphabet {
            Alphabet::Token => format!(
                "std::vector<std::string>({}.begin() + cursor, {}.begin() + ({}))",
                inp, inp, end
            ),
            _ => format!("{}.substr(cursor, ({}) - cursor)", inp, end),
        }
    }

    fn emit(&self) -> Result<String, String> {
        let mut out = String::new();
        out.push_str("// Generated by framec — RFC-0042 @@fsm (C++ backend).\n\n");
        writeln!(out, "class {} {{", self.decl.name).ok();
        out.push_str(" public:\n");
        self.emit_field_decls(&mut out);
        self.emit_ctor(&mut out);
        self.emit_tok_id(&mut out);
        if self.uses_word_boundary() {
            self.emit_word_boundary_helper(&mut out);
        }
        self.emit_dfa_matcher(&mut out);
        self.emit_run(&mut out);
        self.emit_state_methods(&mut out)?;
        self.emit_embed_matchers(&mut out)?;
        self.emit_action_methods(&mut out)?;
        out.push_str("};\n");
        Ok(out)
    }

    fn emit_field_decls(&self, out: &mut String) {
        out.push_str("  bool accepted = false;\n");
        out.push_str("  int reject_position = 0;\n");
        out.push_str("  int cursor = 0;\n");
        writeln!(
            out,
            "  {} return_value = {};",
            Self::cpp_type(&self.decl.return_type),
            cpp_default(&self.decl.return_type, &self.decl.default_expr)
        )
        .ok();
        let mut seen = std::collections::HashSet::new();
        for (i, p) in self.decl.params.iter().enumerate() {
            seen.insert(p.name.clone());
            // Parameters are assigned in the constructor.
            let ty = if i == 0 {
                self.input_type().to_string()
            } else {
                Self::cpp_type(&p.param_type)
            };
            writeln!(out, "  {} {};", ty, p.name).ok();
        }
        if let Some(domain) = &self.decl.domain {
            for v in &domain.vars {
                if !seen.insert(v.name.clone()) {
                    continue;
                }
                writeln!(
                    out,
                    "  {} {} = {};",
                    Self::cpp_type(&v.var_type),
                    v.name,
                    self.expr(&v.default)
                )
                .ok();
            }
        }
        writeln!(out, "  {} matched;", self.matched_type()).ok();
        out.push_str("  int enter = 0;\n");
        for f in self.capture_fields() {
            writeln!(out, "  {} {};", self.matched_type(), f).ok();
        }
        for (f, inner) in self.mode_c_inst_fields() {
            writeln!(out, "  {}* {} = nullptr;", inner, f).ok();
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
                    self.input_type().to_string()
                } else {
                    Self::cpp_type(&p.param_type)
                };
                format!("{} {}", ty, p.name)
            })
            .collect();
        writeln!(out, "  {}({}) {{", self.decl.name, sig.join(", ")).ok();
        for p in &self.decl.params {
            writeln!(out, "    this->{} = {};", p.name, p.name).ok();
        }
        if let Some(domain) = &self.decl.domain {
            for v in &domain.vars {
                if &v.name == input {
                    continue;
                }
                writeln!(out, "    {} = {};", v.name, self.expr(&v.default)).ok();
            }
        }
        out.push_str("    run();\n");
        out.push_str("    if (accepted) reject_position = 0;\n");
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
        // C++ `switch` does not accept strings; use an if-chain.
        out.push_str("  int tokId(const std::string& t) {\n");
        for (name, id) in entries {
            writeln!(out, "    if (t == {:?}) return {};", name, id).ok();
        }
        out.push_str("    return -1;\n  }\n\n");
    }

    fn emit_dfa_matcher(&self, out: &mut String) {
        let inp = &self.decl.params[0].name;
        let read = self.element_read();
        writeln!(
            out,
            "  int dfaMatch({trans}& trans, std::vector<bool>& accept, int start) {{\n\
             \x20   int st = start;\n\
             \x20   int pos = cursor;\n\
             \x20   int n = (int){inp}.size();\n\
             \x20   int last = accept[st] ? pos : -1;\n\
             \x20   while (pos < n) {{\n\
             \x20     int v = {read};\n\
             \x20     int nxt = -1;\n\
             \x20     for (auto& tr : trans[st]) {{ if (tr[0] <= v && v <= tr[1]) {{ nxt = tr[2]; break; }} }}\n\
             \x20     if (nxt < 0) break;\n\
             \x20     st = nxt; pos++;\n\
             \x20     if (accept[st]) last = pos;\n\
             \x20   }}\n\
             \x20   return last;\n\
             \x20 }}\n\n",
            trans = TRANS_TYPE,
            inp = inp,
            read = read
        )
        .ok();
    }

    fn emit_run(&self, out: &mut String) {
        out.push_str("  void run() {\n    int state = 0;\n");
        out.push_str("    while (state >= 0) {\n");
        out.push_str("      int _enter = enter;\n      enter = 0;\n");
        out.push_str("      switch (state) {\n");
        for i in 0..self.decl.states.len() {
            writeln!(
                out,
                "        case {}: state = state{}(_enter); break;",
                i, i
            )
            .ok();
        }
        out.push_str("        default: return;\n      }\n    }\n  }\n\n");
    }

    fn emit_state_methods(&self, out: &mut String) -> Result<(), String> {
        let mut sid = 0usize;
        for (i, st) in self.decl.states.iter().enumerate() {
            match st.matches.len() {
                0 => {
                    writeln!(out, "  int state{}(int _enter) {{ return -1; }}\n", i).ok();
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
        writeln!(out, "  int state{}(int _enter) {{", index).ok();
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
        writeln!(out, "  int state{}(int _enter) {{", index).ok();
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
                             supported by the C++ backend"
                                .into(),
                        );
                    }
                    let my_sid = *sid;
                    *sid += 1;
                    if self.stage_dfas[my_sid].mode_c.is_some() {
                        return Err(
                            "a Mode C (`/@Fsm/`) stage as a `|` alternative selector is not yet \
                             supported by the C++ backend"
                                .into(),
                        );
                    }
                    let MatchElement::Stage(sel) = &m.elements[fs] else {
                        unreachable!("first_stage indexes a Stage element")
                    };
                    self.emit_dfa_decls(out, my_sid, "    ");
                    writeln!(
                        out,
                        "    int _r{} = dfaMatch(t{}, a{}, {});",
                        my_sid, my_sid, my_sid, self.stage_dfas[my_sid].start
                    )
                    .ok();
                    self.emit_anchor_guards(out, my_sid, "    ");
                    writeln!(out, "    if (_r{} >= 0) {{", my_sid).ok();
                    writeln!(
                        out,
                        "      matched = {};",
                        self.matched_slice(&format!("_r{}", my_sid))
                    )
                    .ok();
                    if let Some(lbl) = &sel.label {
                        if !state_label.is_empty() {
                            writeln!(out, "      {} = matched;", cap_field(&state_label, lbl)).ok();
                        }
                    }
                    writeln!(out, "      cursor = _r{};", my_sid).ok();
                    out.push_str("      accepted = true;\n");
                    for el in &m.elements[fs + 1..] {
                        self.emit_element(out, el, m, &state_label, "      ", sid)?;
                    }
                    self.emit_success(out, m, "      ");
                    out.push_str("    }\n");
                }
                None => {
                    out.push_str("    accepted = true;\n");
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
            out.push_str("    accepted = false;\n");
            out.push_str("    reject_position = cursor;\n");
            out.push_str("    return -1;\n");
        }
        out.push_str("  }\n\n");
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
                        "{}int _r{} = dfaMatch(t{}, a{}, {});",
                        ind, my_sid, my_sid, my_sid, self.stage_dfas[my_sid].start
                    )
                    .ok();
                } else {
                    writeln!(out, "{}int _r{} = matchStage{}();", ind, my_sid, my_sid).ok();
                }
                self.emit_anchor_guards(out, my_sid, ind);
                writeln!(out, "{}if (_r{} < 0) {{", ind, my_sid).ok();
                self.emit_failure(out, m, &ind2);
                writeln!(out, "{}}}", ind).ok();
                writeln!(
                    out,
                    "{}matched = {};",
                    ind,
                    self.matched_slice(&format!("_r{}", my_sid))
                )
                .ok();
                if let Some(lbl) = &stage.label {
                    if !state_label.is_empty() {
                        writeln!(out, "{}{} = matched;", ind, cap_field(state_label, lbl)).ok();
                    }
                }
                writeln!(out, "{}cursor = _r{};", ind, my_sid).ok();
                writeln!(out, "{}accepted = true;", ind).ok();
            }
            MatchElement::BareExpression { expr, .. } => {
                writeln!(out, "{}return_value = {};", ind, self.expr(expr)).ok();
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
        // The inner fsm's input is the remaining run from the cursor.
        let sub = match self.alphabet {
            Alphabet::Token => format!(
                "std::vector<std::string>({}.begin() + cursor, {}.end())",
                input, input
            ),
            _ => format!("{}.substr(cursor)", input),
        };
        writeln!(out, "{}{}* {} = new {}({});", ind, inner, iv, inner, sub).ok();
        writeln!(out, "{}if (!{}->accepted) {{", ind, iv).ok();
        self.emit_failure(out, m, ind2);
        writeln!(out, "{}}}", ind).ok();
        let end = format!("cursor + {}->cursor", iv);
        writeln!(out, "{}matched = {};", ind, self.matched_slice(&end)).ok();
        if let Some(lbl) = &stage.label {
            if !state_label.is_empty() {
                writeln!(out, "{}{} = matched;", ind, cap_field(state_label, lbl)).ok();
                writeln!(out, "{}{} = {};", ind, cap_inst_field(state_label, lbl), iv).ok();
            }
        }
        writeln!(out, "{}cursor = cursor + {}->cursor;", ind, iv).ok();
        writeln!(out, "{}accepted = true;", ind).ok();
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
        writeln!(out, "  int matchStage{}() {{", sid).ok();
        self.emit_dfa_decls(out, sid, "    ");
        writeln!(
            out,
            "    int entry = cursor;\n\
             \x20   int st = {start};\n\
             \x20   int pos = entry;\n\
             \x20   int n = (int){inp}.size();\n\
             \x20   int last = a{sid}[st] ? pos : -1;\n\
             \x20   cursor = pos;",
            start = self.stage_dfas[sid].start,
            inp = inp,
            sid = sid
        )
        .ok();
        out.push_str(&self.embed_body(stage, EmbeddingOp::Start, "    ")?);
        writeln!(out, "    bool prev = a{}[st];", sid).ok();
        writeln!(
            out,
            "    while (pos < n) {{\n\
             \x20     int v = {read};\n\
             \x20     int nxt = -1;\n\
             \x20     for (auto& tr : t{sid}[st]) {{ if (tr[0] <= v && v <= tr[1]) {{ nxt = tr[2]; break; }} }}\n\
             \x20     if (nxt < 0) break;\n\
             \x20     st = nxt; pos++;\n\
             \x20     cursor = pos;",
            read = read,
            sid = sid
        )
        .ok();
        out.push_str(&self.embed_body(stage, EmbeddingOp::EveryTransition, "      ")?);
        writeln!(out, "      bool now = a{}[st];", sid).ok();
        let accept = self.embed_body(stage, EmbeddingOp::Accept, "        ")?;
        if !accept.is_empty() {
            out.push_str("      if (now) {\n");
            out.push_str(&accept);
            out.push_str("      }\n");
        }
        out.push_str("      if (now) last = pos;\n      prev = now;\n");
        out.push_str("    }\n");
        // `%{}` — left the last accepting state: a post-scan event firing once
        // when the longest match stops extending (failing element or EOF), with
        // `@@:cursor` at the end of the matched region (`last`), not the failing
        // element (§5.4 / FSM-TEST-603). `last < 0` ⇒ no accepting state was
        // entered, so there is nothing to leave.
        let leave = self.embed_body(stage, EmbeddingOp::LeaveAccept, "      ")?;
        if !leave.is_empty() {
            out.push_str("    if (last >= 0) {\n      cursor = last;\n");
            out.push_str(&leave);
            out.push_str("    }\n");
        }
        let eof = self.embed_body(stage, EmbeddingOp::Eof, "      ")?;
        if !eof.is_empty() {
            out.push_str("    if (pos >= n && !prev) {\n");
            out.push_str(&eof);
            out.push_str("    }\n");
        } else {
            // `prev` is otherwise only read by the `@eof{}` guard; reference it.
            out.push_str("    (void)prev;\n");
        }
        out.push_str("    cursor = entry;\n    return last;\n  }\n\n");
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
            writeln!(out, "{}if (cursor != 0) _r{} = -1;", ind, sid).ok();
        }
        if dfa.requires_end {
            writeln!(
                out,
                "{}if (_r{} != (int){}.size()) _r{} = -1;",
                ind, sid, inp, sid
            )
            .ok();
        }
        // Word-boundary edge anchors (\b / \B). The start boundary is checked
        // at `cursor` (the run's start); the end boundary at `_r{sid}` (the
        // longest match end), guarded so a failed match is left untouched.
        // Required (\b) ⇒ a boundary must exist, so the two sides differing is
        // the wanted state and `==` is the violation; Forbidden (\B) inverts.
        if let Some(b) = dfa.start_boundary {
            let op = match b {
                WordBoundary::Required => "==",
                WordBoundary::Forbidden => "!=",
            };
            writeln!(
                out,
                "{}if (iswordat(cursor - 1) {} iswordat(cursor)) _r{} = -1;",
                ind, op, sid
            )
            .ok();
        }
        if let Some(b) = dfa.end_boundary {
            let op = match b {
                WordBoundary::Required => "==",
                WordBoundary::Forbidden => "!=",
            };
            writeln!(
                out,
                "{}if (_r{} >= 0 && iswordat(_r{} - 1) {} iswordat(_r{})) _r{} = -1;",
                ind, sid, sid, op, sid, sid
            )
            .ok();
        }
    }

    /// Any stage uses a word-boundary anchor → emit the `iswordat` helper.
    fn uses_word_boundary(&self) -> bool {
        self.stage_dfas
            .iter()
            .any(|d| d.start_boundary.is_some() || d.end_boundary.is_some())
    }

    /// Emit `bool iswordat(int p)`: true iff byte `p` is a word byte
    /// (`[0-9A-Za-z_]`). Out-of-bounds positions are non-word. Boundaries are
    /// a bytes-only feature, so the input is always the `std::string` form.
    fn emit_word_boundary_helper(&self, out: &mut String) {
        let inp = &self.decl.params[0].name;
        writeln!(
            out,
            "  bool iswordat(int p) {{\n\
             \x20   if (p < 0 || p >= (int){inp}.size()) return false;\n\
             \x20   int c = (unsigned char){inp}[p];\n\
             \x20   return (c >= '0' && c <= '9') || (c >= 'A' && c <= 'Z') || (c >= 'a' && c <= 'z') || c == '_';\n\
             \x20 }}\n\n",
            inp = inp
        )
        .ok();
    }

    fn emit_success(&self, out: &mut String, m: &MatchAst, ind: &str) {
        match m.transition.as_ref().and_then(|c| c.success.as_ref()) {
            None => {
                writeln!(out, "{}return -1;", ind).ok();
            }
            Some(target) => self.emit_target(out, target, ind, m),
        }
    }

    fn emit_failure(&self, out: &mut String, m: &MatchAst, ind: &str) {
        writeln!(out, "{}accepted = false;", ind).ok();
        writeln!(out, "{}reject_position = cursor;", ind).ok();
        match m.transition.as_ref().and_then(|c| c.failure.as_ref()) {
            None => {
                writeln!(out, "{}return -1;", ind).ok();
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
                "{}throw \"transition to undeclared state ${}\";",
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
                    writeln!(out, "{}enter = {};", ind, entry).ok();
                }
                None => {
                    writeln!(
                        out,
                        "{}throw \"transition to undeclared stage ${}.{}\";",
                        ind, state, s
                    )
                    .ok();
                    return;
                }
            }
        }
        writeln!(out, "{}return {};", ind, idx).ok();
    }

    fn stmt(
        &self,
        s: &crate::frame_c::compiler::frame_ast::Statement,
        ind: &str,
    ) -> Result<String, String> {
        use crate::frame_c::compiler::frame_ast::Statement;
        match s {
            Statement::Expression(e) => Ok(format!("{}{};\n", ind, self.expr(&e.expr))),
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
                "statement {:?} not supported in @@fsm action blocks by the C++ backend",
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
                .map(|p| format!("{} {}", Self::cpp_type(&p.param_type), p.name))
                .collect();
            let ret = match &act.return_type {
                Some(t) => Self::cpp_type(t),
                None => "void".to_string(),
            };
            writeln!(out, "  {} {}({}) {{", ret, act.name, sig.join(", ")).ok();
            let n = act.body.statements.len();
            let has_return = act.return_type.is_some();
            for (i, s) in act.body.statements.iter().enumerate() {
                use crate::frame_c::compiler::frame_ast::Statement;
                if i + 1 == n && has_return {
                    if let Statement::Expression(e) = s {
                        if !matches!(e.expr, Expression::Assign { .. }) {
                            writeln!(out, "    return {};", self.expr(&e.expr)).ok();
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

    /// Emit the per-stage DFA as two parallel local vectors: `t<sid>`
    /// (transitions) and `a<sid>` (accept).
    fn emit_dfa_decls(&self, out: &mut String, sid: usize, ind: &str) {
        let dfa = &self.stage_dfas[sid];
        let trans: Vec<String> = dfa
            .states
            .iter()
            .map(|(t, _)| {
                let ts: Vec<String> = t
                    .iter()
                    .map(|(lo, hi, tgt)| format!("{{{}, {}, {}}}", lo, hi, tgt))
                    .collect();
                format!("{{{}}}", ts.join(", "))
            })
            .collect();
        let accept: Vec<String> = dfa.states.iter().map(|(_, a)| a.to_string()).collect();
        writeln!(
            out,
            "{}{} t{} = {{{}}};",
            ind,
            TRANS_TYPE,
            sid,
            trans.join(", ")
        )
        .ok();
        writeln!(
            out,
            "{}std::vector<bool> a{} = {{{}}};",
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
                Literal::String(s) => format!("std::string({:?})", s),
                Literal::Null => "nullptr".to_string(),
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
                    // inner fsm instance (a pointer) recorded for that stage.
                    if let Some((state, label)) =
                        name.strip_prefix('$').and_then(|c| c.split_once('.'))
                    {
                        if matches!(
                            field.as_str(),
                            "return_value" | "accepted" | "cursor" | "reject_position"
                        ) {
                            return format!("{}->{}", cap_inst_field(state, label), field);
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
            "to_int" => format!("std::stoi({})", a.join(", ")),
            "to_str" => format!("std::to_string({})", a.join(", ")),
            "len" => format!("(int)({}).size()", a.join(", ")),
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

fn cpp_default(ty: &Type, raw: &str) -> String {
    match raw {
        "false" => "false".to_string(),
        "true" => "true".to_string(),
        "" => default_for(ty),
        s if s.starts_with('"') => format!("std::string({})", s),
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
        "str" | "string" | "String" | "bytes" => "std::string()".to_string(),
        _ => "0".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frame_c::compiler::fsm_parser::parse_fsm_block;
    use std::process::Command;

    /// Compile (`c++ -std=c++17`) + run a program: includes, the generated
    /// classes, and a `main` driver. Returns stdout lines. `None` if no C++
    /// compiler is available.
    fn cpp_run(code: &str, driver: &str, tag: &str) -> Option<Vec<String>> {
        let prog = format!(
            "#include <string>\n#include <vector>\n#include <iostream>\n\n{}\nint main() {{\n  std::cout << std::boolalpha;\n{}\n  return 0;\n}}\n",
            code, driver
        );
        let dir = std::env::temp_dir().join(format!("framec_cpp_{}", tag));
        std::fs::create_dir_all(&dir).ok()?;
        let src = dir.join("prog.cpp");
        let bin = dir.join("prog");
        std::fs::write(&src, prog).ok()?;
        let compile = match Command::new("c++")
            .arg("-std=c++17")
            .arg(&src)
            .arg("-o")
            .arg(&bin)
            .output()
        {
            Ok(o) => o,
            Err(_) => return None,
        };
        assert!(
            compile.status.success(),
            "c++ failed for {:?}:\n{}",
            tag,
            String::from_utf8_lossy(&compile.stderr)
        );
        let out = Command::new(&bin).output().expect("run binary");
        assert!(out.status.success(), "binary failed");
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

    /// The scalar core, all built + run in one compile.
    #[test]
    fn cpp_core() {
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
                format!("  {{ auto m = {ctor}; std::cout << m.accepted << \"\\n\" << m.return_value << \"\\n\"; }}")
            })
            .collect::<Vec<_>>()
            .join("\n");
        let Some(lines) = cpp_run(&code, &driver, "core") else {
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
    fn cpp_conditional_target() {
        let code = gen("@@fsm M(text: bytes, mode: int) : int = 0 { \
             /[01]/ -> ( $zero when self.mode == 0, $one when self.mode == 1 ) : -> $error \
             $zero: 0 $one: 1 $error: -1 }");
        let driver = "  for (int md : {0, 1, 2}) { M m(\"0\", md); std::cout << m.return_value << \"\\n\"; }";
        let Some(lines) = cpp_run(&code, driver, "cond") else {
            return;
        };
        assert_eq!(lines, vec!["0", "1", "-1"]);
    }

    #[test]
    fn cpp_multi_match() {
        let code = gen("@@fsm M(text: bytes) : int = 0 { /[0-9]/ -> $num | 99 $num: 1 }");
        let driver = "  for (std::string s : {\"5\", \"a\"}) { M m(s); std::cout << m.return_value << \"\\n\"; }";
        let Some(lines) = cpp_run(&code, driver, "mm") else {
            return;
        };
        assert_eq!(lines, vec!["1", "99"]);
    }

    #[test]
    fn cpp_embed_every_transition() {
        let code = gen(
            "@@fsm M(text: bytes) : int = 0 { /[0-9]+/ ${ tally() } self.count \
             actions: tally() { self.count = self.count + 1 } domain: count: int = 0 }",
        );
        let driver = "  M m(\"123\"); std::cout << m.return_value << \"\\n\";";
        let Some(lines) = cpp_run(&code, driver, "emb") else {
            return;
        };
        assert_eq!(lines[0], "3");
    }

    /// FSM-TEST-603 — `%{...}` fires when the DFA leaves its last accepting
    /// state, capturing the end of the matched region.
    #[test]
    fn cpp_embed_leave_final() {
        let code = gen("@@fsm M(text: bytes) : int = 0 { \
             /[0-9]+/ %{ self.end_pos = @@:cursor } self.end_pos \
             domain: end_pos: int = 0 }");
        let driver = "  for (std::string s : {\"42x\", \"abx\"}) { M m(s); std::cout << m.return_value << \"\\n\"; }";
        let Some(lines) = cpp_run(&code, driver, "leave") else {
            return;
        };
        assert_eq!(lines, vec!["2", "0"]);
    }

    #[test]
    fn cpp_token_alphabet() {
        let code = gen("@@fsm M(toks: token) : bool = false { /IDENT LPAREN RPAREN/ true }");
        let driver = "  std::vector<std::vector<std::string>> ts = {{\"IDENT\",\"LPAREN\",\"RPAREN\"},{\"IDENT\",\"RPAREN\"},{\"IDENT\",\"WAT\"}};\n  for (auto& t : ts) { M m(t); std::cout << m.accepted << \"\\n\"; }";
        let Some(lines) = cpp_run(&code, driver, "tok") else {
            return;
        };
        assert_eq!(lines, vec!["true", "false", "false"]);
    }

    #[test]
    fn cpp_mode_c_callout() {
        let inner = gen("@@fsm Digits(text: bytes) : int = 0 { /[0-9]+/ to_int(@@:matched) }");
        let outer = gen("@@fsm Outer(text: bytes) : int = 0 { $s: .d/@Digits/ $s.d.return_value }");
        let code = format!("{}\n{}", inner, outer);
        let driver = "  for (std::string s : {\"42\", \"x\"}) { Outer m(s); std::cout << m.accepted << \" \" << m.return_value << \"\\n\"; }";
        let Some(lines) = cpp_run(&code, driver, "modec") else {
            return;
        };
        assert_eq!(lines, vec!["true 42", "false 0"]);
    }

    #[test]
    fn cpp_anchors() {
        let start = gen("@@fsm M(text: bytes) : bool = false { /^foo/ true }");
        let end = gen("@@fsm N(text: bytes) : bool = false { /[0-9]+$/ true }");
        let code = format!("{}\n{}", start, end);
        let driver = "  for (std::string s : {\"foo\", \"xfoo\"}) { M m(s); std::cout << m.accepted << \"\\n\"; }\n  for (std::string s : {\"123\", \"123x\"}) { N m(s); std::cout << m.accepted << \"\\n\"; }";
        let Some(lines) = cpp_run(&code, driver, "anc") else {
            return;
        };
        assert_eq!(lines, vec!["true", "false", "true", "false"]);
    }

    #[test]
    fn cpp_word_boundary() {
        let code = gen("@@fsm M(text: bytes) : bool = false { /\\bcat\\b/ true }");
        let driver = "  for (std::string s : {\"cat\", \"cats\"}) { M m(s); std::cout << m.accepted << \"\\n\"; }";
        let Some(lines) = cpp_run(&code, driver, "wb") else {
            return;
        };
        assert_eq!(lines, vec!["true", "false"]);
    }

    #[test]
    fn cpp_unsupported_errors() {
        let decl =
            parse_fsm_block(b"@@fsm M(text: bytes) : bool = false { /a$b/ true }").expect("parses");
        let err = generate(&decl).unwrap_err();
        assert!(err.contains("anchor"), "got {err}");
    }
}
