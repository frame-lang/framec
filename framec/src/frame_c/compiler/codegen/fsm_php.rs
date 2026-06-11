//! PHP backend for `@@fsm` (RFC-0042, Phase 8).
//!
//! Generates a self-contained PHP `class` from a validated `FsmDeclAst`. PHP
//! is class-based with mutable properties and dynamic typing, so the
//! recognition model is a near-transliteration of the Python reference
//! backend ([`super::fsm_python`]): per-stage minimal DFAs (inline array
//! literals) + a per-state dispatch loop (`while` + `switch`) over mutable
//! object properties. Properties are declared (PHP 8.2+ deprecates dynamic
//! properties); the constructor is `new <Name>(...)`; the observable result
//! (§5.1) is the object's public `accepted`, `return_value`, `cursor`, and
//! `reject_position`.
//!
//! The `bytes`/`char` input is the source string (`ord($s[$pos])` is the
//! code point; ASCII-indexed); the `token` input is an array of token-kind
//! names mapped to small integer ids. The matched run is a `substr` /
//! `array_slice`.
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
    self, pike::Program, size_check::DEFAULT_MAX_DFA_STATES, subset::DfaLabel, Alphabet,
    CompileError, WordBoundary,
};
use std::fmt::Write;

/// Generate PHP source implementing `decl`, or a reason it is outside the
/// v0.1 PHP cut.
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
    /// Pike program matched by the VM (`_pike_match`) instead of the DFA, for
    /// leftmost-first match-end semantics. Bytes/char only.
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
                                                actions is not yet supported by the PHP backend"
                                        .into());
                                }
                                self.stage_dfas.push(dfa)
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
                 PHP backend",
                regex
            )),
        }
    }

    /// The empty `matched` value: an empty array for tokens, else `""`.
    fn matched_empty(&self) -> &'static str {
        match self.alphabet {
            Alphabet::Token => "[]",
            _ => "\"\"",
        }
    }

    /// The per-element read as an integer: code point for byte/char, token id
    /// for the token alphabet.
    fn element_read(&self) -> String {
        let inp = &self.decl.params[0].name;
        match self.alphabet {
            Alphabet::Token => format!("$this->tokId($this->{}[$pos])", inp),
            _ => format!("ord($this->{}[$pos])", inp),
        }
    }

    /// The length of the input (`strlen` for a string, `count` for a token
    /// array).
    fn input_len(&self) -> String {
        let inp = &self.decl.params[0].name;
        match self.alphabet {
            Alphabet::Token => format!("count($this->{})", inp),
            _ => format!("strlen($this->{})", inp),
        }
    }

    /// Materialize the matched run `$this-><input>[cursor..end]`.
    fn matched_slice(&self, end: &str) -> String {
        let inp = &self.decl.params[0].name;
        match self.alphabet {
            Alphabet::Token => format!(
                "array_slice($this->{}, $this->cursor, ({}) - $this->cursor)",
                inp, end
            ),
            _ => format!(
                "substr($this->{}, $this->cursor, ({}) - $this->cursor)",
                inp, end
            ),
        }
    }

    fn emit(&self) -> Result<String, String> {
        let mut out = String::new();
        out.push_str("// Generated by framec — RFC-0042 @@fsm (PHP backend).\n\n");
        writeln!(out, "class {} {{", self.decl.name).ok();
        self.emit_property_decls(&mut out);
        self.emit_ctor(&mut out);
        self.emit_tok_id(&mut out);
        self.emit_pike_tables(&mut out);
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

    /// Declared public properties (PHP 8.2+ deprecates dynamic properties);
    /// duplicates elided so a domain field re-declaring a parameter is
    /// declared once.
    fn emit_property_decls(&self, out: &mut String) {
        let mut names: Vec<String> = vec![
            "accepted".into(),
            "reject_position".into(),
            "cursor".into(),
            "return_value".into(),
        ];
        let mut seen: std::collections::HashSet<String> = names.iter().cloned().collect();
        for p in &self.decl.params {
            if seen.insert(p.name.clone()) {
                names.push(p.name.clone());
            }
        }
        if let Some(domain) = &self.decl.domain {
            for v in &domain.vars {
                if seen.insert(v.name.clone()) {
                    names.push(v.name.clone());
                }
            }
        }
        names.push("matched".into());
        names.push("enter".into());
        for f in self.capture_fields() {
            names.push(f);
        }
        for (f, _) in self.mode_c_inst_fields() {
            names.push(f);
        }
        for n in names {
            writeln!(out, "  public ${};", n).ok();
        }
        out.push('\n');
    }

    fn emit_ctor(&self, out: &mut String) {
        let input = &self.decl.params[0].name;
        let sig: Vec<String> = self
            .decl
            .params
            .iter()
            .map(|p| format!("${}", p.name))
            .collect();
        writeln!(out, "  function __construct({}) {{", sig.join(", ")).ok();
        out.push_str("    $this->accepted = false;\n");
        out.push_str("    $this->reject_position = 0;\n");
        out.push_str("    $this->cursor = 0;\n");
        writeln!(
            out,
            "    $this->return_value = {};",
            php_default(&self.decl.default_expr)
        )
        .ok();
        for p in &self.decl.params {
            writeln!(out, "    $this->{} = ${};", p.name, p.name).ok();
        }
        if let Some(domain) = &self.decl.domain {
            for v in &domain.vars {
                if &v.name == input {
                    continue;
                }
                writeln!(out, "    $this->{} = {};", v.name, self.expr(&v.default)).ok();
            }
        }
        writeln!(out, "    $this->matched = {};", self.matched_empty()).ok();
        out.push_str("    $this->enter = 0;\n");
        for f in self.capture_fields() {
            writeln!(out, "    $this->{} = {};", f, self.matched_empty()).ok();
        }
        for (f, _) in self.mode_c_inst_fields() {
            writeln!(out, "    $this->{} = null;", f).ok();
        }
        out.push_str("    $this->run();\n");
        out.push_str("    if ($this->accepted) { $this->reject_position = 0; }\n");
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
            .map(|(name, id)| format!("{:?} => {}", name, id))
            .collect();
        out.push_str("  function tokId($t) {\n");
        writeln!(out, "    $T = [{}];", items.join(", ")).ok();
        out.push_str("    return array_key_exists($t, $T) ? $T[$t] : -1;\n");
        out.push_str("  }\n\n");
    }

    fn emit_dfa_matcher(&self, out: &mut String) {
        let read = self.element_read();
        writeln!(
            out,
            "  function dfaMatch($states, $start) {{\n\
             \x20   $st = $start;\n\
             \x20   $pos = $this->cursor;\n\
             \x20   $n = {len};\n\
             \x20   $last = $states[$st][1] ? $pos : -1;\n\
             \x20   while ($pos < $n) {{\n\
             \x20     $v = {read};\n\
             \x20     $nxt = -1;\n\
             \x20     foreach ($states[$st][0] as [$lo, $hi, $tgt]) {{ if ($lo <= $v && $v <= $hi) {{ $nxt = $tgt; break; }} }}\n\
             \x20     if ($nxt < 0) break;\n\
             \x20     $st = $nxt; $pos++;\n\
             \x20     if ($states[$st][1]) $last = $pos;\n\
             \x20   }}\n\
             \x20   return $last;\n\
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

    /// Emit per-stage Pike program tables (`_OPS_<i>`/`_RNG_<i>`) as `static`
    /// arrays for each lazy stage, in place of the inline DFA literal. Two flat
    /// int arrays from `fsm_regex::pike::encode`.
    fn emit_pike_tables(&self, out: &mut String) {
        for (i, dfa) in self.stage_dfas.iter().enumerate() {
            if let Some(prog) = &dfa.program {
                let (ops, rng) = fsm_regex::pike::encode(prog);
                writeln!(out, "  static $_OPS_{} = [{}];", i, int_list(&ops)).ok();
                writeln!(out, "  static $_RNG_{} = [{}];", i, int_list(&rng)).ok();
            }
        }
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
            "  function _pike_add(&$lst, &$seen, $ops, $pc) {{\n\
             \x20   if ($seen[$pc]) return;\n\
             \x20   $seen[$pc] = true;\n\
             \x20   $op = $ops[$pc * 4];\n\
             \x20   if ($op == 2) {{ $this->_pike_add($lst, $seen, $ops, $ops[$pc * 4 + 1]); }}\n\
             \x20   else if ($op == 1) {{\n\
             \x20     $this->_pike_add($lst, $seen, $ops, $ops[$pc * 4 + 1]);\n\
             \x20     $this->_pike_add($lst, $seen, $ops, $ops[$pc * 4 + 2]);\n\
             \x20   }} else {{ $lst[] = $pc; }}\n\
             \x20 }}\n\n\
             \x20 function _pike_match($ops, $rng) {{\n\
             \x20   $n = strlen($this->{inp});\n\
             \x20   $ninst = intdiv(count($ops), 4);\n\
             \x20   $matched = -1;\n\
             \x20   $clist = [];\n\
             \x20   $cseen = array_fill(0, $ninst, false);\n\
             \x20   $this->_pike_add($clist, $cseen, $ops, 0);\n\
             \x20   $pos = $this->cursor;\n\
             \x20   while (true) {{\n\
             \x20     $nlist = [];\n\
             \x20     $nseen = array_fill(0, $ninst, false);\n\
             \x20     foreach ($clist as $pc) {{\n\
             \x20       $op = $ops[$pc * 4];\n\
             \x20       if ($op == 0) {{\n\
             \x20         if ($pos < $n) {{\n\
             \x20           $v = ord($this->{inp}[$pos]);\n\
             \x20           $rs = $ops[$pc * 4 + 1]; $rc = $ops[$pc * 4 + 2];\n\
             \x20           for ($k = 0; $k < $rc; $k++) {{\n\
             \x20             if ($rng[($rs + $k) * 2] <= $v && $v <= $rng[($rs + $k) * 2 + 1]) {{\n\
             \x20               $this->_pike_add($nlist, $nseen, $ops, $pc + 1);\n\
             \x20               break;\n\
             \x20             }}\n\
             \x20           }}\n\
             \x20         }}\n\
             \x20       }} else if ($op == 3) {{ $matched = $pos; break; }}\n\
             \x20     }}\n\
             \x20     if ($pos >= $n) break;\n\
             \x20     $pos++;\n\
             \x20     $clist = $nlist;\n\
             \x20   }}\n\
             \x20   return $matched;\n\
             \x20 }}\n\n",
            inp = inp
        )
        .ok();
    }

    fn emit_run(&self, out: &mut String) {
        out.push_str("  function run() {\n    $state = 0;\n");
        out.push_str("    while ($state >= 0) {\n");
        out.push_str("      $_enter = $this->enter;\n      $this->enter = 0;\n");
        out.push_str("      switch ($state) {\n");
        for i in 0..self.decl.states.len() {
            writeln!(
                out,
                "        case {}: $state = $this->state{}($_enter); break;",
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
                    writeln!(out, "  function state{}($_enter) {{ return -1; }}\n", i).ok();
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
        writeln!(out, "  function state{}($_enter) {{", index).ok();
        for (idx, el) in m.elements.iter().enumerate() {
            writeln!(out, "    if ($_enter <= {}) {{", idx).ok();
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
        writeln!(out, "  function state{}($_enter) {{", index).ok();
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
                             supported by the PHP backend"
                                .into(),
                        );
                    }
                    let my_sid = *sid;
                    *sid += 1;
                    if self.stage_dfas[my_sid].mode_c.is_some() {
                        return Err(
                            "a Mode C (`/@Fsm/`) stage as a `|` alternative selector is not yet \
                             supported by the PHP backend"
                                .into(),
                        );
                    }
                    let MatchElement::Stage(sel) = &m.elements[fs] else {
                        unreachable!("first_stage indexes a Stage element")
                    };
                    writeln!(out, "    $_r = {};", self.stage_call(sel, my_sid)).ok();
                    self.emit_anchor_guards(out, my_sid, "    ");
                    out.push_str("    if ($_r >= 0) {\n");
                    writeln!(out, "      $this->matched = {};", self.matched_slice("$_r")).ok();
                    if let Some(lbl) = &sel.label {
                        if !state_label.is_empty() {
                            writeln!(
                                out,
                                "      $this->{} = $this->matched;",
                                cap_field(&state_label, lbl)
                            )
                            .ok();
                        }
                    }
                    out.push_str("      $this->cursor = $_r;\n");
                    out.push_str("      $this->accepted = true;\n");
                    for el in &m.elements[fs + 1..] {
                        self.emit_element(out, el, m, &state_label, "      ", sid)?;
                    }
                    self.emit_success(out, m, "      ");
                    out.push_str("    }\n");
                }
                None => {
                    out.push_str("    $this->accepted = true;\n");
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
            out.push_str("    $this->accepted = false;\n");
            out.push_str("    $this->reject_position = $this->cursor;\n");
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
                writeln!(out, "{}$_r = {};", ind, self.stage_call(stage, my_sid)).ok();
                self.emit_anchor_guards(out, my_sid, ind);
                writeln!(out, "{}if ($_r < 0) {{", ind).ok();
                self.emit_failure(out, m, &ind2);
                writeln!(out, "{}}}", ind).ok();
                writeln!(
                    out,
                    "{}$this->matched = {};",
                    ind,
                    self.matched_slice("$_r")
                )
                .ok();
                if let Some(lbl) = &stage.label {
                    if !state_label.is_empty() {
                        writeln!(
                            out,
                            "{}$this->{} = $this->matched;",
                            ind,
                            cap_field(state_label, lbl)
                        )
                        .ok();
                    }
                }
                writeln!(out, "{}$this->cursor = $_r;", ind).ok();
                writeln!(out, "{}$this->accepted = true;", ind).ok();
            }
            MatchElement::BareExpression { expr, .. } => {
                writeln!(out, "{}$this->return_value = {};", ind, self.expr(expr)).ok();
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
        let iv = format!("$inner{}", my_sid);
        let sub = match self.alphabet {
            Alphabet::Token => format!("array_slice($this->{}, $this->cursor)", input),
            _ => format!("substr($this->{}, $this->cursor)", input),
        };
        writeln!(out, "{}{} = new {}({});", ind, iv, inner, sub).ok();
        writeln!(out, "{}if (!{}->accepted) {{", ind, iv).ok();
        self.emit_failure(out, m, ind2);
        writeln!(out, "{}}}", ind).ok();
        let end = format!("$this->cursor + {}->cursor", iv);
        writeln!(out, "{}$this->matched = {};", ind, self.matched_slice(&end)).ok();
        if let Some(lbl) = &stage.label {
            if !state_label.is_empty() {
                writeln!(
                    out,
                    "{}$this->{} = $this->matched;",
                    ind,
                    cap_field(state_label, lbl)
                )
                .ok();
                writeln!(
                    out,
                    "{}$this->{} = {};",
                    ind,
                    cap_inst_field(state_label, lbl),
                    iv
                )
                .ok();
            }
        }
        writeln!(
            out,
            "{}$this->cursor = $this->cursor + {}->cursor;",
            ind, iv
        )
        .ok();
        writeln!(out, "{}$this->accepted = true;", ind).ok();
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
        writeln!(out, "  function matchStage{}() {{", sid).ok();
        writeln!(out, "    $dfa = {};", self.dfa_literal(sid)).ok();
        writeln!(
            out,
            "    $entry = $this->cursor;\n\
             \x20   $st = {start};\n\
             \x20   $pos = $entry;\n\
             \x20   $n = {len};\n\
             \x20   $last = $dfa[$st][1] ? $pos : -1;\n\
             \x20   $this->cursor = $pos;",
            start = self.stage_dfas[sid].start,
            len = self.input_len()
        )
        .ok();
        out.push_str(&self.embed_body(stage, EmbeddingOp::Start, "    ")?);
        out.push_str("    $prev = $dfa[$st][1];\n");
        writeln!(
            out,
            "    while ($pos < $n) {{\n\
             \x20     $v = {read};\n\
             \x20     $nxt = -1;\n\
             \x20     foreach ($dfa[$st][0] as [$lo, $hi, $tgt]) {{ if ($lo <= $v && $v <= $hi) {{ $nxt = $tgt; break; }} }}\n\
             \x20     if ($nxt < 0) break;\n\
             \x20     $st = $nxt; $pos++;\n\
             \x20     $this->cursor = $pos;",
            read = read
        )
        .ok();
        out.push_str(&self.embed_body(stage, EmbeddingOp::EveryTransition, "      ")?);
        out.push_str("      $now = $dfa[$st][1];\n");
        let accept = self.embed_body(stage, EmbeddingOp::Accept, "        ")?;
        if !accept.is_empty() {
            out.push_str("      if ($now) {\n");
            out.push_str(&accept);
            out.push_str("      }\n");
        }
        out.push_str("      if ($now) $last = $pos;\n      $prev = $now;\n");
        out.push_str("    }\n");
        // `%{}` — left the last accepting state: a post-scan event firing once
        // when the longest match stops extending (failing element or EOF), with
        // `@@:cursor` at the end of the matched region (`$last`), not the failing
        // element (§5.4 / FSM-TEST-603). `$last < 0` ⇒ no accepting state was
        // entered, so there is nothing to leave.
        let leave = self.embed_body(stage, EmbeddingOp::LeaveAccept, "      ")?;
        if !leave.is_empty() {
            out.push_str("    if ($last >= 0) {\n      $this->cursor = $last;\n");
            out.push_str(&leave);
            out.push_str("    }\n");
        }
        let eof = self.embed_body(stage, EmbeddingOp::Eof, "      ")?;
        if !eof.is_empty() {
            out.push_str("    if ($pos >= $n && !$prev) {\n");
            out.push_str(&eof);
            out.push_str("    }\n");
        }
        out.push_str("    $this->cursor = $entry;\n    return $last;\n  }\n\n");
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
            writeln!(out, "{}if ($this->cursor !== 0) $_r = -1;", ind).ok();
        }
        if dfa.requires_end {
            writeln!(out, "{}if ($_r !== {}) $_r = -1;", ind, self.input_len()).ok();
        }
        // Word boundaries (`\b`/`\B`): a boundary holds at position p iff the
        // word-ness of byte[p-1] differs from byte[p] (OOB ⇒ non-word). The
        // start boundary is checked at the match start (`$this->cursor`); the
        // end boundary at the match end (`$_r`), guarded so a prior reject is
        // not re-read. `Required` (`\b`) demands a boundary (sides differ, so
        // the violation test is `==`); `Forbidden` (`\B`) demands none (`!=`).
        if let Some(b) = dfa.start_boundary {
            let op = match b {
                WordBoundary::Required => "==",
                WordBoundary::Forbidden => "!=",
            };
            writeln!(
                out,
                "{}if ($this->_iswordat($this->cursor - 1) {} $this->_iswordat($this->cursor)) {{ $_r = -1; }}",
                ind, op
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
                "{}if ($_r >= 0 && $this->_iswordat($_r - 1) {} $this->_iswordat($_r)) {{ $_r = -1; }}",
                ind, op
            )
            .ok();
        }
    }

    /// True iff any stage uses a word-boundary anchor, gating emission of the
    /// `_iswordat` helper.
    fn uses_word_boundary(&self) -> bool {
        self.stage_dfas
            .iter()
            .any(|d| d.start_boundary.is_some() || d.end_boundary.is_some())
    }

    /// The `_iswordat($p)` helper: is the byte at `$p` a word byte
    /// (`[0-9A-Za-z_]`)? Out-of-bounds positions are non-word. Bytes-only
    /// (the engine forbids `\b`/`\B` outside the bytes alphabet).
    fn emit_iswordat(&self, out: &mut String) {
        if !self.uses_word_boundary() {
            return;
        }
        let inp = &self.decl.params[0].name;
        writeln!(out, "  function _iswordat($p) {{").ok();
        writeln!(
            out,
            "    if ($p < 0 || $p >= strlen($this->{})) return false;",
            inp
        )
        .ok();
        writeln!(out, "    $b = ord($this->{}[$p]);", inp).ok();
        out.push_str(
            "    return (48 <= $b && $b <= 57) || (65 <= $b && $b <= 90) || (97 <= $b && $b <= 122) || $b == 95;\n",
        );
        out.push_str("  }\n\n");
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
        writeln!(out, "{}$this->accepted = false;", ind).ok();
        writeln!(out, "{}$this->reject_position = $this->cursor;", ind).ok();
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
                "{}throw new Exception(\"transition to undeclared state ${}\");",
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
                    writeln!(out, "{}$this->enter = {};", ind, entry).ok();
                }
                None => {
                    writeln!(
                        out,
                        "{}throw new Exception(\"transition to undeclared stage ${}.{}\");",
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
                "statement {:?} not supported in @@fsm action blocks by the PHP backend",
                std::mem::discriminant(other)
            )),
        }
    }

    fn emit_action_methods(&self, out: &mut String) -> Result<(), String> {
        let Some(block) = &self.decl.actions else {
            return Ok(());
        };
        for act in &block.actions {
            let params: Vec<String> = act.params.iter().map(|p| format!("${}", p.name)).collect();
            writeln!(out, "  function {}({}) {{", act.name, params.join(", ")).ok();
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

    /// The matcher invocation for a stage: the Pike VM (`_pike_match`) for a
    /// lazy stage, the specialized `matchStage<sid>` when the stage carries
    /// embedding actions, else the inline `dfaMatch` over the DFA literal.
    fn stage_call(&self, stage: &StageAst, sid: usize) -> String {
        if self.stage_dfas[sid].program.is_some() {
            format!("$this->_pike_match(self::$_OPS_{sid}, self::$_RNG_{sid})")
        } else if stage.embedding_actions.is_empty() {
            format!(
                "$this->dfaMatch({}, {})",
                self.dfa_literal(sid),
                self.stage_dfas[sid].start
            )
        } else {
            format!("$this->matchStage{}()", sid)
        }
    }

    /// The PHP array literal for a stage's DFA.
    fn dfa_literal(&self, sid: usize) -> String {
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
        format!("[{}]", states.join(", "))
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
                "@@:matched" => "$this->matched".to_string(),
                "@@:cursor" => "$this->cursor".to_string(),
                "@@:return" => "$this->return_value".to_string(),
                _ => match name.strip_prefix('$').and_then(|c| c.split_once('.')) {
                    Some((state, label)) => format!("$this->{}", cap_field(state, label)),
                    None => format!("${}", name),
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
                            return format!("$this->{}->{}", cap_inst_field(state, label), field);
                        }
                    }
                    if name == "self" {
                        return format!("$this->{}", field);
                    }
                }
                format!("{}->{}", self.expr(object), field)
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
            "to_int" => format!("intval({})", a.join(", ")),
            "to_str" => format!("strval({})", a.join(", ")),
            "len" => match self.alphabet {
                Alphabet::Token => format!("count({})", a.join(", ")),
                _ => format!("strlen({})", a.join(", ")),
            },
            _ => format!("$this->{}({})", func, a.join(", ")),
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

/// Map a raw default-value token to a PHP expression.
fn php_default(raw: &str) -> String {
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

    /// Run a PHP program (generated code + driver) via `php`, returning
    /// stdout lines. `None` if `php` is unavailable.
    fn php_run(code: &str, driver: &str, tag: &str) -> Option<Vec<String>> {
        let prog = format!("<?php\n{}\n{}\n", code, driver);
        let path = std::env::temp_dir().join(format!("framec_php_{}.php", tag));
        std::fs::write(&path, prog).ok()?;
        let out = match Command::new("php").arg(&path).output() {
            Ok(o) => o,
            Err(_) => return None,
        };
        assert!(
            out.status.success(),
            "php failed for {:?}:\n{}",
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
        let driver = format!(
            "$m = {ctor};\necho var_export($m->accepted, true), \"\\n\";\necho var_export($m->return_value, true), \"\\n\";"
        );
        let lines = php_run(&code, &driver, tag)?;
        Some((lines[0].clone(), lines[1].clone()))
    }

    #[test]
    fn php_smoke_bool() {
        let src = "@@fsm M(text: bytes) : bool = false { /a/ true }";
        let Some((acc, ret)) = run(src, "new M(\"a\")", "smoke_a") else {
            return;
        };
        assert_eq!((acc.as_str(), ret.as_str()), ("true", "true"));
        assert_eq!(run(src, "new M(\"b\")", "smoke_b").unwrap().0, "false");
    }

    #[test]
    fn php_matched_to_int() {
        let src = "@@fsm M(text: bytes) : int = 0 { /[0-9]+/ to_int(@@:matched) }";
        let Some((acc, ret)) = run(src, "new M(\"123\")", "tok_a") else {
            return;
        };
        assert_eq!((acc.as_str(), ret.as_str()), ("true", "123"));
        assert_eq!(run(src, "new M(\"x\")", "tok_b").unwrap().0, "false");
    }

    #[test]
    fn php_len_self_input() {
        let src = "@@fsm M(text: bytes) : int = 0 { /[0-9]+/ len(self.text) }";
        let Some((_, ret)) = run(src, "new M(\"123\")", "len_a") else {
            return;
        };
        assert_eq!(ret, "3");
    }

    #[test]
    fn php_stage_capture() {
        let src = "@@fsm M(text: bytes) : int = 0 { $s: .n/[0-9]+/ to_int($s.n) }";
        let Some((acc, ret)) = run(src, "new M(\"42\")", "cap_a") else {
            return;
        };
        assert_eq!((acc.as_str(), ret.as_str()), ("true", "42"));
    }

    #[test]
    fn php_action_block() {
        let src = "@@fsm M(text: bytes) : int = 0 { \
                   /[0-9]/ { self.count = self.count + 1 } self.count \
                   domain: count: int = 0 }";
        let Some((_, ret)) = run(src, "new M(\"5\")", "act_a") else {
            return;
        };
        assert_eq!(ret, "1");
    }

    #[test]
    fn php_declared_action() {
        let src = "@@fsm M(text: bytes) : int = 0 { \
                   /[0-9]+/ parse_int(@@:matched) \
                   actions: parse_int(s: bytes): int { to_int(s) } }";
        let Some((_, ret)) = run(src, "new M(\"42\")", "decl_a") else {
            return;
        };
        assert_eq!(ret, "42");
    }

    #[test]
    fn php_transitions_and_capture() {
        let src = "@@fsm M(text: bytes) : int = 0 { \
                   $0: /[a-z]/ -> $digits : -> $error \
                   $digits: .n/[0-9]+/ to_int($digits.n) \
                   $error: -1 }";
        let Some((acc, ret)) = run(src, "new M(\"x42\")", "tr_a") else {
            return;
        };
        assert_eq!((acc.as_str(), ret.as_str()), ("true", "42"));
        assert_eq!(run(src, "new M(\"X\")", "tr_b").unwrap().1, "-1");
    }

    #[test]
    fn php_conditional_target() {
        let src = "@@fsm M(text: bytes, mode: int) : int = 0 { \
                   /[01]/ -> ( $zero when self.mode == 0, $one when self.mode == 1 ) : -> $error \
                   $zero: 0 \
                   $one: 1 \
                   $error: -1 }";
        let Some(z) = run(src, "new M(\"0\", 0)", "cond_a") else {
            return;
        };
        assert_eq!(z.1, "0");
        assert_eq!(run(src, "new M(\"1\", 1)", "cond_b").unwrap().1, "1");
        assert_eq!(run(src, "new M(\"0\", 2)", "cond_c").unwrap().1, "-1");
    }

    #[test]
    fn php_multi_match() {
        let code = gen("@@fsm M(text: bytes) : int = 0 { /[0-9]/ -> $num | 99 $num: 1 }");
        let driver = "foreach ([\"5\", \"a\"] as $s) { echo (new M($s))->return_value, \"\\n\"; }";
        let Some(lines) = php_run(&code, driver, "mm") else {
            return;
        };
        assert_eq!(lines, vec!["1", "99"]);
    }

    #[test]
    fn php_embed_every_transition() {
        let code = gen(
            "@@fsm M(text: bytes) : int = 0 { /[0-9]+/ ${ tally() } self.count \
             actions: tally() { self.count = self.count + 1 } domain: count: int = 0 }",
        );
        let driver = "echo (new M(\"123\"))->return_value, \"\\n\";";
        let Some(lines) = php_run(&code, driver, "emb") else {
            return;
        };
        assert_eq!(lines[0], "3");
    }

    /// FSM-TEST-603 — `%{...}` fires when the DFA leaves its last accepting
    /// state, capturing the end of the matched region. For `/[0-9]+/` over
    /// "42x" that is `@@:cursor == 2`, not the failing `x` position.
    #[test]
    fn php_embed_leave_final() {
        let code = gen(
            "@@fsm M(text: bytes) : int = 0 { /[0-9]+/ %{ self.end_pos = @@:cursor } self.end_pos \
             domain: end_pos: int = 0 }",
        );
        let driver =
            "foreach ([\"42x\",\"abx\"] as $s) { echo (new M($s))->return_value, \"\\n\"; }";
        let Some(lines) = php_run(&code, driver, "leave") else {
            return;
        };
        assert_eq!(lines, vec!["2", "0"]);
    }

    #[test]
    fn php_token_alphabet() {
        let code = gen("@@fsm M(toks: token) : bool = false { /IDENT LPAREN RPAREN/ true }");
        let driver = "foreach ([[\"IDENT\",\"LPAREN\",\"RPAREN\"],[\"IDENT\",\"RPAREN\"],[\"IDENT\",\"WAT\"]] as $t) { echo var_export((new M($t))->accepted, true), \"\\n\"; }";
        let Some(lines) = php_run(&code, driver, "tok") else {
            return;
        };
        assert_eq!(lines, vec!["true", "false", "false"]);
    }

    #[test]
    fn php_mode_c_callout() {
        let inner = gen("@@fsm Digits(text: bytes) : int = 0 { /[0-9]+/ to_int(@@:matched) }");
        let outer = gen("@@fsm Outer(text: bytes) : int = 0 { $s: .d/@Digits/ $s.d.return_value }");
        let code = format!("{}\n{}", inner, outer);
        let driver = "foreach ([\"42\", \"x\"] as $s) { $m = new Outer($s); echo var_export($m->accepted, true), \" \", $m->return_value, \"\\n\"; }";
        let Some(lines) = php_run(&code, driver, "modec") else {
            return;
        };
        assert_eq!(lines, vec!["true 42", "false 0"]);
    }

    #[test]
    fn php_anchors() {
        let start = gen("@@fsm M(text: bytes) : bool = false { /^foo/ true }");
        let d1 = "foreach ([\"foo\", \"xfoo\"] as $s) { echo var_export((new M($s))->accepted, true), \"\\n\"; }";
        let Some(l1) = php_run(&start, d1, "anc_s") else {
            return;
        };
        assert_eq!(l1, vec!["true", "false"]);
        let end = gen("@@fsm M(text: bytes) : bool = false { /[0-9]+$/ true }");
        let d2 = "foreach ([\"123\", \"123x\"] as $s) { echo var_export((new M($s))->accepted, true), \"\\n\"; }";
        let Some(l2) = php_run(&end, d2, "anc_e") else {
            return;
        };
        assert_eq!(l2, vec!["true", "false"]);
    }

    #[test]
    fn php_word_boundary() {
        let wb = gen("@@fsm M(text: bytes) : bool = false { /\\bcat\\b/ true }");
        let d = "foreach ([\"cat\", \"cats\"] as $s) { echo var_export((new M($s))->accepted, true), \"\\n\"; }";
        let Some(l) = php_run(&wb, d, "wb") else {
            return;
        };
        assert_eq!(l, vec!["true", "false"]);
    }

    /// §11.1 — a lazy quantifier matches via the emitted Pike VM, not the DFA.
    /// `/.*?,/` is minimal up to the first comma; `/a*?b+/` is lazy-then-greedy.
    #[test]
    fn php_lazy_quantifier() {
        // `/.*?,/` over "ab,cd,ef" → leftmost-first match "ab," (@@:matched).
        let lazy = gen("@@fsm M(text: bytes) : bytes = \"\" { /.*?,/ @@:matched }");
        let d1 = "echo (new M(\"ab,cd,ef\"))->return_value, \"\\n\";";
        let Some(l1) = php_run(&lazy, d1, "lazy_comma") else {
            return;
        };
        assert_eq!(l1, vec!["ab,"]);

        // `/a*?b+/` over "aabbb" → lazy `a*?` (minimal) then greedy `b+` to the
        // end: match-end cursor is 5.
        let lazy2 = gen("@@fsm M(text: bytes) : int = 0 { /a*?b+/ @@:cursor }");
        let d2 = "echo (new M(\"aabbb\"))->return_value, \"\\n\";";
        let Some(l2) = php_run(&lazy2, d2, "lazy_ab") else {
            return;
        };
        assert_eq!(l2, vec!["5"]);
    }

    #[test]
    fn php_unsupported_errors() {
        let decl =
            parse_fsm_block(b"@@fsm M(text: bytes) : bool = false { /a$b/ true }").expect("parses");
        let err = generate(&decl).unwrap_err();
        assert!(err.contains("anchor"), "got {err}");
    }
}
