//! C backend for `@@fsm` (RFC-0042, Phase 8).
//!
//! C has no classes, so the recognizer is a plain `struct` plus a family of
//! free functions taking a `struct <Name>* self` — `<Name>_init`,
//! `<Name>_run`, `<Name>_state0`, … — that mutate it. The recognition model
//! is otherwise the Java/C++ one ([`super::fsm_cpp`]): a per-state dispatch
//! loop (`switch`) over the struct's mutable fields. The "constructor" is
//! `<Name>_init(&m, ...)`; the observable result (§5.1) is the struct's
//! `accepted`, `return_value`, `cursor`, `reject_position`.
//!
//! Frame's abstract types map as `int` → `int`, `float` → `double`, `bool` →
//! `bool` (`<stdbool.h>`), `str`/`bytes` → `const char*`. Because C functions
//! must be declared before use, the whole function family is forward-declared
//! (prototypes) right after the struct, then defined.
//!
//! ## Representations chosen for C's missing value types
//!
//! - **Input** is held as `const char*` (bytes/char) or `const char**`
//!   (token) plus an explicit length `_len`; a code point is
//!   `(int)(unsigned char)text[pos]` and a token is mapped through
//!   `<Name>_tokId`.
//! - **A per-stage DFA** is three `static const` arrays: a flat
//!   `int data[][3]` of `{lo, hi, target}` rows, an `int off[]` giving each
//!   state's `[off[s], off[s+1])` row range, and an `int accept[]`.
//! - **The matched run** is a heap-allocated null-terminated copy
//!   (`<Name>_slice`) for bytes/char, or an aliasing `const char**` + length
//!   for tokens. Captures take the same shape. (Recognizers are short-lived,
//!   so the slice allocations are intentionally not freed.)
//! - **Mode C** inner recognizers are nested `struct`s stored by value;
//!   inner fsms must therefore be emitted before the outer (inner-first).
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

/// Generate C source implementing `decl`, or a reason it is outside the v0.1
/// C cut.
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
    /// program matched by the VM (`pikeMatch`) instead of the DFA, for
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
                                // A lazy quantifier matches via the Pike VM, which
                                // has no per-element scan for embedding actions to
                                // hook into (§3.5.5/§11.1). Reject the combination
                                // rather than silently giving greedy semantics.
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
                "regex `/{}/` uses a mid-pattern anchor, not yet supported by the C backend",
                regex
            )),
        }
    }

    fn c_type(t: &Type) -> String {
        let s = match t {
            Type::Custom(s) => s.as_str(),
            _ => "int",
        };
        match s {
            "int" => "int".to_string(),
            "float" => "double".to_string(),
            "bool" => "bool".to_string(),
            "str" | "string" | "String" | "bytes" => "const char*".to_string(),
            other => other.to_string(),
        }
    }

    fn fname(&self, suffix: &str) -> String {
        format!("{}_{}", self.decl.name, suffix)
    }

    fn input_name(&self) -> &str {
        &self.decl.params[0].name
    }

    fn element_read(&self) -> String {
        match self.alphabet {
            Alphabet::Token => format!(
                "{}(self, self->{}[pos])",
                self.fname("tokId"),
                self.input_name()
            ),
            _ => format!("(int)(unsigned char)self->{}[pos]", self.input_name()),
        }
    }

    fn emit(&self) -> Result<String, String> {
        let mut out = String::new();
        out.push_str("/* Generated by framec — RFC-0042 @@fsm (C backend). */\n");
        out.push_str("/* Requires <stdbool.h>, <stdlib.h>, <string.h>, <stdio.h>. */\n\n");
        self.emit_struct(&mut out);
        let mut protos: Vec<String> = Vec::new();
        let mut defs = String::new();
        self.emit_helpers(&mut protos, &mut defs);
        self.emit_action_methods(&mut protos, &mut defs)?;
        self.emit_embed_matchers(&mut protos, &mut defs)?;
        self.emit_state_methods(&mut protos, &mut defs)?;
        self.emit_run(&mut protos, &mut defs);
        self.emit_init(&mut protos, &mut defs);
        out.push('\n');
        for p in &protos {
            writeln!(out, "{};", p).ok();
        }
        out.push('\n');
        out.push_str(&defs);
        Ok(out)
    }

    fn emit_struct(&self, out: &mut String) {
        writeln!(out, "struct {} {{", self.decl.name).ok();
        out.push_str("  bool accepted;\n");
        out.push_str("  int reject_position;\n");
        out.push_str("  int cursor;\n");
        writeln!(
            out,
            "  {} return_value;",
            Self::c_type(&self.decl.return_type)
        )
        .ok();
        let mut seen = std::collections::HashSet::new();
        for (i, p) in self.decl.params.iter().enumerate() {
            seen.insert(p.name.clone());
            if i == 0 {
                match self.alphabet {
                    Alphabet::Token => writeln!(out, "  const char** {};", p.name).ok(),
                    _ => writeln!(out, "  const char* {};", p.name).ok(),
                };
            } else {
                writeln!(out, "  {} {};", Self::c_type(&p.param_type), p.name).ok();
            }
        }
        out.push_str("  int _len;\n");
        if let Some(domain) = &self.decl.domain {
            for v in &domain.vars {
                if seen.insert(v.name.clone()) {
                    writeln!(out, "  {} {};", Self::c_type(&v.var_type), v.name).ok();
                }
            }
        }
        match self.alphabet {
            Alphabet::Token => {
                out.push_str("  const char** matched;\n  int matched_len;\n");
            }
            _ => out.push_str("  char* matched;\n"),
        }
        out.push_str("  int enter;\n");
        for f in self.capture_fields() {
            match self.alphabet {
                Alphabet::Token => {
                    writeln!(out, "  const char** {};\n  int {}_len;", f, f).ok();
                }
                _ => {
                    writeln!(out, "  char* {};", f).ok();
                }
            };
        }
        for (f, inner) in self.mode_c_inst_fields() {
            writeln!(out, "  struct {} {};", inner, f).ok();
        }
        out.push_str("};\n\n");
    }

    fn capture_fields(&self) -> Vec<String> {
        let mut out = Vec::new();
        for st in &self.decl.states {
            let Some(slabel) = &st.label else { continue };
            for m in &st.matches {
                for el in &m.elements {
                    if let MatchElement::Stage(stage) = el {
                        if let Some(lbl) = &stage.label {
                            out.push(cap_name(slabel, lbl));
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
                            out.push((cap_inst_name(slabel, lbl), inner.to_string()));
                        }
                    }
                }
            }
        }
        out
    }

    /// Slice helper (bytes/char only), `tokId` (token only), the always-on
    /// `to_str`, and the shared `dfaMatch`.
    fn emit_helpers(&self, protos: &mut Vec<String>, defs: &mut String) {
        let n = &self.decl.name;
        if self.alphabet != Alphabet::Token {
            let sig = format!(
                "char* {}(struct {}* self, int start, int len)",
                self.fname("slice"),
                n
            );
            protos.push(sig.clone());
            writeln!(
                defs,
                "{} {{\n  char* s = (char*)malloc(len + 1);\n  memcpy(s, self->{} + start, len);\n  s[len] = '\\0';\n  return s;\n}}\n",
                sig,
                self.input_name()
            )
            .ok();
        }
        // iswordat: is the byte at index `p` a word byte ([0-9A-Za-z_])?
        // Out-of-bounds (before start / at-or-past end) is non-word (0). Used
        // by the `\b`/`\B` edge-boundary guards; bytes alphabet only.
        if self.uses_word_boundary() {
            let sig = format!("int {}(struct {}* self, int p)", self.fname("iswordat"), n);
            protos.push(sig.clone());
            writeln!(
                defs,
                "{sig} {{\n\
                 \x20 if (p < 0 || p >= self->_len) return 0;\n\
                 \x20 int b = (int)(unsigned char)self->{inp}[p];\n\
                 \x20 return (b >= '0' && b <= '9') || (b >= 'A' && b <= 'Z') || (b >= 'a' && b <= 'z') || b == '_';\n\
                 }}\n",
                sig = sig,
                inp = self.input_name()
            )
            .ok();
        }
        // to_str: numeric → decimal string.
        let sig = format!("char* {}(long v)", self.fname("to_str"));
        protos.push(sig.clone());
        writeln!(
            defs,
            "{} {{\n  char* s = (char*)malloc(32);\n  snprintf(s, 32, \"%ld\", v);\n  return s;\n}}\n",
            sig
        )
        .ok();
        // tokId.
        if self.alphabet == Alphabet::Token {
            let sig = format!(
                "int {}(struct {}* self, const char* t)",
                self.fname("tokId"),
                n
            );
            protos.push(sig.clone());
            writeln!(defs, "{} {{\n  (void)self;", sig).ok();
            let mut entries: Vec<(&String, &u32)> = self.token_ids.iter().collect();
            entries.sort_by_key(|(_, id)| **id);
            for (name, id) in entries {
                writeln!(defs, "  if (strcmp(t, {:?}) == 0) return {};", name, id).ok();
            }
            defs.push_str("  return -1;\n}\n\n");
        }
        // dfaMatch.
        let sig = format!(
            "int {}(struct {}* self, const int (*data)[3], const int* off, const int* accept, int start)",
            self.fname("dfaMatch"),
            n
        );
        protos.push(sig.clone());
        let read = self.element_read();
        writeln!(
            defs,
            "{sig} {{\n\
             \x20 int st = start;\n\
             \x20 int pos = self->cursor;\n\
             \x20 int n = self->_len;\n\
             \x20 int last = accept[st] ? pos : -1;\n\
             \x20 while (pos < n) {{\n\
             \x20   int v = {read};\n\
             \x20   int nxt = -1;\n\
             \x20   for (int k = off[st]; k < off[st + 1]; k++) {{\n\
             \x20     if (data[k][0] <= v && v <= data[k][1]) {{ nxt = data[k][2]; break; }}\n\
             \x20   }}\n\
             \x20   if (nxt < 0) break;\n\
             \x20   st = nxt; pos++;\n\
             \x20   if (accept[st]) last = pos;\n\
             \x20 }}\n\
             \x20 return last;\n\
             }}\n",
            sig = sig,
            read = read
        )
        .ok();
        // Pike VM (priority NFA simulation) for lazy-quantifier stages (§11.1),
        // over the flat `ops`/`rng` arrays (`fsm_regex::pike::encode`). `ops` is
        // 4 ints per instruction `[op, a, b, _]`: 0 Char (a = pair index, b =
        // pair count), 1 Split (a/b targets, a higher), 2 Jmp, 3 Match.
        if self.uses_pike() {
            // pikeIsWord: is the element at absolute position `p` a word char
            // (in the `word` pair-table)? Out-of-range is non-word.
            let iw_sig = format!(
                "int {}(struct {}* self, int p, const int* word, int word_len)",
                self.fname("pikeIsWord"),
                n
            );
            protos.push(iw_sig.clone());
            writeln!(
                defs,
                "{iw_sig} {{\n\
                 \x20 if (p < 0 || p >= self->_len) return 0;\n\
                 \x20 int v = (int)(unsigned char)self->{inp}[p];\n\
                 \x20 for (int k = 0; k < word_len / 2; k++) {{\n\
                 \x20   if (word[k * 2] <= v && v <= word[k * 2 + 1]) return 1;\n\
                 \x20 }}\n\
                 \x20 return 0;\n\
                 }}\n",
                iw_sig = iw_sig,
                inp = self.input_name()
            )
            .ok();
            // pikeAssert: evaluate zero-width assertion `kind` at position `pos`
            // (0 InputStart, 1 InputEnd, 2 LineStart, 3 LineEnd, 4 \b, 5 \B).
            let as_sig = format!(
                "int {}(struct {}* self, int kind, int pos, const int* word, int word_len)",
                self.fname("pikeAssert"),
                n
            );
            protos.push(as_sig.clone());
            writeln!(
                defs,
                "{as_sig} {{\n\
                 \x20 int n = self->_len;\n\
                 \x20 if (kind == 0) return pos == 0;\n\
                 \x20 if (kind == 1) return pos == n;\n\
                 \x20 if (kind == 2) return pos == 0 || (int)(unsigned char)self->{inp}[pos - 1] == 10;\n\
                 \x20 if (kind == 3) return pos == n || (int)(unsigned char)self->{inp}[pos] == 10;\n\
                 \x20 if (kind == 4) return {iw}(self, pos - 1, word, word_len) != {iw}(self, pos, word, word_len);\n\
                 \x20 return {iw}(self, pos - 1, word, word_len) == {iw}(self, pos, word, word_len);\n\
                 }}\n",
                as_sig = as_sig,
                iw = self.fname("pikeIsWord"),
                inp = self.input_name()
            )
            .ok();
            // pikeAdd: ε-closure expansion into a thread list (recursive).
            let add_sig = format!(
                "void {}(struct {}* self, const int* ops, const int* word, int word_len, int* lst, int* len, char* seen, int pc, int pos)",
                self.fname("pikeAdd"),
                n
            );
            protos.push(add_sig.clone());
            writeln!(
                defs,
                "{add_sig} {{\n\
                 \x20 if (seen[pc]) return;\n\
                 \x20 seen[pc] = 1;\n\
                 \x20 int op = ops[pc * 4];\n\
                 \x20 if (op == 2) {{\n\
                 \x20   {add}(self, ops, word, word_len, lst, len, seen, ops[pc * 4 + 1], pos);\n\
                 \x20 }} else if (op == 1) {{\n\
                 \x20   {add}(self, ops, word, word_len, lst, len, seen, ops[pc * 4 + 1], pos);\n\
                 \x20   {add}(self, ops, word, word_len, lst, len, seen, ops[pc * 4 + 2], pos);\n\
                 \x20 }} else if (op == 4) {{\n\
                 \x20   if ({assert}(self, ops[pc * 4 + 1], pos, word, word_len))\n\
                 \x20     {add}(self, ops, word, word_len, lst, len, seen, pc + 1, pos);\n\
                 \x20 }} else {{\n\
                 \x20   lst[(*len)++] = pc;\n\
                 \x20 }}\n\
                 }}\n",
                add_sig = add_sig,
                add = self.fname("pikeAdd"),
                assert = self.fname("pikeAssert")
            )
            .ok();
            // pikeMatch: returns the highest-priority (leftmost-first) match-end
            // from the cursor, or -1.
            let m_sig = format!(
                "int {}(struct {}* self, const int* ops, int ops_len, const int* rng, const int* word, int word_len)",
                self.fname("pikeMatch"),
                n
            );
            protos.push(m_sig.clone());
            writeln!(
                defs,
                "{m_sig} {{\n\
                 \x20 int n = self->_len;\n\
                 \x20 int ninst = ops_len / 4;\n\
                 \x20 int cap = ninst > 0 ? ninst : 1;\n\
                 \x20 int matched = -1;\n\
                 \x20 int* clist = (int*)malloc(sizeof(int) * cap);\n\
                 \x20 int* nlist = (int*)malloc(sizeof(int) * cap);\n\
                 \x20 char* cseen = (char*)malloc(cap);\n\
                 \x20 char* nseen = (char*)malloc(cap);\n\
                 \x20 int clen = 0;\n\
                 \x20 memset(cseen, 0, cap);\n\
                 \x20 {add}(self, ops, word, word_len, clist, &clen, cseen, 0, self->cursor);\n\
                 \x20 int pos = self->cursor;\n\
                 \x20 while (1) {{\n\
                 \x20   int nlen = 0;\n\
                 \x20   memset(nseen, 0, cap);\n\
                 \x20   for (int i = 0; i < clen; i++) {{\n\
                 \x20     int pc = clist[i];\n\
                 \x20     int op = ops[pc * 4];\n\
                 \x20     if (op == 0) {{\n\
                 \x20       if (pos < n) {{\n\
                 \x20         int v = (int)(unsigned char)self->{inp}[pos];\n\
                 \x20         int rs = ops[pc * 4 + 1];\n\
                 \x20         int rc = ops[pc * 4 + 2];\n\
                 \x20         for (int k = 0; k < rc; k++) {{\n\
                 \x20           if (rng[(rs + k) * 2] <= v && v <= rng[(rs + k) * 2 + 1]) {{\n\
                 \x20             {add}(self, ops, word, word_len, nlist, &nlen, nseen, pc + 1, pos + 1);\n\
                 \x20             break;\n\
                 \x20           }}\n\
                 \x20         }}\n\
                 \x20       }}\n\
                 \x20     }} else if (op == 3) {{\n\
                 \x20       matched = pos;\n\
                 \x20       break;\n\
                 \x20     }}\n\
                 \x20   }}\n\
                 \x20   if (pos >= n) break;\n\
                 \x20   pos++;\n\
                 \x20   {{ int* tl = clist; clist = nlist; nlist = tl; }}\n\
                 \x20   clen = nlen;\n\
                 \x20 }}\n\
                 \x20 free(clist); free(nlist); free(cseen); free(nseen);\n\
                 \x20 return matched;\n\
                 }}\n",
                m_sig = m_sig,
                add = self.fname("pikeAdd"),
                inp = self.input_name()
            )
            .ok();
        }
    }

    fn emit_run(&self, protos: &mut Vec<String>, defs: &mut String) {
        let sig = format!(
            "void {}(struct {}* self)",
            self.fname("run"),
            self.decl.name
        );
        protos.push(sig.clone());
        writeln!(defs, "{} {{\n  int state = 0;", sig).ok();
        defs.push_str("  while (state >= 0) {\n");
        defs.push_str("    int _enter = self->enter;\n    self->enter = 0;\n");
        defs.push_str("    switch (state) {\n");
        for i in 0..self.decl.states.len() {
            writeln!(
                defs,
                "      case {}: state = {}(self, _enter); break;",
                i,
                self.fname(&format!("state{}", i))
            )
            .ok();
        }
        defs.push_str("      default: return;\n    }\n  }\n}\n\n");
    }

    fn emit_init(&self, protos: &mut Vec<String>, defs: &mut String) {
        let input = self.input_name().to_string();
        let mut params = vec![format!("struct {}* self", self.decl.name)];
        match self.alphabet {
            Alphabet::Token => {
                params.push(format!("const char** {}", input));
                params.push("int _len".to_string());
            }
            _ => params.push(format!("const char* {}", input)),
        }
        for p in self.decl.params.iter().skip(1) {
            params.push(format!("{} {}", Self::c_type(&p.param_type), p.name));
        }
        let sig = format!("void {}({})", self.fname("init"), params.join(", "));
        protos.push(sig.clone());
        writeln!(defs, "{} {{", sig).ok();
        defs.push_str(
            "  self->accepted = false;\n  self->reject_position = 0;\n  self->cursor = 0;\n",
        );
        writeln!(
            defs,
            "  self->return_value = {};",
            c_default(&self.decl.return_type, &self.decl.default_expr)
        )
        .ok();
        defs.push_str("  self->enter = 0;\n");
        match self.alphabet {
            Alphabet::Token => defs.push_str("  self->matched = NULL;\n  self->matched_len = 0;\n"),
            _ => defs.push_str("  self->matched = NULL;\n"),
        }
        for f in self.capture_fields() {
            match self.alphabet {
                Alphabet::Token => {
                    writeln!(defs, "  self->{} = NULL;\n  self->{}_len = 0;", f, f).ok()
                }
                _ => writeln!(defs, "  self->{} = NULL;", f).ok(),
            };
        }
        if let Some(domain) = &self.decl.domain {
            for v in &domain.vars {
                if v.name == input {
                    continue;
                }
                writeln!(defs, "  self->{} = {};", v.name, self.expr(&v.default)).ok();
            }
        }
        // Bind parameters.
        match self.alphabet {
            Alphabet::Token => {
                writeln!(defs, "  self->{} = {};", input, input).ok();
                defs.push_str("  self->_len = _len;\n");
            }
            _ => {
                writeln!(defs, "  self->{} = {};", input, input).ok();
                writeln!(defs, "  self->_len = (int)strlen({});", input).ok();
            }
        }
        for p in self.decl.params.iter().skip(1) {
            writeln!(defs, "  self->{} = {};", p.name, p.name).ok();
        }
        writeln!(defs, "  {}(self);", self.fname("run")).ok();
        defs.push_str("  if (self->accepted) self->reject_position = 0;\n}\n\n");
    }

    fn emit_state_methods(
        &self,
        protos: &mut Vec<String>,
        defs: &mut String,
    ) -> Result<(), String> {
        let mut sid = 0usize;
        for (i, st) in self.decl.states.iter().enumerate() {
            let sig = format!(
                "int {}(struct {}* self, int _enter)",
                self.fname(&format!("state{}", i)),
                self.decl.name
            );
            protos.push(sig.clone());
            match st.matches.len() {
                0 => {
                    writeln!(defs, "{} {{ (void)self; (void)_enter; return -1; }}\n", sig).ok();
                }
                1 => self.emit_one_state(defs, &sig, st, &st.matches[0], &mut sid)?,
                _ => self.emit_multi_match(defs, &sig, st, &mut sid)?,
            }
        }
        Ok(())
    }

    fn emit_one_state(
        &self,
        out: &mut String,
        sig: &str,
        st: &FsmStateAst,
        m: &MatchAst,
        sid: &mut usize,
    ) -> Result<(), String> {
        let state_label = st.label.clone().unwrap_or_default();
        writeln!(out, "{} {{", sig).ok();
        for (idx, el) in m.elements.iter().enumerate() {
            writeln!(out, "  if (_enter <= {}) {{", idx).ok();
            self.emit_element(out, el, m, &state_label, "    ", sid)?;
            out.push_str("  }\n");
        }
        self.emit_success(out, m, "  ");
        out.push_str("}\n\n");
        Ok(())
    }

    fn emit_multi_match(
        &self,
        out: &mut String,
        sig: &str,
        st: &FsmStateAst,
        sid: &mut usize,
    ) -> Result<(), String> {
        let state_label = st.label.clone().unwrap_or_default();
        writeln!(out, "{} {{\n  (void)_enter;", sig).ok();
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
                             supported by the C backend"
                                .into(),
                        );
                    }
                    let my_sid = *sid;
                    *sid += 1;
                    if self.stage_dfas[my_sid].mode_c.is_some() {
                        return Err(
                            "a Mode C (`/@Fsm/`) stage as a `|` alternative selector is not yet \
                             supported by the C backend"
                                .into(),
                        );
                    }
                    let MatchElement::Stage(sel) = &m.elements[fs] else {
                        unreachable!("first_stage indexes a Stage element")
                    };
                    if self.stage_dfas[my_sid].program.is_some() {
                        self.emit_pike_arrays(out, my_sid, "  ");
                        writeln!(
                            out,
                            "  int _r{sid} = {f}(self, t{sid}_ops, (int)(sizeof(t{sid}_ops)/sizeof(int)), t{sid}_rng, t{sid}_word, t{sid}_word_len);",
                            sid = my_sid,
                            f = self.fname("pikeMatch")
                        )
                        .ok();
                    } else {
                        self.emit_dfa_arrays(out, my_sid, "  ");
                        writeln!(
                            out,
                            "  int _r{sid} = {f}(self, t{sid}_data, t{sid}_off, t{sid}_acc, {start});",
                            sid = my_sid,
                            f = self.fname("dfaMatch"),
                            start = self.stage_dfas[my_sid].start
                        )
                        .ok();
                    }
                    self.emit_anchor_guards(out, my_sid, "  ");
                    writeln!(out, "  if (_r{} >= 0) {{", my_sid).ok();
                    self.emit_set_matched(out, my_sid, "    ");
                    if let Some(lbl) = &sel.label {
                        if !state_label.is_empty() {
                            self.emit_capture(out, &state_label, lbl, "    ");
                        }
                    }
                    writeln!(out, "    self->cursor = _r{};", my_sid).ok();
                    out.push_str("    self->accepted = true;\n");
                    for el in &m.elements[fs + 1..] {
                        self.emit_element(out, el, m, &state_label, "    ", sid)?;
                    }
                    self.emit_success(out, m, "    ");
                    out.push_str("  }\n");
                }
                None => {
                    out.push_str("  self->accepted = true;\n");
                    for el in &m.elements {
                        self.emit_element(out, el, m, &state_label, "  ", sid)?;
                    }
                    self.emit_success(out, m, "  ");
                    catch_all = true;
                    break;
                }
            }
        }
        if !catch_all {
            out.push_str("  self->accepted = false;\n");
            out.push_str("  self->reject_position = self->cursor;\n");
            out.push_str("  return -1;\n");
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
        let ind2 = format!("{}  ", ind);
        match el {
            MatchElement::Stage(stage) => {
                let my_sid = *sid;
                *sid += 1;
                if let Some(inner) = self.stage_dfas[my_sid].mode_c.clone() {
                    self.emit_mode_c(out, &inner, stage, m, state_label, my_sid, ind, &ind2);
                    return Ok(());
                }
                if self.stage_dfas[my_sid].program.is_some() {
                    // A lazy quantifier: the Pike VM over the stage's program.
                    self.emit_pike_arrays(out, my_sid, ind);
                    writeln!(
                        out,
                        "{ind}int _r{sid} = {f}(self, t{sid}_ops, (int)(sizeof(t{sid}_ops)/sizeof(int)), t{sid}_rng, t{sid}_word, t{sid}_word_len);",
                        ind = ind,
                        sid = my_sid,
                        f = self.fname("pikeMatch")
                    )
                    .ok();
                } else if stage.embedding_actions.is_empty() {
                    self.emit_dfa_arrays(out, my_sid, ind);
                    writeln!(
                        out,
                        "{ind}int _r{sid} = {f}(self, t{sid}_data, t{sid}_off, t{sid}_acc, {start});",
                        ind = ind,
                        sid = my_sid,
                        f = self.fname("dfaMatch"),
                        start = self.stage_dfas[my_sid].start
                    )
                    .ok();
                } else {
                    writeln!(
                        out,
                        "{}int _r{} = {}(self);",
                        ind,
                        my_sid,
                        self.fname(&format!("matchStage{}", my_sid))
                    )
                    .ok();
                }
                self.emit_anchor_guards(out, my_sid, ind);
                writeln!(out, "{}if (_r{} < 0) {{", ind, my_sid).ok();
                self.emit_failure(out, m, &ind2);
                writeln!(out, "{}}}", ind).ok();
                self.emit_set_matched(out, my_sid, ind);
                if let Some(lbl) = &stage.label {
                    if !state_label.is_empty() {
                        self.emit_capture(out, state_label, lbl, ind);
                    }
                }
                writeln!(out, "{}self->cursor = _r{};", ind, my_sid).ok();
                writeln!(out, "{}self->accepted = true;", ind).ok();
            }
            MatchElement::BareExpression { expr, .. } => {
                writeln!(out, "{}self->return_value = {};", ind, self.expr(expr)).ok();
            }
            MatchElement::ActionBlock(blk) => {
                for s in &blk.statements {
                    out.push_str(&self.stmt(s, ind)?);
                }
            }
        }
        Ok(())
    }

    /// `self->matched = <run [cursor, _r<sid>)>` in the alphabet's shape.
    fn emit_set_matched(&self, out: &mut String, sid: usize, ind: &str) {
        match self.alphabet {
            Alphabet::Token => {
                writeln!(
                    out,
                    "{}self->matched = &self->{}[self->cursor];",
                    ind,
                    self.input_name()
                )
                .ok();
                writeln!(out, "{}self->matched_len = _r{} - self->cursor;", ind, sid).ok();
            }
            _ => {
                writeln!(
                    out,
                    "{}self->matched = {}(self, self->cursor, _r{} - self->cursor);",
                    ind,
                    self.fname("slice"),
                    sid
                )
                .ok();
            }
        }
    }

    fn emit_capture(&self, out: &mut String, state: &str, lbl: &str, ind: &str) {
        let f = cap_name(state, lbl);
        match self.alphabet {
            Alphabet::Token => {
                writeln!(out, "{}self->{} = self->matched;", ind, f).ok();
                writeln!(out, "{}self->{}_len = self->matched_len;", ind, f).ok();
            }
            _ => {
                writeln!(out, "{}self->{} = self->matched;", ind, f).ok();
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn emit_mode_c(
        &self,
        out: &mut String,
        inner: &str,
        stage: &StageAst,
        m: &MatchAst,
        state_label: &str,
        _my_sid: usize,
        ind: &str,
        ind2: &str,
    ) {
        let input = self.input_name();
        let (inst, has_label) = match &stage.label {
            Some(lbl) if !state_label.is_empty() => {
                (cap_inst_name(state_label, lbl), Some(lbl.clone()))
            }
            _ => ("__inner_scratch".to_string(), None),
        };
        // A scratch instance when the stage is unlabelled (no struct field);
        // the init call itself is emitted by the alphabet match below.
        if has_label.is_none() {
            writeln!(out, "{}struct {} {};", ind, inner, inst).ok();
        }
        let target = if has_label.is_some() {
            format!("self->{}", inst)
        } else {
            inst.clone()
        };
        match self.alphabet {
            Alphabet::Token => {
                writeln!(
                    out,
                    "{}{}(&{}, &self->{}[self->cursor], self->_len - self->cursor);",
                    ind,
                    self.fname_for(inner, "init"),
                    target,
                    input
                )
                .ok();
            }
            _ => {
                writeln!(
                    out,
                    "{}{}(&{}, self->{} + self->cursor);",
                    ind,
                    self.fname_for(inner, "init"),
                    target,
                    input
                )
                .ok();
            }
        }
        writeln!(out, "{}if (!{}.accepted) {{", ind, target).ok();
        self.emit_failure(out, m, ind2);
        writeln!(out, "{}}}", ind).ok();
        // matched run = [cursor, cursor + inner.cursor).
        match self.alphabet {
            Alphabet::Token => {
                writeln!(
                    out,
                    "{}self->matched = &self->{}[self->cursor];",
                    ind, input
                )
                .ok();
                writeln!(out, "{}self->matched_len = {}.cursor;", ind, target).ok();
            }
            _ => {
                writeln!(
                    out,
                    "{}self->matched = {}(self, self->cursor, {}.cursor);",
                    ind,
                    self.fname("slice"),
                    target
                )
                .ok();
            }
        }
        if let Some(lbl) = &has_label {
            self.emit_capture(out, state_label, lbl, ind);
        }
        writeln!(
            out,
            "{}self->cursor = self->cursor + {}.cursor;",
            ind, target
        )
        .ok();
        writeln!(out, "{}self->accepted = true;", ind).ok();
    }

    fn fname_for(&self, ty: &str, suffix: &str) -> String {
        format!("{}_{}", ty, suffix)
    }

    fn emit_embed_matchers(
        &self,
        protos: &mut Vec<String>,
        defs: &mut String,
    ) -> Result<(), String> {
        let mut sid = 0usize;
        for st in &self.decl.states {
            for m in &st.matches {
                for el in &m.elements {
                    if let MatchElement::Stage(stage) = el {
                        if !stage.embedding_actions.is_empty() {
                            self.emit_one_matcher(protos, defs, sid, stage)?;
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
        protos: &mut Vec<String>,
        out: &mut String,
        sid: usize,
        stage: &StageAst,
    ) -> Result<(), String> {
        let read = self.element_read();
        let sig = format!(
            "int {}(struct {}* self)",
            self.fname(&format!("matchStage{}", sid)),
            self.decl.name
        );
        protos.push(sig.clone());
        writeln!(out, "{} {{", sig).ok();
        self.emit_dfa_arrays(out, sid, "  ");
        writeln!(
            out,
            "  int entry = self->cursor;\n  int st = {start};\n  int pos = entry;\n  int n = self->_len;\n  int last = t{sid}_acc[st] ? pos : -1;\n  self->cursor = pos;",
            start = self.stage_dfas[sid].start,
            sid = sid
        )
        .ok();
        out.push_str(&self.embed_body(stage, EmbeddingOp::Start, "  ")?);
        writeln!(out, "  int prev = t{}_acc[st];", sid).ok();
        writeln!(
            out,
            "  while (pos < n) {{\n    int v = {read};\n    int nxt = -1;\n    for (int k = t{sid}_off[st]; k < t{sid}_off[st + 1]; k++) {{\n      if (t{sid}_data[k][0] <= v && v <= t{sid}_data[k][1]) {{ nxt = t{sid}_data[k][2]; break; }}\n    }}\n    if (nxt < 0) break;\n    st = nxt; pos++;\n    self->cursor = pos;",
            read = read,
            sid = sid
        )
        .ok();
        out.push_str(&self.embed_body(stage, EmbeddingOp::EveryTransition, "    ")?);
        writeln!(out, "    int now = t{}_acc[st];", sid).ok();
        let accept = self.embed_body(stage, EmbeddingOp::Accept, "      ")?;
        if !accept.is_empty() {
            out.push_str("    if (now) {\n");
            out.push_str(&accept);
            out.push_str("    }\n");
        }
        out.push_str("    if (now) last = pos;\n    prev = now;\n");
        out.push_str("  }\n");
        // `%{}` — left the last accepting state: a post-scan event firing once
        // when the longest match stops extending (failing element or EOF), with
        // `@@:cursor` at the end of the matched region (`last`), not the failing
        // element (§5.4 / FSM-TEST-603). `last < 0` ⇒ no accepting state was
        // entered, so there is nothing to leave.
        let leave = self.embed_body(stage, EmbeddingOp::LeaveAccept, "    ")?;
        if !leave.is_empty() {
            out.push_str("  if (last >= 0) {\n    self->cursor = last;\n");
            out.push_str(&leave);
            out.push_str("  }\n");
        }
        let eof = self.embed_body(stage, EmbeddingOp::Eof, "    ")?;
        if !eof.is_empty() {
            out.push_str("  if (pos >= n && !prev) {\n");
            out.push_str(&eof);
            out.push_str("  }\n");
        } else {
            out.push_str("  (void)prev;\n");
        }
        out.push_str("  self->cursor = entry;\n  return last;\n}\n\n");
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
            writeln!(out, "{}if (self->cursor != 0) _r{} = -1;", ind, sid).ok();
        }
        if dfa.requires_end {
            writeln!(out, "{}if (_r{} != self->_len) _r{} = -1;", ind, sid, sid).ok();
        }
        if let Some(b) = dfa.start_boundary {
            let op = boundary_op(b);
            let f = self.fname("iswordat");
            writeln!(
                out,
                "{ind}if ({f}(self, self->cursor - 1) {op} {f}(self, self->cursor)) _r{sid} = -1;",
                ind = ind,
                f = f,
                op = op,
                sid = sid
            )
            .ok();
        }
        if let Some(b) = dfa.end_boundary {
            let op = boundary_op(b);
            let f = self.fname("iswordat");
            writeln!(
                out,
                "{ind}if (_r{sid} >= 0 && {f}(self, _r{sid} - 1) {op} {f}(self, _r{sid})) _r{sid} = -1;",
                ind = ind,
                f = f,
                op = op,
                sid = sid
            )
            .ok();
        }
    }

    /// True iff any compiled stage uses a `\b`/`\B` edge boundary, gating the
    /// `iswordat` helper emission.
    fn uses_word_boundary(&self) -> bool {
        self.stage_dfas
            .iter()
            .any(|d| d.start_boundary.is_some() || d.end_boundary.is_some())
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
        writeln!(out, "{}self->accepted = false;", ind).ok();
        writeln!(out, "{}self->reject_position = self->cursor;", ind).ok();
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
            writeln!(out, "{}return -1; /* undeclared state ${} */", ind, state).ok();
            return;
        }
        if let Some(s) = stage {
            match self
                .stage_entry
                .get(&(state.to_string(), s.clone()))
                .copied()
            {
                Some(entry) => {
                    writeln!(out, "{}self->enter = {};", ind, entry).ok();
                }
                None => {
                    writeln!(
                        out,
                        "{}return -1; /* undeclared stage ${}.{} */",
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
                "statement {:?} not supported in @@fsm action blocks by the C backend",
                std::mem::discriminant(other)
            )),
        }
    }

    fn emit_action_methods(
        &self,
        protos: &mut Vec<String>,
        defs: &mut String,
    ) -> Result<(), String> {
        let Some(block) = &self.decl.actions else {
            return Ok(());
        };
        for act in &block.actions {
            let mut params = vec![format!("struct {}* self", self.decl.name)];
            for p in &act.params {
                params.push(format!("{} {}", Self::c_type(&p.param_type), p.name));
            }
            let ret = match &act.return_type {
                Some(t) => Self::c_type(t),
                None => "void".to_string(),
            };
            let sig = format!("{} {}({})", ret, self.fname(&act.name), params.join(", "));
            protos.push(sig.clone());
            writeln!(defs, "{} {{", sig).ok();
            let n = act.body.statements.len();
            let has_return = act.return_type.is_some();
            for (i, s) in act.body.statements.iter().enumerate() {
                use crate::frame_c::compiler::frame_ast::Statement;
                if i + 1 == n && has_return {
                    if let Statement::Expression(e) = s {
                        if !matches!(e.expr, Expression::Assign { .. }) {
                            writeln!(defs, "  return {};", self.expr(&e.expr)).ok();
                            continue;
                        }
                    }
                }
                defs.push_str(&self.stmt(s, "  ")?);
            }
            defs.push_str("}\n\n");
        }
        Ok(())
    }

    /// Does any stage match via the Pike VM (a lazy quantifier, §11.1)?
    fn uses_pike(&self) -> bool {
        self.stage_dfas.iter().any(|d| d.program.is_some())
    }

    /// Emit a lazy stage's Pike program as two `static const int` arrays:
    /// `t<sid>_ops[]` (4 ints per instruction) and `t<sid>_rng[]` (`lo,hi`
    /// pairs). The op-array length is recovered at the call site via
    /// `sizeof` (C arrays carry no length).
    fn emit_pike_arrays(&self, out: &mut String, sid: usize, ind: &str) {
        let prog = self.stage_dfas[sid]
            .program
            .as_ref()
            .expect("emit_pike_arrays called on a non-lazy stage");
        let (ops, rng) = fsm_regex::pike::encode(prog);
        // A range-less program (no Char op) still needs a non-empty array so
        // the C declaration is valid; a `{0}` filler is never indexed.
        let rng_lit = if rng.is_empty() {
            "0".to_string()
        } else {
            int_list(&rng)
        };
        writeln!(
            out,
            "{}static const int t{}_ops[] = {{{}}};",
            ind,
            sid,
            int_list(&ops)
        )
        .ok();
        writeln!(
            out,
            "{}static const int t{}_rng[] = {{{}}};",
            ind, sid, rng_lit
        )
        .ok();
        // The word-character table for `\b`/`\B`; `{0}` filler when unused so
        // the declaration is valid (the real length is passed separately).
        let word = fsm_regex::pike::program_word_table(prog, self.alphabet);
        let word_lit = if word.is_empty() {
            "0".to_string()
        } else {
            int_list(&word)
        };
        writeln!(
            out,
            "{}static const int t{}_word[] = {{{}}};",
            ind, sid, word_lit
        )
        .ok();
        writeln!(
            out,
            "{}static const int t{}_word_len = {};",
            ind,
            sid,
            word.len()
        )
        .ok();
    }

    /// Emit a per-stage DFA as three `static const` arrays:
    /// `t<sid>_data[][3]`, `t<sid>_off[]`, `t<sid>_acc[]`.
    fn emit_dfa_arrays(&self, out: &mut String, sid: usize, ind: &str) {
        let dfa = &self.stage_dfas[sid];
        let mut rows: Vec<String> = Vec::new();
        let mut off: Vec<usize> = vec![0];
        let mut acc: Vec<String> = Vec::new();
        for (trans, is_acc) in &dfa.states {
            for (lo, hi, tgt) in trans {
                rows.push(format!("{{{}, {}, {}}}", lo, hi, tgt));
            }
            off.push(rows.len());
            acc.push(if *is_acc { "1" } else { "0" }.to_string());
        }
        if rows.is_empty() {
            rows.push("{0, 0, 0}".to_string());
        }
        writeln!(
            out,
            "{}static const int t{}_data[][3] = {{{}}};",
            ind,
            sid,
            rows.join(", ")
        )
        .ok();
        writeln!(
            out,
            "{}static const int t{}_off[] = {{{}}};",
            ind,
            sid,
            off.iter()
                .map(|n| n.to_string())
                .collect::<Vec<_>>()
                .join(", ")
        )
        .ok();
        writeln!(
            out,
            "{}static const int t{}_acc[] = {{{}}};",
            ind,
            sid,
            acc.join(", ")
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
                Literal::Null => "NULL".to_string(),
            },
            Expression::Var(name) => match name.as_str() {
                "@@:matched" => "self->matched".to_string(),
                "@@:cursor" => "self->cursor".to_string(),
                "@@:return" => "self->return_value".to_string(),
                _ => match name.strip_prefix('$').and_then(|c| c.split_once('.')) {
                    Some((state, label)) => format!("self->{}", cap_name(state, label)),
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
                            return format!("self->{}.{}", cap_inst_name(state, label), field);
                        }
                    }
                    if name == "self" {
                        return format!("self->{}", field);
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
        match func {
            "to_int" => format!("atoi({})", self.expr(&args[0])),
            "to_str" => format!("{}((long)({}))", self.fname("to_str"), self.expr(&args[0])),
            "len" => self.len_call(&args[0]),
            // A declared action: prefix with the fsm name and thread `self`.
            _ => {
                let mut a = vec!["self".to_string()];
                a.extend(args.iter().map(|e| self.expr(e)));
                format!("{}({})", self.fname(func), a.join(", "))
            }
        }
    }

    /// `len(x)`: the input length is `_len`; a bytes/char string measures by
    /// `strlen`; a token run carries its own `<field>_len`.
    fn len_call(&self, arg: &Expression) -> String {
        let r = self.expr(arg);
        let input = format!("self->{}", self.input_name());
        if r == input {
            return "self->_len".to_string();
        }
        match self.alphabet {
            Alphabet::Token => {
                if r == "self->matched" {
                    "self->matched_len".to_string()
                } else if let Some(field) = r.strip_prefix("self->") {
                    format!("self->{}_len", field)
                } else {
                    format!("(int)strlen({})", r)
                }
            }
            _ => format!("(int)strlen({})", r),
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

fn cap_name(state: &str, label: &str) -> String {
    format!("cap_{}_{}", state, label)
}

fn cap_inst_name(state: &str, label: &str) -> String {
    format!("cap_inst_{}_{}", state, label)
}

/// `\b` (Required) demands the two sides of the edge differ in word-ness
/// (`==` would falsify the violation guard), `\B` (Forbidden) demands they
/// match. The guard fires `_r = -1` when the predicate holds, so the operator
/// is inverted relative to the semantic: Required→`==`, Forbidden→`!=`.
fn boundary_op(b: WordBoundary) -> &'static str {
    match b {
        WordBoundary::Required => "==",
        WordBoundary::Forbidden => "!=",
    }
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

fn c_default(ty: &Type, raw: &str) -> String {
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

    /// Compile (`cc -std=c11`) + run a program: includes, the generated
    /// structs/functions, and a `main` driver. Returns stdout lines. `None`
    /// if no C compiler is available.
    fn c_run(code: &str, driver: &str, tag: &str) -> Option<Vec<String>> {
        let prog = format!(
            "#include <stdbool.h>\n#include <stdlib.h>\n#include <string.h>\n#include <stdio.h>\n\n{}\nint main(void) {{\n{}\n  return 0;\n}}\n",
            code, driver
        );
        let dir = std::env::temp_dir().join(format!("framec_c_{}", tag));
        std::fs::create_dir_all(&dir).ok()?;
        let src = dir.join("prog.c");
        let bin = dir.join("prog");
        std::fs::write(&src, prog).ok()?;
        let compile = match Command::new("cc")
            .arg("-std=c11")
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
            "cc failed for {:?}:\n{}",
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
    fn c_core() {
        let cases: &[(&str, &str, &str, &str)] = &[
            ("@@fsm A(text: bytes) : bool = false { /a/ true }", "A", "true", "%bool"),
            ("@@fsm B(text: bytes) : bool = false { /a/ true }", "B", "false", "%bool"),
            (
                "@@fsm C(text: bytes) : int = 0 { /[0-9]+/ to_int(@@:matched) }",
                "C",
                "true",
                "%d",
            ),
            (
                "@@fsm D(text: bytes) : int = 0 { /[0-9]+/ len(self.text) }",
                "D",
                "true",
                "%d",
            ),
            (
                "@@fsm E(text: bytes) : int = 0 { $s: .n/[0-9]+/ to_int($s.n) }",
                "E",
                "true",
                "%d",
            ),
            (
                "@@fsm F(text: bytes) : int = 0 { /[0-9]/ { self.count = self.count + 1 } self.count domain: count: int = 0 }",
                "F",
                "true",
                "%d",
            ),
            (
                "@@fsm G(text: bytes) : int = 0 { /[0-9]+/ parse_int(@@:matched) actions: parse_int(s: bytes): int { to_int(s) } }",
                "G",
                "true",
                "%d",
            ),
            (
                "@@fsm H(text: bytes) : int = 0 { $0: /[a-z]/ -> $digits : -> $error $digits: .n/[0-9]+/ to_int($digits.n) $error: -1 }",
                "H",
                "true",
                "%d",
            ),
        ];
        let inputs = ["a", "b", "123", "123", "42", "5", "42", "x42"];
        let expect_ret = ["", "", "123", "3", "42", "1", "42", "42"];
        let code = cases
            .iter()
            .map(|(s, ..)| gen(s))
            .collect::<Vec<_>>()
            .join("\n");
        let driver = cases
            .iter()
            .enumerate()
            .map(|(i, (_, name, ..))| {
                let fmt = if cases[i].3 == "%bool" {
                    format!(
                        "  {{ struct {n} m; {n}_init(&m, \"{inp}\"); printf(\"%s\\n\", m.accepted ? \"true\" : \"false\"); }}",
                        n = name,
                        inp = inputs[i]
                    )
                } else {
                    format!(
                        "  {{ struct {n} m; {n}_init(&m, \"{inp}\"); printf(\"%s\\n%d\\n\", m.accepted ? \"true\" : \"false\", m.return_value); }}",
                        n = name,
                        inp = inputs[i]
                    )
                };
                fmt
            })
            .collect::<Vec<_>>()
            .join("\n");
        let Some(lines) = c_run(&code, &driver, "core") else {
            return;
        };
        let mut i = 0;
        for (idx, (_, _, exp_acc, fmt)) in cases.iter().enumerate() {
            assert_eq!(&lines[i], exp_acc, "accepted case {idx}");
            if *fmt != "%bool" {
                assert_eq!(&lines[i + 1], expect_ret[idx], "return case {idx}");
                i += 2;
            } else {
                i += 1;
            }
        }
    }

    #[test]
    fn c_conditional_target() {
        let code = gen("@@fsm M(text: bytes, mode: int) : int = 0 { \
             /[01]/ -> ( $zero when self.mode == 0, $one when self.mode == 1 ) : -> $error \
             $zero: 0 $one: 1 $error: -1 }");
        let driver = "  for (int md = 0; md < 3; md++) { struct M m; M_init(&m, \"0\", md); printf(\"%d\\n\", m.return_value); }";
        let Some(lines) = c_run(&code, driver, "cond") else {
            return;
        };
        assert_eq!(lines, vec!["0", "1", "-1"]);
    }

    #[test]
    fn c_multi_match() {
        let code = gen("@@fsm M(text: bytes) : int = 0 { /[0-9]/ -> $num | 99 $num: 1 }");
        let driver = "  { struct M m; M_init(&m, \"5\"); printf(\"%d\\n\", m.return_value); }\n  { struct M m; M_init(&m, \"a\"); printf(\"%d\\n\", m.return_value); }";
        let Some(lines) = c_run(&code, driver, "mm") else {
            return;
        };
        assert_eq!(lines, vec!["1", "99"]);
    }

    #[test]
    fn c_embed_every_transition() {
        let code = gen(
            "@@fsm M(text: bytes) : int = 0 { /[0-9]+/ ${ tally() } self.count \
             actions: tally() { self.count = self.count + 1 } domain: count: int = 0 }",
        );
        let driver = "  struct M m; M_init(&m, \"123\"); printf(\"%d\\n\", m.return_value);";
        let Some(lines) = c_run(&code, driver, "emb") else {
            return;
        };
        assert_eq!(lines[0], "3");
    }

    /// FSM-TEST-603 — `%{...}` fires when the DFA leaves its last accepting
    /// state, capturing the end of the matched region (`last`), not the failing
    /// element. For "42x" the longest match is "42" (end = 2); for "abx" no
    /// accepting state is ever entered so `%{}` does not fire and `end_pos`
    /// keeps its default 0.
    #[test]
    fn c_embed_leave_final() {
        let code = gen("@@fsm M(text: bytes) : int = 0 { \
             /[0-9]+/ %{ self.end_pos = @@:cursor } self.end_pos \
             domain: end_pos: int = 0 }");
        let driver = "  { struct M m; M_init(&m, \"42x\"); printf(\"%d\\n\", m.return_value); }\n  { struct M m; M_init(&m, \"abx\"); printf(\"%d\\n\", m.return_value); }";
        let Some(lines) = c_run(&code, driver, "leave") else {
            return;
        };
        assert_eq!(lines, vec!["2", "0"]);
    }

    #[test]
    fn c_token_alphabet() {
        let code = gen("@@fsm M(toks: token) : bool = false { /IDENT LPAREN RPAREN/ true }");
        let driver = "  const char* a[] = {\"IDENT\",\"LPAREN\",\"RPAREN\"};\n  const char* b[] = {\"IDENT\",\"RPAREN\"};\n  const char* c[] = {\"IDENT\",\"WAT\"};\n  { struct M m; M_init(&m, a, 3); printf(\"%s\\n\", m.accepted?\"true\":\"false\"); }\n  { struct M m; M_init(&m, b, 2); printf(\"%s\\n\", m.accepted?\"true\":\"false\"); }\n  { struct M m; M_init(&m, c, 2); printf(\"%s\\n\", m.accepted?\"true\":\"false\"); }";
        let Some(lines) = c_run(&code, driver, "tok") else {
            return;
        };
        assert_eq!(lines, vec!["true", "false", "false"]);
    }

    #[test]
    fn c_mode_c_callout() {
        let inner = gen("@@fsm Digits(text: bytes) : int = 0 { /[0-9]+/ to_int(@@:matched) }");
        let outer = gen("@@fsm Outer(text: bytes) : int = 0 { $s: .d/@Digits/ $s.d.return_value }");
        let code = format!("{}\n{}", inner, outer);
        let driver = "  { struct Outer m; Outer_init(&m, \"42\"); printf(\"%s %d\\n\", m.accepted?\"true\":\"false\", m.return_value); }\n  { struct Outer m; Outer_init(&m, \"x\"); printf(\"%s %d\\n\", m.accepted?\"true\":\"false\", m.return_value); }";
        let Some(lines) = c_run(&code, driver, "modec") else {
            return;
        };
        assert_eq!(lines, vec!["true 42", "false 0"]);
    }

    #[test]
    fn c_mode_c_unlabeled() {
        // An unlabelled Mode C stage runs the inner recognizer for its cursor
        // advance only — no capture field, a scratch struct instead.
        let inner = gen("@@fsm Digits(text: bytes) : int = 0 { /[0-9]+/ to_int(@@:matched) }");
        let outer = gen("@@fsm Outer(text: bytes) : bool = false { $s: /@Digits/ true }");
        let code = format!("{}\n{}", inner, outer);
        let driver = "  { struct Outer m; Outer_init(&m, \"42\"); printf(\"%s\\n\", m.accepted?\"true\":\"false\"); }\n  { struct Outer m; Outer_init(&m, \"x\"); printf(\"%s\\n\", m.accepted?\"true\":\"false\"); }";
        let Some(lines) = c_run(&code, driver, "modec_unlabeled") else {
            return;
        };
        assert_eq!(lines, vec!["true", "false"]);
    }

    #[test]
    fn c_anchors() {
        let start = gen("@@fsm M(text: bytes) : bool = false { /^foo/ true }");
        let end = gen("@@fsm N(text: bytes) : bool = false { /[0-9]+$/ true }");
        let code = format!("{}\n{}", start, end);
        let driver = "  { struct M m; M_init(&m, \"foo\"); printf(\"%s\\n\", m.accepted?\"true\":\"false\"); }\n  { struct M m; M_init(&m, \"xfoo\"); printf(\"%s\\n\", m.accepted?\"true\":\"false\"); }\n  { struct N m; N_init(&m, \"123\"); printf(\"%s\\n\", m.accepted?\"true\":\"false\"); }\n  { struct N m; N_init(&m, \"123x\"); printf(\"%s\\n\", m.accepted?\"true\":\"false\"); }";
        let Some(lines) = c_run(&code, driver, "anc") else {
            return;
        };
        assert_eq!(lines, vec!["true", "false", "true", "false"]);
    }

    #[test]
    fn c_word_boundary() {
        let code = gen("@@fsm M(text: bytes) : bool = false { /\\bcat\\b/ true }");
        let driver = "  { struct M m; M_init(&m, \"cat\"); printf(\"%s\\n\", m.accepted?\"true\":\"false\"); }\n  { struct M m; M_init(&m, \"cats\"); printf(\"%s\\n\", m.accepted?\"true\":\"false\"); }";
        let Some(lines) = c_run(&code, driver, "wb") else {
            return;
        };
        assert_eq!(lines, vec!["true", "false"]);
    }

    /// §11.1 — a lazy quantifier matches via the Pike VM (leftmost-first
    /// match-end), not the greedy DFA. `/.*?,/` stops at the first comma
    /// ("ab,"); `/a*?b+/` over "aabbb" matches the whole run (cursor 5).
    #[test]
    fn c_lazy_quantifier() {
        let star = gen("@@fsm M(text: bytes) : bytes = \"\" { /.*?,/ @@:matched }");
        let plus = gen("@@fsm N(text: bytes) : int = 0 { /a*?b+/ @@:cursor }");
        let code = format!("{}\n{}", star, plus);
        let driver = "  { struct M m; M_init(&m, \"ab,cd,ef\"); printf(\"%s\\n\", m.return_value); }\n  { struct N m; N_init(&m, \"aabbb\"); printf(\"%d\\n\", m.return_value); }";
        let Some(lines) = c_run(&code, driver, "lazy") else {
            return;
        };
        assert_eq!(lines, vec!["ab,", "5"]);
    }

    /// An interior anchor (`a$b`) routes to the Pike VM; the `$` assert can
    /// never hold mid-string, so it rejects.
    #[test]
    fn c_interior_anchor() {
        let code = gen("@@fsm M(text: bytes) : bool = false { /a$b/ true }");
        let driver = "  { struct M m; M_init(&m, \"ab\"); printf(\"%s\\n\", m.accepted?\"true\":\"false\"); }";
        let Some(lines) = c_run(&code, driver, "ia_mid") else {
            return;
        };
        assert_eq!(lines, vec!["false"]);
    }
}
