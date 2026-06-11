//! GDScript backend for `@@fsm` (RFC-0042, Phase 8) — the final target.
//!
//! GDScript is indentation-based and dynamically typed, so the recognizer
//! follows the Python reference model ([`super::fsm_python`]) closely: a
//! per-state dispatch loop over instance fields, with each stage's `/regex/`
//! compiled to a minimal DFA carried as nested-array data. The recognizer is
//! emitted as an **inner class** (`class <Name>:`) so multiple recognizers —
//! e.g. a Mode C outer + inner — compose in one `.gd` script; the
//! "constructor" is `<Name>.new(...)` and the observable result (§5.1) is the
//! instance's `accepted`, `return_value`, `cursor`, `reject_position`.
//!
//! GDScript differs from Python in three load-bearing ways:
//!
//! - **Every member must be declared** with `var` at class scope (you cannot
//!   create fields on the fly), and **every local needs `var` on first use**.
//!   Because GDScript forbids redeclaring a local in the same function, the
//!   per-stage match result uses an sid-unique name `_r<sid>` (likewise
//!   `_inner<sid>` for Mode C) so sequential stages / `|` alternatives don't
//!   collide.
//! - **A `String` is not sliced with `[a:b]`**: a code point is
//!   `text.unicode_at(pos)`, the matched run is `text.substr(cursor, len)`
//!   (bytes/char) or `toks.slice(cursor, end)` (token). `len()` unifies the
//!   length of both. Booleans are lowercase `true`/`false`; `null` is `null`;
//!   integer `/` is integer division.
//! - **Inner fsms are referenced by their class name** (`<Inner>.new(...)`),
//!   so they must be emitted before the outer (inner-first), as for C/C++.
//!
//! # v0.1 scope
//!
//! Full parity with the Python reference backend: single-match and
//! multi-match (`|`) ordered-choice states, captures, bare-expression
//! returns, action blocks, declared `actions:` methods, all transition
//! forms, embedding actions, Mode C sub-fsm call-out, all three alphabets,
//! position anchors, and edge `\b`/`\B` word boundaries (bytes alphabet).
//! Not yet handled (clear `Unsupported` error): mid-pattern anchors and
//! `\b`/`\B` on char/token, a Mode C stage as a `|` selector, and a `|`
//! alternative with elements before its first stage.

use crate::frame_c::compiler::frame_ast::{
    BinaryOp, BlockAst, EmbeddingOp, Expression, FsmDeclAst, FsmStateAst, FsmTransitionTarget,
    Literal, MatchAst, MatchElement, StageAst, Statement, Type, UnaryOp,
};
use crate::frame_c::compiler::fsm_regex::{
    self, pike::Program, size_check::DEFAULT_MAX_DFA_STATES, subset::DfaLabel, Alphabet,
    CompileError, WordBoundary,
};
use std::collections::HashMap;
use std::fmt::Write;

/// Generate GDScript source implementing `decl`, or a reason it is outside
/// the v0.1 GDScript cut.
pub fn generate(decl: &FsmDeclAst) -> Result<String, String> {
    Generator::new(decl)?.emit()
}

/// Per-stage compiled DFA, flattened to the data the emitter needs.
struct StageDfa {
    states: Vec<(Vec<(u32, u32, usize)>, bool)>,
    start: usize,
    requires_start: bool,
    requires_end: bool,
    start_boundary: Option<WordBoundary>,
    end_boundary: Option<WordBoundary>,
    /// `Some` when the stage's regex contains a lazy quantifier (§11.1): a Pike
    /// program matched by the VM (`_pike_match`) instead of the DFA, for
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
    label_to_index: HashMap<String, usize>,
    stage_entry: HashMap<(String, String), usize>,
    token_ids: HashMap<String, u32>,
    stage_dfas: Vec<StageDfa>,
}

impl<'a> Generator<'a> {
    fn new(decl: &'a FsmDeclAst) -> Result<Self, String> {
        let alphabet = match decl.params.first().map(|p| &p.param_type) {
            Some(Type::Custom(t)) if t == "char" => Alphabet::Char,
            Some(Type::Custom(t)) if t == "token" => Alphabet::Token,
            _ => Alphabet::Bytes,
        };
        let mut label_to_index = HashMap::new();
        let mut stage_entry = HashMap::new();
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
            token_ids: HashMap::new(),
            stage_dfas: Vec::new(),
        };
        g.compile_stage_dfas()?;
        Ok(g)
    }

    fn compile_stage_dfas(&mut self) -> Result<(), String> {
        let mut token_ids = std::mem::take(&mut self.token_ids);
        let alphabet = self.alphabet;
        let mut dfas = Vec::new();
        for st in &self.decl.states {
            for m in &st.matches {
                for el in &m.elements {
                    if let MatchElement::Stage(stage) = el {
                        if let Some(inner) = mode_c_inner(&stage.regex) {
                            dfas.push(StageDfa {
                                states: Vec::new(),
                                start: 0,
                                requires_start: false,
                                requires_end: false,
                                start_boundary: None,
                                end_boundary: None,
                                program: None,
                                mode_c: Some(inner.to_string()),
                            });
                        } else {
                            let dfa = Self::compile_one(alphabet, &stage.regex, &mut token_ids)?;
                            // A lazy quantifier matches via the Pike VM, which has no
                            // per-element scan for embedding actions to hook into
                            // (§3.5.5/§11.1). Reject the combination rather than
                            // silently giving greedy semantics.
                            if dfa.program.is_some() && !stage.embedding_actions.is_empty() {
                                self.token_ids = token_ids;
                                return Err("a lazy quantifier in a stage with embedding \
                                            actions is not yet supported by the GDScript backend"
                                    .to_string());
                            }
                            dfas.push(dfa);
                        }
                    }
                }
            }
        }
        self.stage_dfas = dfas;
        self.token_ids = token_ids;
        Ok(())
    }

    fn compile_one(
        alphabet: Alphabet,
        regex: &str,
        token_ids: &mut HashMap<String, u32>,
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
                 GDScript backend",
                regex
            )),
        }
    }

    fn input_field(&self) -> &str {
        self.decl
            .params
            .first()
            .map(|p| p.name.as_str())
            .unwrap_or("text")
    }

    /// The per-element read: a byte/char maps to its code point via
    /// `unicode_at`; a token maps to its small integer id (`-1` if unknown).
    fn element_read(&self) -> String {
        let inp = self.input_field();
        match self.alphabet {
            Alphabet::Token => format!("self._tok_id(self.{}[pos])", inp),
            _ => format!("self.{}.unicode_at(pos)", inp),
        }
    }

    /// The matched run `[cursor, end)`: `substr` for a string, `slice` for the
    /// token array.
    fn matched_expr(&self, end: &str) -> String {
        let inp = self.input_field();
        match self.alphabet {
            Alphabet::Token => format!("self.{}.slice(self.cursor, {})", inp, end),
            _ => format!("self.{}.substr(self.cursor, ({}) - self.cursor)", inp, end),
        }
    }

    fn emit(&self) -> Result<String, String> {
        let mut out = String::new();
        out.push_str("# Generated by framec — RFC-0042 @@fsm (GDScript backend).\n\n");
        writeln!(out, "class {}:", self.decl.name).ok();
        self.emit_member_decls(&mut out);
        self.emit_ctor(&mut out);
        self.emit_token_lookup(&mut out);
        self.emit_word_boundary(&mut out);
        self.emit_dfa_matcher(&mut out);
        if self.uses_pike() {
            self.emit_pike_matcher(&mut out);
        }
        self.emit_embed_matchers(&mut out)?;
        self.emit_run(&mut out);
        self.emit_state_methods(&mut out)?;
        self.emit_action_methods(&mut out)?;
        Ok(out)
    }

    /// All instance fields must be declared (GDScript has no on-the-fly
    /// attributes): the DFA tables, the token map, the observable result,
    /// the auto-promoted parameters + domain, and recognition scratch.
    fn emit_member_decls(&self, out: &mut String) {
        for (i, dfa) in self.stage_dfas.iter().enumerate() {
            // A lazy stage carries a Pike program (flat `_OPS_`/`_RNG_` int
            // arrays) instead of a DFA table; only its program is referenced.
            if let Some(prog) = &dfa.program {
                let (ops, rng) = fsm_regex::pike::encode(prog);
                writeln!(out, "\tvar _OPS_{} = [{}]", i, int_list(&ops)).ok();
                writeln!(out, "\tvar _RNG_{} = [{}]", i, int_list(&rng)).ok();
            } else {
                writeln!(out, "\tvar _DFA_{} = {}", i, dfa_literal(dfa)).ok();
            }
        }
        if self.alphabet == Alphabet::Token {
            let mut entries: Vec<(&String, &u32)> = self.token_ids.iter().collect();
            entries.sort_by_key(|(_, id)| **id);
            let items: Vec<String> = entries
                .iter()
                .map(|(name, id)| format!("{:?}: {}", name, id))
                .collect();
            writeln!(out, "\tvar _TOK_IDS = {{{}}}", items.join(", ")).ok();
        }
        out.push_str("\tvar accepted = false\n");
        out.push_str("\tvar reject_position = 0\n");
        out.push_str("\tvar cursor = 0\n");
        out.push_str("\tvar return_value\n");
        let mut seen = std::collections::HashSet::new();
        for p in &self.decl.params {
            seen.insert(p.name.clone());
            writeln!(out, "\tvar {}", p.name).ok();
        }
        if let Some(domain) = &self.decl.domain {
            for v in &domain.vars {
                if seen.insert(v.name.clone()) {
                    writeln!(out, "\tvar {}", v.name).ok();
                }
            }
        }
        out.push_str("\tvar _matched\n");
        out.push_str("\tvar _cap = {}\n");
        out.push_str("\tvar _cap_inst = {}\n");
        out.push_str("\tvar _enter = 0\n");
    }

    fn emit_ctor(&self, out: &mut String) {
        let mut sig = String::new();
        for (i, p) in self.decl.params.iter().enumerate() {
            if i > 0 {
                sig.push_str(", ");
            }
            match (i, &p.default) {
                (0, _) => sig.push_str(&p.name),
                (_, Some(d)) => write!(sig, "{} = {}", p.name, gd_default(d))
                    .ok()
                    .map(|_| ())
                    .unwrap_or(()),
                (_, None) => sig.push_str(&p.name),
            }
        }
        writeln!(out, "\tfunc _init({}):", sig).ok();
        for p in &self.decl.params {
            writeln!(out, "\t\tself.{} = {}", p.name, p.name).ok();
        }
        if let Some(domain) = &self.decl.domain {
            for v in &domain.vars {
                writeln!(out, "\t\tself.{} = {}", v.name, expr_to_gd(&v.default)).ok();
            }
        }
        writeln!(
            out,
            "\t\tself.return_value = {}",
            gd_default(&self.decl.default_expr)
        )
        .ok();
        let empty = if self.alphabet == Alphabet::Token {
            "[]"
        } else {
            "\"\""
        };
        writeln!(out, "\t\tself._matched = {}", empty).ok();
        out.push_str("\t\tself._cap = {}\n\t\tself._cap_inst = {}\n");
        out.push_str("\t\tself.cursor = 0\n\t\tself.accepted = false\n\t\tself.reject_position = 0\n\t\tself._enter = 0\n");
        out.push_str("\t\tself._run()\n\t\tif self.accepted:\n\t\t\tself.reject_position = 0\n");
    }

    fn emit_token_lookup(&self, out: &mut String) {
        if self.alphabet != Alphabet::Token {
            return;
        }
        out.push_str("\tfunc _tok_id(t):\n\t\treturn self._TOK_IDS.get(t, -1)\n");
    }

    /// Any stage in this fsm carries an edge `\b`/`\B`.
    fn uses_word_boundary(&self) -> bool {
        self.stage_dfas
            .iter()
            .any(|d| d.start_boundary.is_some() || d.end_boundary.is_some())
    }

    /// `_iswordat(p)` — is the byte at `p` a word byte `[0-9A-Za-z_]`?
    /// Out-of-bounds (`p < 0` or `p >= len`) is non-word. Bytes only, so the
    /// code unit from `unicode_at` is the byte value. Emitted only when some
    /// stage needs a boundary check.
    fn emit_word_boundary(&self, out: &mut String) {
        if !self.uses_word_boundary() {
            return;
        }
        writeln!(
            out,
            "\tfunc _iswordat(p: int) -> bool:\n\
             \t\tif p < 0 or p >= len(self.{inp}):\n\
             \t\t\treturn false\n\
             \t\tvar c = self.{inp}.unicode_at(p)\n\
             \t\treturn (c >= 48 and c <= 57) or (c >= 65 and c <= 90) or (c >= 97 and c <= 122) or c == 95",
            inp = self.input_field()
        )
        .ok();
    }

    fn emit_dfa_matcher(&self, out: &mut String) {
        writeln!(
            out,
            "\tfunc _dfa_match(dfa):\n\
             \t\tvar states = dfa[0]\n\
             \t\tvar st = dfa[1]\n\
             \t\tvar pos = self.cursor\n\
             \t\tvar n = len(self.{inp})\n\
             \t\tvar last = pos if states[st][1] else -1\n\
             \t\twhile pos < n:\n\
             \t\t\tvar v = {read}\n\
             \t\t\tvar nxt = -1\n\
             \t\t\tfor tr in states[st][0]:\n\
             \t\t\t\tif tr[0] <= v and v <= tr[1]:\n\
             \t\t\t\t\tnxt = tr[2]\n\
             \t\t\t\t\tbreak\n\
             \t\t\tif nxt < 0:\n\
             \t\t\t\tbreak\n\
             \t\t\tst = nxt\n\
             \t\t\tpos += 1\n\
             \t\t\tif states[st][1]:\n\
             \t\t\t\tlast = pos\n\
             \t\treturn last\n",
            inp = self.input_field(),
            read = self.element_read()
        )
        .ok();
    }

    /// Does any stage match via the Pike VM (a lazy quantifier, §11.1)?
    fn uses_pike(&self) -> bool {
        self.stage_dfas.iter().any(|d| d.program.is_some())
    }

    /// The matcher invocation for a stage: the Pike VM for a lazy stage, the
    /// embedding-aware matcher when the stage carries embedding actions, else
    /// the shared `_dfa_match`.
    fn stage_call(&self, stage: &StageAst, sid: usize) -> String {
        if self.stage_dfas[sid].program.is_some() {
            format!("self._pike_match(self._OPS_{sid}, self._RNG_{sid})")
        } else if stage.embedding_actions.is_empty() {
            format!("self._dfa_match(self._DFA_{sid})")
        } else {
            format!("self._match_stage_{sid}()")
        }
    }

    /// Pike VM (priority NFA simulation) for lazy-quantifier stages, over the
    /// flat `ops`/`rng` int arrays (`fsm_regex::pike::encode`). Returns the end
    /// position of the highest-priority (leftmost-first) match from the cursor,
    /// or -1. `ops` is 4 ints per instruction `[op, a, b, _]`: 0 Char (a = pair
    /// index, b = pair count), 1 Split (a/b targets, a higher), 2 Jmp, 3 Match.
    fn emit_pike_matcher(&self, out: &mut String) {
        // Lazy is bytes/char only (token gated out by the engine), so the
        // element read is always `self.<input>.unicode_at(pos)`.
        let read = self.element_read();
        writeln!(
            out,
            "\tfunc _pike_add(ops, lst, seen, pc):\n\
             \t\tif seen[pc]:\n\
             \t\t\treturn\n\
             \t\tseen[pc] = true\n\
             \t\tvar op = ops[pc * 4]\n\
             \t\tif op == 2:\n\
             \t\t\tself._pike_add(ops, lst, seen, ops[pc * 4 + 1])\n\
             \t\telif op == 1:\n\
             \t\t\tself._pike_add(ops, lst, seen, ops[pc * 4 + 1])\n\
             \t\t\tself._pike_add(ops, lst, seen, ops[pc * 4 + 2])\n\
             \t\telse:\n\
             \t\t\tlst.append(pc)"
        )
        .ok();
        writeln!(
            out,
            "\tfunc _pike_match(ops, rng) -> int:\n\
             \t\tvar n = len(self.{inp})\n\
             \t\tvar ninst = ops.size() / 4\n\
             \t\tvar matched = -1\n\
             \t\tvar clist = []\n\
             \t\tvar cseen = []\n\
             \t\tfor i in range(ninst):\n\
             \t\t\tcseen.append(false)\n\
             \t\tself._pike_add(ops, clist, cseen, 0)\n\
             \t\tvar pos = self.cursor\n\
             \t\twhile true:\n\
             \t\t\tvar nlist = []\n\
             \t\t\tvar nseen = []\n\
             \t\t\tfor i in range(ninst):\n\
             \t\t\t\tnseen.append(false)\n\
             \t\t\tfor pc in clist:\n\
             \t\t\t\tvar op = ops[pc * 4]\n\
             \t\t\t\tif op == 0:\n\
             \t\t\t\t\tif pos < n:\n\
             \t\t\t\t\t\tvar v = {read}\n\
             \t\t\t\t\t\tvar rs = ops[pc * 4 + 1]\n\
             \t\t\t\t\t\tvar rc = ops[pc * 4 + 2]\n\
             \t\t\t\t\t\tfor k in range(rc):\n\
             \t\t\t\t\t\t\tif rng[(rs + k) * 2] <= v and v <= rng[(rs + k) * 2 + 1]:\n\
             \t\t\t\t\t\t\t\tself._pike_add(ops, nlist, nseen, pc + 1)\n\
             \t\t\t\t\t\t\t\tbreak\n\
             \t\t\t\telif op == 3:\n\
             \t\t\t\t\tmatched = pos\n\
             \t\t\t\t\tbreak\n\
             \t\t\tif pos >= n:\n\
             \t\t\t\tbreak\n\
             \t\t\tpos += 1\n\
             \t\t\tclist = nlist\n\
             \t\treturn matched\n",
            inp = self.input_field(),
            read = read
        )
        .ok();
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
        writeln!(out, "\tfunc _match_stage_{}():", sid).ok();
        writeln!(out, "\t\tvar states = self._DFA_{}[0]", sid).ok();
        writeln!(
            out,
            "\t\tvar _entry = self.cursor\n\
             \t\tvar st = self._DFA_{sid}[1]\n\
             \t\tvar pos = _entry\n\
             \t\tvar n = len(self.{inp})\n\
             \t\tvar last = pos if states[st][1] else -1\n\
             \t\tself.cursor = pos",
            sid = sid,
            inp = self.input_field()
        )
        .ok();
        out.push_str(&self.embed_bodies(stage, EmbeddingOp::Start, "\t\t")?);
        writeln!(out, "\t\tvar prev = states[st][1]").ok();
        writeln!(
            out,
            "\t\twhile pos < n:\n\
             \t\t\tvar v = {read}\n\
             \t\t\tvar nxt = -1\n\
             \t\t\tfor tr in states[st][0]:\n\
             \t\t\t\tif tr[0] <= v and v <= tr[1]:\n\
             \t\t\t\t\tnxt = tr[2]\n\
             \t\t\t\t\tbreak\n\
             \t\t\tif nxt < 0:\n\
             \t\t\t\tbreak\n\
             \t\t\tst = nxt\n\
             \t\t\tpos += 1\n\
             \t\t\tself.cursor = pos",
            read = self.element_read()
        )
        .ok();
        out.push_str(&self.embed_bodies(stage, EmbeddingOp::EveryTransition, "\t\t\t")?);
        out.push_str("\t\t\tvar _now = states[st][1]\n");
        let accept = self.embed_bodies(stage, EmbeddingOp::Accept, "\t\t\t\t")?;
        if !accept.is_empty() {
            out.push_str("\t\t\tif _now:\n");
            out.push_str(&accept);
        }
        out.push_str("\t\t\tif _now:\n\t\t\t\tlast = pos\n\t\t\tprev = _now\n");
        // `%{}` — left the last accepting state: a post-scan event firing once
        // when the longest match stops extending (failing element or EOF), with
        // `@@:cursor` at the end of the matched region (`last`), not the failing
        // element (§5.4 / FSM-TEST-603). `last < 0` ⇒ no accepting state was
        // entered, so there is nothing to leave.
        let leave = self.embed_bodies(stage, EmbeddingOp::LeaveAccept, "\t\t\t")?;
        if !leave.is_empty() {
            out.push_str("\t\tif last >= 0:\n\t\t\tself.cursor = last\n");
            out.push_str(&leave);
        }
        let eof = self.embed_bodies(stage, EmbeddingOp::Eof, "\t\t\t")?;
        if !eof.is_empty() {
            out.push_str("\t\tif pos >= n and not prev:\n");
            out.push_str(&eof);
        }
        out.push_str("\t\tself.cursor = _entry\n\t\treturn last\n");
        Ok(())
    }

    fn embed_bodies(&self, stage: &StageAst, op: EmbeddingOp, ind: &str) -> Result<String, String> {
        let mut s = String::new();
        for ea in &stage.embedding_actions {
            if ea.op == op {
                for st in &ea.body.statements {
                    s.push_str(&stmt_to_gd(st, ind)?);
                }
            }
        }
        Ok(s)
    }

    fn emit_run(&self, out: &mut String) {
        out.push_str(
            "\tfunc _run():\n\t\tvar state = 0\n\t\twhile state >= 0:\n\
             \t\t\tvar _enter = self._enter\n\t\t\tself._enter = 0\n",
        );
        for i in 0..self.decl.states.len() {
            let kw = if i == 0 { "if" } else { "elif" };
            writeln!(
                out,
                "\t\t\t{} state == {}:\n\t\t\t\tstate = self._state_{}(_enter)",
                kw, i, i
            )
            .ok();
        }
        out.push_str("\t\t\telse:\n\t\t\t\treturn\n");
    }

    fn emit_state_methods(&self, out: &mut String) -> Result<(), String> {
        let mut sid = 0usize;
        for (i, st) in self.decl.states.iter().enumerate() {
            match st.matches.len() {
                0 => {
                    writeln!(out, "\tfunc _state_{}(_enter):\n\t\treturn -1", i).ok();
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
        writeln!(out, "\tfunc _state_{}(_enter):", index).ok();
        for (idx, el) in m.elements.iter().enumerate() {
            writeln!(out, "\t\tif _enter <= {}:", idx).ok();
            self.emit_element(out, el, m, &state_label, "\t\t\t", sid)?;
        }
        self.emit_success(out, m, "\t\t")?;
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
        writeln!(out, "\tfunc _state_{}(_enter):", index).ok();
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
                             supported by the GDScript backend"
                                .into(),
                        );
                    }
                    let my_sid = *sid;
                    *sid += 1;
                    if self.stage_dfas[my_sid].mode_c.is_some() {
                        return Err(
                            "a Mode C (`/@Fsm/`) stage as a `|` alternative selector is not yet \
                             supported by the GDScript backend"
                                .into(),
                        );
                    }
                    let MatchElement::Stage(sel) = &m.elements[fs] else {
                        unreachable!("first_stage indexes a Stage element")
                    };
                    writeln!(
                        out,
                        "\t\tvar _r{} = {}",
                        my_sid,
                        self.stage_call(sel, my_sid)
                    )
                    .ok();
                    self.emit_anchor_guards(out, my_sid, "\t\t");
                    writeln!(out, "\t\tif _r{} >= 0:", my_sid).ok();
                    writeln!(
                        out,
                        "\t\t\tself._matched = {}",
                        self.matched_expr(&format!("_r{}", my_sid))
                    )
                    .ok();
                    if let Some(slabel) = &sel.label {
                        writeln!(
                            out,
                            "\t\t\tself._cap[{:?}] = self._matched",
                            format!("{}.{}", state_label, slabel)
                        )
                        .ok();
                    }
                    writeln!(out, "\t\t\tself.cursor = _r{}", my_sid).ok();
                    out.push_str("\t\t\tself.accepted = true\n");
                    for el in &m.elements[fs + 1..] {
                        self.emit_element(out, el, m, &state_label, "\t\t\t", sid)?;
                    }
                    self.emit_success(out, m, "\t\t\t")?;
                }
                None => {
                    out.push_str("\t\tself.accepted = true\n");
                    for el in &m.elements {
                        self.emit_element(out, el, m, &state_label, "\t\t", sid)?;
                    }
                    self.emit_success(out, m, "\t\t")?;
                }
            }
        }
        out.push_str(
            "\t\tself.accepted = false\n\t\tself.reject_position = self.cursor\n\t\treturn -1\n",
        );
        Ok(())
    }

    fn emit_anchor_guards(&self, out: &mut String, sid: usize, ind: &str) {
        let dfa = &self.stage_dfas[sid];
        if dfa.requires_start {
            writeln!(out, "{ind}if self.cursor != 0:\n{ind}\t_r{} = -1", sid).ok();
        }
        if dfa.requires_end {
            writeln!(
                out,
                "{ind}if _r{} != len(self.{}):\n{ind}\t_r{} = -1",
                sid,
                self.input_field(),
                sid
            )
            .ok();
        }
        // Word-boundary guards (§6.6). A boundary exists at position `p` iff
        // the word-class of byte `p-1` differs from byte `p` (OOB ⇒ non-word).
        // `\b` (Required) demands the sides differ, so `==` (no boundary) ⇒
        // reject; `\B` (Forbidden) demands they match, so `!=` ⇒ reject. The
        // start side tests the match start (`self.cursor`); the end side tests
        // `_r` (the match end), guarded by `_r >= 0` so a prior failure stays
        // rejected.
        if let Some(wb) = dfa.start_boundary {
            let op = boundary_op(wb);
            writeln!(
                out,
                "{ind}if self._iswordat(self.cursor - 1) {op} self._iswordat(self.cursor):\n\
                 {ind}\t_r{} = -1",
                sid
            )
            .ok();
        }
        if let Some(wb) = dfa.end_boundary {
            let op = boundary_op(wb);
            writeln!(
                out,
                "{ind}if _r{sid} >= 0 and self._iswordat(_r{sid} - 1) {op} self._iswordat(_r{sid}):\n\
                 {ind}\t_r{sid} = -1",
                sid = sid
            )
            .ok();
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
        let ind2 = format!("{}\t", ind);
        match el {
            MatchElement::Stage(stage) => {
                let my_sid = *sid;
                *sid += 1;
                if let Some(inner) = self.stage_dfas[my_sid].mode_c.clone() {
                    self.emit_mode_c(out, &inner, stage, m, state_label, my_sid, ind, &ind2)?;
                    return Ok(());
                }
                writeln!(
                    out,
                    "{}var _r{} = {}",
                    ind,
                    my_sid,
                    self.stage_call(stage, my_sid)
                )
                .ok();
                self.emit_anchor_guards(out, my_sid, ind);
                writeln!(out, "{}if _r{} < 0:", ind, my_sid).ok();
                self.emit_failure(out, m, &ind2)?;
                writeln!(
                    out,
                    "{}self._matched = {}",
                    ind,
                    self.matched_expr(&format!("_r{}", my_sid))
                )
                .ok();
                if let Some(slabel) = &stage.label {
                    writeln!(
                        out,
                        "{}self._cap[{:?}] = self._matched",
                        ind,
                        format!("{}.{}", state_label, slabel)
                    )
                    .ok();
                }
                writeln!(out, "{}self.cursor = _r{}", ind, my_sid).ok();
                writeln!(out, "{}self.accepted = true", ind).ok();
            }
            MatchElement::BareExpression { expr, .. } => {
                writeln!(out, "{}self.return_value = {}", ind, expr_to_gd(expr)).ok();
            }
            MatchElement::ActionBlock(blk) => {
                for s in &blk.statements {
                    out.push_str(&stmt_to_gd(s, ind)?);
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
    ) -> Result<(), String> {
        let input = self.input_field();
        let iv = format!("_inner{}", my_sid);
        let sub = match self.alphabet {
            Alphabet::Token => format!("self.{}.slice(self.cursor, len(self.{}))", input, input),
            _ => format!("self.{}.substr(self.cursor)", input),
        };
        writeln!(out, "{}var {} = {}.new({})", ind, iv, inner, sub).ok();
        writeln!(out, "{}if not {}.accepted:", ind, iv).ok();
        self.emit_failure(out, m, ind2)?;
        let end = format!("self.cursor + {}.cursor", iv);
        writeln!(out, "{}self._matched = {}", ind, self.matched_expr(&end)).ok();
        if let Some(slabel) = &stage.label {
            let key = format!("{}.{}", state_label, slabel);
            writeln!(out, "{}self._cap[{:?}] = self._matched", ind, key).ok();
            writeln!(out, "{}self._cap_inst[{:?}] = {}", ind, key, iv).ok();
        }
        writeln!(out, "{}self.cursor = self.cursor + {}.cursor", ind, iv).ok();
        writeln!(out, "{}self.accepted = true", ind).ok();
        Ok(())
    }

    fn emit_success(&self, out: &mut String, m: &MatchAst, indent: &str) -> Result<(), String> {
        match m.transition.as_ref().and_then(|c| c.success.as_ref()) {
            None => {
                writeln!(out, "{}return -1", indent).ok();
                Ok(())
            }
            Some(success) => self.emit_target(out, success, indent, &|out, indent| {
                self.emit_failure(out, m, indent)
            }),
        }
    }

    fn emit_failure(&self, out: &mut String, m: &MatchAst, indent: &str) -> Result<(), String> {
        writeln!(out, "{}self.accepted = false", indent).ok();
        writeln!(out, "{}self.reject_position = self.cursor", indent).ok();
        match m.transition.as_ref().and_then(|c| c.failure.as_ref()) {
            None => {
                writeln!(out, "{}return -1", indent).ok();
                Ok(())
            }
            Some(target) => self.emit_target(out, target, indent, &|out, indent| {
                writeln!(out, "{}return -1", indent).ok();
                Ok(())
            }),
        }
    }

    fn emit_target(
        &self,
        out: &mut String,
        target: &FsmTransitionTarget,
        indent: &str,
        on_none: &dyn Fn(&mut String, &str) -> Result<(), String>,
    ) -> Result<(), String> {
        match target {
            FsmTransitionTarget::Static { .. } => self.emit_goto(out, target, indent),
            FsmTransitionTarget::Conditional(alts) => {
                let inner = format!("{}\t", indent);
                for alt in alts {
                    writeln!(out, "{}if {}:", indent, expr_to_gd(&alt.condition)).ok();
                    self.emit_goto(out, &alt.target, &inner)?;
                }
                on_none(out, indent)
            }
        }
    }

    fn emit_goto(
        &self,
        out: &mut String,
        t: &FsmTransitionTarget,
        indent: &str,
    ) -> Result<(), String> {
        match t {
            FsmTransitionTarget::Static {
                state, stage: None, ..
            } => {
                let idx = self
                    .label_to_index
                    .get(state)
                    .copied()
                    .ok_or_else(|| format!("transition to undeclared state `${}`", state))?;
                writeln!(out, "{}return {}", indent, idx).ok();
                Ok(())
            }
            FsmTransitionTarget::Static {
                state,
                stage: Some(s),
                ..
            } => {
                let idx = self
                    .label_to_index
                    .get(state)
                    .copied()
                    .ok_or_else(|| format!("transition to undeclared state `${}`", state))?;
                let entry = self
                    .stage_entry
                    .get(&(state.clone(), s.clone()))
                    .copied()
                    .ok_or_else(|| format!("transition to undeclared stage `${}.{}`", state, s))?;
                writeln!(out, "{}self._enter = {}", indent, entry).ok();
                writeln!(out, "{}return {}", indent, idx).ok();
                Ok(())
            }
            FsmTransitionTarget::Conditional(_) => {
                Err("a conditional target may not nest another conditional target".into())
            }
        }
    }

    fn emit_action_methods(&self, out: &mut String) -> Result<(), String> {
        let Some(block) = &self.decl.actions else {
            return Ok(());
        };
        for act in &block.actions {
            let sig: Vec<String> = act.params.iter().map(|p| p.name.clone()).collect();
            writeln!(out, "\tfunc {}({}):", act.name, sig.join(", ")).ok();
            self.emit_action_body(out, &act.body, act.return_type.is_some())?;
        }
        Ok(())
    }

    fn emit_action_body(
        &self,
        out: &mut String,
        body: &BlockAst,
        has_return: bool,
    ) -> Result<(), String> {
        if body.statements.is_empty() {
            out.push_str("\t\tpass\n");
            return Ok(());
        }
        let last = body.statements.len() - 1;
        for (i, st) in body.statements.iter().enumerate() {
            if i == last && has_return {
                if let Statement::Expression(e) = st {
                    if !matches!(e.expr, Expression::Assign { .. }) {
                        writeln!(out, "\t\treturn {}", expr_to_gd(&e.expr)).ok();
                        continue;
                    }
                }
            }
            out.push_str(&stmt_to_gd(st, "\t\t")?);
        }
        Ok(())
    }
}

/// Comma-joined `i64`s for a flat Pike array literal (`_OPS_`/`_RNG_`).
fn int_list(xs: &[i64]) -> String {
    xs.iter()
        .map(|x| x.to_string())
        .collect::<Vec<_>>()
        .join(", ")
}

/// One stage's DFA as a GDScript nested-array literal `[[states...], start]`,
/// each state `[[[lo, hi, tgt], ...], accept]`.
fn dfa_literal(dfa: &StageDfa) -> String {
    let states: Vec<String> = dfa
        .states
        .iter()
        .map(|(trans, acc)| {
            let ts: Vec<String> = trans
                .iter()
                .map(|(lo, hi, tgt)| format!("[{}, {}, {}]", lo, hi, tgt))
                .collect();
            format!("[[{}], {}]", ts.join(", "), gd_bool(*acc))
        })
        .collect();
    format!("[[{}], {}]", states.join(", "), dfa.start)
}

// ---------------------------------------------------------------------------
// Statement + expression translation
// ---------------------------------------------------------------------------

fn stmt_to_gd(stmt: &Statement, indent: &str) -> Result<String, String> {
    match stmt {
        Statement::Expression(e) => Ok(format!("{}{}\n", indent, expr_to_gd(&e.expr))),
        Statement::If(if_ast) => {
            let inner = format!("{}\t", indent);
            let mut s = format!("{}if {}:\n", indent, expr_to_gd(&if_ast.condition));
            s.push_str(&stmt_to_gd(&if_ast.then_branch, &inner)?);
            if let Some(else_b) = &if_ast.else_branch {
                s.push_str(&format!("{}else:\n", indent));
                s.push_str(&stmt_to_gd(else_b, &inner)?);
            }
            Ok(s)
        }
        Statement::Block(blk) => {
            if blk.statements.is_empty() {
                return Ok(format!("{}pass\n", indent));
            }
            let mut s = String::new();
            for st in &blk.statements {
                s.push_str(&stmt_to_gd(st, indent)?);
            }
            Ok(s)
        }
        other => Err(format!(
            "statement form {:?} is not supported in @@fsm action blocks by the GDScript backend",
            std::mem::discriminant(other)
        )),
    }
}

fn expr_to_gd(e: &Expression) -> String {
    match e {
        Expression::Literal(l) => literal_to_gd(l),
        Expression::Var(name) => var_to_gd(name),
        Expression::Binary { left, op, right } => {
            format!(
                "({} {} {})",
                expr_to_gd(left),
                binop_to_gd(op),
                expr_to_gd(right)
            )
        }
        Expression::Unary { op, expr } => match op {
            UnaryOp::Not => format!("(not {})", expr_to_gd(expr)),
            UnaryOp::Neg => format!("(-{})", expr_to_gd(expr)),
            UnaryOp::BitNot => format!("(~{})", expr_to_gd(expr)),
        },
        Expression::Call { func, args } => call_to_gd(func, args),
        Expression::Member { object, field } => {
            if let Expression::Var(name) = object.as_ref() {
                if let Some(cap) = name.strip_prefix('$') {
                    if matches!(
                        field.as_str(),
                        "return_value" | "accepted" | "cursor" | "reject_position"
                    ) {
                        return format!("self._cap_inst[{:?}].{}", cap, field);
                    }
                }
            }
            format!("{}.{}", expr_to_gd(object), field)
        }
        Expression::Index { object, index } => {
            format!("{}[{}]", expr_to_gd(object), expr_to_gd(index))
        }
        Expression::Assign { target, value } => {
            format!("{} = {}", expr_to_gd(target), expr_to_gd(value))
        }
        Expression::NativeExpr(s) => s.clone(),
    }
}

fn literal_to_gd(l: &Literal) -> String {
    match l {
        Literal::Int(i) => i.to_string(),
        Literal::Float(f) => f.to_string(),
        Literal::String(s) => format!("{:?}", s),
        Literal::Bool(b) => gd_bool(*b).to_string(),
        Literal::Null => "null".to_string(),
    }
}

fn var_to_gd(name: &str) -> String {
    match name {
        "@@:matched" => "self._matched".to_string(),
        "@@:cursor" => "self.cursor".to_string(),
        "@@:return" => "self.return_value".to_string(),
        _ => {
            if let Some(cap) = name.strip_prefix('$') {
                format!("self._cap[{:?}]", cap)
            } else {
                name.to_string()
            }
        }
    }
}

fn call_to_gd(func: &str, args: &[Expression]) -> String {
    let rendered: Vec<String> = args.iter().map(expr_to_gd).collect();
    match func {
        "to_int" => format!("int({})", rendered.join(", ")),
        "to_str" => format!("str({})", rendered.join(", ")),
        "len" => format!("len({})", rendered.join(", ")),
        _ => format!("self.{}({})", func, rendered.join(", ")),
    }
}

fn binop_to_gd(op: &BinaryOp) -> &'static str {
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
        BinaryOp::And => "and",
        BinaryOp::Or => "or",
        BinaryOp::BitAnd => "&",
        BinaryOp::BitOr => "|",
        BinaryOp::BitXor => "^",
    }
}

fn gd_bool(b: bool) -> &'static str {
    if b {
        "true"
    } else {
        "false"
    }
}

/// The comparison that signals a *violated* boundary. `\b` (Required) needs a
/// boundary (sides differ): equal word-classes (`==`) ⇒ no boundary ⇒ reject.
/// `\B` (Forbidden) needs no boundary (sides match): differing classes (`!=`)
/// ⇒ a boundary ⇒ reject.
fn boundary_op(wb: WordBoundary) -> &'static str {
    match wb {
        WordBoundary::Required => "==",
        WordBoundary::Forbidden => "!=",
    }
}

fn gd_default(raw: &str) -> String {
    match raw {
        "false" => "false".to_string(),
        "true" => "true".to_string(),
        "null" | "nil" | "None" => "null".to_string(),
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frame_c::compiler::fsm_parser::parse_fsm_block;
    use std::process::Command;

    /// Compile + run a GDScript program: the generated recognizer inner
    /// class(es) plus a `SceneTree` driver, executed headless via `godot`.
    /// Returns stdout lines. `None` if no `godot` binary is available.
    fn gd_run(code: &str, driver: &str, tag: &str) -> Option<Vec<String>> {
        let prog = format!(
            "extends SceneTree\n\n{}\nfunc _init():\n{}\n\tquit()\n",
            code, driver
        );
        let dir = std::env::temp_dir().join(format!("framec_gd_{}", tag));
        std::fs::create_dir_all(&dir).ok()?;
        let src = dir.join("prog.gd");
        std::fs::write(&src, prog).ok()?;
        let out = match Command::new("godot")
            .arg("--headless")
            .arg("--script")
            .arg(&src)
            .output()
        {
            Ok(o) => o,
            Err(_) => return None,
        };
        let stdout = String::from_utf8_lossy(&out.stdout);
        let stderr = String::from_utf8_lossy(&out.stderr);
        // Godot prints its banner + any script errors to stderr; surface a
        // parse/runtime error rather than silently returning empty output.
        assert!(
            !stderr.contains("SCRIPT ERROR") && !stderr.contains("Parse Error"),
            "godot error for {:?}:\nSTDERR:\n{}\nSTDOUT:\n{}",
            tag,
            stderr,
            stdout
        );
        Some(
            stdout
                .lines()
                .filter(|l| !l.starts_with("Godot Engine") && !l.trim().is_empty())
                .map(|s| s.to_string())
                .collect(),
        )
    }

    fn gen(src: &str) -> String {
        let decl = parse_fsm_block(src.as_bytes()).expect("fixture must parse");
        generate(&decl).expect("fixture must generate")
    }

    /// The scalar core, all built + run in one program.
    #[test]
    fn gd_core() {
        let cases: &[(&str, &str, &str, bool)] = &[
            ("@@fsm A(text: bytes) : bool = false { /a/ true }", "\"a\"", "true", false),
            ("@@fsm B(text: bytes) : bool = false { /a/ true }", "\"b\"", "false", false),
            (
                "@@fsm C(text: bytes) : int = 0 { /[0-9]+/ to_int(@@:matched) }",
                "\"123\"",
                "true",
                true,
            ),
            (
                "@@fsm D(text: bytes) : int = 0 { /[0-9]+/ len(self.text) }",
                "\"123\"",
                "true",
                true,
            ),
            (
                "@@fsm E(text: bytes) : int = 0 { $s: .n/[0-9]+/ to_int($s.n) }",
                "\"42\"",
                "true",
                true,
            ),
            (
                "@@fsm F(text: bytes) : int = 0 { /[0-9]/ { self.count = self.count + 1 } self.count domain: count: int = 0 }",
                "\"5\"",
                "true",
                true,
            ),
            (
                "@@fsm G(text: bytes) : int = 0 { /[0-9]+/ parse_int(@@:matched) actions: parse_int(s: bytes): int { to_int(s) } }",
                "\"42\"",
                "true",
                true,
            ),
            (
                "@@fsm H(text: bytes) : int = 0 { $0: /[a-z]/ -> $digits : -> $error $digits: .n/[0-9]+/ to_int($digits.n) $error: -1 }",
                "\"x42\"",
                "true",
                true,
            ),
        ];
        let expect_ret = ["", "", "123", "3", "42", "1", "42", "42"];
        let code = cases
            .iter()
            .map(|(s, ..)| gen(s))
            .collect::<Vec<_>>()
            .join("\n");
        let names = ["A", "B", "C", "D", "E", "F", "G", "H"];
        let driver = cases
            .iter()
            .enumerate()
            .map(|(i, (_, ctor, _, has_ret))| {
                if *has_ret {
                    format!(
                        "\tvar m{i} = {n}.new({c})\n\tprint(m{i}.accepted)\n\tprint(m{i}.return_value)",
                        i = i,
                        n = names[i],
                        c = ctor
                    )
                } else {
                    format!(
                        "\tvar m{i} = {n}.new({c})\n\tprint(m{i}.accepted)",
                        i = i,
                        n = names[i],
                        c = ctor
                    )
                }
            })
            .collect::<Vec<_>>()
            .join("\n");
        let Some(lines) = gd_run(&code, &driver, "core") else {
            return;
        };
        let mut i = 0;
        for (idx, (_, _, exp_acc, has_ret)) in cases.iter().enumerate() {
            assert_eq!(&lines[i], exp_acc, "accepted case {idx}");
            if *has_ret {
                assert_eq!(&lines[i + 1], expect_ret[idx], "return case {idx}");
                i += 2;
            } else {
                i += 1;
            }
        }
    }

    #[test]
    fn gd_conditional_target() {
        let code = gen("@@fsm M(text: bytes, mode: int) : int = 0 { \
             /[01]/ -> ( $zero when self.mode == 0, $one when self.mode == 1 ) : -> $error \
             $zero: 0 $one: 1 $error: -1 }");
        let driver =
            "\tfor md in [0, 1, 2]:\n\t\tvar m = M.new(\"0\", md)\n\t\tprint(m.return_value)";
        let Some(lines) = gd_run(&code, driver, "cond") else {
            return;
        };
        assert_eq!(lines, vec!["0", "1", "-1"]);
    }

    #[test]
    fn gd_multi_match() {
        let code = gen("@@fsm M(text: bytes) : int = 0 { /[0-9]/ -> $num | 99 $num: 1 }");
        let driver = "\tfor s in [\"5\", \"a\"]:\n\t\tvar m = M.new(s)\n\t\tprint(m.return_value)";
        let Some(lines) = gd_run(&code, driver, "mm") else {
            return;
        };
        assert_eq!(lines, vec!["1", "99"]);
    }

    #[test]
    fn gd_embed_every_transition() {
        let code = gen(
            "@@fsm M(text: bytes) : int = 0 { /[0-9]+/ ${ tally() } self.count \
             actions: tally() { self.count = self.count + 1 } domain: count: int = 0 }",
        );
        let driver = "\tvar m = M.new(\"123\")\n\tprint(m.return_value)";
        let Some(lines) = gd_run(&code, driver, "emb") else {
            return;
        };
        assert_eq!(lines[0], "3");
    }

    /// FSM-TEST-603 — `%{...}` fires when the DFA leaves its last accepting
    /// state, capturing the end of the matched region.
    #[test]
    fn gd_embed_leave_final() {
        let code = gen("@@fsm M(text: bytes) : int = 0 { \
             /[0-9]+/ %{ self.end_pos = @@:cursor } self.end_pos \
             domain: end_pos: int = 0 }");
        let driver =
            "\tfor s in [\"42x\", \"abx\"]:\n\t\tvar m = M.new(s)\n\t\tprint(m.return_value)";
        let Some(lines) = gd_run(&code, driver, "leavefin") else {
            return;
        };
        assert_eq!(lines, vec!["2", "0"]);
    }

    #[test]
    fn gd_token_alphabet() {
        let code = gen("@@fsm M(toks: token) : bool = false { /IDENT LPAREN RPAREN/ true }");
        let driver = "\tprint(M.new([\"IDENT\",\"LPAREN\",\"RPAREN\"]).accepted)\n\tprint(M.new([\"IDENT\",\"RPAREN\"]).accepted)\n\tprint(M.new([\"IDENT\",\"WAT\"]).accepted)";
        let Some(lines) = gd_run(&code, driver, "tok") else {
            return;
        };
        assert_eq!(lines, vec!["true", "false", "false"]);
    }

    #[test]
    fn gd_mode_c_callout() {
        let inner = gen("@@fsm Digits(text: bytes) : int = 0 { /[0-9]+/ to_int(@@:matched) }");
        let outer = gen("@@fsm Outer(text: bytes) : int = 0 { $s: .d/@Digits/ $s.d.return_value }");
        let code = format!("{}\n{}", inner, outer);
        let driver = "\tfor s in [\"42\", \"x\"]:\n\t\tvar m = Outer.new(s)\n\t\tprint(str(m.accepted) + \" \" + str(m.return_value))";
        let Some(lines) = gd_run(&code, driver, "modec") else {
            return;
        };
        assert_eq!(lines, vec!["true 42", "false 0"]);
    }

    #[test]
    fn gd_anchors() {
        let start = gen("@@fsm M(text: bytes) : bool = false { /^foo/ true }");
        let end = gen("@@fsm N(text: bytes) : bool = false { /[0-9]+$/ true }");
        let code = format!("{}\n{}", start, end);
        let driver = "\tprint(M.new(\"foo\").accepted)\n\tprint(M.new(\"xfoo\").accepted)\n\tprint(N.new(\"123\").accepted)\n\tprint(N.new(\"123x\").accepted)";
        let Some(lines) = gd_run(&code, driver, "anc") else {
            return;
        };
        assert_eq!(lines, vec!["true", "false", "true", "false"]);
    }

    #[test]
    fn gd_word_boundary() {
        let code = gen("@@fsm M(text: bytes) : bool = false { /\\bcat\\b/ true }");
        let driver = "\tprint(M.new(\"cat\").accepted)\n\tprint(M.new(\"cats\").accepted)";
        let Some(lines) = gd_run(&code, driver, "wb") else {
            return;
        };
        assert_eq!(lines, vec!["true", "false"]);
    }

    /// §11.1 — a lazy quantifier compiles to a Pike program and matches with
    /// leftmost-first (minimal) semantics: `.*?,` stops at the FIRST comma, and
    /// `a*?b+` is minimal on `a*?` but greedy on `b+`.
    #[test]
    fn gd_lazy_quantifier() {
        let lazy = gen("@@fsm M(text: bytes) : bool = false { /.*?,/ @@:matched }");
        let mixed = gen("@@fsm N(text: bytes) : int = 0 { /a*?b+/ @@:cursor }");
        let code = format!("{}\n{}", lazy, mixed);
        let driver = "\tvar m = M.new(\"ab,cd,ef\")\n\tprint(m.return_value)\n\
                      \tvar n = N.new(\"aabbb\")\n\tprint(n.return_value)";
        let Some(lines) = gd_run(&code, driver, "lazy") else {
            return;
        };
        assert_eq!(lines, vec!["ab,", "5"]);
    }

    #[test]
    fn gd_unsupported_errors() {
        let decl =
            parse_fsm_block(b"@@fsm M(text: bytes) : bool = false { /a$b/ true }").expect("parses");
        let err = generate(&decl).unwrap_err();
        assert!(err.contains("anchor"), "got {err}");
    }
}
