//! Java backend for `@@fsm` (RFC-0042, Phase 8).
//!
//! Generates a self-contained Java `class` from a validated `FsmDeclAst`. The
//! recognition model mirrors the Python reference backend
//! ([`super::fsm_python`]) with the JavaScript structure (running `sid`
//! counter, inline `if (enter <= idx)` guards) over mutable instance fields,
//! with static types. Frame's abstract types map as `int` → `int`, `float`
//! → `double`, `bool` → `boolean`, `str`/`bytes` → `String`. The constructor
//! is `new <Name>(...)`; the observable result (§5.1) is the instance's
//! public `accepted`, `return_value`, `cursor`, `reject_position`.
//!
//! Implementation notes:
//! - The class is emitted **package-private** (no `public` modifier) so that
//!   several generated classes (e.g. a Mode C outer fsm and its inner) plus a
//!   driver can share one `.java` file (Java allows one public top-level
//!   class per file). A user can add `public` if needed.
//! - The recognizer is **import-free**: `to_int` is `Integer.parseInt`
//!   (`java.lang`), and the token alphabet's array slice is a manual `_slice`
//!   helper.
//! - A per-stage DFA is two parallel arrays — `int[][][]` transitions and
//!   `boolean[]` accept-flags — since a state mixes an array and a bool.
//!
//! # v0.1 scope
//!
//! Full parity with the Python reference backend: single-match and
//! multi-match (`|`) ordered-choice states, captures, bare-expression
//! returns, action blocks, declared `actions:` methods, all transition
//! forms, embedding actions, Mode C sub-fsm call-out, all three alphabets,
//! edge anchors, `\b`/`\B` word boundaries, interior anchors, and lazy
//! quantifiers (the last three via the Pike VM with zero-width `Assert`s).
//! Not yet handled (clear `Unsupported` error): a Mode C stage as a `|`
//! selector, and a `|` alternative with elements before its first stage.

use crate::frame_c::compiler::frame_ast::{
    BinaryOp, EmbeddingOp, Expression, FsmDeclAst, FsmStateAst, FsmTransitionTarget, Literal,
    MatchAst, MatchElement, StageAst, Type, UnaryOp,
};
use crate::frame_c::compiler::fsm_regex::{
    self, pike::Program, size_check::DEFAULT_MAX_DFA_STATES, subset::DfaLabel, Alphabet,
    CompileError, WordBoundary,
};
use std::fmt::Write;

/// Generate Java source implementing `decl`, or a reason it is outside the
/// v0.1 Java cut.
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
    /// `Some` when the stage's regex contains a lazy quantifier (§11.1): a Pike
    /// program matched by the VM (`pikeMatch`) instead of the DFA.
    program: Option<Program>,
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
                                program: None,
                                mode_c: Some(inner.to_string()),
                            });
                            continue;
                        }
                        match Self::compile_one(self.alphabet, &stage.regex, &mut token_ids) {
                            Ok(dfa) => {
                                // A lazy quantifier matches via the Pike VM, which
                                // has no per-element scan for embedding actions to
                                // hook into (§3.5.5/§11.1).
                                if dfa.program.is_some() && !stage.embedding_actions.is_empty() {
                                    self.token_ids = token_ids;
                                    return Err("a lazy quantifier in a stage with embedding \
                                                actions is not yet supported by the Java backend"
                                        .to_string());
                                }
                                self.stage_dfas.push(dfa);
                            }
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
                    program: compiled.program,
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
                 Java backend",
                regex
            )),
        }
    }

    fn java_type(t: &Type) -> String {
        let s = match t {
            Type::Custom(s) => s.as_str(),
            _ => "Object",
        };
        match s {
            "int" => "int".to_string(),
            "float" => "double".to_string(),
            "bool" => "boolean".to_string(),
            "str" | "string" | "String" | "bytes" => "String".to_string(),
            other => other.to_string(),
        }
    }

    fn input_type(&self) -> &'static str {
        match self.alphabet {
            Alphabet::Token => "String[]",
            _ => "String",
        }
    }

    fn matched_type(&self) -> &'static str {
        match self.alphabet {
            Alphabet::Token => "String[]",
            _ => "String",
        }
    }

    fn matched_empty(&self) -> &'static str {
        match self.alphabet {
            Alphabet::Token => "new String[0]",
            _ => "\"\"",
        }
    }

    /// The input length (`.length()` for a `String`, `.length` for an array).
    fn input_len(&self) -> String {
        let inp = &self.decl.params[0].name;
        match self.alphabet {
            Alphabet::Token => format!("{}.length", inp),
            _ => format!("{}.length()", inp),
        }
    }

    fn element_read(&self) -> String {
        let inp = &self.decl.params[0].name;
        match self.alphabet {
            Alphabet::Token => format!("tokId({}[pos])", inp),
            _ => format!("{}.charAt(pos)", inp),
        }
    }

    fn matched_slice(&self, end: &str) -> String {
        let inp = &self.decl.params[0].name;
        match self.alphabet {
            Alphabet::Token => format!("_slice({}, cursor, {})", inp, end),
            _ => format!("{}.substring(cursor, {})", inp, end),
        }
    }

    fn emit(&self) -> Result<String, String> {
        let mut out = String::new();
        out.push_str("// Generated by framec — RFC-0042 @@fsm (Java backend).\n\n");
        // Package-private so several classes + a driver can share one file.
        writeln!(out, "class {} {{", self.decl.name).ok();
        self.emit_field_decls(&mut out);
        self.emit_ctor(&mut out);
        self.emit_tok_id(&mut out);
        self.emit_slice(&mut out);
        self.emit_iswordat(&mut out);
        self.emit_dfa_matcher(&mut out);
        if self.uses_pike() {
            self.emit_pike_matcher(&mut out);
        }
        self.emit_run(&mut out);
        self.emit_state_methods(&mut out)?;
        self.emit_embed_matchers(&mut out)?;
        self.emit_action_methods(&mut out)?;
        out.push_str("}\n");
        Ok(out)
    }

    fn emit_field_decls(&self, out: &mut String) {
        out.push_str("  public boolean accepted;\n");
        out.push_str("  public int reject_position;\n");
        out.push_str("  public int cursor;\n");
        writeln!(
            out,
            "  public {} return_value;",
            Self::java_type(&self.decl.return_type)
        )
        .ok();
        let mut seen = std::collections::HashSet::new();
        for (i, p) in self.decl.params.iter().enumerate() {
            seen.insert(p.name.clone());
            let ty = if i == 0 {
                self.input_type().to_string()
            } else {
                Self::java_type(&p.param_type)
            };
            writeln!(out, "  public {} {};", ty, p.name).ok();
        }
        if let Some(domain) = &self.decl.domain {
            for v in &domain.vars {
                if !seen.insert(v.name.clone()) {
                    continue;
                }
                writeln!(out, "  public {} {};", Self::java_type(&v.var_type), v.name).ok();
            }
        }
        writeln!(out, "  public {} matched;", self.matched_type()).ok();
        out.push_str("  public int enter;\n");
        for f in self.capture_fields() {
            writeln!(out, "  public {} {};", self.matched_type(), f).ok();
        }
        for (f, inner) in self.mode_c_inst_fields() {
            writeln!(out, "  public {} {};", inner, f).ok();
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
                    Self::java_type(&p.param_type)
                };
                format!("{} {}", ty, p.name)
            })
            .collect();
        writeln!(out, "  {}({}) {{", self.decl.name, sig.join(", ")).ok();
        writeln!(
            out,
            "    return_value = {};",
            java_default(&self.decl.return_type, &self.decl.default_expr)
        )
        .ok();
        for p in &self.decl.params {
            writeln!(out, "    this.{} = {};", p.name, p.name).ok();
        }
        if let Some(domain) = &self.decl.domain {
            for v in &domain.vars {
                if &v.name == input {
                    continue;
                }
                writeln!(out, "    {} = {};", v.name, self.expr(&v.default)).ok();
            }
        }
        writeln!(out, "    matched = {};", self.matched_empty()).ok();
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
        out.push_str("  int tokId(String t) {\n    switch (t) {\n");
        for (name, id) in entries {
            writeln!(out, "      case {:?}: return {};", name, id).ok();
        }
        out.push_str("      default: return -1;\n    }\n  }\n\n");
    }

    /// Token alphabet only: a `[lo, hi)` array slice (import-free).
    fn emit_slice(&self, out: &mut String) {
        if self.alphabet != Alphabet::Token {
            return;
        }
        out.push_str("  String[] _slice(String[] a, int lo, int hi) {\n");
        out.push_str("    String[] r = new String[hi - lo];\n");
        out.push_str("    for (int i = lo; i < hi; i++) r[i - lo] = a[i];\n");
        out.push_str("    return r;\n  }\n\n");
    }

    /// True iff any stage carries a `\b`/`\B` edge boundary, requiring the
    /// `iswordat` helper to be emitted.
    fn uses_word_boundary(&self) -> bool {
        self.stage_dfas
            .iter()
            .any(|d| d.start_boundary.is_some() || d.end_boundary.is_some())
    }

    /// `iswordat(p)` — is the byte at position `p` a word byte
    /// (`[0-9A-Za-z_]`)? Out-of-bounds positions are non-word (`false`).
    /// Word boundaries are bytes-only (the engine rejects them on other
    /// alphabets), so the input is a `String` read via `charAt`.
    fn emit_iswordat(&self, out: &mut String) {
        if !self.uses_word_boundary() {
            return;
        }
        let inp = &self.decl.params[0].name;
        writeln!(out, "  boolean iswordat(int p) {{").ok();
        writeln!(out, "    if (p < 0 || p >= {}.length()) return false;", inp).ok();
        writeln!(out, "    char c = {}.charAt(p);", inp).ok();
        out.push_str(
            "    return (c >= '0' && c <= '9') || (c >= 'A' && c <= 'Z') || (c >= 'a' && c <= 'z') || c == '_';\n",
        );
        out.push_str("  }\n\n");
    }

    fn emit_dfa_matcher(&self, out: &mut String) {
        let read = self.element_read();
        writeln!(
            out,
            "  int dfaMatch(int[][][] trans, boolean[] accept, int start) {{\n\
             \x20   int st = start;\n\
             \x20   int pos = cursor;\n\
             \x20   int n = {len};\n\
             \x20   int last = accept[st] ? pos : -1;\n\
             \x20   while (pos < n) {{\n\
             \x20     int v = {read};\n\
             \x20     int nxt = -1;\n\
             \x20     for (int[] tr : trans[st]) {{ if (tr[0] <= v && v <= tr[1]) {{ nxt = tr[2]; break; }} }}\n\
             \x20     if (nxt < 0) break;\n\
             \x20     st = nxt; pos++;\n\
             \x20     if (accept[st]) last = pos;\n\
             \x20   }}\n\
             \x20   return last;\n\
             \x20 }}\n\n",
            len = self.input_len(),
            read = read
        )
        .ok();
    }

    /// Does any stage match via the Pike VM (a lazy quantifier, §11.1)?
    fn uses_pike(&self) -> bool {
        self.stage_dfas.iter().any(|d| d.program.is_some())
    }

    /// Pike VM (priority NFA simulation) for lazy-quantifier stages, over the
    /// flat `ops`/`rng` arrays (`fsm_regex::pike::encode`). Returns the end
    /// position of the highest-priority (leftmost-first) match from the cursor,
    /// or -1. `ops` is 4 ints per instruction `[op, a, b, _]`: 0 Char (a = pair
    /// index, b = pair count), 1 Split (a/b targets, a higher), 2 Jmp, 3 Match.
    fn emit_pike_matcher(&self, out: &mut String) {
        let inp = &self.decl.params[0].name;
        writeln!(
            out,
            "  boolean pikeIsWord(int p, int[] word) {{\n\
             \x20   if (p < 0 || p >= {len}) return false;\n\
             \x20   int v = {inp}.charAt(p);\n\
             \x20   for (int k = 0; k < word.length / 2; k++) {{\n\
             \x20     if (word[k * 2] <= v && v <= word[k * 2 + 1]) return true;\n\
             \x20   }}\n\
             \x20   return false;\n\
             \x20 }}\n\n\
             \x20 boolean pikeAssert(int kind, int pos, int[] word) {{\n\
             \x20   int n = {len};\n\
             \x20   if (kind == 0) return pos == 0;\n\
             \x20   if (kind == 1) return pos == n;\n\
             \x20   if (kind == 2) return pos == 0 || {inp}.charAt(pos - 1) == 10;\n\
             \x20   if (kind == 3) return pos == n || {inp}.charAt(pos) == 10;\n\
             \x20   if (kind == 4) return pikeIsWord(pos - 1, word) != pikeIsWord(pos, word);\n\
             \x20   return pikeIsWord(pos - 1, word) == pikeIsWord(pos, word);\n\
             \x20 }}\n\n\
             \x20 void pikeAdd(int[] ops, int[] word, java.util.List<Integer> lst, boolean[] seen, int pc, int pos) {{\n\
             \x20   if (seen[pc]) return;\n\
             \x20   seen[pc] = true;\n\
             \x20   int op = ops[pc * 4];\n\
             \x20   if (op == 2) {{\n\
             \x20     pikeAdd(ops, word, lst, seen, ops[pc * 4 + 1], pos);\n\
             \x20   }} else if (op == 1) {{\n\
             \x20     pikeAdd(ops, word, lst, seen, ops[pc * 4 + 1], pos);\n\
             \x20     pikeAdd(ops, word, lst, seen, ops[pc * 4 + 2], pos);\n\
             \x20   }} else if (op == 4) {{\n\
             \x20     if (pikeAssert(ops[pc * 4 + 1], pos, word)) pikeAdd(ops, word, lst, seen, pc + 1, pos);\n\
             \x20   }} else {{\n\
             \x20     lst.add(pc);\n\
             \x20   }}\n\
             \x20 }}\n\n\
             \x20 int pikeMatch(int[] ops, int[] rng, int[] word) {{\n\
             \x20   int n = {len};\n\
             \x20   int ninst = ops.length / 4;\n\
             \x20   int matched = -1;\n\
             \x20   java.util.List<Integer> clist = new java.util.ArrayList<>();\n\
             \x20   pikeAdd(ops, word, clist, new boolean[ninst], 0, cursor);\n\
             \x20   int pos = cursor;\n\
             \x20   while (true) {{\n\
             \x20     java.util.List<Integer> nlist = new java.util.ArrayList<>();\n\
             \x20     boolean[] nseen = new boolean[ninst];\n\
             \x20     for (int pc : clist) {{\n\
             \x20       int op = ops[pc * 4];\n\
             \x20       if (op == 0) {{\n\
             \x20         if (pos < n) {{\n\
             \x20           int v = {inp}.charAt(pos);\n\
             \x20           int rs = ops[pc * 4 + 1];\n\
             \x20           int rc = ops[pc * 4 + 2];\n\
             \x20           for (int k = 0; k < rc; k++) {{\n\
             \x20             if (rng[(rs + k) * 2] <= v && v <= rng[(rs + k) * 2 + 1]) {{\n\
             \x20               pikeAdd(ops, word, nlist, nseen, pc + 1, pos + 1);\n\
             \x20               break;\n\
             \x20             }}\n\
             \x20           }}\n\
             \x20         }}\n\
             \x20       }} else if (op == 3) {{\n\
             \x20         matched = pos;\n\
             \x20         break;\n\
             \x20       }}\n\
             \x20     }}\n\
             \x20     if (pos >= n) break;\n\
             \x20     pos++;\n\
             \x20     clist = nlist;\n\
             \x20   }}\n\
             \x20   return matched;\n\
             \x20 }}\n\n",
            len = self.input_len(),
            inp = inp,
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
                             supported by the Java backend"
                                .into(),
                        );
                    }
                    let my_sid = *sid;
                    *sid += 1;
                    if self.stage_dfas[my_sid].mode_c.is_some() {
                        return Err(
                            "a Mode C (`/@Fsm/`) stage as a `|` alternative selector is not yet \
                             supported by the Java backend"
                                .into(),
                        );
                    }
                    let MatchElement::Stage(sel) = &m.elements[fs] else {
                        unreachable!("first_stage indexes a Stage element")
                    };
                    self.emit_stage_match(out, my_sid, sel, "    ");
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
                self.emit_stage_match(out, my_sid, stage, ind);
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
        let sub = match self.alphabet {
            Alphabet::Token => format!("_slice({}, cursor, {}.length)", input, input),
            _ => format!("{}.substring(cursor)", input),
        };
        writeln!(out, "{}{} {} = new {}({});", ind, inner, iv, inner, sub).ok();
        writeln!(out, "{}if (!{}.accepted) {{", ind, iv).ok();
        self.emit_failure(out, m, ind2);
        writeln!(out, "{}}}", ind).ok();
        let end = format!("cursor + {}.cursor", iv);
        writeln!(out, "{}matched = {};", ind, self.matched_slice(&end)).ok();
        if let Some(lbl) = &stage.label {
            if !state_label.is_empty() {
                writeln!(out, "{}{} = matched;", ind, cap_field(state_label, lbl)).ok();
                writeln!(out, "{}{} = {};", ind, cap_inst_field(state_label, lbl), iv).ok();
            }
        }
        writeln!(out, "{}cursor = cursor + {}.cursor;", ind, iv).ok();
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
        let read = self.element_read();
        writeln!(out, "  int matchStage{}() {{", sid).ok();
        self.emit_dfa_decls(out, sid, "    ");
        writeln!(
            out,
            "    int entry = cursor;\n\
             \x20   int st = {start};\n\
             \x20   int pos = entry;\n\
             \x20   int n = {len};\n\
             \x20   int last = a{sid}[st] ? pos : -1;\n\
             \x20   cursor = pos;",
            start = self.stage_dfas[sid].start,
            len = self.input_len(),
            sid = sid
        )
        .ok();
        out.push_str(&self.embed_body(stage, EmbeddingOp::Start, "    ")?);
        writeln!(out, "    boolean prev = a{}[st];", sid).ok();
        writeln!(
            out,
            "    while (pos < n) {{\n\
             \x20     int v = {read};\n\
             \x20     int nxt = -1;\n\
             \x20     for (int[] tr : t{sid}[st]) {{ if (tr[0] <= v && v <= tr[1]) {{ nxt = tr[2]; break; }} }}\n\
             \x20     if (nxt < 0) break;\n\
             \x20     st = nxt; pos++;\n\
             \x20     cursor = pos;",
            read = read,
            sid = sid
        )
        .ok();
        out.push_str(&self.embed_body(stage, EmbeddingOp::EveryTransition, "      ")?);
        writeln!(out, "      boolean now = a{}[st];", sid).ok();
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
        if dfa.requires_start {
            writeln!(out, "{}if (cursor != 0) _r{} = -1;", ind, sid).ok();
        }
        if dfa.requires_end {
            writeln!(
                out,
                "{}if (_r{} != {}) _r{} = -1;",
                ind,
                sid,
                self.input_len(),
                sid
            )
            .ok();
        }
        if let Some(wb) = dfa.start_boundary {
            let op = match wb {
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
        if let Some(wb) = dfa.end_boundary {
            let op = match wb {
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
                "{}throw new RuntimeException(\"transition to undeclared state ${}\");",
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
                        "{}throw new RuntimeException(\"transition to undeclared stage ${}.{}\");",
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
                "statement {:?} not supported in @@fsm action blocks by the Java backend",
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
                .map(|p| format!("{} {}", Self::java_type(&p.param_type), p.name))
                .collect();
            let ret = match &act.return_type {
                Some(t) => Self::java_type(t),
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

    /// Emit the per-stage DFA as two parallel local arrays at indent `ind`:
    /// `int[][][] t<sid>` (transitions) and `boolean[] a<sid>` (accept).
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
        writeln!(out, "{}int[][][] t{} = {{{}}};", ind, sid, trans.join(", ")).ok();
        writeln!(
            out,
            "{}boolean[] a{} = {{{}}};",
            ind,
            sid,
            accept.join(", ")
        )
        .ok();
    }

    /// Emit `int _r<sid> = <matcher>;` at indent `ind`, choosing the matcher
    /// for the stage: the Pike VM (`pikeMatch`) for a lazy stage (emitting its
    /// `ops`/`rng` arrays first), the specialized `matchStage<sid>` when the
    /// stage carries embedding actions, else the shared `dfaMatch` (emitting
    /// its `t`/`a` arrays first).
    fn emit_stage_match(&self, out: &mut String, sid: usize, stage: &StageAst, ind: &str) {
        let dfa = &self.stage_dfas[sid];
        if let Some(prog) = &dfa.program {
            let (ops, rng) = fsm_regex::pike::encode(prog);
            let word = fsm_regex::pike::program_word_table(prog, self.alphabet);
            writeln!(out, "{}int[] ops{} = {{{}}};", ind, sid, int_list(&ops)).ok();
            writeln!(out, "{}int[] rng{} = {{{}}};", ind, sid, int_list(&rng)).ok();
            writeln!(out, "{}int[] word{} = {{{}}};", ind, sid, int_list(&word)).ok();
            writeln!(
                out,
                "{}int _r{} = pikeMatch(ops{}, rng{}, word{});",
                ind, sid, sid, sid, sid
            )
            .ok();
        } else if stage.embedding_actions.is_empty() {
            self.emit_dfa_decls(out, sid, ind);
            writeln!(
                out,
                "{}int _r{} = dfaMatch(t{}, a{}, {});",
                ind, sid, sid, sid, dfa.start
            )
            .ok();
        } else {
            writeln!(out, "{}int _r{} = matchStage{}();", ind, sid, sid).ok();
        }
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
                            return format!("{}.{}", cap_inst_field(state, label), field);
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
            "to_int" => format!("Integer.parseInt({})", a.join(", ")),
            "to_str" => format!("String.valueOf({})", a.join(", ")),
            "len" => match self.alphabet {
                Alphabet::Token => format!("({}).length", a.join(", ")),
                _ => format!("({}).length()", a.join(", ")),
            },
            _ => format!("{}({})", func, a.join(", ")),
        }
    }
}

/// Comma-joined `i64` list literal (the Pike `ops`/`rng` arrays).
fn int_list(xs: &[i64]) -> String {
    xs.iter()
        .map(|x| x.to_string())
        .collect::<Vec<_>>()
        .join(", ")
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

fn java_default(ty: &Type, raw: &str) -> String {
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

    /// Compile (`javac`) + run (`java Main`) a program: the generated
    /// classes plus a public `Main` driver, all in one `Main.java`. Returns
    /// stdout lines. `None` if the JDK is unavailable.
    fn java_run(code: &str, body: &str, tag: &str) -> Option<Vec<String>> {
        let prog = format!(
            "{}\npublic class Main {{\n  public static void main(String[] args) {{\n{}\n  }}\n}}\n",
            code, body
        );
        let dir = std::env::temp_dir().join(format!("framec_java_{}", tag));
        std::fs::create_dir_all(&dir).ok()?;
        let src = dir.join("Main.java");
        std::fs::write(&src, prog).ok()?;
        let compile = match Command::new("javac").arg("-d").arg(&dir).arg(&src).output() {
            Ok(o) => o,
            Err(_) => return None,
        };
        assert!(
            compile.status.success(),
            "javac failed for {:?}:\n{}",
            tag,
            String::from_utf8_lossy(&compile.stderr)
        );
        let out = Command::new("java")
            .arg("-cp")
            .arg(&dir)
            .arg("Main")
            .output()
            .expect("java");
        assert!(
            out.status.success(),
            "java failed:\n{}",
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

    /// `ctor` is the constructor call without `new`, e.g. `M("a")`.
    fn run(src: &str, ctor: &str, tag: &str) -> Option<(String, String)> {
        let code = gen(src);
        let body = format!(
            "    var m = new {ctor};\n    System.out.println(m.accepted);\n    System.out.println(m.return_value);"
        );
        let lines = java_run(&code, &body, tag)?;
        Some((lines[0].clone(), lines[1].clone()))
    }

    #[test]
    fn java_smoke_bool() {
        let src = "@@fsm M(text: bytes) : bool = false { /a/ true }";
        let Some((acc, ret)) = run(src, "M(\"a\")", "smoke_a") else {
            return;
        };
        assert_eq!((acc.as_str(), ret.as_str()), ("true", "true"));
        assert_eq!(run(src, "M(\"b\")", "smoke_b").unwrap().0, "false");
    }

    #[test]
    fn java_matched_to_int() {
        let src = "@@fsm M(text: bytes) : int = 0 { /[0-9]+/ to_int(@@:matched) }";
        let Some((acc, ret)) = run(src, "M(\"123\")", "tok_a") else {
            return;
        };
        assert_eq!((acc.as_str(), ret.as_str()), ("true", "123"));
        assert_eq!(run(src, "M(\"x\")", "tok_b").unwrap().0, "false");
    }

    #[test]
    fn java_len_self_input() {
        let src = "@@fsm M(text: bytes) : int = 0 { /[0-9]+/ len(self.text) }";
        let Some((_, ret)) = run(src, "M(\"123\")", "len_a") else {
            return;
        };
        assert_eq!(ret, "3");
    }

    #[test]
    fn java_stage_capture() {
        let src = "@@fsm M(text: bytes) : int = 0 { $s: .n/[0-9]+/ to_int($s.n) }";
        let Some((acc, ret)) = run(src, "M(\"42\")", "cap_a") else {
            return;
        };
        assert_eq!((acc.as_str(), ret.as_str()), ("true", "42"));
    }

    #[test]
    fn java_action_block() {
        let src = "@@fsm M(text: bytes) : int = 0 { \
                   /[0-9]/ { self.count = self.count + 1 } self.count \
                   domain: count: int = 0 }";
        let Some((_, ret)) = run(src, "M(\"5\")", "act_a") else {
            return;
        };
        assert_eq!(ret, "1");
    }

    #[test]
    fn java_declared_action() {
        let src = "@@fsm M(text: bytes) : int = 0 { \
                   /[0-9]+/ parse_int(@@:matched) \
                   actions: parse_int(s: bytes): int { to_int(s) } }";
        let Some((_, ret)) = run(src, "M(\"42\")", "decl_a") else {
            return;
        };
        assert_eq!(ret, "42");
    }

    #[test]
    fn java_transitions_and_capture() {
        let src = "@@fsm M(text: bytes) : int = 0 { \
                   $0: /[a-z]/ -> $digits : -> $error \
                   $digits: .n/[0-9]+/ to_int($digits.n) \
                   $error: -1 }";
        let Some((acc, ret)) = run(src, "M(\"x42\")", "tr_a") else {
            return;
        };
        assert_eq!((acc.as_str(), ret.as_str()), ("true", "42"));
        assert_eq!(run(src, "M(\"X\")", "tr_b").unwrap().1, "-1");
    }

    #[test]
    fn java_conditional_target() {
        let src = "@@fsm M(text: bytes, mode: int) : int = 0 { \
                   /[01]/ -> ( $zero when self.mode == 0, $one when self.mode == 1 ) : -> $error \
                   $zero: 0 \
                   $one: 1 \
                   $error: -1 }";
        let Some(z) = run(src, "M(\"0\", 0)", "cond_a") else {
            return;
        };
        assert_eq!(z.1, "0");
        assert_eq!(run(src, "M(\"1\", 1)", "cond_b").unwrap().1, "1");
        assert_eq!(run(src, "M(\"0\", 2)", "cond_c").unwrap().1, "-1");
    }

    #[test]
    fn java_multi_match() {
        let code = gen("@@fsm M(text: bytes) : int = 0 { /[0-9]/ -> $num | 99 $num: 1 }");
        let body = "    for (String s : new String[]{\"5\", \"a\"}) { System.out.println(new M(s).return_value); }";
        let Some(lines) = java_run(&code, body, "mm") else {
            return;
        };
        assert_eq!(lines, vec!["1", "99"]);
    }

    #[test]
    fn java_embed_every_transition() {
        let code = gen(
            "@@fsm M(text: bytes) : int = 0 { /[0-9]+/ ${ tally() } self.count \
             actions: tally() { self.count = self.count + 1 } domain: count: int = 0 }",
        );
        let body = "    System.out.println(new M(\"123\").return_value);";
        let Some(lines) = java_run(&code, body, "emb") else {
            return;
        };
        assert_eq!(lines[0], "3");
    }

    /// FSM-TEST-603 — `%{...}` fires when the DFA leaves its last accepting
    /// state, capturing the end of the matched region.
    #[test]
    fn java_embed_leave_final() {
        let src = "@@fsm M(text: bytes) : int = 0 { \
                   /[0-9]+/ %{ self.end_pos = @@:cursor } self.end_pos \
                   domain: end_pos: int = 0 }";
        // "42x": two digits matched, then `x` fails to extend — leave fires once
        // with cursor at the end of the match (2), not the failing element.
        let Some((_, ret)) = run(src, "M(\"42x\")", "leave_a") else {
            return;
        };
        assert_eq!(ret, "2");
        // "abx": no accepting state ever entered (`last < 0`) — leave does not
        // fire, end_pos keeps its initial value.
        assert_eq!(run(src, "M(\"abx\")", "leave_b").unwrap().1, "0");
    }

    #[test]
    fn java_token_alphabet() {
        let code = gen("@@fsm M(toks: token) : bool = false { /IDENT LPAREN RPAREN/ true }");
        let body = "    for (String[] t : new String[][]{{\"IDENT\",\"LPAREN\",\"RPAREN\"},{\"IDENT\",\"RPAREN\"},{\"IDENT\",\"WAT\"}}) { System.out.println(new M(t).accepted); }";
        let Some(lines) = java_run(&code, body, "tok") else {
            return;
        };
        assert_eq!(lines, vec!["true", "false", "false"]);
    }

    #[test]
    fn java_mode_c_callout() {
        let inner = gen("@@fsm Digits(text: bytes) : int = 0 { /[0-9]+/ to_int(@@:matched) }");
        let outer = gen("@@fsm Outer(text: bytes) : int = 0 { $s: .d/@Digits/ $s.d.return_value }");
        let code = format!("{}\n{}", inner, outer);
        let body = "    for (String s : new String[]{\"42\", \"x\"}) { var m = new Outer(s); System.out.println(m.accepted + \" \" + m.return_value); }";
        let Some(lines) = java_run(&code, body, "modec") else {
            return;
        };
        assert_eq!(lines, vec!["true 42", "false 0"]);
    }

    /// Lazy quantifiers (§11.1) via the Pike VM: `/.*?,/` matches up to the
    /// FIRST comma (greedy `/.*,/` would take the last), and the mixed
    /// `/a*?b+/` keeps `b+` greedy ("aabbb" → cursor 5, not 3).
    #[test]
    fn java_lazy_quantifier() {
        let src = "@@fsm M(text: bytes) : bytes = \"\" { /.*?,/ @@:matched }";
        let Some((_, ret)) = run(src, "M(\"ab,cd,ef\")", "lz_a") else {
            return;
        };
        assert_eq!(ret, "ab,");
        let mixed = "@@fsm M(text: bytes) : int = 0 { /a*?b+/ @@:cursor }";
        assert_eq!(run(mixed, "M(\"aabbb\")", "lz_b").unwrap().1, "5");
    }

    #[test]
    fn java_anchors() {
        let start = gen("@@fsm M(text: bytes) : bool = false { /^foo/ true }");
        let d1 = "    for (String s : new String[]{\"foo\", \"xfoo\"}) { System.out.println(new M(s).accepted); }";
        let Some(l1) = java_run(&start, d1, "anc_s") else {
            return;
        };
        assert_eq!(l1, vec!["true", "false"]);
        let end = gen("@@fsm M(text: bytes) : bool = false { /[0-9]+$/ true }");
        let d2 = "    for (String s : new String[]{\"123\", \"123x\"}) { System.out.println(new M(s).accepted); }";
        let Some(l2) = java_run(&end, d2, "anc_e") else {
            return;
        };
        assert_eq!(l2, vec!["true", "false"]);
    }

    #[test]
    fn java_word_boundary() {
        let code = gen("@@fsm M(text: bytes) : bool = false { /\\bcat\\b/ true }");
        let d = "    for (String s : new String[]{\"cat\", \"cats\"}) { System.out.println(new M(s).accepted); }";
        let Some(lines) = java_run(&code, d, "wb") else {
            return;
        };
        assert_eq!(lines, vec!["true", "false"]);
    }

    #[test]
    fn java_interior_anchor_runs_on_pike_vm() {
        let src = "@@fsm M(text: bytes) : bool = false { /a$b/ true }";
        let decl = parse_fsm_block(src.as_bytes()).expect("parses");
        generate(&decl).expect("interior anchor compiles to a Pike program");
        let Some((acc, _)) = run(src, "M(\"ab\")", "ia_mid") else {
            return;
        };
        assert_eq!(acc, "false");
    }
}
