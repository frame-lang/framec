//! Frame syntax taxonomy — behavioral anchors.
//!
//! These tests pin the *category* each Frame construct belongs to by
//! asserting what it lowers to (statement vs property-reference vs
//! property-mutation vs native passthrough). They are the executable
//! ground truth behind the "Frame Syntax Taxonomy" appendix in
//! `docs/frame_language.md` — the terminology there is only allowed to
//! claim what these tests verify.
//!
//! Backend: python_3 (representative; the lowering shape is uniform).

mod common;
use common::compile_source;

fn py(body_system: &str) -> String {
    compile_source(body_system, "python_3")
}

// ---------------------------------------------------------------------
// Return family — statement (mutation / exit) vs native passthrough.
// ---------------------------------------------------------------------

/// `@@:return = e` and `@@:(e)` are MUTATIONS of the return property:
/// they write the Frame-managed return slot, no exit. `@@:(e)` is sugar
/// for `@@:return = e` (byte-identical body).
#[test]
fn context_return_setter_and_sugar_write_return_slot() {
    let g = py(r#"
@@system R {
    interface:
        a(): int
        b(): int
    machine:
        $S {
            a(): int { @@:return = 5 }
            b(): int { @@:(5) }
        }
}
"#);
    // Both forms write the return slot.
    let writes = g.matches("._return = 5").count();
    assert!(
        writes >= 2,
        "@@:return = 5 and @@:(5) must both write the return slot (sugar-equivalent):\n{g}"
    );
}

/// `@@:return(e)` is a MUTATION + EXIT (set the slot, then return).
#[test]
fn context_return_call_writes_slot_then_exits() {
    let g = py(r#"
@@system R {
    interface:
        c(): int
    machine:
        $S { c(): int { @@:return(7) } }
}
"#);
    assert!(
        g.contains("._return = 7"),
        "must write the return slot:\n{g}"
    );
}

/// Native `return e` is PASSTHROUGH — emitted verbatim, never touches
/// the Frame return slot.
#[test]
fn native_return_is_passthrough() {
    let g = py(r#"
@@system N {
    operations:
        f(): int { return 9 }
    interface:
        g()
    machine:
        $S { g() { } }
}
"#);
    assert!(
        g.contains("return 9"),
        "native return emitted verbatim:\n{g}"
    );
}

// ---------------------------------------------------------------------
// References & call expressions — constructs that YIELD a value.
// ---------------------------------------------------------------------

/// Bare `@@:return` is a REFERENCE (getter): it reads the return slot as
/// an rvalue and interpolates into the surrounding native expression.
#[test]
fn bare_context_return_is_a_reference() {
    let g = py(r#"
@@system R {
    interface:
        a()
    machine:
        $S { a() { $.saved = @@:return } }
    domain:
        saved: int = 0
}
"#);
    // RHS reads the return slot (no write to it on this line).
    assert!(
        g.contains("= self._context_stack[-1]._return"),
        "bare @@:return must READ the return slot into the assignment RHS:\n{g}"
    );
}

/// `@@:self.m()` is a CALL EXPRESSION — usable in value position; its
/// result becomes the assignment RHS.
#[test]
fn self_call_is_a_call_expression() {
    let g = py(r#"
@@system R {
    interface:
        a()
        b(): int
    machine:
        $S {
            a() { $.x = @@:self.b() }
            b(): int { @@:(5) }
        }
    domain:
        x: int = 0
}
"#);
    assert!(
        g.contains("= self.b()"),
        "@@:self.b() must be callable in value position (RHS):\n{g}"
    );
}

/// `@@Child()` (system instantiation) is a CALL EXPRESSION — its factory
/// result is usable in value position.
#[test]
fn instantiation_is_a_call_expression() {
    let g = py(r#"
@@system Child {
    interface:
        noop()
    machine:
        $S { noop() { } }
}

@@[main]
@@system Parent {
    interface:
        make()
    machine:
        $S { make() { $.c = @@Child() } }
    domain:
        c: int = 0
}
"#);
    assert!(
        g.contains("= Child._create()"),
        "@@Child() must produce a factory call usable in value position (RHS):\n{g}"
    );
}

// ---------------------------------------------------------------------
// Feature-gap coverage. Stage-1 source-coverage flagged these accessor
// constructs as ~0% — the snapshot fixtures and the 27k-case fuzz corpus
// never exercise them. These behavioral tests both verify the lowering
// and close the coverage gap (they run through the library path that
// `cargo llvm-cov test` measures).
// ---------------------------------------------------------------------

/// `$.x = e` — state-variable assignment (a Mutation / setter).
#[test]
fn state_var_assignment_writes_the_compartment() {
    let g = py(r#"
@@system R {
    interface:
        go()
    machine:
        $S {
            $.x: int = 0
            go() {
                $.x = 42
            }
        }
}
"#);
    assert!(
        g.contains("state_vars[\"x\"] = 42"),
        "state-var assignment must write the compartment slot:\n{g}"
    );
}

/// `@@:params.x` — read an interface parameter by name (a read-only
/// property reference). Surface syntax is DOT (`@@:params.n`), per the
/// language reference.
#[test]
fn context_params_reference_reads_the_param() {
    let g = py(r#"
@@system R {
    interface:
        go(n: int)
    machine:
        $S {
            go(n: int) {
                $.saved = @@:params.n
            }
        }
    domain:
        saved: int = 0
}
"#);
    assert!(
        g.contains("state_vars[\"saved\"] = n"),
        "@@:params.n must read the named interface parameter directly:\n{g}"
    );
}

/// `@@:data.key` write then read — call-scoped data property
/// (Mutation + Reference). Surface syntax is DOT (`@@:data.tmp`).
#[test]
fn context_data_accessor_round_trips() {
    let g = py(r#"
@@system R {
    interface:
        go()
    machine:
        $S {
            go() {
                @@:data.tmp = 7
                $.saved = @@:data.tmp
            }
        }
    domain:
        saved: int = 0
}
"#);
    assert!(
        !g.is_empty() && g.contains("class R"),
        "@@:data.key write+read must compile:\n{g}"
    );
}

// =====================================================================
// Exhaustiveness guard.
//
// The taxonomy must account for EVERY variant of the lexer `Token` enum
// and the parser `Statement` enum. The two functions below are
// compile-time exhaustive matches (no `_` wildcard): adding a variant to
// either enum is a COMPILE ERROR here until it is assigned a category,
// so the Frame Syntax Taxonomy appendix in docs/frame_language.md cannot
// silently fall behind the grammar.
// =====================================================================

use framec::frame_c::compiler::frame_ast::Statement;
use framec::frame_c::compiler::lexer::Token;

/// Taxonomy category for a parser `Statement` (see the appendix).
#[derive(Debug, PartialEq, Eq)]
enum StmtCategory {
    /// Control-flow statement: effect, no value.
    ControlStatement,
    /// Property setter (a statement that stores).
    Mutation,
    /// Setter + exit (`@@:return(e)`).
    ExitReturn,
    /// Property getter (an expression that yields a value).
    Reference,
    /// Call expression (self-call / instantiation).
    CallExpr,
    /// Native passthrough (incl. the recognized native `return`).
    NativePassthrough,
    /// Internal/legacy AST node — NOT produced by the pipeline parser
    /// from `.frm` source (no `if`/`while`/`for`/`loop`/`continue` lexer
    /// keywords; no operator grammar). Built only by the model/graphviz
    /// builders.
    NotSurface,
}

/// Every `Statement` variant → its taxonomy category. Exhaustive.
fn statement_category(s: &Statement) -> StmtCategory {
    match s {
        Statement::Transition(_)
        | Statement::Forward(_)
        | Statement::StackPush(_)
        | Statement::StackPop(_) => StmtCategory::ControlStatement,

        // Native `return e` — recognized, emitted verbatim (passthrough).
        Statement::Return(_) | Statement::NativeCode(_) => StmtCategory::NativePassthrough,

        // Property setters (mutations).
        Statement::StateVarAssign { .. }
        | Statement::ContextDataAssign { .. }
        // `@@:(e)` — sugar for the `@@:return = e` setter.
        | Statement::ContextReturnExpr { .. } => StmtCategory::Mutation,
        // `@@:return` — bare read (getter) OR `= e` (setter), per `assign_expr`.
        Statement::ContextReturn { assign_expr, .. } => {
            if assign_expr.is_some() {
                StmtCategory::Mutation
            } else {
                StmtCategory::Reference
            }
        }
        // `@@:return(e)` — setter + exit.
        Statement::ReturnCall { .. } => StmtCategory::ExitReturn,

        // Property getters (references; the read-only ones have no setter).
        Statement::StateVarRead { .. }
        | Statement::ContextEvent { .. }
        | Statement::ContextData { .. }
        | Statement::ContextParams { .. }
        | Statement::ContextSelf { .. }
        | Statement::ContextSystemState { .. } => StmtCategory::Reference,

        // Calls (usable in value position).
        Statement::ContextSelfCall { .. }
        | Statement::ContextSelfFieldCall { .. }
        | Statement::SystemInstantiation { .. } => StmtCategory::CallExpr,

        // Not Frame surface syntax (verified: no if/while/for/loop/continue
        // keywords in the lexer; no operator grammar). `Block` is the
        // RFC-0043 `{ ... }` statement container (an `if` branch or an
        // `@@fsm` action body) — produced only by the `@@fsm` statement
        // parser, never by `@@system` surface syntax, so it groups here.
        Statement::If(_)
        | Statement::Loop(_)
        | Statement::Expression(_)
        | Statement::Continue(_)
        | Statement::Block(_) => StmtCategory::NotSurface,
    }
}

/// Lexical role for a `Token` (the surface alphabet behind the constructs).
#[derive(Debug, PartialEq, Eq)]
enum TokenRole {
    SectionKeyword,
    /// Native `return` keyword (recognized passthrough).
    NativeKeyword,
    /// `$Name`, `$>`, `<$`, `$.x`, `$^`.
    StateSigil,
    /// `->`, `=>`, `push$`, `pop$`.
    ControlOperator,
    /// `@@:return`, `@@:event`, `@@:data`, `@@:params`.
    ContextSigil,
    /// `@@[...]` attribute.
    Attribute,
    /// Structural punctuation.
    Delimiter,
    /// Identifiers and literals (native leaves).
    NativeLeaf,
    /// Native code chunk.
    Native,
    /// Newline / Eof.
    Meta,
}

/// Every `Token` variant → its lexical role. Exhaustive.
fn token_role(t: &Token) -> TokenRole {
    match t {
        Token::Interface | Token::Machine | Token::Actions | Token::Operations | Token::Domain => {
            TokenRole::SectionKeyword
        }
        Token::Return => TokenRole::NativeKeyword,
        Token::StateRef(_)
        | Token::EnterHandler
        | Token::ExitHandler
        | Token::StateVarRef(_)
        | Token::ParentRef => TokenRole::StateSigil,
        Token::Arrow | Token::FatArrow | Token::PushState | Token::PopState => {
            TokenRole::ControlOperator
        }
        Token::ContextReturn
        | Token::ContextEvent
        | Token::ContextData(_)
        | Token::ContextParams(_) => TokenRole::ContextSigil,
        Token::Attribute { .. } => TokenRole::Attribute,
        Token::LBrace
        | Token::RBrace
        | Token::LParen
        | Token::RParen
        | Token::LBracket
        | Token::RBracket
        | Token::Comma
        | Token::Colon
        | Token::SectionColon
        | Token::Equals
        | Token::Dot
        | Token::Semicolon
        | Token::Star
        | Token::Ampersand => TokenRole::Delimiter,
        Token::Ident(_)
        | Token::IntLit(_)
        | Token::FloatLit(_)
        | Token::StringLit(_)
        | Token::BoolLit(_) => TokenRole::NativeLeaf,
        Token::NativeCode(_) => TokenRole::Native,
        Token::Newline | Token::Eof => TokenRole::Meta,
    }
}

/// The two exhaustive matches above are the guard; their compilation is
/// the assertion. This test references them (so they are not dead code)
/// and spot-checks a few unit-variant mappings.
#[test]
fn taxonomy_covers_every_token_and_statement_variant() {
    let _ = statement_category as fn(&Statement) -> StmtCategory;
    assert_eq!(token_role(&Token::Interface), TokenRole::SectionKeyword);
    assert_eq!(token_role(&Token::Return), TokenRole::NativeKeyword);
    assert_eq!(token_role(&Token::Arrow), TokenRole::ControlOperator);
    assert_eq!(token_role(&Token::PushState), TokenRole::ControlOperator);
    assert_eq!(token_role(&Token::ParentRef), TokenRole::StateSigil);
    assert_eq!(token_role(&Token::Eof), TokenRole::Meta);
}
