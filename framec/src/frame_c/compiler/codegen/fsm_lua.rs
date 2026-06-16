//! Lua backend for `@@fsm` (RFC-0042, Phase 8).
//!
//! Generates a self-contained Lua "class" (a table + metatable) from a
//! validated `FsmDeclAst`. Lua is the first structurally-different target:
//! it has no classes (metatable-based OOP), is 1-indexed, and uses word
//! operators (`and`/`or`/`not`) and `~=` for inequality. The recognition
//! model is the same as the Python reference backend
//! ([`super::fsm_python`]) — per-stage minimal DFAs + a per-state dispatch
//! loop over mutable fields — kept **0-indexed in the algorithm** (cursor,
//! DFA state indices, positions), with `+1` applied only at the Lua
//! string/table access points (`string.byte(text, pos+1)`, `dfa[st+1]`,
//! `string.sub(text, cursor+1, r)`).
//!
//! The class is `<Name> = {}` with `<Name>.new(...)` returning the instance;
//! the observable result (§5.1) is the instance's `accepted`,
//! `return_value`, `cursor`, `reject_position` fields. The `bytes`/`char`
//! input is the source string (`string.byte`, `string.sub`); the `token`
//! input is an array (a `_slice` helper, `tokId`).
//!
//! # v0.1 scope
//!
//! Full parity with the Python reference backend: single-match and
//! multi-match (`|`) ordered-choice states, captures, bare-expression
//! returns, action blocks, declared `actions:` methods, all transition
//! forms, embedding actions, Mode C sub-fsm call-out, all three alphabets,
//! edge anchors, `\b`/`\B` word boundaries, interior anchors, and lazy
//! quantifiers (the latter three via the Pike VM with zero-width `Assert`s).
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

/// Generate Lua source implementing `decl`, or a reason it is outside the
/// v0.1 Lua cut.
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
    /// `Some` when the stage's regex contains a lazy quantifier (§11.1): a
    /// Pike program matched by the VM (`_pikeMatch`) instead of the DFA, for
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
                                                actions is not yet supported by the Lua backend"
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
                 Lua backend",
                regex
            )),
        }
    }

    fn matched_empty(&self) -> &'static str {
        match self.alphabet {
            Alphabet::Token => "{}",
            _ => "\"\"",
        }
    }

    /// The per-element read as an integer (0-indexed `pos`, +1 for Lua):
    /// code point via `string.byte` for byte/char, token id for tokens.
    fn element_read(&self) -> String {
        let inp = &self.decl.params[0].name;
        match self.alphabet {
            Alphabet::Token => format!("self:tokId(self.{}[pos + 1])", inp),
            _ => format!("string.byte(self.{}, pos + 1)", inp),
        }
    }

    /// Materialize the matched run `[cursor, end)` (0-indexed) — `string.sub`
    /// for byte/char, a `_slice` for tokens.
    fn matched_slice(&self, end: &str) -> String {
        let inp = &self.decl.params[0].name;
        match self.alphabet {
            Alphabet::Token => format!("self:_slice(self.{}, self.cursor, {})", inp, end),
            _ => format!("string.sub(self.{}, self.cursor + 1, {})", inp, end),
        }
    }

    fn emit(&self) -> Result<String, String> {
        let n = &self.decl.name;
        let mut out = String::new();
        out.push_str("-- Generated by framec — RFC-0042 @@fsm (Lua backend).\n\n");
        writeln!(out, "{} = {{}}", n).ok();
        writeln!(out, "{}.__index = {}\n", n, n).ok();
        self.emit_pike_tables(&mut out);
        self.emit_ctor(&mut out);
        self.emit_tok_id(&mut out);
        self.emit_slice(&mut out);
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
        Ok(out)
    }

    fn emit_ctor(&self, out: &mut String) {
        let n = &self.decl.name;
        let input = &self.decl.params[0].name;
        let params: Vec<&str> = self.decl.params.iter().map(|p| p.name.as_str()).collect();
        writeln!(out, "function {}.new({})", n, params.join(", ")).ok();
        writeln!(out, "  local self = setmetatable({{}}, {})", n).ok();
        out.push_str("  self.accepted = false\n");
        out.push_str("  self.reject_position = 0\n");
        out.push_str("  self.cursor = 0\n");
        writeln!(
            out,
            "  self.return_value = {}",
            lua_default(&self.decl.default_expr)
        )
        .ok();
        for p in &self.decl.params {
            writeln!(out, "  self.{} = {}", p.name, p.name).ok();
        }
        if let Some(domain) = &self.decl.domain {
            for v in &domain.vars {
                if &v.name == input {
                    continue;
                }
                writeln!(out, "  self.{} = {}", v.name, self.expr(&v.default)).ok();
            }
        }
        writeln!(out, "  self.matched = {}", self.matched_empty()).ok();
        out.push_str("  self.enter = 0\n");
        for f in self.capture_fields() {
            writeln!(out, "  self.{} = {}", f, self.matched_empty()).ok();
        }
        for (f, _) in self.mode_c_inst_fields() {
            writeln!(out, "  self.{} = nil", f).ok();
        }
        out.push_str("  self:run()\n");
        out.push_str("  if self.accepted then self.reject_position = 0 end\n");
        out.push_str("  return self\nend\n\n");
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
        let n = &self.decl.name;
        let mut entries: Vec<(&String, &u32)> = self.token_ids.iter().collect();
        entries.sort_by_key(|(_, id)| **id);
        let items: Vec<String> = entries
            .iter()
            .map(|(name, id)| format!("[{:?}] = {}", name, id))
            .collect();
        writeln!(out, "function {}:tokId(t)", n).ok();
        writeln!(out, "  local m = {{{}}}", items.join(", ")).ok();
        // `m[t] or -1` is safe: ids are >= 0 but 0 is truthy in Lua, so only
        // a missing key (`nil`) falls through to -1.
        out.push_str("  return m[t] or -1\nend\n\n");
    }

    /// Token alphabet only: a 0-indexed `[lo, hi)` array slice.
    fn emit_slice(&self, out: &mut String) {
        if self.alphabet != Alphabet::Token {
            return;
        }
        let n = &self.decl.name;
        writeln!(out, "function {}:_slice(arr, lo, hi)", n).ok();
        out.push_str("  local r = {}\n");
        out.push_str("  for i = lo + 1, hi do r[#r + 1] = arr[i] end\n");
        out.push_str("  return r\nend\n\n");
    }

    fn emit_dfa_matcher(&self, out: &mut String) {
        let n = &self.decl.name;
        let input = &self.decl.params[0].name;
        let read = self.element_read();
        // DFA state `st` is 0-indexed; Lua tables are 1-indexed, so each
        // access is `states[st + 1]` with `[1]` = transitions, `[2]` =
        // accepting. Transition targets are 0-indexed state numbers.
        writeln!(
            out,
            "function {n}:dfaMatch(states, start)\n\
             \x20 local st = start\n\
             \x20 local pos = self.cursor\n\
             \x20 local n = #self.{input}\n\
             \x20 local last = -1\n\
             \x20 if states[st + 1][2] then last = pos end\n\
             \x20 while pos < n do\n\
             \x20   local v = {read}\n\
             \x20   local nxt = -1\n\
             \x20   for _, tr in ipairs(states[st + 1][1]) do\n\
             \x20     if tr[1] <= v and v <= tr[2] then nxt = tr[3]; break end\n\
             \x20   end\n\
             \x20   if nxt < 0 then break end\n\
             \x20   st = nxt\n\
             \x20   pos = pos + 1\n\
             \x20   if states[st + 1][2] then last = pos end\n\
             \x20 end\n\
             \x20 return last\n\
             end\n\n",
            n = n,
            input = input,
            read = read
        )
        .ok();
    }

    /// Does any stage match via the Pike VM (a lazy quantifier, §11.1)?
    fn uses_pike(&self) -> bool {
        self.stage_dfas.iter().any(|d| d.program.is_some())
    }

    /// Emit the flat `_OPS_<i>`/`_RNG_<i>` Lua integer tables for each lazy
    /// stage (`fsm_regex::pike::encode`). These are plain 0-indexed integer
    /// arrays; Lua stores them 1-based as `{v1, v2, ...}` and the VM indexes
    /// with `[k + 1]`.
    fn emit_pike_tables(&self, out: &mut String) {
        let n = &self.decl.name;
        for (i, dfa) in self.stage_dfas.iter().enumerate() {
            if let Some(prog) = &dfa.program {
                let (ops, rng) = fsm_regex::pike::encode(prog);
                let word = fsm_regex::pike::program_word_table(prog, self.alphabet);
                writeln!(out, "{}._OPS_{} = {{{}}}", n, i, int_list(&ops)).ok();
                writeln!(out, "{}._RNG_{} = {{{}}}", n, i, int_list(&rng)).ok();
                writeln!(out, "{}._WORD_{} = {{{}}}", n, i, int_list(&word)).ok();
            }
        }
        if self.uses_pike() {
            out.push('\n');
        }
    }

    /// Pike VM (priority NFA simulation) for lazy-quantifier stages, over the
    /// flat `ops`/`rng` arrays (`fsm_regex::pike::encode`). Returns the end
    /// position of the highest-priority (leftmost-first) match from the cursor,
    /// or -1. `ops` is 4 ints per instruction `[op, a, b, _]`: 0 Char (a = pair
    /// index, b = pair count), 1 Split (a/b targets, a higher), 2 Jmp, 3 Match.
    ///
    /// The algorithm stays 0-indexed (`pc`, `pos`, opcodes); `+1` is applied
    /// only at each Lua table read (`ops[pc * 4 + 1]`, `rng[(rs + k) * 2 + 1]`)
    /// and the input read (`string.byte(self.text, pos + 1)`), mirroring the
    /// DFA matcher's convention. `seen` is a 1-based Lua table keyed by
    /// `pc + 1`; `lst` is a 1-based array appended with `table.insert`.
    fn emit_pike_matcher(&self, out: &mut String) {
        let n = &self.decl.name;
        let inp = &self.decl.params[0].name;
        let read = match self.alphabet {
            Alphabet::Token => format!("self:tokId(self.{}[pos + 1])", inp),
            _ => format!("string.byte(self.{}, pos + 1)", inp),
        };
        writeln!(out, "function {}:_pikeIsWord(p, word)", n).ok();
        writeln!(
            out,
            "  if p < 0 or p >= #self.{} then return false end",
            inp
        )
        .ok();
        writeln!(out, "  local v = string.byte(self.{}, p + 1)", inp).ok();
        out.push_str(
            "  for k = 0, #word / 2 - 1 do\n\
             \x20   if word[k * 2 + 1] <= v and v <= word[k * 2 + 1 + 1] then return true end\n\
             \x20 end\n\
             \x20 return false\n\
             end\n\n",
        );
        writeln!(out, "function {}:_pikeAssert(kind, pos, word)", n).ok();
        writeln!(out, "  local n = #self.{}", inp).ok();
        writeln!(out, "  if kind == 0 then return pos == 0 end").ok();
        writeln!(out, "  if kind == 1 then return pos == n end").ok();
        writeln!(
            out,
            "  if kind == 2 then return pos == 0 or string.byte(self.{}, pos) == 10 end",
            inp
        )
        .ok();
        writeln!(
            out,
            "  if kind == 3 then return pos == n or string.byte(self.{}, pos + 1) == 10 end",
            inp
        )
        .ok();
        out.push_str(
            "  if kind == 4 then return self:_pikeIsWord(pos - 1, word) ~= self:_pikeIsWord(pos, word) end\n\
             \x20 return self:_pikeIsWord(pos - 1, word) == self:_pikeIsWord(pos, word)\n\
             end\n\n",
        );
        writeln!(
            out,
            "function {}:_pikeAdd(ops, word, lst, seen, pc, pos)",
            n
        )
        .ok();
        out.push_str(
            "  if seen[pc + 1] then return end\n\
             \x20 seen[pc + 1] = true\n\
             \x20 local op = ops[pc * 4 + 1]\n\
             \x20 if op == 2 then\n\
             \x20   self:_pikeAdd(ops, word, lst, seen, ops[pc * 4 + 1 + 1], pos)\n\
             \x20 elseif op == 1 then\n\
             \x20   self:_pikeAdd(ops, word, lst, seen, ops[pc * 4 + 1 + 1], pos)\n\
             \x20   self:_pikeAdd(ops, word, lst, seen, ops[pc * 4 + 2 + 1], pos)\n\
             \x20 elseif op == 4 then\n\
             \x20   if self:_pikeAssert(ops[pc * 4 + 1 + 1], pos, word) then\n\
             \x20     self:_pikeAdd(ops, word, lst, seen, pc + 1, pos)\n\
             \x20   end\n\
             \x20 else\n\
             \x20   table.insert(lst, pc)\n\
             \x20 end\n\
             end\n\n",
        );
        writeln!(out, "function {}:_pikeMatch(ops, rng, word)", n).ok();
        writeln!(out, "  local n = #self.{}", inp).ok();
        out.push_str(
            "  local matched = -1\n\
             \x20 local clist = {}\n\
             \x20 local seen0 = {}\n\
             \x20 self:_pikeAdd(ops, word, clist, seen0, 0, self.cursor)\n\
             \x20 local pos = self.cursor\n\
             \x20 while true do\n\
             \x20   local nlist = {}\n\
             \x20   local nseen = {}\n\
             \x20   for _, pc in ipairs(clist) do\n\
             \x20     local op = ops[pc * 4 + 1]\n\
             \x20     if op == 0 then\n\
             \x20       if pos < n then\n",
        );
        writeln!(out, "          local v = {}", read).ok();
        out.push_str(
            "          local rs = ops[pc * 4 + 1 + 1]\n\
             \x20         local rc = ops[pc * 4 + 2 + 1]\n\
             \x20         for k = 0, rc - 1 do\n\
             \x20           if rng[(rs + k) * 2 + 1] <= v and v <= rng[(rs + k) * 2 + 1 + 1] then\n\
             \x20             self:_pikeAdd(ops, word, nlist, nseen, pc + 1, pos + 1)\n\
             \x20             break\n\
             \x20           end\n\
             \x20         end\n\
             \x20       end\n\
             \x20     elseif op == 3 then\n\
             \x20       matched = pos\n\
             \x20       break\n\
             \x20     end\n\
             \x20   end\n\
             \x20   if pos >= n then break end\n\
             \x20   pos = pos + 1\n\
             \x20   clist = nlist\n\
             \x20 end\n\
             \x20 return matched\n\
             end\n\n",
        );
    }

    fn emit_run(&self, out: &mut String) {
        let n = &self.decl.name;
        writeln!(out, "function {}:run()", n).ok();
        out.push_str("  local state = 0\n  while state >= 0 do\n");
        out.push_str("    local enter = self.enter\n    self.enter = 0\n");
        for (i, _) in self.decl.states.iter().enumerate() {
            let kw = if i == 0 { "if" } else { "elseif" };
            writeln!(out, "    {} state == {} then", kw, i).ok();
            writeln!(out, "      state = self:state{}(enter)", i).ok();
        }
        if self.decl.states.is_empty() {
            out.push_str("    if true then return\n");
        } else {
            out.push_str("    else\n      return\n");
        }
        out.push_str("    end\n  end\nend\n\n");
    }

    fn emit_state_methods(&self, out: &mut String) -> Result<(), String> {
        let mut sid = 0usize;
        for (i, st) in self.decl.states.iter().enumerate() {
            match st.matches.len() {
                0 => {
                    writeln!(
                        out,
                        "function {}:state{}(enter)\n  return -1\nend\n",
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
        writeln!(out, "function {}:state{}(enter)", self.decl.name, index).ok();
        for (idx, el) in m.elements.iter().enumerate() {
            writeln!(out, "  if enter <= {} then", idx).ok();
            self.emit_element(out, el, m, &state_label, "    ", sid)?;
            out.push_str("  end\n");
        }
        self.emit_success(out, m, "  ");
        out.push_str("end\n\n");
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
        writeln!(out, "function {}:state{}(enter)", self.decl.name, index).ok();
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
                             supported by the Lua backend"
                                .into(),
                        );
                    }
                    let my_sid = *sid;
                    *sid += 1;
                    if self.stage_dfas[my_sid].mode_c.is_some() {
                        return Err(
                            "a Mode C (`/@Fsm/`) stage as a `|` alternative selector is not yet \
                             supported by the Lua backend"
                                .into(),
                        );
                    }
                    let MatchElement::Stage(sel) = &m.elements[fs] else {
                        unreachable!("first_stage indexes a Stage element")
                    };
                    writeln!(out, "  local _r = {}", self.stage_match_call(my_sid)).ok();
                    self.emit_anchor_guards(out, my_sid, "  ");
                    out.push_str("  if _r >= 0 then\n");
                    writeln!(out, "    self.matched = {}", self.matched_slice("_r")).ok();
                    if let Some(lbl) = &sel.label {
                        if !state_label.is_empty() {
                            writeln!(
                                out,
                                "    self.{} = self.matched",
                                cap_field(&state_label, lbl)
                            )
                            .ok();
                        }
                    }
                    out.push_str("    self.cursor = _r\n");
                    out.push_str("    self.accepted = true\n");
                    for el in &m.elements[fs + 1..] {
                        self.emit_element(out, el, m, &state_label, "    ", sid)?;
                    }
                    self.emit_success(out, m, "    ");
                    out.push_str("  end\n");
                }
                None => {
                    out.push_str("  self.accepted = true\n");
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
            out.push_str("  self.accepted = false\n");
            out.push_str("  self.reject_position = self.cursor\n");
            out.push_str("  return -1\n");
        }
        out.push_str("end\n\n");
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
                    writeln!(out, "{}local _r = {}", ind, self.stage_match_call(my_sid)).ok();
                } else {
                    writeln!(out, "{}local _r = self:matchStage{}()", ind, my_sid).ok();
                }
                self.emit_anchor_guards(out, my_sid, ind);
                writeln!(out, "{}if _r < 0 then", ind).ok();
                self.emit_failure(out, m, &ind2);
                writeln!(out, "{}end", ind).ok();
                writeln!(out, "{}self.matched = {}", ind, self.matched_slice("_r")).ok();
                if let Some(lbl) = &stage.label {
                    if !state_label.is_empty() {
                        writeln!(
                            out,
                            "{}self.{} = self.matched",
                            ind,
                            cap_field(state_label, lbl)
                        )
                        .ok();
                    }
                }
                writeln!(out, "{}self.cursor = _r", ind).ok();
                writeln!(out, "{}self.accepted = true", ind).ok();
            }
            MatchElement::BareExpression { expr, .. } => {
                writeln!(out, "{}self.return_value = {}", ind, self.expr(expr)).ok();
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
            Alphabet::Token => format!("self:_slice(self.{}, self.cursor, #self.{})", input, input),
            _ => format!("string.sub(self.{}, self.cursor + 1)", input),
        };
        writeln!(out, "{}local {} = {}.new({})", ind, iv, inner, sub).ok();
        writeln!(out, "{}if not {}.accepted then", ind, iv).ok();
        self.emit_failure(out, m, ind2);
        writeln!(out, "{}end", ind).ok();
        let end = format!("self.cursor + {}.cursor", iv);
        writeln!(out, "{}self.matched = {}", ind, self.matched_slice(&end)).ok();
        if let Some(lbl) = &stage.label {
            if !state_label.is_empty() {
                writeln!(
                    out,
                    "{}self.{} = self.matched",
                    ind,
                    cap_field(state_label, lbl)
                )
                .ok();
                writeln!(
                    out,
                    "{}self.{} = {}",
                    ind,
                    cap_inst_field(state_label, lbl),
                    iv
                )
                .ok();
            }
        }
        writeln!(out, "{}self.cursor = self.cursor + {}.cursor", ind, iv).ok();
        writeln!(out, "{}self.accepted = true", ind).ok();
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
        let n = &self.decl.name;
        let input = &self.decl.params[0].name;
        let read = self.element_read();
        writeln!(out, "function {}:matchStage{}()", n, sid).ok();
        writeln!(out, "  local dfa = {}", self.dfa_literal(sid)).ok();
        writeln!(
            out,
            "  local entry = self.cursor\n\
             \x20 local st = {start}\n\
             \x20 local pos = entry\n\
             \x20 local n = #self.{input}\n\
             \x20 local last = -1\n\
             \x20 if dfa[st + 1][2] then last = pos end\n\
             \x20 self.cursor = pos",
            start = self.stage_dfas[sid].start,
            input = input
        )
        .ok();
        out.push('\n');
        out.push_str(&self.embed_body(stage, EmbeddingOp::Start, "  ")?);
        out.push_str("  local prev = dfa[st + 1][2]\n");
        writeln!(
            out,
            "  while pos < n do\n\
             \x20   local v = {read}\n\
             \x20   local nxt = -1\n\
             \x20   for _, tr in ipairs(dfa[st + 1][1]) do\n\
             \x20     if tr[1] <= v and v <= tr[2] then nxt = tr[3]; break end\n\
             \x20   end\n\
             \x20   if nxt < 0 then break end\n\
             \x20   st = nxt\n\
             \x20   pos = pos + 1\n\
             \x20   self.cursor = pos",
            read = read
        )
        .ok();
        out.push('\n');
        out.push_str(&self.embed_body(stage, EmbeddingOp::EveryTransition, "    ")?);
        out.push_str("    local now = dfa[st + 1][2]\n");
        let accept = self.embed_body(stage, EmbeddingOp::Accept, "      ")?;
        if !accept.is_empty() {
            out.push_str("    if now then\n");
            out.push_str(&accept);
            out.push_str("    end\n");
        }
        out.push_str("    if now then last = pos end\n    prev = now\n");
        out.push_str("  end\n");
        // `%{}` — left the last accepting state: a post-scan event firing once
        // when the longest match stops extending (failing element or EOF), with
        // `@@:cursor` at the end of the matched region (`last`), not the failing
        // element (§5.4 / FSM-TEST-603). `last < 0` ⇒ no accepting state was
        // entered, so there is nothing to leave.
        let leave = self.embed_body(stage, EmbeddingOp::LeaveAccept, "    ")?;
        if !leave.is_empty() {
            out.push_str("  if last >= 0 then\n    self.cursor = last\n");
            out.push_str(&leave);
            out.push_str("  end\n");
        }
        let eof = self.embed_body(stage, EmbeddingOp::Eof, "    ")?;
        if !eof.is_empty() {
            out.push_str("  if pos >= n and not prev then\n");
            out.push_str(&eof);
            out.push_str("  end\n");
        }
        out.push_str("  self.cursor = entry\n  return last\nend\n\n");
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
            writeln!(out, "{}if self.cursor ~= 0 then _r = -1 end", ind).ok();
        }
        if dfa.requires_end {
            writeln!(out, "{}if _r ~= #self.{} then _r = -1 end", ind, input).ok();
        }
        // Edge `\b`/`\B` (bytes): a boundary exists at position p iff the
        // word-ness of bytes p-1 and p differ. `\b` (Required) demands they
        // differ (so equal word-ness → reject, `==`); `\B` (Forbidden)
        // demands they match (so differing word-ness → reject, `~=`). The
        // start boundary is checked at `self.cursor`, the end at `_r` (guarded
        // by `_r >= 0` so a prior miss stays a miss).
        if let Some(kind) = dfa.start_boundary {
            let op = match kind {
                WordBoundary::Required => "==",
                WordBoundary::Forbidden => "~=",
            };
            writeln!(
                out,
                "{ind}if self:_iswordat(self.cursor - 1) {op} self:_iswordat(self.cursor) then _r = -1 end"
            )
            .ok();
        }
        if let Some(kind) = dfa.end_boundary {
            let op = match kind {
                WordBoundary::Required => "==",
                WordBoundary::Forbidden => "~=",
            };
            writeln!(
                out,
                "{ind}if _r >= 0 and (self:_iswordat(_r - 1) {op} self:_iswordat(_r)) then _r = -1 end"
            )
            .ok();
        }
    }

    /// Any stage carries an edge `\b`/`\B`, so the `_iswordat` helper is
    /// emitted.
    fn uses_word_boundary(&self) -> bool {
        self.stage_dfas
            .iter()
            .any(|d| d.start_boundary.is_some() || d.end_boundary.is_some())
    }

    /// `_iswordat(p)` — is the byte at 0-indexed input position `p` a word
    /// character (`[0-9A-Za-z_]`)? Mirrors `element_read`'s convention: the
    /// 0-indexed `p` reads `string.byte(self.text, p + 1)` (Lua strings are
    /// 1-indexed). Out-of-range positions are non-word, so a boundary at the
    /// input edge resolves correctly.
    fn emit_word_boundary_helper(&self, out: &mut String) {
        let n = &self.decl.name;
        let inp = &self.decl.params[0].name;
        writeln!(out, "function {}:_iswordat(p)", n).ok();
        writeln!(
            out,
            "  if p < 0 or p >= #self.{} then return false end",
            inp
        )
        .ok();
        writeln!(out, "  local b = string.byte(self.{}, p + 1)", inp).ok();
        out.push_str(
            "  return b == 95 or (b >= 48 and b <= 57) or (b >= 65 and b <= 90) or (b >= 97 and b <= 122)\nend\n\n",
        );
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
        writeln!(out, "{}self.accepted = false", ind).ok();
        writeln!(out, "{}self.reject_position = self.cursor", ind).ok();
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
                    writeln!(out, "{}if {} then", ind, self.expr(&alt.condition)).ok();
                    if let FsmTransitionTarget::Static { state, stage, .. } = &alt.target {
                        self.emit_goto(out, state, stage, &format!("{}  ", ind));
                    }
                    writeln!(out, "{}end", ind).ok();
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
                "{}error(\"transition to undeclared state ${}\")",
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
                    writeln!(out, "{}self.enter = {}", ind, entry).ok();
                }
                None => {
                    writeln!(
                        out,
                        "{}error(\"transition to undeclared stage ${}.{}\")",
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
                let mut out = format!("{}if {} then\n", ind, self.expr(&if_ast.condition));
                out.push_str(&self.stmt(&if_ast.then_branch, &inner)?);
                if let Some(else_b) = &if_ast.else_branch {
                    out.push_str(&format!("{}else\n", ind));
                    out.push_str(&self.stmt(else_b, &inner)?);
                }
                out.push_str(&format!("{}end\n", ind));
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
                "statement {:?} not supported in @@fsm action blocks by the Lua backend",
                std::mem::discriminant(other)
            )),
        }
    }

    fn emit_action_methods(&self, out: &mut String) -> Result<(), String> {
        let Some(block) = &self.decl.actions else {
            return Ok(());
        };
        for act in &block.actions {
            let params: Vec<&str> = act.params.iter().map(|p| p.name.as_str()).collect();
            writeln!(
                out,
                "function {}:{}({})",
                self.decl.name,
                act.name,
                params.join(", ")
            )
            .ok();
            let n = act.body.statements.len();
            let has_return = act.return_type.is_some();
            for (i, s) in act.body.statements.iter().enumerate() {
                use crate::frame_c::compiler::frame_ast::Statement;
                if i + 1 == n && has_return {
                    if let Statement::Expression(e) = s {
                        if !matches!(e.expr, Expression::Assign { .. }) {
                            writeln!(out, "  return {}", self.expr(&e.expr)).ok();
                            continue;
                        }
                    }
                }
                out.push_str(&self.stmt(s, "  ")?);
            }
            out.push_str("end\n\n");
        }
        Ok(())
    }

    /// The Lua table literal for a stage's DFA: a 1-indexed array of
    /// `{transitions, accept}` where `transitions` is a 1-indexed array of
    /// `{lo, hi, target}` (target a 0-indexed state number).
    fn dfa_literal(&self, sid: usize) -> String {
        let dfa = &self.stage_dfas[sid];
        let states: Vec<String> = dfa
            .states
            .iter()
            .map(|(trans, acc)| {
                let ts: Vec<String> = trans
                    .iter()
                    .map(|(lo, hi, tgt)| format!("{{{}, {}, {}}}", lo, hi, tgt))
                    .collect();
                format!("{{{{{}}}, {}}}", ts.join(", "), acc)
            })
            .collect();
        format!("{{{}}}", states.join(", "))
    }

    /// The non-embedding matcher invocation for a stage: the Pike VM
    /// (`_pikeMatch` over the named `_OPS_<sid>`/`_RNG_<sid>` tables) for a lazy
    /// stage, else the shared `dfaMatch` over the inline DFA literal.
    fn stage_match_call(&self, sid: usize) -> String {
        if self.stage_dfas[sid].program.is_some() {
            let n = &self.decl.name;
            format!("self:_pikeMatch({n}._OPS_{sid}, {n}._RNG_{sid}, {n}._WORD_{sid})")
        } else {
            format!(
                "self:dfaMatch({}, {})",
                self.dfa_literal(sid),
                self.stage_dfas[sid].start
            )
        }
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
                "@@:matched" => "self.matched".to_string(),
                "@@:cursor" => "self.cursor".to_string(),
                "@@:return" => "self.return_value".to_string(),
                _ => match name.strip_prefix('$').and_then(|c| c.split_once('.')) {
                    Some((state, label)) => format!("self.{}", cap_field(state, label)),
                    None => name.clone(),
                },
            },
            Expression::Binary { left, op, right } => {
                format!("({} {} {})", self.expr(left), binop(op), self.expr(right))
            }
            Expression::Unary { op, expr } => match op {
                UnaryOp::Not => format!("(not {})", self.expr(expr)),
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
                            return format!("self.{}.{}", cap_inst_field(state, label), field);
                        }
                    }
                    if name == "self" {
                        return format!("self.{}", field);
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
            "to_int" => format!("tonumber({})", a.join(", ")),
            "to_str" => format!("tostring({})", a.join(", ")),
            "len" => format!("#({})", a.join(", ")),
            _ => format!("self:{}({})", func, a.join(", ")),
        }
    }
}

/// Comma-joined `i64` list literal body (shared by the Pike `ops`/`rng`
/// integer tables; the caller wraps it in `{...}`).
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
        BinaryOp::Div => "//",
        BinaryOp::Mod => "%",
        BinaryOp::Eq => "==",
        BinaryOp::Ne => "~=",
        BinaryOp::Lt => "<",
        BinaryOp::Le => "<=",
        BinaryOp::Gt => ">",
        BinaryOp::Ge => ">=",
        BinaryOp::And => "and",
        BinaryOp::Or => "or",
        BinaryOp::BitAnd => "&",
        BinaryOp::BitOr => "|",
        BinaryOp::BitXor => "~",
    }
}

/// Map a raw default-value token to a Lua expression.
fn lua_default(raw: &str) -> String {
    match raw {
        "false" => "false".to_string(),
        "true" => "true".to_string(),
        "" => "nil".to_string(),
        s if s.starts_with('"') => s.to_string(),
        s => s.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frame_c::compiler::fsm_parser::parse_fsm_block;
    use std::process::Command;

    /// Run a Lua program (generated code + driver) via `lua`, returning
    /// stdout lines. `None` if `lua` is unavailable.
    fn lua_run(code: &str, driver: &str, tag: &str) -> Option<Vec<String>> {
        let prog = format!("{}\n{}\n", code, driver);
        let path = std::env::temp_dir().join(format!("framec_lua_{}.lua", tag));
        std::fs::write(&path, prog).ok()?;
        let out = match Command::new("lua").arg(&path).output() {
            Ok(o) => o,
            Err(_) => return None,
        };
        assert!(
            out.status.success(),
            "lua failed for {:?}:\n{}",
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

    fn run(src: &str, ctor: &str, tag: &str) -> Option<(String, String)> {
        let code = gen(src);
        let driver = format!("local m = {ctor}\nprint(m.accepted)\nprint(m.return_value)");
        let lines = lua_run(&code, &driver, tag)?;
        Some((lines[0].clone(), lines[1].clone()))
    }

    #[test]
    fn lua_smoke_bool() {
        let src = "@@fsm M(text: bytes) : bool = false { /a/ true }";
        let Some((acc, ret)) = run(src, "M.new(\"a\")", "smoke_a") else {
            return;
        };
        assert_eq!((acc.as_str(), ret.as_str()), ("true", "true"));
        assert_eq!(run(src, "M.new(\"b\")", "smoke_b").unwrap().0, "false");
    }

    #[test]
    fn lua_matched_to_int() {
        let src = "@@fsm M(text: bytes) : int = 0 { /[0-9]+/ to_int(@@:matched) }";
        let Some((acc, ret)) = run(src, "M.new(\"123\")", "tok_a") else {
            return;
        };
        assert_eq!((acc.as_str(), ret.as_str()), ("true", "123"));
        assert_eq!(run(src, "M.new(\"x\")", "tok_b").unwrap().0, "false");
    }

    #[test]
    fn lua_len_self_input() {
        let src = "@@fsm M(text: bytes) : int = 0 { /[0-9]+/ len(self.text) }";
        let Some((_, ret)) = run(src, "M.new(\"123\")", "len_a") else {
            return;
        };
        assert_eq!(ret, "3");
    }

    #[test]
    fn lua_stage_capture() {
        let src = "@@fsm M(text: bytes) : int = 0 { $s: .n/[0-9]+/ to_int($s.n) }";
        let Some((acc, ret)) = run(src, "M.new(\"42\")", "cap_a") else {
            return;
        };
        assert_eq!((acc.as_str(), ret.as_str()), ("true", "42"));
    }

    #[test]
    fn lua_action_block() {
        let src = "@@fsm M(text: bytes) : int = 0 { \
                   /[0-9]/ { self.count = self.count + 1 } self.count \
                   domain: count: int = 0 }";
        let Some((_, ret)) = run(src, "M.new(\"5\")", "act_a") else {
            return;
        };
        assert_eq!(ret, "1");
    }

    #[test]
    fn lua_declared_action() {
        let src = "@@fsm M(text: bytes) : int = 0 { \
                   /[0-9]+/ parse_int(@@:matched) \
                   actions: parse_int(s: bytes): int { to_int(s) } }";
        let Some((_, ret)) = run(src, "M.new(\"42\")", "decl_a") else {
            return;
        };
        assert_eq!(ret, "42");
    }

    #[test]
    fn lua_transitions_and_capture() {
        let src = "@@fsm M(text: bytes) : int = 0 { \
                   $0: /[a-z]/ -> $digits : -> $error \
                   $digits: .n/[0-9]+/ to_int($digits.n) \
                   $error: -1 }";
        let Some((acc, ret)) = run(src, "M.new(\"x42\")", "tr_a") else {
            return;
        };
        assert_eq!((acc.as_str(), ret.as_str()), ("true", "42"));
        assert_eq!(run(src, "M.new(\"X\")", "tr_b").unwrap().1, "-1");
    }

    #[test]
    fn lua_conditional_target() {
        let src = "@@fsm M(text: bytes, mode: int) : int = 0 { \
                   /[01]/ -> ( $zero when self.mode == 0, $one when self.mode == 1 ) : -> $error \
                   $zero: 0 \
                   $one: 1 \
                   $error: -1 }";
        let Some(z) = run(src, "M.new(\"0\", 0)", "cond_a") else {
            return;
        };
        assert_eq!(z.1, "0");
        assert_eq!(run(src, "M.new(\"1\", 1)", "cond_b").unwrap().1, "1");
        assert_eq!(run(src, "M.new(\"0\", 2)", "cond_c").unwrap().1, "-1");
    }

    #[test]
    fn lua_multi_match() {
        let code = gen("@@fsm M(text: bytes) : int = 0 { /[0-9]/ -> $num | 99 $num: 1 }");
        let driver = "for _, s in ipairs({\"5\", \"a\"}) do print(M.new(s).return_value) end";
        let Some(lines) = lua_run(&code, driver, "mm") else {
            return;
        };
        assert_eq!(lines, vec!["1", "99"]);
    }

    #[test]
    fn lua_embed_every_transition() {
        let code = gen(
            "@@fsm M(text: bytes) : int = 0 { /[0-9]+/ ${ tally() } self.count \
             actions: tally() { self.count = self.count + 1 } domain: count: int = 0 }",
        );
        let driver = "print(M.new(\"123\").return_value)";
        let Some(lines) = lua_run(&code, driver, "emb") else {
            return;
        };
        assert_eq!(lines[0], "3");
    }

    /// FSM-TEST-603 — `%{...}` fires when the DFA leaves its last accepting
    /// state, capturing the end of the matched region.
    #[test]
    fn lua_embed_leave_final() {
        let src = "@@fsm M(text: bytes) : int = 0 { \
                   /[0-9]+/ %{ self.end_pos = @@:cursor } self.end_pos \
                   domain: end_pos: int = 0 }";
        let Some((_, ret)) = run(src, "M.new(\"42x\")", "leave_a") else {
            return;
        };
        assert_eq!(ret, "2");
        assert_eq!(run(src, "M.new(\"abx\")", "leave_b").unwrap().1, "0");
    }

    #[test]
    fn lua_token_alphabet() {
        let code = gen("@@fsm M(toks: token) : bool = false { /IDENT LPAREN RPAREN/ true }");
        let driver = "for _, t in ipairs({{\"IDENT\",\"LPAREN\",\"RPAREN\"},{\"IDENT\",\"RPAREN\"},{\"IDENT\",\"WAT\"}}) do print(M.new(t).accepted) end";
        let Some(lines) = lua_run(&code, driver, "tok") else {
            return;
        };
        assert_eq!(lines, vec!["true", "false", "false"]);
    }

    #[test]
    fn lua_mode_c_callout() {
        let inner = gen("@@fsm Digits(text: bytes) : int = 0 { /[0-9]+/ to_int(@@:matched) }");
        let outer = gen("@@fsm Outer(text: bytes) : int = 0 { $s: .d/@Digits/ $s.d.return_value }");
        let code = format!("{}\n{}", inner, outer);
        let driver = "for _, s in ipairs({\"42\", \"x\"}) do local m = Outer.new(s); print(tostring(m.accepted) .. \" \" .. tostring(m.return_value)) end";
        let Some(lines) = lua_run(&code, driver, "modec") else {
            return;
        };
        assert_eq!(lines, vec!["true 42", "false 0"]);
    }

    #[test]
    fn lua_anchors() {
        let start = gen("@@fsm M(text: bytes) : bool = false { /^foo/ true }");
        let d1 = "for _, s in ipairs({\"foo\", \"xfoo\"}) do print(M.new(s).accepted) end";
        let Some(l1) = lua_run(&start, d1, "anc_s") else {
            return;
        };
        assert_eq!(l1, vec!["true", "false"]);
        let end = gen("@@fsm M(text: bytes) : bool = false { /[0-9]+$/ true }");
        let d2 = "for _, s in ipairs({\"123\", \"123x\"}) do print(M.new(s).accepted) end";
        let Some(l2) = lua_run(&end, d2, "anc_e") else {
            return;
        };
        assert_eq!(l2, vec!["true", "false"]);
    }

    /// Edge `\b` word boundaries (bytes): `/\bcat\b/` accepts "cat" (word
    /// boundary on both edges against input start/end) but rejects "cats"
    /// (no boundary between the matched `t` and the following word byte `s`).
    #[test]
    fn lua_word_boundary() {
        let src = "@@fsm M(text: bytes) : bool = false { /\\bcat\\b/ true }";
        let Some((acc, _)) = run(src, "M.new(\"cat\")", "wb_a") else {
            return;
        };
        assert_eq!(acc, "true");
        assert_eq!(run(src, "M.new(\"cats\")", "wb_b").unwrap().0, "false");
    }

    /// Lazy quantifiers (§11.1) via the Pike VM: `/.*?,/` matches up to the
    /// FIRST comma (greedy `/.*,/` would take the last), and the mixed
    /// `/a*?b+/` keeps `b+` greedy ("aabbb" → cursor 5, not 3).
    #[test]
    fn lua_lazy_quantifier() {
        let src = "@@fsm M(text: bytes) : bytes = \"\" { /.*?,/ @@:matched }";
        let Some((_, ret)) = run(src, "M.new(\"ab,cd,ef\")", "lza") else {
            return;
        };
        assert_eq!(ret, "ab,");
        let mixed = "@@fsm M(text: bytes) : int = 0 { /a*?b+/ @@:cursor }";
        assert_eq!(run(mixed, "M.new(\"aabbb\")", "lzb").unwrap().1, "5");
    }

    #[test]
    fn lua_interior_anchor_runs_on_pike_vm() {
        let src = "@@fsm M(text: bytes) : bool = false { /a$b/ true }";
        let decl = parse_fsm_block(src.as_bytes()).expect("parses");
        generate(&decl).expect("interior anchor compiles to a Pike program");
        let Some((acc, _)) = run(src, "M.new(\"ab\")", "ia_mid") else {
            return;
        };
        assert_eq!(acc, "false");
    }

    /// `\bcat\b` on `char` runs on the Pike VM with the word table.
    #[test]
    fn lua_word_boundary_runs_on_pike_vm() {
        let src = "@@fsm M(text: char) : bool = false { /\\bcat\\b/ true }";
        let Some((hit, _)) = run(src, "M.new(\"cat\")", "wb_hit") else {
            return;
        };
        assert_eq!(hit, "true");
        assert_eq!(run(src, "M.new(\"cats\")", "wb_miss").unwrap().0, "false");
    }
}
