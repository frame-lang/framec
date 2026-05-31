//! Python reference backend for `@@fsm` (RFC-0042).
//!
//! This is the first target that actually *runs* an `@@fsm`. It is a
//! self-contained generator — the `@@fsm` runtime model (§5) is distinct
//! from `@@system`'s, so it does not reuse the `CodegenNode` pipeline.
//!
//! Each stage's `/regex/` is compiled to a minimal DFA by
//! [`crate::frame_c::compiler::fsm_regex`]; the emitted class carries the
//! DFA tables as data and drives recognition with a per-state dispatch
//! loop implementing §5.2 (construction), §5.3 (execution), §5.5
//! (transitions), and §5.6 (failure).
//!
//! Acceptance follows the RE2 recognizer model (§5.3): the fsm accepts
//! iff the input is in the recognized language — i.e. recognition halts
//! on a *successful match completion* at a terminal state. A stage
//! failure (or a conditional transition matching no `when`) that routes
//! to a terminal rejects; a failure branch to a non-terminal continues
//! and may still accept (the classifier idiom). `emit_failure` records
//! the rejection (`accepted = False`); a later stage success flips it
//! back. `reject_position` is normalized to 0 on acceptance.
//!
//! # v0.1 scope
//!
//! Supports single-match states, match stages (with `.label` captures),
//! bare-expression returns, action blocks (assignment / `if`-`else`
//! statements), and static + conditional (`when`) success/failure
//! transitions over the `bytes`/`char` alphabets. Constructs not yet
//! handled — multi-match (`|`) states, stage-ref transition targets,
//! failure-only clauses, embedding actions, declared `actions:`, and the
//! token alphabet — produce a clear `Unsupported` error rather than a
//! silent miscompile.

use crate::frame_c::compiler::frame_ast::{
    BinaryOp, Expression, FsmDeclAst, FsmStateAst, FsmTransitionTarget, Literal, MatchAst,
    MatchElement, Statement, Type, UnaryOp,
};
use crate::frame_c::compiler::fsm_regex::{
    self, size_check::DEFAULT_MAX_DFA_STATES, subset::DfaLabel, Alphabet, CompileError,
};
use std::collections::HashMap;
use std::fmt::Write;

/// Generate a Python module source implementing `decl`. Returns the
/// source on success, or a human-readable reason the construct is not yet
/// supported by this backend.
pub fn generate(decl: &FsmDeclAst) -> Result<String, String> {
    Generator::new(decl)?.emit()
}

/// Per-stage compiled DFA, flattened to the data the emitter needs.
struct StageDfa {
    /// One entry per DFA state: `(transitions, is_accept)` where each
    /// transition is `(low, high, target)`.
    states: Vec<(Vec<(u32, u32, usize)>, bool)>,
    start: usize,
}

struct Generator<'a> {
    decl: &'a FsmDeclAst,
    alphabet: Alphabet,
    /// State label → dispatch index. Unlabeled start state has no entry
    /// but is index 0.
    label_to_index: HashMap<String, usize>,
    /// Compiled DFAs, one per stage, in traversal order; the state code
    /// references them by index as `self._DFA_<n>`.
    stage_dfas: Vec<StageDfa>,
}

impl<'a> Generator<'a> {
    fn new(decl: &'a FsmDeclAst) -> Result<Self, String> {
        let alphabet = match decl.params.first().map(|p| &p.param_type) {
            Some(Type::Custom(t)) if t == "bytes" => Alphabet::Bytes,
            Some(Type::Custom(t)) if t == "char" => Alphabet::Char,
            Some(Type::Custom(t)) if t == "token" => {
                return Err("the token alphabet is not yet supported by the Python backend".into())
            }
            _ => Alphabet::Bytes,
        };

        // Map state labels to dispatch indices (declaration order).
        let mut label_to_index = HashMap::new();
        for (i, st) in decl.states.iter().enumerate() {
            if let Some(l) = &st.label {
                label_to_index.insert(l.clone(), i);
            }
        }

        let mut g = Generator {
            decl,
            alphabet,
            label_to_index,
            stage_dfas: Vec::new(),
        };
        g.compile_stage_dfas()?;
        Ok(g)
    }

    /// Compile every stage regex up-front so codegen can reference DFA
    /// tables by index and reject unsupported constructs early.
    fn compile_stage_dfas(&mut self) -> Result<(), String> {
        for st in &self.decl.states {
            if st.matches.len() > 1 {
                return Err(format!(
                    "multi-match (`|`) states are not yet supported (state `{}`)",
                    st.label.as_deref().unwrap_or("<start>")
                ));
            }
            let Some(m) = st.matches.first() else {
                continue;
            };
            for el in &m.elements {
                if let MatchElement::Stage(stage) = el {
                    if !stage.embedding_actions.is_empty() {
                        return Err(
                            "embedding actions are not yet supported by the Python backend".into(),
                        );
                    }
                    let dfa = self.compile_one(&stage.regex)?;
                    self.stage_dfas.push(dfa);
                }
            }
        }
        Ok(())
    }

    fn compile_one(&self, regex: &str) -> Result<StageDfa, String> {
        match fsm_regex::compile(regex, self.alphabet, DEFAULT_MAX_DFA_STATES) {
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
                "regex `/{}/` uses anchors, which the v0.1 DFA engine does not yet support",
                regex
            )),
        }
    }

    fn emit(&self) -> Result<String, String> {
        let mut out = String::new();
        out.push_str("# Generated by framec — RFC-0042 @@fsm (Python reference backend).\n\n");
        self.emit_preamble(&mut out);
        self.emit_class(&mut out)?;
        Ok(out)
    }

    /// Shared runtime helpers, emitted once.
    fn emit_preamble(&self, out: &mut String) {
        out.push_str(
            "def _frame_to_int(s):\n    return int(s)\n\n\
             def _frame_len(s):\n    return len(s)\n\n",
        );
    }

    fn emit_class(&self, out: &mut String) -> Result<(), String> {
        let name = &self.decl.name;
        writeln!(out, "class {}:", name).ok();

        self.emit_dfa_tables(out);
        self.emit_ctor(out);
        self.emit_dfa_matcher(out);
        self.emit_run(out);
        self.emit_state_methods(out)?;
        Ok(())
    }

    fn emit_dfa_tables(&self, out: &mut String) {
        for (i, dfa) in self.stage_dfas.iter().enumerate() {
            let states: Vec<String> = dfa
                .states
                .iter()
                .map(|(trans, acc)| {
                    let ts: Vec<String> = trans
                        .iter()
                        .map(|(lo, hi, tgt)| format!("({}, {}, {})", lo, hi, tgt))
                        .collect();
                    format!("([{}], {})", ts.join(", "), py_bool(*acc))
                })
                .collect();
            writeln!(
                out,
                "    _DFA_{} = ([{}], {})",
                i,
                states.join(", "),
                dfa.start
            )
            .ok();
        }
        out.push('\n');
    }

    fn emit_ctor(&self, out: &mut String) {
        // __init__ signature: input param positional, others with defaults.
        let mut sig = String::from("self");
        for (i, p) in self.decl.params.iter().enumerate() {
            if i == 0 {
                write!(sig, ", {}", p.name).ok();
            } else {
                match &p.default {
                    Some(d) => write!(sig, ", {}={}", p.name, py_default(d)).ok(),
                    None => write!(sig, ", {}", p.name).ok(),
                };
            }
        }
        writeln!(out, "    def __init__({}):", sig).ok();

        // §5.2: auto-promote each parameter to a domain field.
        for p in &self.decl.params {
            writeln!(out, "        self.{} = {}", p.name, p.name).ok();
        }
        // Explicit domain fields (auto fields are already in scope).
        if let Some(domain) = &self.decl.domain {
            for v in &domain.vars {
                // An explicit field re-declaring a parameter keeps the
                // parameter binding (§5.2 step 4 overrides only when given
                // a distinct default); emit its initializer.
                writeln!(out, "        self.{} = {}", v.name, expr_to_py(&v.default)).ok();
            }
        }

        // Observable fields (§5.1) + recognition scratch.
        writeln!(
            out,
            "        self.return_value = {}",
            py_default(&self.decl.default_expr)
        )
        .ok();
        out.push_str(
            "        self.accepted = False\n\
             \x20       self.reject_position = 0\n\
             \x20       self.cursor = 0\n\
             \x20       self._matched = \"\"\n\
             \x20       self._cap = {}\n\
             \x20       self._run()\n\
             \x20       if self.accepted:\n\
             \x20           self.reject_position = 0\n\n",
        );
    }

    /// Greedy longest-match DFA executor over `self.text` from the cursor.
    /// Returns the end position of the longest match, or -1 if the stage
    /// does not match (not even the empty string).
    fn emit_dfa_matcher(&self, out: &mut String) {
        out.push_str(
            "    def _dfa_match(self, dfa):\n\
             \x20       states, start = dfa\n\
             \x20       st = start\n\
             \x20       pos = self.cursor\n\
             \x20       n = len(self.text)\n\
             \x20       last = pos if states[st][1] else -1\n\
             \x20       while pos < n:\n\
             \x20           v = ord(self.text[pos])\n\
             \x20           nxt = None\n\
             \x20           for lo, hi, tgt in states[st][0]:\n\
             \x20               if lo <= v <= hi:\n\
             \x20                   nxt = tgt\n\
             \x20                   break\n\
             \x20           if nxt is None:\n\
             \x20               break\n\
             \x20           st = nxt\n\
             \x20           pos += 1\n\
             \x20           if states[st][1]:\n\
             \x20               last = pos\n\
             \x20       return last\n\n",
        );
    }

    fn emit_run(&self, out: &mut String) {
        out.push_str("    def _run(self):\n        state = 0\n        while state >= 0:\n");
        for i in 0..self.decl.states.len() {
            let kw = if i == 0 { "if" } else { "elif" };
            writeln!(
                out,
                "            {} state == {}:\n                state = self._state_{}()",
                kw, i, i
            )
            .ok();
        }
        // A target index out of range should never occur (validator E731),
        // but guard defensively.
        out.push_str("            else:\n                return\n\n");
    }

    fn emit_state_methods(&self, out: &mut String) -> Result<(), String> {
        let mut sid = 0usize; // running global stage-DFA index
        for (i, st) in self.decl.states.iter().enumerate() {
            let m = match st.matches.first() {
                Some(m) => m,
                None => {
                    // A stateless state just halts (terminal, no change to
                    // accepted). Rare; emit a no-op method.
                    writeln!(out, "    def _state_{}(self):\n        return -1\n", i).ok();
                    continue;
                }
            };
            self.emit_one_state(out, i, st, m, &mut sid)?;
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
        writeln!(out, "    def _state_{}(self):", index).ok();

        for el in &m.elements {
            match el {
                MatchElement::Stage(stage) => {
                    let my_sid = *sid;
                    *sid += 1;
                    writeln!(out, "        _r = self._dfa_match(self._DFA_{})", my_sid).ok();
                    out.push_str("        if _r < 0:\n");
                    // The stage failed: follow the failure branch (or §5.6).
                    // emit_failure records the rejection (accepted=False).
                    self.emit_failure(out, m, "            ")?;
                    out.push_str("        self._matched = self.text[self.cursor:_r]\n");
                    if let Some(slabel) = &stage.label {
                        writeln!(
                            out,
                            "        self._cap[{:?}] = self._matched",
                            format!("{}.{}", state_label, slabel)
                        )
                        .ok();
                    }
                    out.push_str("        self.cursor = _r\n");
                    out.push_str("        self.accepted = True\n");
                }
                MatchElement::BareExpression { expr, .. } => {
                    writeln!(out, "        self.return_value = {}", expr_to_py(expr)).ok();
                }
                MatchElement::ActionBlock(blk) => {
                    // Action blocks consume no input and (in v0.1) cannot
                    // fail; emit their statements inline at the method body
                    // indent.
                    for st in &blk.statements {
                        out.push_str(&stmt_to_py(st, "        ")?);
                    }
                }
            }
        }

        // All elements succeeded: follow the success branch (or halt at a
        // terminal). `accepted` already reflects the last stage.
        self.emit_success(out, m, "        ")?;
        out.push('\n');
        Ok(())
    }

    /// Emit the success-branch transition after a match completes. A
    /// static target returns its index; a conditional target evaluates
    /// each `when` in order and, if none holds, the failure branch fires
    /// (FSM-TEST-402). No transition halts (terminal, accepted stands).
    fn emit_success(&self, out: &mut String, m: &MatchAst, indent: &str) -> Result<(), String> {
        match m.transition.as_ref() {
            None => {
                writeln!(out, "{}return -1", indent).ok();
                Ok(())
            }
            Some(clause) => self.emit_target(out, &clause.success, indent, &|out, indent| {
                // No success condition held → the failure branch fires.
                self.emit_failure(out, m, indent)
            }),
        }
    }

    /// Emit the failure-branch resolution: the failure target's transition
    /// if present, else §5.6 (halt). Used both on a stage failure and as
    /// the fallback when a conditional success matches no `when`.
    ///
    /// Per the RE2 recognizer model (§5.3), reaching here is a rejection
    /// event: it records `accepted = False` and the reject position. If the
    /// failure branch routes to a non-terminal state that later completes a
    /// match, a subsequent stage success flips `accepted` back to True.
    fn emit_failure(&self, out: &mut String, m: &MatchAst, indent: &str) -> Result<(), String> {
        writeln!(out, "{}self.accepted = False", indent).ok();
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

    /// Emit `return <index>` for a target. A conditional target emits an
    /// ordered `if <when>: return <idx>` chain, then `on_none` for the
    /// no-match case.
    fn emit_target(
        &self,
        out: &mut String,
        target: &FsmTransitionTarget,
        indent: &str,
        on_none: &dyn Fn(&mut String, &str) -> Result<(), String>,
    ) -> Result<(), String> {
        match target {
            FsmTransitionTarget::Static { .. } => {
                let idx = self.static_index(target)?;
                writeln!(out, "{}return {}", indent, idx).ok();
                Ok(())
            }
            FsmTransitionTarget::Conditional(alts) => {
                let inner = format!("{}    ", indent);
                for alt in alts {
                    writeln!(out, "{}if {}:", indent, expr_to_py(&alt.condition)).ok();
                    let idx = self.static_index(&alt.target)?;
                    writeln!(out, "{}return {}", inner, idx).ok();
                }
                on_none(out, indent)
            }
        }
    }

    /// Dispatch index for a static (state-only) target. Stage-ref targets
    /// are not yet supported.
    fn static_index(&self, t: &FsmTransitionTarget) -> Result<usize, String> {
        match t {
            FsmTransitionTarget::Static {
                state, stage: None, ..
            } => self
                .label_to_index
                .get(state)
                .copied()
                .ok_or_else(|| format!("transition to undeclared state `${}`", state)),
            FsmTransitionTarget::Static { stage: Some(_), .. } => Err(
                "stage-ref transition targets (`$State.stage`) are not yet supported \
                 by the Python backend"
                    .into(),
            ),
            FsmTransitionTarget::Conditional(_) => {
                Err("a conditional target may not nest another conditional target".into())
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Statement translation (action-block bodies, RFC-0043)
// ---------------------------------------------------------------------------

/// Render a statement as Python source lines, each prefixed with `indent`.
/// Action-block statements are expression statements (incl. assignments)
/// and `if/else`; transitions are not valid here (E712, caught earlier).
fn stmt_to_py(stmt: &Statement, indent: &str) -> Result<String, String> {
    match stmt {
        Statement::Expression(e) => Ok(format!("{}{}\n", indent, expr_to_py(&e.expr))),
        Statement::If(if_ast) => {
            let inner = format!("{}    ", indent);
            let mut s = format!("{}if {}:\n", indent, expr_to_py(&if_ast.condition));
            s.push_str(&stmt_to_py(&if_ast.then_branch, &inner)?);
            if let Some(else_b) = &if_ast.else_branch {
                s.push_str(&format!("{}else:\n", indent));
                s.push_str(&stmt_to_py(else_b, &inner)?);
            }
            Ok(s)
        }
        Statement::Block(blk) => {
            if blk.statements.is_empty() {
                return Ok(format!("{}pass\n", indent));
            }
            let mut s = String::new();
            for st in &blk.statements {
                s.push_str(&stmt_to_py(st, indent)?);
            }
            Ok(s)
        }
        other => Err(format!(
            "statement form {:?} is not supported in @@fsm action blocks by the Python backend",
            std::mem::discriminant(other)
        )),
    }
}

// ---------------------------------------------------------------------------
// Expression translation
// ---------------------------------------------------------------------------

/// Translate a Frame expression to a Python expression string.
fn expr_to_py(e: &Expression) -> String {
    match e {
        Expression::Literal(l) => literal_to_py(l),
        Expression::Var(name) => var_to_py(name),
        Expression::Binary { left, op, right } => {
            format!(
                "({} {} {})",
                expr_to_py(left),
                binop_to_py(op),
                expr_to_py(right)
            )
        }
        Expression::Unary { op, expr } => match op {
            UnaryOp::Not => format!("(not {})", expr_to_py(expr)),
            UnaryOp::Neg => format!("(-{})", expr_to_py(expr)),
            UnaryOp::BitNot => format!("(~{})", expr_to_py(expr)),
        },
        Expression::Call { func, args } => call_to_py(func, args),
        Expression::Member { object, field } => {
            // `self.field` stays `self.field`; nested members chain.
            format!("{}.{}", expr_to_py(object), field)
        }
        Expression::Index { object, index } => {
            format!("{}[{}]", expr_to_py(object), expr_to_py(index))
        }
        Expression::Assign { target, value } => {
            format!("{} = {}", expr_to_py(target), expr_to_py(value))
        }
        Expression::NativeExpr(s) => s.clone(),
    }
}

fn literal_to_py(l: &Literal) -> String {
    match l {
        Literal::Int(i) => i.to_string(),
        Literal::Float(f) => f.to_string(),
        Literal::String(s) => format!("{:?}", s),
        Literal::Bool(b) => py_bool(*b).to_string(),
        Literal::Null => "None".to_string(),
    }
}

/// Variable references include the `@@:` context probes and `$state.stage`
/// captures, which map to recognition scratch.
fn var_to_py(name: &str) -> String {
    match name {
        "@@:matched" => "self._matched".to_string(),
        "@@:cursor" => "self.cursor".to_string(),
        "@@:return" => "self.return_value".to_string(),
        _ => {
            if let Some(cap) = name.strip_prefix('$') {
                // `$state.stage` capture reference.
                format!("self._cap[{:?}]", cap)
            } else {
                name.to_string()
            }
        }
    }
}

fn call_to_py(func: &str, args: &[Expression]) -> String {
    let rendered: Vec<String> = args.iter().map(expr_to_py).collect();
    match func {
        // RFC-0042 built-ins.
        "to_int" => format!("_frame_to_int({})", rendered.join(", ")),
        "len" => format!("_frame_len({})", rendered.join(", ")),
        // Anything else is emitted verbatim (e.g. a declared action, once
        // those are supported).
        _ => format!("{}({})", func, rendered.join(", ")),
    }
}

fn binop_to_py(op: &BinaryOp) -> &'static str {
    match op {
        BinaryOp::Add => "+",
        BinaryOp::Sub => "-",
        BinaryOp::Mul => "*",
        BinaryOp::Div => "//",
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

fn py_bool(b: bool) -> &'static str {
    if b {
        "True"
    } else {
        "False"
    }
}

/// Map a raw default-value token (from the parser) to Python source.
fn py_default(raw: &str) -> String {
    match raw {
        "false" => "False".to_string(),
        "true" => "True".to_string(),
        "null" | "nil" | "None" => "None".to_string(),
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frame_c::compiler::fsm_parser::parse_fsm_block;
    use std::process::Command;

    /// One observed run of a generated fsm: the four observable fields,
    /// as their Python `repr`/`str` text.
    struct Run {
        accepted: String,
        return_value: String,
        cursor: String,
        reject: String,
    }

    /// Generate Python for `src`, run it on `input` via `python3`, and
    /// return the observed fields. Returns `None` if `python3` is absent
    /// (so the test self-skips in environments without it).
    fn run(src: &str, input: &str, tag: &str) -> Option<Run> {
        let decl = parse_fsm_block(src.as_bytes()).expect("fixture must parse");
        let code = generate(&decl).expect("fixture must generate");
        let driver = format!(
            "{code}\nimport sys\nm = {name}(sys.argv[1])\n\
             print(m.accepted)\nprint(repr(m.return_value))\n\
             print(m.cursor)\nprint(m.reject_position)\n",
            code = code,
            name = decl.name
        );
        let path = std::env::temp_dir().join(format!("framec_fsm_{}.py", tag));
        std::fs::write(&path, driver).expect("write temp py");

        let out = match Command::new("python3").arg(&path).arg(input).output() {
            Ok(o) => o,
            Err(_) => return None, // python3 not available — skip
        };
        assert!(
            out.status.success(),
            "python3 failed for {:?} on {:?}: {}",
            src,
            input,
            String::from_utf8_lossy(&out.stderr)
        );
        let text = String::from_utf8_lossy(&out.stdout);
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 4, "unexpected output: {:?}", text);
        Some(Run {
            accepted: lines[0].to_string(),
            return_value: lines[1].to_string(),
            cursor: lines[2].to_string(),
            reject: lines[3].to_string(),
        })
    }

    /// Generate + run `src` on `input`, returning the `repr` of each
    /// requested instance expression (e.g. `"m.return_value"`, `"m.flag"`).
    /// `None` if python3 is unavailable.
    fn eval_py(src: &str, input: &str, exprs: &[&str], tag: &str) -> Option<Vec<String>> {
        let decl = parse_fsm_block(src.as_bytes()).expect("fixture must parse");
        let code = generate(&decl).expect("fixture must generate");
        let prints: String = exprs
            .iter()
            .map(|e| format!("print(repr({}))\n", e))
            .collect();
        let driver = format!(
            "{code}\nimport sys\nm = {name}(sys.argv[1])\n{prints}",
            code = code,
            name = decl.name,
            prints = prints
        );
        let path = std::env::temp_dir().join(format!("framec_fsm_{}.py", tag));
        std::fs::write(&path, driver).expect("write temp py");
        let out = match Command::new("python3").arg(&path).arg(input).output() {
            Ok(o) => o,
            Err(_) => return None,
        };
        assert!(
            out.status.success(),
            "python3 failed for {:?}: {}",
            src,
            String::from_utf8_lossy(&out.stderr)
        );
        Some(
            String::from_utf8_lossy(&out.stdout)
                .lines()
                .map(|l| l.to_string())
                .collect(),
        )
    }

    /// FSM-TEST-030 — multi-statement action block (`;`-separated),
    /// mutating domain fields.
    #[test]
    fn fsm_test_030_action_block_semicolons() {
        let src = "@@fsm M(text: bytes) : int = 0 { \
                   /[0-9]/ { self.count = self.count + 1; self.flag = true } \
                   self.count \
                   domain: count: int = 0  flag: bool = false }";
        let Some(v) = eval_py(src, "5", &["m.return_value", "m.flag", "m.count"], "t030") else {
            return;
        };
        assert_eq!(v, vec!["1", "True", "1"]);
    }

    /// FSM-TEST-031 — same logic, whitespace-separated statements.
    #[test]
    fn fsm_test_031_action_block_whitespace() {
        let src = "@@fsm M(text: bytes) : int = 0 { \
                   /[0-9]/ { self.count = self.count + 1  self.flag = true } \
                   self.count \
                   domain: count: int = 0  flag: bool = false }";
        let Some(v) = eval_py(src, "5", &["m.return_value", "m.flag", "m.count"], "t031") else {
            return;
        };
        assert_eq!(v, vec!["1", "True", "1"]);
    }

    /// FSM-TEST-032 — if/else in an action block.
    #[test]
    fn fsm_test_032_if_else() {
        let src = "@@fsm M(text: bytes) : int = 0 { \
                   /[0-9]/ { if to_int(@@:matched) > 5 { self.flag = true } else { self.flag = false } } \
                   to_int(@@:matched) \
                   domain: flag: bool = false }";
        let Some(seven) = eval_py(src, "7", &["m.return_value", "m.flag"], "t032a") else {
            return;
        };
        assert_eq!(seven, vec!["7", "True"]);
        let three = eval_py(src, "3", &["m.return_value", "m.flag"], "t032b").unwrap();
        assert_eq!(three, vec!["3", "False"]);
    }

    /// FSM-TEST-001 — the smoke test.
    #[test]
    fn fsm_test_001_minimal() {
        let src = "@@fsm M(text: bytes) : bool = false { /a/ true }";
        let Some(a) = run(src, "a", "t001a") else {
            return;
        };
        assert_eq!(a.accepted, "True");
        assert_eq!(a.return_value, "True");
        assert_eq!(a.cursor, "1");

        let b = run(src, "b", "t001b").unwrap();
        assert_eq!(b.accepted, "False");
        assert_eq!(b.return_value, "False");
        assert_eq!(b.reject, "0");

        let empty = run(src, "", "t001e").unwrap();
        assert_eq!(empty.accepted, "False");
    }

    /// FSM-TEST-002 — single-digit, `to_int(@@:matched)`.
    #[test]
    fn fsm_test_002_matched_builtin() {
        let src = "@@fsm M(text: bytes) : int = 0 { /[0-9]/ to_int(@@:matched) }";
        let seven = run(src, "7", "t002a").unwrap_or_else(no_py);
        if seven.accepted.is_empty() {
            return;
        }
        assert_eq!(seven.accepted, "True");
        assert_eq!(seven.return_value, "7");
        let a = run(src, "a", "t002b").unwrap();
        assert_eq!(a.accepted, "False");
        assert_eq!(a.reject, "0");
    }

    /// FSM-TEST-005 — `len(self.text)` is the full input length.
    #[test]
    fn fsm_test_005_self_text() {
        let src = "@@fsm M(text: bytes) : int = 0 { /[0-9]+/ len(self.text) }";
        let Some(r) = run(src, "123", "t005a") else {
            return;
        };
        assert_eq!(r.return_value, "3");
        assert_eq!(r.accepted, "True");
        let r2 = run(src, "123456789", "t005b").unwrap();
        assert_eq!(r2.return_value, "9");
    }

    /// FSM-TEST-006 — labeled states, success + failure transitions.
    /// Reaching `$error` via a failure branch is `accepted == false`.
    #[test]
    fn fsm_test_006_transitions() {
        let src = "@@fsm M(text: bytes) : int = 0 { \
                   $0: /[a-z]/ -> $digits : -> $error \
                   $digits: .n/[0-9]+/ to_int($digits.n) \
                   $error: -1 }";
        let Some(ok) = run(src, "x42", "t006a") else {
            return;
        };
        assert_eq!(ok.accepted, "True");
        assert_eq!(ok.return_value, "42");

        let big = run(src, "X", "t006b").unwrap();
        assert_eq!(
            big.accepted, "False",
            "failure-branch terminal is not accepted"
        );
        assert_eq!(big.return_value, "-1");

        let dig = run(src, "3", "t006c").unwrap();
        assert_eq!(dig.accepted, "False");
        assert_eq!(dig.return_value, "-1");
    }

    /// FSM-TEST-007 — stage label capture, anchored-prefix match.
    #[test]
    fn fsm_test_007_capture() {
        let src = "@@fsm M(text: bytes) : bytes = \"\" { $main: .x/[0-9]+/ $main.x }";
        let Some(r) = run(src, "123", "t007a") else {
            return;
        };
        assert_eq!(r.return_value, "'123'");
        let r2 = run(src, "123abc", "t007b").unwrap();
        assert_eq!(r2.return_value, "'123'");
        assert_eq!(r2.cursor, "3");
        let r3 = run(src, "abc", "t007c").unwrap();
        assert_eq!(r3.accepted, "False");
    }

    /// FSM-TEST-402 — conditional transition target; first true `when`
    /// wins, falling through all conditions fires the failure branch.
    #[test]
    fn fsm_test_402_conditional_target() {
        let src = "@@fsm M(text: bytes, mode: int) : int = 0 { \
                   /[01]/ -> ( $zero when self.mode == 0, $one when self.mode == 1 ) : -> $error \
                   $zero: 0 \
                   $one: 1 \
                   $error: -1 }";
        // M takes two args; the driver passes mode as a second arg.
        let decl = parse_fsm_block(src.as_bytes()).expect("parses");
        let code = generate(&decl).expect("generates");
        let run_mode = |inp: &str, mode: &str, tag: &str| -> Option<String> {
            let driver = format!(
                "{code}\nimport sys\nm = M(sys.argv[1], int(sys.argv[2]))\nprint(repr(m.return_value))\n",
                code = code
            );
            let path = std::env::temp_dir().join(format!("framec_fsm_{}.py", tag));
            std::fs::write(&path, driver).ok()?;
            let out = Command::new("python3")
                .arg(&path)
                .arg(inp)
                .arg(mode)
                .output()
                .ok()?;
            assert!(
                out.status.success(),
                "{}",
                String::from_utf8_lossy(&out.stderr)
            );
            Some(String::from_utf8_lossy(&out.stdout).trim().to_string())
        };
        let Some(zero) = run_mode("0", "0", "t402a") else {
            return;
        };
        assert_eq!(zero, "0");
        assert_eq!(run_mode("1", "1", "t402b").unwrap(), "1");
        assert_eq!(run_mode("0", "2", "t402c").unwrap(), "-1"); // no when matches → failure
    }

    /// Static transitions across multiple states (success + failure
    /// branches). Uses an explicit success arrow on the intermediate
    /// state, since failure-only clauses (`/b/ true : -> $error`) are a
    /// separate parser feature (tracked follow-up).
    #[test]
    fn static_transitions_multi_state() {
        let src = "@@fsm M(text: bytes) : bool = false { \
                   /a/ -> $next : -> $error \
                   $next: /b/ -> $ok : -> $error \
                   $ok: true \
                   $error: false }";
        let Some(ab) = run(src, "ab", "t400a") else {
            return;
        };
        assert_eq!(ab.accepted, "True");
        assert_eq!(ab.return_value, "True");
        let ax = run(src, "ax", "t400b").unwrap();
        assert_eq!(ax.accepted, "False");
        assert_eq!(ax.return_value, "False");
        assert_eq!(run(src, "x", "t400c").unwrap().accepted, "False");
    }

    /// A construct outside the v0.1 backend cut errors clearly rather than
    /// miscompiling.
    #[test]
    fn unsupported_anchor_errors() {
        let decl =
            parse_fsm_block(b"@@fsm M(text: bytes) : bool = false { /^a/ true }").expect("parses");
        let err = generate(&decl).unwrap_err();
        assert!(err.contains("anchor"), "got {err}");
    }

    /// Sentinel for the python3-absent skip path in tests that can't use
    /// `let-else` for the first run.
    fn no_py() -> Run {
        Run {
            accepted: String::new(),
            return_value: String::new(),
            cursor: String::new(),
            reject: String::new(),
        }
    }
}
