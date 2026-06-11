//! TypeScript backend for `@@fsm` (RFC-0042, Phase 8).
//!
//! Generates a self-contained TypeScript `class` from a validated
//! `FsmDeclAst`. The recognition model is identical to the JavaScript backend
//! ([`super::fsm_javascript`]) — per-stage minimal DFAs + a per-state
//! dispatch loop over mutable instance state — with static type annotations
//! layered on: declared, typed instance fields; a typed constructor and
//! method signatures; and `FrameDfaState` type aliases for the DFA tables.
//! Frame's abstract types map as `int`/`float` → `number`, `bool` →
//! `boolean`, `str`/`bytes` → `string`; the `token` alphabet's input is a
//! `string[]`. The observable result (§5.1) is the constructed instance's
//! `accepted`, `return_value`, `cursor`, and `reject_position`.
//!
//! # v0.1 scope
//!
//! Full parity with the Python reference backend: single-match and
//! multi-match (`|`) ordered-choice states, stages with `.label` captures,
//! bare-expression returns, action blocks, declared `actions:` methods, all
//! transition forms (static / conditional / stage-ref / failure-only),
//! embedding actions, Mode C sub-fsm call-out, all three alphabets, and
//! boundary anchors. Not yet handled (clear `Unsupported` error): mid-pattern
//! anchors and `\b`/`\B`, a Mode C stage as a `|` selector, and a `|`
//! alternative with elements before its first stage.

use crate::frame_c::compiler::frame_ast::{
    BinaryOp, EmbeddingOp, Expression, FsmDeclAst, FsmStateAst, FsmTransitionTarget, Literal,
    MatchAst, MatchElement, StageAst, Type, UnaryOp,
};
use crate::frame_c::compiler::fsm_regex::{
    self, pike::Program, size_check::DEFAULT_MAX_DFA_STATES, subset::DfaLabel, Alphabet,
    CompileError, WordBoundary,
};
use std::fmt::Write;

/// Generate TypeScript source implementing `decl`, or a reason it is outside
/// the v0.1 TypeScript cut.
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
    /// program matched by the VM (`_pikeMatch`) instead of the DFA, for
    /// leftmost-first match-end semantics.
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
                                // A lazy quantifier matches via the Pike VM,
                                // which has no per-element scan for embedding
                                // actions to hook into (§3.5.5/§11.1). Reject
                                // the combination rather than silently giving
                                // greedy semantics.
                                if dfa.program.is_some() && !stage.embedding_actions.is_empty() {
                                    self.token_ids = token_ids;
                                    return Err("a lazy quantifier in a stage with embedding \
                                                actions is not yet supported"
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
                 TypeScript backend",
                regex
            )),
        }
    }

    /// The TypeScript type for a Frame type string.
    fn ts_type(t: &Type) -> String {
        let s = match t {
            Type::Custom(s) => s.as_str(),
            _ => "any",
        };
        match s {
            "int" | "float" => "number".to_string(),
            "bool" => "boolean".to_string(),
            "str" | "string" | "String" | "bytes" => "string".to_string(),
            other => other.to_string(),
        }
    }

    /// The TS type of the input parameter: a `string[]` for tokens, else a
    /// `string`.
    fn input_type(&self) -> &'static str {
        match self.alphabet {
            Alphabet::Token => "string[]",
            _ => "string",
        }
    }

    /// The TS type of `matched` / captures: `string[]` for tokens, else
    /// `string`.
    fn matched_type(&self) -> &'static str {
        match self.alphabet {
            Alphabet::Token => "string[]",
            _ => "string",
        }
    }

    fn matched_empty(&self) -> &'static str {
        match self.alphabet {
            Alphabet::Token => "[]",
            _ => "\"\"",
        }
    }

    /// The per-element read as a number: code point for byte/char, token id
    /// for the token alphabet.
    fn element_read(&self) -> String {
        let inp = &self.decl.params[0].name;
        match self.alphabet {
            Alphabet::Token => format!("this._tokId(this.{}[pos])", inp),
            _ => format!("this.{}.charCodeAt(pos)", inp),
        }
    }

    fn emit(&self) -> Result<String, String> {
        let mut out = String::new();
        out.push_str("// Generated by framec — RFC-0042 @@fsm (TypeScript backend).\n\n");
        // The DFA-table type is inlined (not a module-level alias) so that
        // several generated fsms can be concatenated — e.g. a Mode C outer
        // fsm and its inner — without a duplicate type declaration.
        writeln!(out, "class {} {{", self.decl.name).ok();
        self.emit_field_decls(&mut out);
        self.emit_pike_consts(&mut out);
        self.emit_ctor(&mut out);
        self.emit_tok_id(&mut out);
        if self.uses_word_boundary() {
            self.emit_word_boundary_helper(&mut out);
        }
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

    /// Declared, typed instance fields (TypeScript requires field
    /// declarations; duplicates are elided so a domain field re-declaring a
    /// parameter is declared once).
    fn emit_field_decls(&self, out: &mut String) {
        out.push_str("  accepted: boolean;\n");
        out.push_str("  reject_position: number;\n");
        out.push_str("  cursor: number;\n");
        writeln!(
            out,
            "  return_value: {};",
            Self::ts_type(&self.decl.return_type)
        )
        .ok();
        let mut seen = std::collections::HashSet::new();
        for (i, p) in self.decl.params.iter().enumerate() {
            seen.insert(p.name.clone());
            let ty = if i == 0 {
                self.input_type().to_string()
            } else {
                Self::ts_type(&p.param_type)
            };
            writeln!(out, "  {}: {};", p.name, ty).ok();
        }
        if let Some(domain) = &self.decl.domain {
            for v in &domain.vars {
                if !seen.insert(v.name.clone()) {
                    continue;
                }
                writeln!(out, "  {}: {};", v.name, Self::ts_type(&v.var_type)).ok();
            }
        }
        writeln!(out, "  matched: {};", self.matched_type()).ok();
        out.push_str("  enter: number;\n");
        for f in self.capture_fields() {
            writeln!(out, "  {}: {};", f, self.matched_type()).ok();
        }
        for (f, inner) in self.mode_c_inst_fields() {
            writeln!(out, "  {}: {} | null;", f, inner).ok();
        }
    }

    fn emit_ctor(&self, out: &mut String) {
        let sig: Vec<String> = self
            .decl
            .params
            .iter()
            .enumerate()
            .map(|(i, p)| {
                let ty = if i == 0 {
                    self.input_type().to_string()
                } else {
                    Self::ts_type(&p.param_type)
                };
                format!("{}: {}", p.name, ty)
            })
            .collect();
        writeln!(out, "  constructor({}) {{", sig.join(", ")).ok();
        out.push_str("    this.accepted = false;\n");
        out.push_str("    this.reject_position = 0;\n");
        out.push_str("    this.cursor = 0;\n");
        writeln!(
            out,
            "    this.return_value = {};",
            js_default(&self.decl.default_expr)
        )
        .ok();
        let input = self.decl.params.first().map(|p| p.name.clone());
        for p in &self.decl.params {
            writeln!(out, "    this.{} = {};", p.name, p.name).ok();
        }
        if let Some(domain) = &self.decl.domain {
            for v in &domain.vars {
                if input.as_ref() == Some(&v.name) {
                    continue;
                }
                writeln!(out, "    this.{} = {};", v.name, self.expr(&v.default)).ok();
            }
        }
        writeln!(out, "    this.matched = {};", self.matched_empty()).ok();
        out.push_str("    this.enter = 0;\n");
        for f in self.capture_fields() {
            writeln!(out, "    this.{} = {};", f, self.matched_empty()).ok();
        }
        for (f, _) in self.mode_c_inst_fields() {
            writeln!(out, "    this.{} = null;", f).ok();
        }
        out.push_str("    this.run();\n");
        out.push_str("    if (this.accepted) this.reject_position = 0;\n");
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
        let items: Vec<String> = entries
            .iter()
            .map(|(name, id)| format!("{:?}: {}", name, id))
            .collect();
        writeln!(out, "  _tokId(t: string): number {{").ok();
        writeln!(
            out,
            "    const T: {{ [k: string]: number }} = {{{}}};",
            items.join(", ")
        )
        .ok();
        out.push_str("    return (t in T) ? T[t] : -1;\n");
        out.push_str("  }\n\n");
    }

    /// Does any stage carry a `\b`/`\B` boundary requiring the `_iswordat`
    /// helper?
    fn uses_word_boundary(&self) -> bool {
        self.stage_dfas
            .iter()
            .any(|d| d.start_boundary.is_some() || d.end_boundary.is_some())
    }

    /// `_iswordat(p)` — is the byte at input position `p` a word character
    /// (`[0-9A-Za-z_]`)? Out-of-range positions are non-word, so a boundary at
    /// the input edge resolves correctly. Used by the `\b`/`\B` guards. The
    /// boundary alphabet is bytes only, so the input is a `string` read via
    /// `charCodeAt`.
    fn emit_word_boundary_helper(&self, out: &mut String) {
        let inp = &self.decl.params[0].name;
        writeln!(
            out,
            "  _iswordat(p: number): boolean {{\n\
             \x20   if (p < 0 || p >= this.{inp}.length) return false;\n\
             \x20   const b = this.{inp}.charCodeAt(p);\n\
             \x20   return (48 <= b && b <= 57) || (65 <= b && b <= 90) || (97 <= b && b <= 122) || b === 95;\n\
             \x20 }}\n",
            inp = inp,
        )
        .ok();
    }

    /// Emit the lazy stages' Pike programs as class fields (`_OPS_<i>` /
    /// `_RNG_<i>`, two flat `number[]`s per stage; see `pike::encode`). A lazy
    /// stage carries a Pike program instead of a DFA table.
    fn emit_pike_consts(&self, out: &mut String) {
        for (i, dfa) in self.stage_dfas.iter().enumerate() {
            if let Some(prog) = &dfa.program {
                let (ops, rng) = fsm_regex::pike::encode(prog);
                writeln!(out, "  _OPS_{}: number[] = [{}];", i, int_list(&ops)).ok();
                writeln!(out, "  _RNG_{}: number[] = [{}];", i, int_list(&rng)).ok();
            }
        }
    }

    /// Does any stage match via the Pike VM (a lazy quantifier, §11.1)?
    fn uses_pike(&self) -> bool {
        self.stage_dfas.iter().any(|d| d.program.is_some())
    }

    /// Pike VM (priority NFA simulation) for lazy-quantifier stages, over the
    /// flat `ops`/`rng` arrays (`pike::encode`). Returns the end position of the
    /// highest-priority (leftmost-first) match from the cursor, or -1. `ops` is
    /// 4 ints per instruction `[op, a, b, _]`: 0 Char (a = pair index, b = pair
    /// count), 1 Split (a/b targets, a higher), 2 Jmp, 3 Match.
    fn emit_pike_matcher(&self, out: &mut String) {
        let inp = &self.decl.params[0].name;
        writeln!(
            out,
            "  _pikeAdd(ops: number[], lst: number[], seen: boolean[], pc: number): void {{\n\
             \x20   if (seen[pc]) return;\n\
             \x20   seen[pc] = true;\n\
             \x20   const op = ops[pc * 4];\n\
             \x20   if (op === 2) {{\n\
             \x20     this._pikeAdd(ops, lst, seen, ops[pc * 4 + 1]);\n\
             \x20   }} else if (op === 1) {{\n\
             \x20     this._pikeAdd(ops, lst, seen, ops[pc * 4 + 1]);\n\
             \x20     this._pikeAdd(ops, lst, seen, ops[pc * 4 + 2]);\n\
             \x20   }} else {{\n\
             \x20     lst.push(pc);\n\
             \x20   }}\n\
             \x20 }}\n\n\
             \x20 _pikeMatch(ops: number[], rng: number[]): number {{\n\
             \x20   const n = this.{inp}.length;\n\
             \x20   const ninst = ops.length / 4;\n\
             \x20   let matched = -1;\n\
             \x20   let clist: number[] = [];\n\
             \x20   this._pikeAdd(ops, clist, new Array(ninst).fill(false), 0);\n\
             \x20   let pos = this.cursor;\n\
             \x20   while (true) {{\n\
             \x20     const nlist: number[] = [];\n\
             \x20     const nseen: boolean[] = new Array(ninst).fill(false);\n\
             \x20     for (const pc of clist) {{\n\
             \x20       const op = ops[pc * 4];\n\
             \x20       if (op === 0) {{\n\
             \x20         if (pos < n) {{\n\
             \x20           const v = this.{inp}.charCodeAt(pos);\n\
             \x20           const rs = ops[pc * 4 + 1];\n\
             \x20           const rc = ops[pc * 4 + 2];\n\
             \x20           for (let k = 0; k < rc; k++) {{\n\
             \x20             if (rng[(rs + k) * 2] <= v && v <= rng[(rs + k) * 2 + 1]) {{\n\
             \x20               this._pikeAdd(ops, nlist, nseen, pc + 1);\n\
             \x20               break;\n\
             \x20             }}\n\
             \x20           }}\n\
             \x20         }}\n\
             \x20       }} else if (op === 3) {{\n\
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
            inp = inp,
        )
        .ok();
    }

    fn emit_dfa_matcher(&self, out: &mut String) {
        let input = &self.decl.params[0].name;
        let read = self.element_read();
        writeln!(
            out,
            "  _dfaMatch(states: [[number, number, number][], boolean][], start: number): number {{\n\
             \x20   let st = start;\n\
             \x20   let pos = this.cursor;\n\
             \x20   const n = this.{input}.length;\n\
             \x20   let last = states[st][1] ? pos : -1;\n\
             \x20   while (pos < n) {{\n\
             \x20     const v = {read};\n\
             \x20     let nxt: number | null = null;\n\
             \x20     for (const [lo, hi, tgt] of states[st][0]) {{ if (lo <= v && v <= hi) {{ nxt = tgt; break; }} }}\n\
             \x20     if (nxt === null) break;\n\
             \x20     st = nxt; pos++;\n\
             \x20     if (states[st][1]) last = pos;\n\
             \x20   }}\n\
             \x20   return last;\n\
             \x20 }}\n\n",
            input = input,
            read = read
        )
        .ok();
    }

    fn emit_run(&self, out: &mut String) {
        out.push_str("  run(): void {\n    let state = 0;\n");
        out.push_str("    while (state >= 0) {\n");
        out.push_str("      const _enter = this.enter;\n      this.enter = 0;\n");
        out.push_str("      switch (state) {\n");
        for i in 0..self.decl.states.len() {
            writeln!(
                out,
                "        case {}: state = this.state_{}(_enter); break;",
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
                    writeln!(
                        out,
                        "  state_{}(_enter: number): number {{ return -1; }}\n",
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
        writeln!(out, "  state_{}(_enter: number): number {{", index).ok();
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
        let input = &self.decl.params[0].name;
        writeln!(out, "  state_{}(_enter: number): number {{", index).ok();
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
                             supported by the TypeScript backend"
                                .into(),
                        );
                    }
                    let my_sid = *sid;
                    *sid += 1;
                    if self.stage_dfas[my_sid].mode_c.is_some() {
                        return Err(
                            "a Mode C (`/@Fsm/`) stage as a `|` alternative selector is not yet \
                             supported by the TypeScript backend"
                                .into(),
                        );
                    }
                    let MatchElement::Stage(sel) = &m.elements[fs] else {
                        unreachable!("first_stage indexes a Stage element")
                    };
                    if self.stage_dfas[my_sid].program.is_none() {
                        self.emit_dfa_const(out, my_sid, "    ");
                    }
                    writeln!(
                        out,
                        "    let _r{} = {};",
                        my_sid,
                        stage_call(self, sel, my_sid)
                    )
                    .ok();
                    self.emit_anchor_guards(out, my_sid, "    ");
                    writeln!(out, "    if (_r{} >= 0) {{", my_sid).ok();
                    writeln!(
                        out,
                        "      this.matched = this.{}.slice(this.cursor, _r{});",
                        input, my_sid
                    )
                    .ok();
                    if let Some(lbl) = &sel.label {
                        if !state_label.is_empty() {
                            writeln!(
                                out,
                                "      this.{} = this.matched;",
                                cap_field(&state_label, lbl)
                            )
                            .ok();
                        }
                    }
                    writeln!(out, "      this.cursor = _r{};", my_sid).ok();
                    out.push_str("      this.accepted = true;\n");
                    for el in &m.elements[fs + 1..] {
                        self.emit_element(out, el, m, &state_label, "      ", sid)?;
                    }
                    self.emit_success(out, m, "      ");
                    out.push_str("    }\n");
                }
                None => {
                    out.push_str("    this.accepted = true;\n");
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
            out.push_str("    this.accepted = false;\n");
            out.push_str("    this.reject_position = this.cursor;\n");
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
        let input = &self.decl.params[0].name;
        let ind2 = format!("{}  ", ind);
        match el {
            MatchElement::Stage(stage) => {
                let my_sid = *sid;
                *sid += 1;
                if let Some(inner) = self.stage_dfas[my_sid].mode_c.clone() {
                    self.emit_mode_c(out, &inner, stage, m, state_label, my_sid, ind, &ind2);
                    return Ok(());
                }
                if self.stage_dfas[my_sid].program.is_none() && stage.embedding_actions.is_empty() {
                    self.emit_dfa_const(out, my_sid, ind);
                }
                writeln!(
                    out,
                    "{}let _r{} = {};",
                    ind,
                    my_sid,
                    stage_call(self, stage, my_sid)
                )
                .ok();
                self.emit_anchor_guards(out, my_sid, ind);
                writeln!(out, "{}if (_r{} < 0) {{", ind, my_sid).ok();
                self.emit_failure(out, m, &ind2);
                writeln!(out, "{}}}", ind).ok();
                writeln!(
                    out,
                    "{}this.matched = this.{}.slice(this.cursor, _r{});",
                    ind, input, my_sid
                )
                .ok();
                if let Some(lbl) = &stage.label {
                    if !state_label.is_empty() {
                        writeln!(
                            out,
                            "{}this.{} = this.matched;",
                            ind,
                            cap_field(state_label, lbl)
                        )
                        .ok();
                    }
                }
                writeln!(out, "{}this.cursor = _r{};", ind, my_sid).ok();
                writeln!(out, "{}this.accepted = true;", ind).ok();
            }
            MatchElement::BareExpression { expr, .. } => {
                writeln!(out, "{}this.return_value = {};", ind, self.expr(expr)).ok();
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
        let iv = format!("_inner{}", my_sid);
        writeln!(
            out,
            "{}const {} = new {}(this.{}.slice(this.cursor));",
            ind, iv, inner, input
        )
        .ok();
        writeln!(out, "{}if (!{}.accepted) {{", ind, iv).ok();
        self.emit_failure(out, m, ind2);
        writeln!(out, "{}}}", ind).ok();
        writeln!(
            out,
            "{}this.matched = this.{}.slice(this.cursor, this.cursor + {}.cursor);",
            ind, input, iv
        )
        .ok();
        if let Some(lbl) = &stage.label {
            if !state_label.is_empty() {
                writeln!(
                    out,
                    "{}this.{} = this.matched;",
                    ind,
                    cap_field(state_label, lbl)
                )
                .ok();
                writeln!(
                    out,
                    "{}this.{} = {};",
                    ind,
                    cap_inst_field(state_label, lbl),
                    iv
                )
                .ok();
            }
        }
        writeln!(out, "{}this.cursor = this.cursor + {}.cursor;", ind, iv).ok();
        writeln!(out, "{}this.accepted = true;", ind).ok();
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
        writeln!(out, "  _matchStage_{}(): number {{", sid).ok();
        self.emit_dfa_const(out, sid, "    ");
        writeln!(
            out,
            "    const _entry = this.cursor;\n\
             \x20   let st = {start};\n\
             \x20   let pos = _entry;\n\
             \x20   const n = this.{input}.length;\n\
             \x20   let last = DFA_{sid}[st][1] ? pos : -1;\n\
             \x20   this.cursor = pos;",
            start = self.stage_dfas[sid].start,
            input = input,
            sid = sid
        )
        .ok();
        out.push_str(&self.embed_body(stage, EmbeddingOp::Start, "    ")?);
        writeln!(out, "    let prev = DFA_{}[st][1];", sid).ok();
        writeln!(
            out,
            "    while (pos < n) {{\n\
             \x20     const v = {read};\n\
             \x20     let nxt: number | null = null;\n\
             \x20     for (const [lo, hi, tgt] of DFA_{sid}[st][0]) {{ if (lo <= v && v <= hi) {{ nxt = tgt; break; }} }}\n\
             \x20     if (nxt === null) break;\n\
             \x20     st = nxt; pos++;\n\
             \x20     this.cursor = pos;",
            read = read,
            sid = sid
        )
        .ok();
        out.push_str(&self.embed_body(stage, EmbeddingOp::EveryTransition, "      ")?);
        writeln!(out, "      const _now = DFA_{}[st][1];", sid).ok();
        let accept = self.embed_body(stage, EmbeddingOp::Accept, "        ")?;
        if !accept.is_empty() {
            out.push_str("      if (_now) {\n");
            out.push_str(&accept);
            out.push_str("      }\n");
        }
        out.push_str("      if (_now) last = pos;\n      prev = _now;\n");
        out.push_str("    }\n");
        // `%{}` — left the last accepting state: a post-scan event firing once
        // when the longest match stops extending (failing element or EOF), with
        // `@@:cursor` at the end of the matched region (`last`), not the failing
        // element (§5.4 / FSM-TEST-603). `last < 0` ⇒ no accepting state was
        // entered, so there is nothing to leave.
        let leave = self.embed_body(stage, EmbeddingOp::LeaveAccept, "      ")?;
        if !leave.is_empty() {
            out.push_str("    if (last >= 0) {\n      this.cursor = last;\n");
            out.push_str(&leave);
            out.push_str("    }\n");
        }
        let eof = self.embed_body(stage, EmbeddingOp::Eof, "      ")?;
        if !eof.is_empty() {
            out.push_str("    if (pos >= n && !prev) {\n");
            out.push_str(&eof);
            out.push_str("    }\n");
        }
        out.push_str("    this.cursor = _entry;\n    return last;\n  }\n\n");
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
            writeln!(out, "{}if (this.cursor !== 0) _r{} = -1;", ind, sid).ok();
        }
        if dfa.requires_end {
            writeln!(
                out,
                "{}if (_r{} !== this.{}.length) _r{} = -1;",
                ind, sid, input, sid
            )
            .ok();
        }
        // `\b`/`\B` word boundaries: a boundary exists at position p iff the
        // word-class of byte[p-1] differs from byte[p]. Required (`\b`) fails
        // when the sides match (`==`); Forbidden (`\B`) fails when they differ
        // (`!=`). The end check is gated on `_r >= 0` so a prior miss stays a
        // miss.
        if let Some(kind) = dfa.start_boundary {
            let op = match kind {
                WordBoundary::Required => "==",
                WordBoundary::Forbidden => "!=",
            };
            writeln!(
                out,
                "{ind}if (this._iswordat(this.cursor - 1) {op} this._iswordat(this.cursor)) _r{sid} = -1;"
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
                "{ind}if (_r{sid} >= 0 && this._iswordat(_r{sid} - 1) {op} this._iswordat(_r{sid})) _r{sid} = -1;"
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
        writeln!(out, "{}this.accepted = false;", ind).ok();
        writeln!(out, "{}this.reject_position = this.cursor;", ind).ok();
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
                "{}throw new Error(\"transition to undeclared state ${}\");",
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
                    writeln!(out, "{}this.enter = {};", ind, entry).ok();
                }
                None => {
                    writeln!(
                        out,
                        "{}throw new Error(\"transition to undeclared stage ${}.{}\");",
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
                "statement {:?} not supported in @@fsm action blocks by the TypeScript backend",
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
                .map(|p| format!("{}: {}", p.name, Self::ts_type(&p.param_type)))
                .collect();
            let ret = match &act.return_type {
                Some(t) => format!(": {}", Self::ts_type(t)),
                None => ": void".to_string(),
            };
            writeln!(out, "  {}({}){} {{", act.name, sig.join(", "), ret).ok();
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

    fn emit_dfa_const(&self, out: &mut String, sid: usize, ind: &str) {
        let dfa = &self.stage_dfas[sid];
        let states: Vec<String> = dfa
            .states
            .iter()
            .map(|(trans, acc)| {
                let ts: Vec<String> = trans
                    .iter()
                    .map(|(lo, hi, tgt)| format!("[{}, {}, {}]", lo, hi, tgt))
                    .collect();
                format!("[[{}], {}]", ts.join(", "), acc)
            })
            .collect();
        // The explicit type annotation makes the nested array literals
        // contextually-typed as tuples (else `number[]` would not be
        // assignable to the `[number, number, number][]` element type).
        writeln!(
            out,
            "{}const DFA_{}: [[number, number, number][], boolean][] = [{}];",
            ind,
            sid,
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
                Literal::Null => "null".to_string(),
            },
            Expression::Var(name) => match name.as_str() {
                "@@:matched" => "this.matched".to_string(),
                "@@:cursor" => "this.cursor".to_string(),
                "@@:return" => "this.return_value".to_string(),
                _ => match name.strip_prefix('$').and_then(|c| c.split_once('.')) {
                    Some((state, label)) => format!("this.{}", cap_field(state, label)),
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
                    if let Some((state, label)) =
                        name.strip_prefix('$').and_then(|c| c.split_once('.'))
                    {
                        if matches!(
                            field.as_str(),
                            "return_value" | "accepted" | "cursor" | "reject_position"
                        ) {
                            // Non-null assertion: the inner instance is set on
                            // the Mode C commit before any read.
                            return format!("this.{}!.{}", cap_inst_field(state, label), field);
                        }
                    }
                    if name == "self" {
                        return format!("this.{}", field);
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
            "to_int" => format!("parseInt({}, 10)", a.join(", ")),
            "to_str" => format!("String({})", a.join(", ")),
            "len" => format!("({}).length", a.join(", ")),
            _ => format!("this.{}({})", func, a.join(", ")),
        }
    }
}

/// Comma-joined `i64` list literal (shared by the Pike `ops`/`rng` arrays).
fn int_list(xs: &[i64]) -> String {
    xs.iter()
        .map(|x| x.to_string())
        .collect::<Vec<_>>()
        .join(", ")
}

/// The matcher invocation for a stage: the Pike VM (`_pikeMatch`) for a lazy
/// stage, the specialized `_matchStage_<sid>` when the stage carries embedding
/// actions, else the shared `_dfaMatch`.
fn stage_call(gen: &Generator, stage: &StageAst, sid: usize) -> String {
    if gen.stage_dfas[sid].program.is_some() {
        format!("this._pikeMatch(this._OPS_{sid}, this._RNG_{sid})")
    } else if stage.embedding_actions.is_empty() {
        format!("this._dfaMatch(DFA_{}, {})", sid, gen.stage_dfas[sid].start)
    } else {
        format!("this._matchStage_{}()", sid)
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
        BinaryOp::Eq => "===",
        BinaryOp::Ne => "!==",
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

fn js_default(raw: &str) -> String {
    match raw {
        "false" => "false".to_string(),
        "true" => "true".to_string(),
        "" => "null".to_string(),
        s if s.starts_with('"') => s.to_string(),
        s => s.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frame_c::compiler::fsm_parser::parse_fsm_block;
    use std::process::Command;

    /// Type-check + emit `code`+`driver` via `npx --no-install tsc`, then run
    /// the emitted JS via `node`, returning stdout lines. `None` if the
    /// toolchain is unavailable. A `tsc` type error fails the test.
    fn ts_run(code: &str, driver: &str, tag: &str) -> Option<Vec<String>> {
        let dir = std::env::temp_dir().join(format!("framec_ts_{}", tag));
        std::fs::create_dir_all(&dir).ok()?;
        let ts_path = dir.join("m.ts");
        std::fs::write(&ts_path, format!("{}\n{}\n", code, driver)).ok()?;
        let compile = match Command::new("npx")
            .args(["--no-install", "tsc"])
            .arg("--target")
            .arg("es2020")
            .arg("--noEmitOnError")
            .arg("--outDir")
            .arg(&dir)
            .arg(&ts_path)
            .output()
        {
            Ok(o) => o,
            Err(_) => return None,
        };
        assert!(
            compile.status.success(),
            "tsc failed for {:?}:\n{}\n{}",
            tag,
            String::from_utf8_lossy(&compile.stdout),
            String::from_utf8_lossy(&compile.stderr)
        );
        let js_path = dir.join("m.js");
        let out = Command::new("node").arg(&js_path).output().expect("node");
        assert!(
            out.status.success(),
            "node failed:\n{}",
            String::from_utf8_lossy(&out.stderr)
        );
        Some(
            String::from_utf8_lossy(&out.stdout)
                .lines()
                .map(|s| s.to_string())
                .collect(),
        )
    }

    /// Generate TS for `src` and run `new M(<arg>)` for each `(arg, tag)`,
    /// printing `accepted` + `return_value`; returns the per-input results.
    fn gen(src: &str) -> String {
        let decl = parse_fsm_block(src.as_bytes()).expect("fixture must parse");
        generate(&decl).expect("fixture must generate")
    }

    #[test]
    fn ts_core_constructs() {
        // smoke + matched/to_int + len + capture + action block + declared
        // action + transitions, all type-checked together.
        let cases: &[(&str, &str, &str, &str)] = &[
            ("@@fsm M(text: bytes) : bool = false { /a/ true }", "\"a\"", "true", "true"),
            ("@@fsm M(text: bytes) : bool = false { /a/ true }", "\"b\"", "false", "false"),
            (
                "@@fsm M(text: bytes) : int = 0 { /[0-9]+/ to_int(@@:matched) }",
                "\"123\"",
                "true",
                "123",
            ),
            (
                "@@fsm M(text: bytes) : int = 0 { /[0-9]+/ len(self.text) }",
                "\"123\"",
                "true",
                "3",
            ),
            (
                "@@fsm M(text: bytes) : int = 0 { $s: .n/[0-9]+/ to_int($s.n) }",
                "\"42\"",
                "true",
                "42",
            ),
            (
                "@@fsm M(text: bytes) : int = 0 { /[0-9]/ { self.count = self.count + 1 } self.count domain: count: int = 0 }",
                "\"5\"",
                "true",
                "1",
            ),
            (
                "@@fsm M(text: bytes) : int = 0 { /[0-9]+/ parse_int(@@:matched) actions: parse_int(s: bytes): int { to_int(s) } }",
                "\"42\"",
                "true",
                "42",
            ),
            (
                "@@fsm M(text: bytes) : int = 0 { $0: /[a-z]/ -> $digits : -> $error $digits: .n/[0-9]+/ to_int($digits.n) $error: -1 }",
                "\"x42\"",
                "true",
                "42",
            ),
        ];
        for (i, (src, arg, exp_acc, exp_ret)) in cases.iter().enumerate() {
            let code = gen(src);
            let driver = format!(
                "const m = new M({arg});\nconsole.log(String(m.accepted));\nconsole.log(String(m.return_value));"
            );
            let Some(lines) = ts_run(&code, &driver, &format!("core_{}", i)) else {
                return;
            };
            assert_eq!(lines[0], *exp_acc, "accepted for case {i}");
            assert_eq!(lines[1], *exp_ret, "return for case {i}");
        }
    }

    #[test]
    fn ts_conditional_target() {
        let code = gen("@@fsm M(text: bytes, mode: int) : int = 0 { \
             /[01]/ -> ( $zero when self.mode == 0, $one when self.mode == 1 ) : -> $error \
             $zero: 0 $one: 1 $error: -1 }");
        let driver = "for (const [i,md] of [[\"0\",0],[\"1\",1],[\"0\",2]]) { const m = new M(i as string, md as number); console.log(String(m.return_value)); }";
        let Some(lines) = ts_run(&code, driver, "cond") else {
            return;
        };
        assert_eq!(lines, vec!["0", "1", "-1"]);
    }

    #[test]
    fn ts_multi_match() {
        let code = gen("@@fsm M(text: bytes) : int = 0 { /[0-9]/ -> $num | 99 $num: 1 }");
        let driver = "for (const s of [\"5\",\"a\"]) { const m = new M(s); console.log(String(m.return_value)); }";
        let Some(lines) = ts_run(&code, driver, "mm") else {
            return;
        };
        assert_eq!(lines, vec!["1", "99"]);
    }

    #[test]
    fn ts_embed_every_transition() {
        let code = gen(
            "@@fsm M(text: bytes) : int = 0 { /[0-9]+/ ${ this_count() } self.count \
             actions: this_count() { self.count = self.count + 1 } domain: count: int = 0 }",
        );
        let driver = "const m = new M(\"123\"); console.log(String(m.return_value));";
        let Some(lines) = ts_run(&code, driver, "emb") else {
            return;
        };
        assert_eq!(lines[0], "3");
    }

    /// FSM-TEST-603 — `%{...}` fires when the DFA leaves its last accepting
    /// state, capturing the end of the matched region.
    #[test]
    fn ts_embed_leave_final() {
        let code = gen(
            "@@fsm M(text: bytes) : int = 0 { /[0-9]+/ %{ self.end_pos = @@:cursor } self.end_pos \
             domain: end_pos: int = 0 }",
        );
        let driver = "for (const s of [\"42x\",\"abx\"]) { const m = new M(s); console.log(String(m.return_value)); }";
        let Some(lines) = ts_run(&code, driver, "leave") else {
            return;
        };
        assert_eq!(lines, vec!["2", "0"]);
    }

    #[test]
    fn ts_token_alphabet() {
        let code = gen("@@fsm M(toks: token) : bool = false { /IDENT LPAREN RPAREN/ true }");
        let driver = "for (const t of [[\"IDENT\",\"LPAREN\",\"RPAREN\"],[\"IDENT\",\"RPAREN\"],[\"IDENT\",\"WAT\"]]) { const m = new M(t); console.log(String(m.accepted)); }";
        let Some(lines) = ts_run(&code, driver, "tok") else {
            return;
        };
        assert_eq!(lines, vec!["true", "false", "false"]);
    }

    #[test]
    fn ts_mode_c_callout() {
        let inner = gen("@@fsm Digits(text: bytes) : int = 0 { /[0-9]+/ to_int(@@:matched) }");
        let outer = gen("@@fsm Outer(text: bytes) : int = 0 { $s: .d/@Digits/ $s.d.return_value }");
        let code = format!("{}\n{}", inner, outer);
        let driver = "for (const s of [\"42\",\"x\"]) { const m = new Outer(s); console.log(String(m.accepted)+\" \"+String(m.return_value)); }";
        let Some(lines) = ts_run(&code, driver, "modec") else {
            return;
        };
        assert_eq!(lines, vec!["true 42", "false 0"]);
    }

    #[test]
    fn ts_anchors() {
        let start = gen("@@fsm M(text: bytes) : bool = false { /^foo/ true }");
        let driver = "for (const s of [\"foo\",\"xfoo\"]) { const m = new M(s); console.log(String(m.accepted)); }";
        let Some(lines) = ts_run(&start, driver, "anc_s") else {
            return;
        };
        assert_eq!(lines, vec!["true", "false"]);
        let end = gen("@@fsm M(text: bytes) : bool = false { /[0-9]+$/ true }");
        let d2 = "for (const s of [\"123\",\"123x\"]) { const m = new M(s); console.log(String(m.accepted)); }";
        let Some(l2) = ts_run(&end, d2, "anc_e") else {
            return;
        };
        assert_eq!(l2, vec!["true", "false"]);
    }

    #[test]
    fn ts_word_boundary() {
        let code = gen("@@fsm M(text: bytes) : bool = false { /\\bcat\\b/ true }");
        let driver = "for (const s of [\"cat\",\"cats\"]) { const m = new M(s); console.log(String(m.accepted)); }";
        let Some(lines) = ts_run(&code, driver, "wb") else {
            return;
        };
        assert_eq!(lines, vec!["true", "false"]);
    }

    #[test]
    fn ts_unsupported_errors() {
        let decl =
            parse_fsm_block(b"@@fsm M(text: bytes) : bool = false { /a$b/ true }").expect("parses");
        let err = generate(&decl).unwrap_err();
        assert!(err.contains("anchor"), "got {err}");
    }

    /// Lazy quantifiers (§11.1) via the Pike VM: `/.*?,/` matches up to the
    /// FIRST comma (greedy `/.*,/` would take the last), and the mixed
    /// `/a*?b+/` keeps `b+` greedy ("aabbb" → cursor 5, not 3).
    #[test]
    fn ts_lazy_quantifier() {
        let code = gen("@@fsm M(text: bytes) : bytes = \"\" { /.*?,/ @@:matched }");
        let driver = "const m = new M(\"ab,cd,ef\"); console.log(String(m.return_value));";
        let Some(lines) = ts_run(&code, driver, "lazy_a") else {
            return;
        };
        assert_eq!(lines[0], "ab,");
        let mixed = gen("@@fsm M(text: bytes) : int = 0 { /a*?b+/ @@:cursor }");
        let d2 = "const m = new M(\"aabbb\"); console.log(String(m.return_value));";
        let Some(l2) = ts_run(&mixed, d2, "lazy_b") else {
            return;
        };
        assert_eq!(l2[0], "5");
    }
}
