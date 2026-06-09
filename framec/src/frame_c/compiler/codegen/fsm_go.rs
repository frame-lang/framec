//! Go backend for `@@fsm` (RFC-0042, Phase 8).
//!
//! Generates a self-contained Go `struct` + methods from a validated
//! `FsmDeclAst`. The recognition model mirrors the Python reference backend
//! ([`super::fsm_python`]) and the Rust backend ([`super::fsm_rust`]) —
//! per-stage minimal DFAs + a per-state dispatch loop over a mutable struct
//! (pointer receiver) — but with static types. Frame's abstract types map as
//! `int` → `int`, `float` → `float64`, `bool` → `bool`, `str`/`bytes` →
//! `string`. The `bytes`/`char` input is held as a `[]rune` (so the cursor
//! indexes code points) and the matched run materialized back to a `string`;
//! the `token` input is a `[]string` mapped to small integer ids. The
//! constructor is `New<Name>(...) *<Name>`; the observable result (§5.1) is
//! the returned struct's `accepted`, `return_value`, `cursor`, and
//! `reject_position` fields.
//!
//! The generated recognizer is **import-free** (Go forbids unused imports and
//! the code is substituted into a user file): `to_int` is a manual `_atoi`
//! helper method, DFA tables use an inline anonymous struct type, and there
//! are no top-level declarations besides the struct + constructor (so several
//! generated fsms compose in one file — needed for Mode C).
//!
//! # v0.1 scope
//!
//! Full parity with the Python reference backend: single-match and
//! multi-match (`|`) ordered-choice states, captures, bare-expression
//! returns, action blocks, declared `actions:` methods, all transition forms,
//! embedding actions, Mode C sub-fsm call-out, all three alphabets, and
//! boundary anchors. Not yet handled (clear `Unsupported` error): mid-pattern
//! anchors and `\b`/`\B`, a Mode C stage as a `|` selector, and a `|`
//! alternative with elements before its first stage.

use crate::frame_c::compiler::frame_ast::{
    BinaryOp, EmbeddingOp, Expression, FsmDeclAst, FsmStateAst, FsmTransitionTarget, Literal,
    MatchAst, MatchElement, StageAst, Type, UnaryOp,
};
use crate::frame_c::compiler::fsm_regex::{
    self, size_check::DEFAULT_MAX_DFA_STATES, subset::DfaLabel, Alphabet, CompileError,
};
use std::fmt::Write;

/// The inline Go type of a DFA-table slice (an anonymous struct type, so no
/// top-level declaration is needed and several fsms compose in one file).
const DFA_TYPE: &str = "[]struct{ T [][3]int; A bool }";

/// Generate Go source implementing `decl`, or a reason it is outside the v0.1
/// Go cut.
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
                 Go backend",
                regex
            )),
        }
    }

    /// The Go type for a Frame type string.
    fn go_type(t: &Type) -> String {
        let s = match t {
            Type::Custom(s) => s.as_str(),
            _ => "interface{}",
        };
        match s {
            "int" => "int".to_string(),
            "float" => "float64".to_string(),
            "bool" => "bool".to_string(),
            "str" | "string" | "String" | "bytes" => "string".to_string(),
            other => other.to_string(),
        }
    }

    /// The Go type of the input field: `[]string` for tokens, else `[]rune`.
    fn input_type(&self) -> &'static str {
        match self.alphabet {
            Alphabet::Token => "[]string",
            _ => "[]rune",
        }
    }

    /// The Go type of the input constructor parameter (before materialization
    /// into the `[]rune`/`[]string` field).
    fn input_ctor_type(&self) -> &'static str {
        match self.alphabet {
            Alphabet::Token => "[]string",
            _ => "string",
        }
    }

    /// The Go type of `matched` / captures: `[]string` for tokens, else
    /// `string`.
    fn matched_type(&self) -> &'static str {
        match self.alphabet {
            Alphabet::Token => "[]string",
            _ => "string",
        }
    }

    /// Materialize the matched run `m.<input>[cursor..end]` into a value of
    /// [`Self::matched_type`].
    fn matched_slice(&self, end: &str) -> String {
        let inp = &self.decl.params[0].name;
        match self.alphabet {
            Alphabet::Token => format!("m.{}[m.cursor:{}]", inp, end),
            _ => format!("string(m.{}[m.cursor:{}])", inp, end),
        }
    }

    /// The per-element read as an `int`: code point for byte/char, token id
    /// for the token alphabet.
    fn element_read(&self) -> String {
        let inp = &self.decl.params[0].name;
        match self.alphabet {
            Alphabet::Token => format!("m.tokId(m.{}[pos])", inp),
            _ => format!("int(m.{}[pos])", inp),
        }
    }

    fn emit(&self) -> Result<String, String> {
        let mut out = String::new();
        out.push_str("// Generated by framec — RFC-0042 @@fsm (Go backend).\n\n");
        self.emit_struct(&mut out);
        self.emit_ctor(&mut out);
        self.emit_atoi(&mut out);
        self.emit_tok_id(&mut out);
        self.emit_dfa_matcher(&mut out);
        self.emit_run(&mut out);
        self.emit_state_methods(&mut out)?;
        self.emit_embed_matchers(&mut out)?;
        self.emit_action_methods(&mut out)?;
        Ok(out)
    }

    fn emit_struct(&self, out: &mut String) {
        writeln!(out, "type {} struct {{", self.decl.name).ok();
        out.push_str("\taccepted bool\n");
        out.push_str("\treject_position int\n");
        out.push_str("\tcursor int\n");
        writeln!(
            out,
            "\treturn_value {}",
            Self::go_type(&self.decl.return_type)
        )
        .ok();
        let mut seen = std::collections::HashSet::new();
        for (i, p) in self.decl.params.iter().enumerate() {
            seen.insert(p.name.clone());
            let ty = if i == 0 {
                self.input_type().to_string()
            } else {
                Self::go_type(&p.param_type)
            };
            writeln!(out, "\t{} {}", p.name, ty).ok();
        }
        if let Some(domain) = &self.decl.domain {
            for v in &domain.vars {
                if !seen.insert(v.name.clone()) {
                    continue;
                }
                writeln!(out, "\t{} {}", v.name, Self::go_type(&v.var_type)).ok();
            }
        }
        writeln!(out, "\tmatched {}", self.matched_type()).ok();
        out.push_str("\tenter int\n");
        for f in self.capture_fields() {
            writeln!(out, "\t{} {}", f, self.matched_type()).ok();
        }
        for (f, inner) in self.mode_c_inst_fields() {
            writeln!(out, "\t{} *{}", f, inner).ok();
        }
        out.push_str("}\n\n");
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
                    Self::go_type(&p.param_type)
                };
                format!("{} {}", p.name, ty)
            })
            .collect();
        writeln!(
            out,
            "func New{}({}) *{} {{",
            self.decl.name,
            sig.join(", "),
            self.decl.name
        )
        .ok();
        writeln!(out, "\tm := &{}{{}}", self.decl.name).ok();
        writeln!(
            out,
            "\tm.return_value = {}",
            go_default(&self.decl.return_type, &self.decl.default_expr)
        )
        .ok();
        for (i, p) in self.decl.params.iter().enumerate() {
            // The byte/char input is materialized into a `[]rune`; a token
            // input is already a `[]string`; other params bind directly.
            if i == 0 && self.alphabet != Alphabet::Token {
                writeln!(out, "\tm.{} = []rune({})", p.name, p.name).ok();
            } else {
                writeln!(out, "\tm.{} = {}", p.name, p.name).ok();
            }
        }
        if let Some(domain) = &self.decl.domain {
            for v in &domain.vars {
                if &v.name == input {
                    continue;
                }
                writeln!(out, "\tm.{} = {}", v.name, self.expr(&v.default)).ok();
            }
        }
        out.push_str("\tm.run()\n");
        out.push_str("\tif m.accepted {\n\t\tm.reject_position = 0\n\t}\n");
        out.push_str("\treturn m\n}\n\n");
    }

    /// Import-free integer parse for the `to_int` built-in.
    fn emit_atoi(&self, out: &mut String) {
        writeln!(out, "func (m *{}) _atoi(s string) int {{", self.decl.name).ok();
        out.push_str(
            "\tn := 0\n\
             \tneg := false\n\
             \tfor i, c := range s {\n\
             \t\tif i == 0 && c == '-' {\n\
             \t\t\tneg = true\n\
             \t\t\tcontinue\n\
             \t\t}\n\
             \t\tn = n*10 + int(c-'0')\n\
             \t}\n\
             \tif neg {\n\t\tn = -n\n\t}\n\
             \treturn n\n}\n\n",
        );
    }

    /// Token alphabet: emit the `tokId` atom → id lookup (unknown → -1).
    fn emit_tok_id(&self, out: &mut String) {
        if self.alphabet != Alphabet::Token {
            return;
        }
        let mut entries: Vec<(&String, &u32)> = self.token_ids.iter().collect();
        entries.sort_by_key(|(_, id)| **id);
        writeln!(out, "func (m *{}) tokId(t string) int {{", self.decl.name).ok();
        out.push_str("\tswitch t {\n");
        for (name, id) in entries {
            writeln!(out, "\tcase {:?}:\n\t\treturn {}", name, id).ok();
        }
        out.push_str("\t}\n\treturn -1\n}\n\n");
    }

    fn emit_dfa_matcher(&self, out: &mut String) {
        let input = &self.decl.params[0].name;
        let read = self.element_read();
        writeln!(
            out,
            "func (m *{name}) dfaMatch(states {dfa}, start int) int {{\n\
             \tst := start\n\
             \tpos := m.cursor\n\
             \tn := len(m.{input})\n\
             \tlast := -1\n\
             \tif states[st].A {{\n\t\tlast = pos\n\t}}\n\
             \tfor pos < n {{\n\
             \t\tv := {read}\n\
             \t\tnxt := -1\n\
             \t\tfor _, tr := range states[st].T {{\n\
             \t\t\tif tr[0] <= v && v <= tr[1] {{\n\t\t\t\tnxt = tr[2]\n\t\t\t\tbreak\n\t\t\t}}\n\
             \t\t}}\n\
             \t\tif nxt < 0 {{\n\t\t\tbreak\n\t\t}}\n\
             \t\tst = nxt\n\t\tpos++\n\
             \t\tif states[st].A {{\n\t\t\tlast = pos\n\t\t}}\n\
             \t}}\n\
             \treturn last\n}}\n\n",
            name = self.decl.name,
            dfa = DFA_TYPE,
            input = input,
            read = read
        )
        .ok();
    }

    fn emit_run(&self, out: &mut String) {
        writeln!(out, "func (m *{}) run() {{", self.decl.name).ok();
        out.push_str("\tstate := 0\n\tfor state >= 0 {\n");
        out.push_str("\t\tenter := m.enter\n\t\tm.enter = 0\n");
        out.push_str("\t\tswitch state {\n");
        for i in 0..self.decl.states.len() {
            writeln!(out, "\t\tcase {}:\n\t\t\tstate = m.state{}(enter)", i, i).ok();
        }
        out.push_str("\t\tdefault:\n\t\t\treturn\n\t\t}\n\t}\n}\n\n");
    }

    fn emit_state_methods(&self, out: &mut String) -> Result<(), String> {
        let mut sid = 0usize;
        for (i, st) in self.decl.states.iter().enumerate() {
            match st.matches.len() {
                0 => {
                    writeln!(
                        out,
                        "func (m *{}) state{}(enter int) int {{ return -1 }}\n",
                        self.decl.name, i
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
            "func (m *{}) state{}(enter int) int {{",
            self.decl.name, index
        )
        .ok();
        for (idx, el) in m.elements.iter().enumerate() {
            writeln!(out, "\tif enter <= {} {{", idx).ok();
            self.emit_element(out, el, m, &state_label, "\t\t", sid)?;
            out.push_str("\t}\n");
        }
        self.emit_success(out, m, "\t");
        out.push_str("}\n\n");
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
        writeln!(
            out,
            "func (m *{}) state{}(enter int) int {{",
            self.decl.name, index
        )
        .ok();
        // `enter` is unused for multi-match (selection is by first stage);
        // discard it so Go does not flag an unused parameter — actually Go
        // permits unused parameters, so no discard is needed.
        let mut catch_all = false;
        for mt in &st.matches {
            let first_stage = mt
                .elements
                .iter()
                .position(|e| matches!(e, MatchElement::Stage(_)));
            match first_stage {
                Some(fs) => {
                    if fs > 0 {
                        return Err(
                            "a `|` alternative with elements before its first stage is not yet \
                             supported by the Go backend"
                                .into(),
                        );
                    }
                    let my_sid = *sid;
                    *sid += 1;
                    if self.stage_dfas[my_sid].mode_c.is_some() {
                        return Err(
                            "a Mode C (`/@Fsm/`) stage as a `|` alternative selector is not yet \
                             supported by the Go backend"
                                .into(),
                        );
                    }
                    let MatchElement::Stage(sel) = &mt.elements[fs] else {
                        unreachable!("first_stage indexes a Stage element")
                    };
                    self.emit_dfa_var(out, my_sid, "\t");
                    writeln!(
                        out,
                        "\tr{} := m.dfaMatch(dfa{}, {})",
                        my_sid, my_sid, self.stage_dfas[my_sid].start
                    )
                    .ok();
                    self.emit_anchor_guards(out, my_sid, "\t");
                    writeln!(out, "\tif r{} >= 0 {{", my_sid).ok();
                    writeln!(
                        out,
                        "\t\tm.matched = {}",
                        self.matched_slice(&format!("r{}", my_sid))
                    )
                    .ok();
                    if let Some(lbl) = &sel.label {
                        if !state_label.is_empty() {
                            writeln!(out, "\t\tm.{} = m.matched", cap_field(&state_label, lbl))
                                .ok();
                        }
                    }
                    writeln!(out, "\t\tm.cursor = r{}", my_sid).ok();
                    out.push_str("\t\tm.accepted = true\n");
                    for el in &mt.elements[fs + 1..] {
                        self.emit_element(out, el, mt, &state_label, "\t\t", sid)?;
                    }
                    self.emit_success(out, mt, "\t\t");
                    out.push_str("\t}\n");
                }
                None => {
                    out.push_str("\tm.accepted = true\n");
                    for el in &mt.elements {
                        self.emit_element(out, el, mt, &state_label, "\t", sid)?;
                    }
                    self.emit_success(out, mt, "\t");
                    catch_all = true;
                    break;
                }
            }
        }
        if !catch_all {
            out.push_str("\tm.accepted = false\n");
            out.push_str("\tm.reject_position = m.cursor\n");
            out.push_str("\treturn -1\n");
        }
        out.push_str("}\n\n");
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
        let ind2 = format!("{}\t", ind);
        match el {
            MatchElement::Stage(stage) => {
                let my_sid = *sid;
                *sid += 1;
                if let Some(inner) = self.stage_dfas[my_sid].mode_c.clone() {
                    self.emit_mode_c(out, &inner, stage, m, state_label, my_sid, ind, &ind2);
                    return Ok(());
                }
                if stage.embedding_actions.is_empty() {
                    self.emit_dfa_var(out, my_sid, ind);
                    writeln!(
                        out,
                        "{}r{} := m.dfaMatch(dfa{}, {})",
                        ind, my_sid, my_sid, self.stage_dfas[my_sid].start
                    )
                    .ok();
                } else {
                    writeln!(out, "{}r{} := m.matchStage{}()", ind, my_sid, my_sid).ok();
                }
                self.emit_anchor_guards(out, my_sid, ind);
                writeln!(out, "{}if r{} < 0 {{", ind, my_sid).ok();
                self.emit_failure(out, m, &ind2);
                writeln!(out, "{}}}", ind).ok();
                writeln!(
                    out,
                    "{}m.matched = {}",
                    ind,
                    self.matched_slice(&format!("r{}", my_sid))
                )
                .ok();
                if let Some(lbl) = &stage.label {
                    if !state_label.is_empty() {
                        writeln!(out, "{}m.{} = m.matched", ind, cap_field(state_label, lbl)).ok();
                    }
                }
                writeln!(out, "{}m.cursor = r{}", ind, my_sid).ok();
                writeln!(out, "{}m.accepted = true", ind).ok();
            }
            MatchElement::BareExpression { expr, .. } => {
                writeln!(out, "{}m.return_value = {}", ind, self.expr(expr)).ok();
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
        // The inner fsm's input is the remaining run from the cursor — a
        // `string` for byte/char, a `[]string` for tokens.
        let sub = match self.alphabet {
            Alphabet::Token => format!("m.{}[m.cursor:]", input),
            _ => format!("string(m.{}[m.cursor:])", input),
        };
        writeln!(out, "{}{} := New{}({})", ind, iv, inner, sub).ok();
        writeln!(out, "{}if !{}.accepted {{", ind, iv).ok();
        self.emit_failure(out, m, ind2);
        writeln!(out, "{}}}", ind).ok();
        let end = format!("m.cursor + {}.cursor", iv);
        writeln!(out, "{}m.matched = {}", ind, self.matched_slice(&end)).ok();
        if let Some(lbl) = &stage.label {
            if !state_label.is_empty() {
                writeln!(out, "{}m.{} = m.matched", ind, cap_field(state_label, lbl)).ok();
                writeln!(
                    out,
                    "{}m.{} = {}",
                    ind,
                    cap_inst_field(state_label, lbl),
                    iv
                )
                .ok();
            }
        }
        writeln!(out, "{}m.cursor = m.cursor + {}.cursor", ind, iv).ok();
        writeln!(out, "{}m.accepted = true", ind).ok();
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
        let input = &self.decl.params[0].name;
        let read = self.element_read();
        writeln!(
            out,
            "func (m *{}) matchStage{}() int {{",
            self.decl.name, sid
        )
        .ok();
        self.emit_dfa_var(out, sid, "\t");
        writeln!(
            out,
            "\tentry := m.cursor\n\
             \tst := {start}\n\
             \tpos := entry\n\
             \tn := len(m.{input})\n\
             \tlast := -1\n\
             \tif dfa{sid}[st].A {{\n\t\tlast = pos\n\t}}\n\
             \tm.cursor = pos",
            start = self.stage_dfas[sid].start,
            input = input,
            sid = sid
        )
        .ok();
        out.push_str(&self.embed_body(stage, EmbeddingOp::Start, "\t")?);
        writeln!(out, "\tprev := dfa{}[st].A", sid).ok();
        writeln!(
            out,
            "\tfor pos < n {{\n\
             \t\tv := {read}\n\
             \t\tnxt := -1\n\
             \t\tfor _, tr := range dfa{sid}[st].T {{\n\
             \t\t\tif tr[0] <= v && v <= tr[1] {{\n\t\t\t\tnxt = tr[2]\n\t\t\t\tbreak\n\t\t\t}}\n\
             \t\t}}\n\
             \t\tif nxt < 0 {{\n\t\t\tbreak\n\t\t}}\n\
             \t\tst = nxt\n\t\tpos++\n\
             \t\tm.cursor = pos",
            read = read,
            sid = sid
        )
        .ok();
        out.push_str(&self.embed_body(stage, EmbeddingOp::EveryTransition, "\t\t")?);
        writeln!(out, "\t\tnow := dfa{}[st].A", sid).ok();
        let accept = self.embed_body(stage, EmbeddingOp::Accept, "\t\t\t")?;
        if !accept.is_empty() {
            out.push_str("\t\tif now {\n");
            out.push_str(&accept);
            out.push_str("\t\t}\n");
        }
        out.push_str("\t\tif now {\n\t\t\tlast = pos\n\t\t}\n\t\tprev = now\n");
        out.push_str("\t}\n");
        // `%{}` — left the last accepting state: a post-scan event firing once
        // when the longest match stops extending (failing element or EOF), with
        // `@@:cursor` at the end of the matched region (`last`), not the failing
        // element (§5.4 / FSM-TEST-603). `last < 0` ⇒ no accepting state was
        // entered, so there is nothing to leave.
        let leave = self.embed_body(stage, EmbeddingOp::LeaveAccept, "\t\t")?;
        if !leave.is_empty() {
            out.push_str("\tif last >= 0 {\n\t\tm.cursor = last\n");
            out.push_str(&leave);
            out.push_str("\t}\n");
        }
        let eof = self.embed_body(stage, EmbeddingOp::Eof, "\t\t")?;
        if !eof.is_empty() {
            out.push_str("\tif pos >= n && !prev {\n");
            out.push_str(&eof);
            out.push_str("\t}\n");
        } else {
            // `prev` is otherwise only read by the `%{}` guard; reference it
            // so Go does not flag an unused variable when neither is present.
            out.push_str("\t_ = prev\n");
        }
        out.push_str("\tm.cursor = entry\n\treturn last\n}\n\n");
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
        let input = &self.decl.params[0].name;
        if dfa.requires_start {
            writeln!(
                out,
                "{}if m.cursor != 0 {{\n{}\tr{} = -1\n{}}}",
                ind, ind, sid, ind
            )
            .ok();
        }
        if dfa.requires_end {
            writeln!(
                out,
                "{}if r{} != len(m.{}) {{\n{}\tr{} = -1\n{}}}",
                ind, sid, input, ind, sid, ind
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
        writeln!(out, "{}m.accepted = false", ind).ok();
        writeln!(out, "{}m.reject_position = m.cursor", ind).ok();
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
                        self.emit_goto(out, state, stage, &format!("{}\t", ind));
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
                "{}panic(\"transition to undeclared state ${}\")",
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
                    writeln!(out, "{}m.enter = {}", ind, entry).ok();
                }
                None => {
                    writeln!(
                        out,
                        "{}panic(\"transition to undeclared stage ${}.{}\")",
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
                let inner = format!("{}\t", ind);
                let mut out = format!("{}if {} {{\n", ind, self.expr(&if_ast.condition));
                out.push_str(&self.stmt(&if_ast.then_branch, &inner)?);
                if let Some(else_b) = &if_ast.else_branch {
                    out.push_str(&format!("{}}} else {{\n", ind));
                    out.push_str(&self.stmt(else_b, &inner)?);
                    out.push_str(&format!("{}}}\n", ind));
                } else {
                    out.push_str(&format!("{}}}\n", ind));
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
                "statement {:?} not supported in @@fsm action blocks by the Go backend",
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
                .map(|p| format!("{} {}", p.name, Self::go_type(&p.param_type)))
                .collect();
            let ret = match &act.return_type {
                Some(t) => format!(" {}", Self::go_type(t)),
                None => String::new(),
            };
            writeln!(
                out,
                "func (m *{}) {}({}){} {{",
                self.decl.name,
                act.name,
                sig.join(", "),
                ret
            )
            .ok();
            let n = act.body.statements.len();
            let has_return = act.return_type.is_some();
            for (i, s) in act.body.statements.iter().enumerate() {
                use crate::frame_c::compiler::frame_ast::Statement;
                if i + 1 == n && has_return {
                    if let Statement::Expression(e) = s {
                        if !matches!(e.expr, Expression::Assign { .. }) {
                            writeln!(out, "\treturn {}", self.expr(&e.expr)).ok();
                            continue;
                        }
                    }
                }
                out.push_str(&self.stmt(s, "\t")?);
            }
            out.push_str("}\n\n");
        }
        Ok(())
    }

    /// Emit the per-stage DFA as a local `dfa<sid>` variable at indent `ind`.
    fn emit_dfa_var(&self, out: &mut String, sid: usize, ind: &str) {
        let dfa = &self.stage_dfas[sid];
        let states: Vec<String> = dfa
            .states
            .iter()
            .map(|(trans, acc)| {
                let ts: Vec<String> = trans
                    .iter()
                    .map(|(lo, hi, tgt)| format!("{{{}, {}, {}}}", lo, hi, tgt))
                    .collect();
                format!("{{T: [][3]int{{{}}}, A: {}}}", ts.join(", "), acc)
            })
            .collect();
        writeln!(
            out,
            "{}dfa{} := {}{{{}}}",
            ind,
            sid,
            DFA_TYPE,
            states.join(", ")
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
                Literal::Null => "nil".to_string(),
            },
            Expression::Var(name) => match name.as_str() {
                "@@:matched" => "m.matched".to_string(),
                "@@:cursor" => "m.cursor".to_string(),
                "@@:return" => "m.return_value".to_string(),
                _ => match name.strip_prefix('$').and_then(|c| c.split_once('.')) {
                    Some((state, label)) => format!("m.{}", cap_field(state, label)),
                    None => name.clone(),
                },
            },
            Expression::Binary { left, op, right } => {
                format!("({} {} {})", self.expr(left), binop(op), self.expr(right))
            }
            Expression::Unary { op, expr } => match op {
                UnaryOp::Not => format!("(!{})", self.expr(expr)),
                UnaryOp::Neg => format!("(-{})", self.expr(expr)),
                UnaryOp::BitNot => format!("(^{})", self.expr(expr)),
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
                            return format!("m.{}.{}", cap_inst_field(state, label), field);
                        }
                    }
                    if name == "self" {
                        return format!("m.{}", field);
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
            "to_int" => format!("m._atoi({})", a.join(", ")),
            "to_str" => format!("({})", a.join(", ")),
            "len" => format!("len({})", a.join(", ")),
            _ => format!("m.{}({})", func, a.join(", ")),
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

fn go_default(ty: &Type, raw: &str) -> String {
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
        _ => "nil".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frame_c::compiler::fsm_parser::parse_fsm_block;
    use std::process::Command;

    /// Compile + run a Go `package main` program (generated code + driver)
    /// via `go run`, returning stdout lines. `None` if `go` is unavailable.
    fn go_run(code: &str, driver: &str, tag: &str) -> Option<Vec<String>> {
        let prog = format!(
            "package main\n\nimport \"fmt\"\n\n{code}\nfunc main() {{\n{driver}\n}}\n",
            code = code,
            driver = driver
        );
        let dir = std::env::temp_dir().join(format!("framec_go_{}", tag));
        std::fs::create_dir_all(&dir).ok()?;
        let path = dir.join("main.go");
        std::fs::write(&path, prog).ok()?;
        let out = match Command::new("go").arg("run").arg(&path).output() {
            Ok(o) => o,
            Err(_) => return None,
        };
        assert!(
            out.status.success(),
            "go run failed for {:?}:\n{}",
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

    /// Run `New M(<arg>)` and return `(accepted, return_value)`.
    fn run(src: &str, ctor: &str, tag: &str) -> Option<(String, String)> {
        let code = gen(src);
        let driver =
            format!("\tm := {ctor}\n\tfmt.Println(m.accepted)\n\tfmt.Println(m.return_value)");
        let lines = go_run(&code, &driver, tag)?;
        Some((lines[0].clone(), lines[1].clone()))
    }

    #[test]
    fn go_smoke_bool() {
        let src = "@@fsm M(text: bytes) : bool = false { /a/ true }";
        let Some((acc, ret)) = run(src, "NewM(\"a\")", "smoke_a") else {
            return;
        };
        assert_eq!((acc.as_str(), ret.as_str()), ("true", "true"));
        assert_eq!(run(src, "NewM(\"b\")", "smoke_b").unwrap().0, "false");
    }

    #[test]
    fn go_matched_to_int() {
        let src = "@@fsm M(text: bytes) : int = 0 { /[0-9]+/ to_int(@@:matched) }";
        let Some((acc, ret)) = run(src, "NewM(\"123\")", "tok_a") else {
            return;
        };
        assert_eq!((acc.as_str(), ret.as_str()), ("true", "123"));
        assert_eq!(run(src, "NewM(\"x\")", "tok_b").unwrap().0, "false");
    }

    #[test]
    fn go_len_self_input() {
        let src = "@@fsm M(text: bytes) : int = 0 { /[0-9]+/ len(self.text) }";
        let Some((_, ret)) = run(src, "NewM(\"123\")", "len_a") else {
            return;
        };
        assert_eq!(ret, "3");
    }

    #[test]
    fn go_stage_capture() {
        let src = "@@fsm M(text: bytes) : int = 0 { $s: .n/[0-9]+/ to_int($s.n) }";
        let Some((acc, ret)) = run(src, "NewM(\"42\")", "cap_a") else {
            return;
        };
        assert_eq!((acc.as_str(), ret.as_str()), ("true", "42"));
    }

    #[test]
    fn go_action_block() {
        let src = "@@fsm M(text: bytes) : int = 0 { \
                   /[0-9]/ { self.count = self.count + 1 } self.count \
                   domain: count: int = 0 }";
        let Some((_, ret)) = run(src, "NewM(\"5\")", "act_a") else {
            return;
        };
        assert_eq!(ret, "1");
    }

    #[test]
    fn go_declared_action() {
        let src = "@@fsm M(text: bytes) : int = 0 { \
                   /[0-9]+/ parse_int(@@:matched) \
                   actions: parse_int(s: bytes): int { to_int(s) } }";
        let Some((_, ret)) = run(src, "NewM(\"42\")", "decl_a") else {
            return;
        };
        assert_eq!(ret, "42");
    }

    #[test]
    fn go_transitions_and_capture() {
        let src = "@@fsm M(text: bytes) : int = 0 { \
                   $0: /[a-z]/ -> $digits : -> $error \
                   $digits: .n/[0-9]+/ to_int($digits.n) \
                   $error: -1 }";
        let Some((acc, ret)) = run(src, "NewM(\"x42\")", "tr_a") else {
            return;
        };
        assert_eq!((acc.as_str(), ret.as_str()), ("true", "42"));
        assert_eq!(run(src, "NewM(\"X\")", "tr_b").unwrap().1, "-1");
    }

    #[test]
    fn go_conditional_target() {
        let src = "@@fsm M(text: bytes, mode: int) : int = 0 { \
                   /[01]/ -> ( $zero when self.mode == 0, $one when self.mode == 1 ) : -> $error \
                   $zero: 0 \
                   $one: 1 \
                   $error: -1 }";
        let Some(z) = run(src, "NewM(\"0\", 0)", "cond_a") else {
            return;
        };
        assert_eq!(z.1, "0");
        assert_eq!(run(src, "NewM(\"1\", 1)", "cond_b").unwrap().1, "1");
        assert_eq!(run(src, "NewM(\"0\", 2)", "cond_c").unwrap().1, "-1");
    }

    #[test]
    fn go_multi_match() {
        let code = gen("@@fsm M(text: bytes) : int = 0 { /[0-9]/ -> $num | 99 $num: 1 }");
        let driver = "\tfor _, s := range []string{\"5\", \"a\"} {\n\t\tm := NewM(s)\n\t\tfmt.Println(m.return_value)\n\t}";
        let Some(lines) = go_run(&code, driver, "mm") else {
            return;
        };
        assert_eq!(lines, vec!["1", "99"]);
    }

    #[test]
    fn go_embed_every_transition() {
        let code = gen(
            "@@fsm M(text: bytes) : int = 0 { /[0-9]+/ ${ tally() } self.count \
             actions: tally() { self.count = self.count + 1 } domain: count: int = 0 }",
        );
        let driver = "\tm := NewM(\"123\")\n\tfmt.Println(m.return_value)";
        let Some(lines) = go_run(&code, driver, "emb") else {
            return;
        };
        assert_eq!(lines[0], "3");
    }

    /// FSM-TEST-603 — `%{...}` fires when the DFA leaves its last accepting
    /// state, capturing the end of the matched region (`last`), not the
    /// failing element. `"42x"` ⇒ end_pos 2; `"abx"` never accepts ⇒ 0.
    #[test]
    fn go_embed_leave_final() {
        let code = gen("@@fsm M(text: bytes) : int = 0 { \
             /[0-9]+/ %{ self.end_pos = @@:cursor } self.end_pos \
             domain: end_pos: int = 0 }");
        let driver = "\tfor _, s := range []string{\"42x\", \"abx\"} {\n\t\tm := NewM(s)\n\t\tfmt.Println(m.return_value)\n\t}";
        let Some(lines) = go_run(&code, driver, "embleave") else {
            return;
        };
        assert_eq!(lines, vec!["2", "0"]);
    }

    #[test]
    fn go_token_alphabet() {
        let code = gen("@@fsm M(toks: token) : bool = false { /IDENT LPAREN RPAREN/ true }");
        let driver = "\tfor _, t := range [][]string{{\"IDENT\",\"LPAREN\",\"RPAREN\"},{\"IDENT\",\"RPAREN\"},{\"IDENT\",\"WAT\"}} {\n\t\tm := NewM(t)\n\t\tfmt.Println(m.accepted)\n\t}";
        let Some(lines) = go_run(&code, driver, "tok") else {
            return;
        };
        assert_eq!(lines, vec!["true", "false", "false"]);
    }

    #[test]
    fn go_mode_c_callout() {
        let inner = gen("@@fsm Digits(text: bytes) : int = 0 { /[0-9]+/ to_int(@@:matched) }");
        let outer = gen("@@fsm Outer(text: bytes) : int = 0 { $s: .d/@Digits/ $s.d.return_value }");
        let code = format!("{}\n{}", inner, outer);
        let driver = "\tfor _, s := range []string{\"42\", \"x\"} {\n\t\tm := NewOuter(s)\n\t\tfmt.Println(m.accepted, m.return_value)\n\t}";
        let Some(lines) = go_run(&code, driver, "modec") else {
            return;
        };
        assert_eq!(lines, vec!["true 42", "false 0"]);
    }

    #[test]
    fn go_anchors() {
        let start = gen("@@fsm M(text: bytes) : bool = false { /^foo/ true }");
        let d1 = "\tfor _, s := range []string{\"foo\", \"xfoo\"} {\n\t\tm := NewM(s)\n\t\tfmt.Println(m.accepted)\n\t}";
        let Some(l1) = go_run(&start, d1, "anc_s") else {
            return;
        };
        assert_eq!(l1, vec!["true", "false"]);
        let end = gen("@@fsm M(text: bytes) : bool = false { /[0-9]+$/ true }");
        let d2 = "\tfor _, s := range []string{\"123\", \"123x\"} {\n\t\tm := NewM(s)\n\t\tfmt.Println(m.accepted)\n\t}";
        let Some(l2) = go_run(&end, d2, "anc_e") else {
            return;
        };
        assert_eq!(l2, vec!["true", "false"]);
    }

    #[test]
    fn go_unsupported_errors() {
        let decl =
            parse_fsm_block(b"@@fsm M(text: bytes) : bool = false { /a$b/ true }").expect("parses");
        let err = generate(&decl).unwrap_err();
        assert!(err.contains("anchor"), "got {err}");
    }
}
