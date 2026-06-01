//! Rust backend for `@@fsm` (RFC-0042, Phase 8).
//!
//! Generates a Rust `struct` + `impl` from a validated `FsmDeclAst`,
//! mirroring the Python reference backend ([`super::fsm_python`]) but with
//! static types. The recognition model is identical (per-stage minimal
//! DFAs + a per-state dispatch loop); the novelties are Rust types
//! (Frame's abstract `int`/`bool`/`bytes` → `i64`/`bool`/`String`) and
//! ownership of the matched slice (the input is held as `Vec<char>`, the
//! matched run materialized into an owned `String`).
//!
//! # v0.1 first cut
//!
//! Supports single-match states, match stages, bare-expression returns,
//! and static + failure transitions over the `bytes`/`char` alphabets,
//! with the `@@:matched` / `to_int` / `to_str` / `len` built-ins. Not yet
//! handled (clear `Unsupported` error, never a silent miscompile): stage
//! captures (`$state.label`), conditional / stage-ref transition targets,
//! multi-match (`|`) states, embedding actions, declared `actions:`,
//! Mode C call-out, the token alphabet, and anchors.

use crate::frame_c::compiler::frame_ast::{
    BinaryOp, Expression, FsmDeclAst, FsmStateAst, FsmTransitionTarget, Literal, MatchAst,
    MatchElement, Type, UnaryOp,
};
use crate::frame_c::compiler::fsm_regex::{
    self, size_check::DEFAULT_MAX_DFA_STATES, subset::DfaLabel, Alphabet, CompileError,
};
use std::fmt::Write;

/// Generate Rust source implementing `decl`, or a reason it is outside the
/// v0.1 Rust cut.
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
    stage_dfas: Vec<StageDfa>,
}

impl<'a> Generator<'a> {
    fn new(decl: &'a FsmDeclAst) -> Result<Self, String> {
        let alphabet = match decl.params.first().map(|p| &p.param_type) {
            Some(Type::Custom(t)) if t == "char" => Alphabet::Char,
            Some(Type::Custom(t)) if t == "token" => {
                return Err("the token alphabet is not yet supported by the Rust backend".into())
            }
            _ => Alphabet::Bytes,
        };
        let mut label_to_index = std::collections::HashMap::new();
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

    fn compile_stage_dfas(&mut self) -> Result<(), String> {
        for st in &self.decl.states {
            if st.matches.len() > 1 {
                return Err(
                    "multi-match (`|`) states are not yet supported by the Rust backend".into(),
                );
            }
            let Some(m) = st.matches.first() else {
                continue;
            };
            for el in &m.elements {
                if let MatchElement::Stage(stage) = el {
                    if !stage.embedding_actions.is_empty() {
                        return Err(
                            "embedding actions are not yet supported by the Rust backend".into(),
                        );
                    }
                    if stage.label.is_some() {
                        return Err(
                            "stage captures (`$state.label`) are not yet supported by the Rust \
                             backend"
                                .into(),
                        );
                    }
                    if stage.regex.starts_with('@') {
                        return Err(
                            "Mode C (`/@Fsm/`) is not yet supported by the Rust backend".into()
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
                        "anchors are not yet supported by the Rust backend (v0.1 first cut)".into(),
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
                "regex `/{}/` uses anchors, not yet supported by the Rust backend",
                regex
            )),
        }
    }

    fn emit(&self) -> Result<String, String> {
        let mut out = String::new();
        out.push_str("// Generated by framec — RFC-0042 @@fsm (Rust backend).\n\n");
        self.emit_struct(&mut out);
        self.emit_impl(&mut out)?;
        Ok(out)
    }

    /// The Rust type for a Frame type string (shared with the @@system
    /// Rust backend's conventions: `int`→`i64`, `bool`→`bool`, …).
    fn rust_type(t: &Type) -> String {
        let s = match t {
            Type::Custom(s) => s.as_str(),
            _ => "()",
        };
        match s {
            "int" => "i64".to_string(),
            "float" => "f64".to_string(),
            "bool" => "bool".to_string(),
            "str" | "string" | "String" | "bytes" => "String".to_string(),
            other => other.to_string(),
        }
    }

    fn emit_struct(&self, out: &mut String) {
        writeln!(out, "pub struct {} {{", self.decl.name).ok();
        out.push_str("    pub accepted: bool,\n");
        out.push_str("    pub reject_position: usize,\n");
        out.push_str("    pub cursor: usize,\n");
        writeln!(
            out,
            "    pub return_value: {},",
            Self::rust_type(&self.decl.return_type)
        )
        .ok();
        // Auto-promoted parameters become fields. The input parameter is a
        // `Vec<char>` so the cursor indexes it in O(1); other params keep
        // their declared type.
        for (i, p) in self.decl.params.iter().enumerate() {
            if i == 0 {
                writeln!(out, "    pub {}: Vec<char>,", p.name).ok();
            } else {
                writeln!(
                    out,
                    "    pub {}: {},",
                    p.name,
                    Self::rust_type(&p.param_type)
                )
                .ok();
            }
        }
        if let Some(domain) = &self.decl.domain {
            for v in &domain.vars {
                // Skip a domain field re-declaring the input parameter (it
                // is already the `Vec<char>` field above).
                if self.decl.params.first().map(|p| &p.name) == Some(&v.name) {
                    continue;
                }
                writeln!(out, "    pub {}: {},", v.name, Self::rust_type(&v.var_type)).ok();
            }
        }
        out.push_str("    matched: String,\n");
        out.push_str("}\n\n");
    }

    fn emit_impl(&self, out: &mut String) -> Result<(), String> {
        writeln!(out, "impl {} {{", self.decl.name).ok();
        self.emit_new(out);
        self.emit_dfa_matcher(out);
        self.emit_run(out);
        self.emit_state_methods(out)?;
        out.push_str("}\n");
        Ok(())
    }

    fn emit_new(&self, out: &mut String) {
        let input = &self.decl.params[0].name;
        // Constructor signature: input as Vec<char>, other params typed.
        let mut sig = String::new();
        for (i, p) in self.decl.params.iter().enumerate() {
            if i > 0 {
                sig.push_str(", ");
            }
            if i == 0 {
                write!(sig, "{}: Vec<char>", p.name).ok();
            } else {
                write!(sig, "{}: {}", p.name, Self::rust_type(&p.param_type)).ok();
            }
        }
        writeln!(out, "    pub fn new({}) -> Self {{", sig).ok();
        out.push_str("        let mut _m = Self {\n");
        out.push_str("            accepted: false,\n");
        out.push_str("            reject_position: 0,\n");
        out.push_str("            cursor: 0,\n");
        writeln!(
            out,
            "            return_value: {},",
            Self::rust_default(&self.decl.return_type, &self.decl.default_expr)
        )
        .ok();
        for p in &self.decl.params {
            writeln!(out, "            {}: {},", p.name, p.name).ok();
        }
        if let Some(domain) = &self.decl.domain {
            for v in &domain.vars {
                if self.decl.params.first().map(|p| &p.name) == Some(&v.name) {
                    continue;
                }
                writeln!(out, "            {}: {},", v.name, self.expr(&v.default)).ok();
            }
        }
        out.push_str("            matched: String::new(),\n");
        out.push_str("        };\n");
        out.push_str("        _m.run();\n");
        out.push_str("        if _m.accepted { _m.reject_position = 0; }\n");
        writeln!(out, "        _m").ok();
        out.push_str("    }\n\n");
        let _ = input;
    }

    /// Greedy longest-match DFA executor (mirrors `_dfa_match` in Python).
    fn emit_dfa_matcher(&self, out: &mut String) {
        let input = &self.decl.params[0].name;
        writeln!(
            out,
            "    fn dfa_match(&self, states: &[(&[(u32, u32, usize)], bool)], start: usize) -> i64 {{\n\
             \x20       let mut st = start;\n\
             \x20       let mut pos = self.cursor;\n\
             \x20       let n = self.{input}.len();\n\
             \x20       let mut last: i64 = if states[st].1 {{ pos as i64 }} else {{ -1 }};\n\
             \x20       while pos < n {{\n\
             \x20           let v = self.{input}[pos] as u32;\n\
             \x20           let mut nxt: Option<usize> = None;\n\
             \x20           for &(lo, hi, tgt) in states[st].0 {{\n\
             \x20               if lo <= v && v <= hi {{ nxt = Some(tgt); break; }}\n\
             \x20           }}\n\
             \x20           match nxt {{\n\
             \x20               Some(t) => {{ st = t; pos += 1; if states[st].1 {{ last = pos as i64; }} }}\n\
             \x20               None => break,\n\
             \x20           }}\n\
             \x20       }}\n\
             \x20       last\n\
             \x20   }}\n\n",
            input = input
        )
        .ok();
    }

    fn emit_run(&self, out: &mut String) {
        out.push_str("    fn run(&mut self) {\n        let mut state: i64 = 0;\n");
        out.push_str("        while state >= 0 {\n            state = match state {\n");
        for i in 0..self.decl.states.len() {
            writeln!(out, "                {} => self.state_{}(),", i, i).ok();
        }
        out.push_str("                _ => return,\n            };\n        }\n    }\n\n");
    }

    fn emit_state_methods(&self, out: &mut String) -> Result<(), String> {
        let mut sid = 0usize;
        for (i, st) in self.decl.states.iter().enumerate() {
            match st.matches.first() {
                None => {
                    writeln!(out, "    fn state_{}(&mut self) -> i64 {{ -1 }}\n", i).ok();
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
        _st: &FsmStateAst,
        m: &MatchAst,
        sid: &mut usize,
    ) -> Result<(), String> {
        let input = &self.decl.params[0].name;
        let (success, failure) = self.resolve_targets(m)?;
        writeln!(out, "    fn state_{}(&mut self) -> i64 {{", index).ok();

        for el in &m.elements {
            match el {
                MatchElement::Stage(_) => {
                    let my_sid = *sid;
                    *sid += 1;
                    self.emit_dfa_const(out, my_sid);
                    writeln!(
                        out,
                        "        let _r = self.dfa_match(DFA, {});",
                        self.stage_dfas[my_sid].start
                    )
                    .ok();
                    out.push_str("        if _r < 0 {\n");
                    out.push_str("            self.accepted = false;\n");
                    out.push_str("            self.reject_position = self.cursor;\n");
                    writeln!(out, "            return {};", failure).ok();
                    out.push_str("        }\n");
                    writeln!(
                        out,
                        "        self.matched = self.{}[self.cursor..(_r as usize)].iter().collect();",
                        input
                    )
                    .ok();
                    out.push_str("        self.cursor = _r as usize;\n");
                    out.push_str("        self.accepted = true;\n");
                }
                MatchElement::BareExpression { expr, .. } => {
                    writeln!(out, "        self.return_value = {};", self.expr(expr)).ok();
                }
                MatchElement::ActionBlock(_) => {
                    return Err("action blocks are not yet supported by the Rust backend".into());
                }
            }
        }
        writeln!(out, "        {}", success).ok();
        out.push_str("    }\n\n");
        Ok(())
    }

    /// Emit the per-stage DFA as a `const DFA` inside the state method. A
    /// state with at most one stage is the v0.1 cut, so a single local
    /// `DFA` const per method suffices.
    fn emit_dfa_const(&self, out: &mut String, sid: usize) {
        let dfa = &self.stage_dfas[sid];
        let states: Vec<String> = dfa
            .states
            .iter()
            .map(|(trans, acc)| {
                let ts: Vec<String> = trans
                    .iter()
                    .map(|(lo, hi, tgt)| format!("({}, {}, {})", lo, hi, tgt))
                    .collect();
                format!("(&[{}], {})", ts.join(", "), acc)
            })
            .collect();
        writeln!(
            out,
            "        const DFA: &[(&[(u32, u32, usize)], bool)] = &[{}];",
            states.join(", ")
        )
        .ok();
    }

    /// Resolve a match's success/failure targets to `i64` dispatch
    /// expressions (`-1` halts). Only static state targets in v0.1.
    fn resolve_targets(&self, m: &MatchAst) -> Result<(String, String), String> {
        let Some(clause) = &m.transition else {
            return Ok(("-1".into(), "-1".into()));
        };
        let success = match &clause.success {
            None => "-1".to_string(),
            Some(t) => self.target_index(t)?.to_string(),
        };
        let failure = match &clause.failure {
            None => "-1".to_string(),
            Some(t) => self.target_index(t)?.to_string(),
        };
        Ok((success, failure))
    }

    fn target_index(&self, t: &FsmTransitionTarget) -> Result<i64, String> {
        match t {
            FsmTransitionTarget::Static {
                state, stage: None, ..
            } => self
                .label_to_index
                .get(state)
                .map(|i| *i as i64)
                .ok_or_else(|| format!("transition to undeclared state `${}`", state)),
            FsmTransitionTarget::Static { stage: Some(_), .. } => {
                Err("stage-ref transition targets are not yet supported by the Rust backend".into())
            }
            FsmTransitionTarget::Conditional(_) => Err(
                "conditional transition targets are not yet supported by the Rust backend".into(),
            ),
        }
    }

    /// Map a raw default token to a Rust expression of the field's type.
    fn rust_default(ty: &Type, raw: &str) -> String {
        match raw {
            "false" => "false".to_string(),
            "true" => "true".to_string(),
            "" => default_for(ty),
            s if s.starts_with('"') => format!("{}.to_string()", s),
            s => s.to_string(),
        }
    }

    /// Translate a Frame expression to a Rust expression.
    fn expr(&self, e: &Expression) -> String {
        match e {
            Expression::Literal(l) => match l {
                Literal::Int(i) => i.to_string(),
                Literal::Float(f) => f.to_string(),
                Literal::Bool(b) => b.to_string(),
                Literal::String(s) => format!("{:?}.to_string()", s),
                Literal::Null => "Default::default()".to_string(),
            },
            Expression::Var(name) => match name.as_str() {
                "@@:matched" => "self.matched.clone()".to_string(),
                "@@:cursor" => "(self.cursor as i64)".to_string(),
                "@@:return" => "self.return_value".to_string(),
                _ => name.clone(),
            },
            Expression::Binary { left, op, right } => {
                format!("({} {} {})", self.expr(left), binop(op), self.expr(right))
            }
            Expression::Unary { op, expr } => match op {
                UnaryOp::Not => format!("(!{})", self.expr(expr)),
                UnaryOp::Neg => format!("(-{})", self.expr(expr)),
                UnaryOp::BitNot => format!("(!{})", self.expr(expr)),
            },
            Expression::Call { func, args } => self.call(func, args),
            Expression::Member { object, field } => {
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
            // `to_int` parses the matched slice; `len` is the element count.
            "to_int" => format!("({}).parse::<i64>().unwrap_or(0)", a.join(", ")),
            "to_str" => format!("({}).to_string()", a.join(", ")),
            "len" => format!("({}.len() as i64)", a.join(", ")),
            _ => format!("self.{}({})", func, a.join(", ")),
        }
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

/// A type-appropriate Rust default when no initializer token is present.
fn default_for(ty: &Type) -> String {
    let s = match ty {
        Type::Custom(s) => s.as_str(),
        _ => "",
    };
    match s {
        "int" => "0".to_string(),
        "float" => "0.0".to_string(),
        "bool" => "false".to_string(),
        "str" | "string" | "String" | "bytes" => "String::new()".to_string(),
        _ => "Default::default()".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frame_c::compiler::fsm_parser::parse_fsm_block;
    use std::process::Command;

    /// Generate Rust for `src`, append a `main` that constructs the fsm
    /// over `input` and prints `accepted` + `return_value`, compile with
    /// `rustc`, run, and return `(accepted, return_value)`. `None` if
    /// `rustc` is unavailable.
    fn run(src: &str, class: &str, input: &str, tag: &str) -> Option<(String, String)> {
        let decl = parse_fsm_block(src.as_bytes()).expect("fixture must parse");
        let code = generate(&decl).expect("fixture must generate");
        let chars: String = input
            .chars()
            .map(|c| format!("'{}'", c))
            .collect::<Vec<_>>()
            .join(", ");
        let driver = format!(
            "{code}\nfn main() {{\n    let m = {class}::new(vec![{chars}]);\n    println!(\"{{}}\", m.accepted);\n    println!(\"{{:?}}\", m.return_value);\n}}\n",
            code = code,
            class = class,
            chars = chars
        );
        let dir = std::env::temp_dir();
        let src_path = dir.join(format!("framec_rs_{}.rs", tag));
        let bin_path = dir.join(format!("framec_rs_{}", tag));
        std::fs::write(&src_path, driver).expect("write rs");
        let compile = match Command::new("rustc")
            .arg("-O")
            .arg("--edition=2021")
            .arg(&src_path)
            .arg("-o")
            .arg(&bin_path)
            .output()
        {
            Ok(o) => o,
            Err(_) => return None, // rustc absent — skip
        };
        assert!(
            compile.status.success(),
            "rustc failed for {:?}:\n{}",
            src,
            String::from_utf8_lossy(&compile.stderr)
        );
        let out = Command::new(&bin_path).output().expect("run binary");
        let text = String::from_utf8_lossy(&out.stdout);
        let lines: Vec<&str> = text.lines().collect();
        Some((lines[0].to_string(), lines[1].to_string()))
    }

    #[test]
    fn rust_smoke_bool() {
        let src = "@@fsm M(text: bytes) : bool = false { /a/ true }";
        let Some((acc, ret)) = run(src, "M", "a", "smoke_a") else {
            return;
        };
        assert_eq!((acc.as_str(), ret.as_str()), ("true", "true"));
        let (acc2, _) = run(src, "M", "b", "smoke_b").unwrap();
        assert_eq!(acc2, "false");
    }

    #[test]
    fn rust_matched_to_int() {
        let src = "@@fsm M(text: bytes) : int = 0 { /[0-9]+/ to_int(@@:matched) }";
        let Some((acc, ret)) = run(src, "M", "123", "tok_a") else {
            return;
        };
        assert_eq!((acc.as_str(), ret.as_str()), ("true", "123"));
        assert_eq!(run(src, "M", "x", "tok_b").unwrap().0, "false");
    }

    #[test]
    fn rust_len_self_input() {
        let src = "@@fsm M(text: bytes) : int = 0 { /[0-9]+/ len(self.text) }";
        let Some((_, ret)) = run(src, "M", "123", "len_a") else {
            return;
        };
        assert_eq!(ret, "3");
    }

    /// A construct outside the v0.1 Rust cut errors clearly.
    #[test]
    fn rust_unsupported_errors() {
        let decl =
            parse_fsm_block(b"@@fsm M(toks: token) : bool = false { /A/ true }").expect("parses");
        assert!(generate(&decl).is_err());
    }
}
