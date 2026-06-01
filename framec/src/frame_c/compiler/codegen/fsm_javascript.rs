//! JavaScript backend for `@@fsm` (RFC-0042, Phase 8).
//!
//! Generates a self-contained JavaScript `class` from a validated
//! `FsmDeclAst`, mirroring the Python reference backend
//! ([`super::fsm_python`]). JavaScript is class-based with mutable fields and
//! dynamic typing, so the recognition model is a near-transliteration of the
//! Python generator: per-stage minimal DFAs + a per-state dispatch loop over
//! mutable instance state. The observable result (§5.1) is the constructed
//! instance's `accepted`, `return_value`, `cursor`, and `reject_position`.
//!
//! # v0.1 scope (first cut)
//!
//! Supports single-match states, match stages with `.label` captures,
//! bare-expression returns, action blocks (assignment / `if`-`else`),
//! declared `actions:` helpers, and all transition forms — static,
//! conditional (`when`), stage-ref (`-> $S.stage`, via a `this.enter`
//! re-entry index), and failure-only — over the `bytes`/`char` alphabets,
//! with the `@@:matched` / `to_int` / `to_str` / `len` built-ins. Not yet
//! handled (clear `Unsupported` error, never a silent miscompile):
//! multi-match (`|`) states, embedding actions, Mode C call-out, the token
//! alphabet, and anchors. These land in later increments, matching the Rust
//! backend's build-out.

use crate::frame_c::compiler::frame_ast::{
    BinaryOp, Expression, FsmDeclAst, FsmStateAst, FsmTransitionTarget, Literal, MatchAst,
    MatchElement, Type, UnaryOp,
};
use crate::frame_c::compiler::fsm_regex::{
    self, size_check::DEFAULT_MAX_DFA_STATES, subset::DfaLabel, Alphabet, CompileError,
};
use std::fmt::Write;

/// Generate JavaScript source implementing `decl`, or a reason it is outside
/// the v0.1 JavaScript cut.
pub fn generate(decl: &FsmDeclAst) -> Result<String, String> {
    Generator::new(decl)?.emit()
}

/// One stage's compiled DFA, flattened for emission.
struct StageDfa {
    states: Vec<(Vec<(u32, u32, usize)>, bool)>,
    start: usize,
}

struct Generator<'a> {
    decl: &'a FsmDeclAst,
    alphabet: Alphabet,
    label_to_index: std::collections::HashMap<String, usize>,
    /// `(state label, stage label)` → element index, for stage-ref re-entry
    /// (`-> $State.stage`). Single-match states only.
    stage_entry: std::collections::HashMap<(String, String), usize>,
    stage_dfas: Vec<StageDfa>,
}

impl<'a> Generator<'a> {
    fn new(decl: &'a FsmDeclAst) -> Result<Self, String> {
        let alphabet = match decl.params.first().map(|p| &p.param_type) {
            Some(Type::Custom(t)) if t == "char" => Alphabet::Char,
            Some(Type::Custom(t)) if t == "token" => {
                return Err(
                    "the token alphabet is not yet supported by the JavaScript backend".into(),
                )
            }
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
            stage_dfas: Vec::new(),
        };
        g.compile_stage_dfas()?;
        Ok(g)
    }

    /// Compile every stage's DFA in traversal order, so the emitted `DFA_<n>`
    /// consts line up with the index the state emitters advance.
    fn compile_stage_dfas(&mut self) -> Result<(), String> {
        for st in &self.decl.states {
            if st.matches.len() > 1 {
                return Err(
                    "multi-match (`|`) states are not yet supported by the JavaScript backend"
                        .into(),
                );
            }
            let Some(m) = st.matches.first() else {
                continue;
            };
            for el in &m.elements {
                if let MatchElement::Stage(stage) = el {
                    if !stage.embedding_actions.is_empty() {
                        return Err(
                            "embedding actions are not yet supported by the JavaScript backend"
                                .into(),
                        );
                    }
                    if stage.regex.starts_with('@') {
                        return Err(
                            "Mode C (`/@Fsm/`) is not yet supported by the JavaScript backend"
                                .into(),
                        );
                    }
                    self.stage_dfas.push(self.compile_one(&stage.regex)?);
                }
            }
        }
        Ok(())
    }

    fn compile_one(&self, regex: &str) -> Result<StageDfa, String> {
        match fsm_regex::compile(regex, self.alphabet, DEFAULT_MAX_DFA_STATES) {
            Ok(compiled) => {
                if compiled.requires_start || compiled.requires_end {
                    return Err(
                        "anchors are not yet supported by the JavaScript backend (v0.1 first cut)"
                            .into(),
                    );
                }
                let mut states = Vec::with_capacity(compiled.dfa.states.len());
                for s in &compiled.dfa.states {
                    let mut trans = Vec::new();
                    for t in &s.transitions {
                        let (lo, hi) = match &t.label {
                            DfaLabel::Byte(b) => (*b as u32, *b as u32),
                            DfaLabel::ByteRange { low, high } => (*low as u32, *high as u32),
                            DfaLabel::CodePoint(c) => (*c as u32, *c as u32),
                            DfaLabel::CodePointRange { low, high } => (*low as u32, *high as u32),
                            DfaLabel::Token(_) => {
                                return Err("token-alphabet DFA is not supported here".into())
                            }
                        };
                        trans.push((lo, hi, t.to));
                    }
                    states.push((trans, s.is_accept));
                }
                Ok(StageDfa {
                    states,
                    start: compiled.dfa.start,
                })
            }
            Err(CompileError::Diagnostics(ds)) => Err(format!(
                "regex `/{}/` failed to compile: {}",
                regex,
                ds.first().map(|d| d.message.as_str()).unwrap_or("")
            )),
            Err(CompileError::UnsupportedAnchors(_)) => Err(format!(
                "regex `/{}/` uses anchors, not yet supported by the JavaScript backend",
                regex
            )),
        }
    }

    fn emit(&self) -> Result<String, String> {
        let mut out = String::new();
        out.push_str("// Generated by framec — RFC-0042 @@fsm (JavaScript backend).\n\n");
        writeln!(out, "class {} {{", self.decl.name).ok();
        self.emit_ctor(&mut out);
        self.emit_dfa_matcher(&mut out);
        self.emit_run(&mut out);
        self.emit_state_methods(&mut out)?;
        self.emit_action_methods(&mut out)?;
        out.push_str("}\n");
        Ok(out)
    }

    fn emit_ctor(&self, out: &mut String) {
        let params: Vec<&str> = self.decl.params.iter().map(|p| p.name.as_str()).collect();
        writeln!(out, "  constructor({}) {{", params.join(", ")).ok();
        out.push_str("    this.accepted = false;\n");
        out.push_str("    this.reject_position = 0;\n");
        out.push_str("    this.cursor = 0;\n");
        writeln!(
            out,
            "    this.return_value = {};",
            js_default(&self.decl.default_expr)
        )
        .ok();
        // Auto-promote each parameter to an instance field (§5.2).
        for p in &self.decl.params {
            writeln!(out, "    this.{} = {};", p.name, p.name).ok();
        }
        // Explicit domain fields (auto fields already in scope).
        if let Some(domain) = &self.decl.domain {
            for v in &domain.vars {
                writeln!(out, "    this.{} = {};", v.name, self.expr(&v.default)).ok();
            }
        }
        out.push_str("    this.matched = \"\";\n");
        // Stage-ref re-entry point (`-> $State.stage` sets it; the dispatch
        // loop consumes it). 0 = enter at the state's first element.
        out.push_str("    this.enter = 0;\n");
        // One field per labeled stage in a labeled state, holding the matched
        // slice for `$state.label` reads.
        for f in self.capture_fields() {
            writeln!(out, "    this.{} = \"\";", f).ok();
        }
        out.push_str("    this.run();\n");
        out.push_str("    if (this.accepted) this.reject_position = 0;\n");
        out.push_str("  }\n\n");
    }

    /// Field names for every labeled stage in a labeled state (the targets of
    /// `$state.label` capture reads).
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

    /// Greedy longest-match DFA executor (mirrors `_dfa_match` in Python).
    fn emit_dfa_matcher(&self, out: &mut String) {
        let input = &self.decl.params[0].name;
        writeln!(
            out,
            "  _dfaMatch(states, start) {{\n\
             \x20   let st = start;\n\
             \x20   let pos = this.cursor;\n\
             \x20   const n = this.{input}.length;\n\
             \x20   let last = states[st][1] ? pos : -1;\n\
             \x20   while (pos < n) {{\n\
             \x20     const v = this.{input}.charCodeAt(pos);\n\
             \x20     let nxt = null;\n\
             \x20     for (const [lo, hi, tgt] of states[st][0]) {{ if (lo <= v && v <= hi) {{ nxt = tgt; break; }} }}\n\
             \x20     if (nxt === null) break;\n\
             \x20     st = nxt; pos++;\n\
             \x20     if (states[st][1]) last = pos;\n\
             \x20   }}\n\
             \x20   return last;\n\
             \x20 }}\n\n",
            input = input
        )
        .ok();
    }

    fn emit_run(&self, out: &mut String) {
        out.push_str("  run() {\n    let state = 0;\n");
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
            match st.matches.first() {
                None => {
                    writeln!(out, "  state_{}(_enter) {{ return -1; }}\n", i).ok();
                }
                Some(m) => self.emit_one_state(out, i, st, m, &mut sid)?,
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
        writeln!(out, "  state_{}(_enter) {{", index).ok();
        // Each element is guarded by `if (_enter <= idx)` so a stage-ref
        // re-entry skips leading elements (plain entry has `_enter === 0`).
        for (idx, el) in m.elements.iter().enumerate() {
            writeln!(out, "    if (_enter <= {}) {{", idx).ok();
            self.emit_element(out, el, m, &state_label, "      ", sid)?;
            out.push_str("    }\n");
        }
        self.emit_success(out, m, "    ");
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
                self.emit_dfa_const(out, my_sid, ind);
                writeln!(
                    out,
                    "{}const _r = this._dfaMatch(DFA_{}, {});",
                    ind, my_sid, self.stage_dfas[my_sid].start
                )
                .ok();
                writeln!(out, "{}if (_r < 0) {{", ind).ok();
                self.emit_failure(out, m, &ind2);
                writeln!(out, "{}}}", ind).ok();
                writeln!(
                    out,
                    "{}this.matched = this.{}.slice(this.cursor, _r);",
                    ind, input
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
                writeln!(out, "{}this.cursor = _r;", ind).ok();
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

    /// Emit the success-branch transition after a match completes.
    fn emit_success(&self, out: &mut String, m: &MatchAst, ind: &str) {
        match m.transition.as_ref().and_then(|c| c.success.as_ref()) {
            None => {
                writeln!(out, "{}return -1;", ind).ok();
            }
            Some(target) => self.emit_target(out, target, ind, m),
        }
    }

    /// Emit the failure-branch resolution (sets `accepted = false` and the
    /// reject position, then routes to the failure target or §5.6 halt).
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

    /// Emit a transition target. A static target returns its state index; a
    /// conditional target is an ordered `if (<when>) return <idx>;` chain, the
    /// match's failure branch as the no-`when` fallback (§3.5.4).
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
                // No `when` held → the failure branch fires (a reject).
                self.emit_failure(out, m, ind);
            }
        }
    }

    /// Emit `return <idx>;` for a static target; a stage-ref sets
    /// `this.enter` first.
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

    /// Translate an action-block statement to JavaScript source lines.
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
                "statement {:?} not supported in @@fsm action blocks by the JavaScript backend",
                std::mem::discriminant(other)
            )),
        }
    }

    /// Emit declared `actions:` helpers as methods. A trailing bare
    /// expression is the action's return value (§3.7 implicit tail).
    fn emit_action_methods(&self, out: &mut String) -> Result<(), String> {
        let Some(block) = &self.decl.actions else {
            return Ok(());
        };
        for act in &block.actions {
            let params: Vec<&str> = act.params.iter().map(|p| p.name.as_str()).collect();
            writeln!(out, "  {}({}) {{", act.name, params.join(", ")).ok();
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

    /// Emit the per-stage DFA as a `const DFA_<sid>` (sid-suffixed so a
    /// multi-stage state has distinct consts), at indent `ind`.
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
        writeln!(out, "{}const DFA_{} = [{}];", ind, sid, states.join(", ")).ok();
    }

    /// Translate a Frame expression to a JavaScript expression.
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
                // `$state.label` reads a stage capture (the matched slice).
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
                // `self.field` reads an instance field.
                if let Expression::Var(name) = object.as_ref() {
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

/// Instance field name holding a stage capture: `$state.label` →
/// `cap_state_label`.
fn cap_field(state: &str, label: &str) -> String {
    format!("cap_{}_{}", state, label)
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

/// Map a raw default-value token to a JavaScript expression.
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

    /// Generate JS for `src`, append a driver that constructs the fsm over
    /// `input` and prints `accepted` + `return_value`, run via `node`, and
    /// return `(accepted, return_value)`. `None` if `node` is unavailable.
    fn run(src: &str, class: &str, input: &str, tag: &str) -> Option<(String, String)> {
        let decl = parse_fsm_block(src.as_bytes()).expect("fixture must parse");
        let code = generate(&decl).expect("fixture must generate");
        let driver = format!(
            "{code}\nconst m = new {class}({input:?});\nconsole.log(String(m.accepted));\nconsole.log(String(m.return_value));\n",
            code = code,
            class = class,
            input = input
        );
        let dir = std::env::temp_dir();
        let path = dir.join(format!("framec_js_{}.js", tag));
        std::fs::write(&path, driver).expect("write js");
        let out = match Command::new("node").arg(&path).output() {
            Ok(o) => o,
            Err(_) => return None, // node absent — skip
        };
        assert!(
            out.status.success(),
            "node failed for {:?}:\n{}",
            src,
            String::from_utf8_lossy(&out.stderr)
        );
        let text = String::from_utf8_lossy(&out.stdout);
        let lines: Vec<&str> = text.lines().collect();
        Some((lines[0].to_string(), lines[1].to_string()))
    }

    #[test]
    fn js_smoke_bool() {
        let src = "@@fsm M(text: bytes) : bool = false { /a/ true }";
        let Some((acc, ret)) = run(src, "M", "a", "smoke_a") else {
            return;
        };
        assert_eq!((acc.as_str(), ret.as_str()), ("true", "true"));
        assert_eq!(run(src, "M", "b", "smoke_b").unwrap().0, "false");
    }

    #[test]
    fn js_matched_to_int() {
        let src = "@@fsm M(text: bytes) : int = 0 { /[0-9]+/ to_int(@@:matched) }";
        let Some((acc, ret)) = run(src, "M", "123", "tok_a") else {
            return;
        };
        assert_eq!((acc.as_str(), ret.as_str()), ("true", "123"));
        assert_eq!(run(src, "M", "x", "tok_b").unwrap().0, "false");
    }

    #[test]
    fn js_len_self_input() {
        let src = "@@fsm M(text: bytes) : int = 0 { /[0-9]+/ len(self.text) }";
        let Some((_, ret)) = run(src, "M", "123", "len_a") else {
            return;
        };
        assert_eq!(ret, "3");
    }

    /// Stage capture: `.n/[0-9]+/` captures the matched slice as `$s.n`.
    #[test]
    fn js_stage_capture() {
        let src = "@@fsm M(text: bytes) : int = 0 { $s: .n/[0-9]+/ to_int($s.n) }";
        let Some((acc, ret)) = run(src, "M", "42", "cap_a") else {
            return;
        };
        assert_eq!((acc.as_str(), ret.as_str()), ("true", "42"));
    }

    /// Action block mutating a domain field, returned by a bare expression.
    #[test]
    fn js_action_block() {
        let src = "@@fsm M(text: bytes) : int = 0 { \
                   /[0-9]/ { self.count = self.count + 1 } self.count \
                   domain: count: int = 0 }";
        let Some((_, ret)) = run(src, "M", "5", "act_a") else {
            return;
        };
        assert_eq!(ret, "1");
    }

    /// Declared `actions:` helper, callable from a match with a return value.
    #[test]
    fn js_declared_action() {
        let src = "@@fsm M(text: bytes) : int = 0 { \
                   /[0-9]+/ parse_int(@@:matched) \
                   actions: parse_int(s: bytes): int { to_int(s) } }";
        let Some((_, ret)) = run(src, "M", "42", "decl_a") else {
            return;
        };
        assert_eq!(ret, "42");
    }

    /// FSM-TEST-006: labeled states, static success + failure transitions,
    /// capture read across states.
    #[test]
    fn js_transitions_and_capture() {
        let src = "@@fsm M(text: bytes) : int = 0 { \
                   $0: /[a-z]/ -> $digits : -> $error \
                   $digits: .n/[0-9]+/ to_int($digits.n) \
                   $error: -1 }";
        let Some((acc, ret)) = run(src, "M", "x42", "tr_a") else {
            return;
        };
        assert_eq!((acc.as_str(), ret.as_str()), ("true", "42"));
        assert_eq!(run(src, "M", "X", "tr_b").unwrap().1, "-1");
    }

    /// Conditional `when` target (FSM-TEST-402).
    #[test]
    fn js_conditional_target() {
        let src = "@@fsm M(text: bytes, mode: int) : int = 0 { \
                   /[01]/ -> ( $zero when self.mode == 0, $one when self.mode == 1 ) : -> $error \
                   $zero: 0 \
                   $one: 1 \
                   $error: -1 }";
        let decl = parse_fsm_block(src.as_bytes()).expect("parses");
        let code = generate(&decl).expect("generates");
        let run_mode = |inp: &str, mode: i64, tag: &str| -> Option<String> {
            let driver = format!(
                "{code}\nconst m = new M({inp:?}, {mode});\nconsole.log(String(m.return_value));\n",
                code = code,
                inp = inp,
                mode = mode
            );
            let path = std::env::temp_dir().join(format!("framec_js_{}.js", tag));
            std::fs::write(&path, driver).ok()?;
            let o = Command::new("node").arg(&path).output().ok()?;
            assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
            Some(String::from_utf8_lossy(&o.stdout).trim().to_string())
        };
        let Some(z) = run_mode("0", 0, "cond_a") else {
            return;
        };
        assert_eq!(z, "0");
        assert_eq!(run_mode("1", 1, "cond_b").unwrap(), "1");
        assert_eq!(run_mode("0", 2, "cond_c").unwrap(), "-1"); // no when → failure
    }

    /// A construct outside the first cut errors clearly.
    #[test]
    fn js_unsupported_errors() {
        let decl =
            parse_fsm_block(b"@@fsm M(toks: token) : bool = false { /A/ true }").expect("parses");
        assert!(generate(&decl).is_err());
    }
}
