//! Frame AST - Abstract Syntax Tree for Frame language constructs
//!
//! This module defines the AST representation for Frame, which is used
//! in the hybrid compiler architecture to represent Frame constructs independently
//! of native code, before merging into a unified Hybrid AST.
//!
//! This is the SINGLE unified AST for Frame V4. The old `ast.rs` module has been
//! merged into this file to eliminate the dual-AST problem.

/// Span represents a source location in the original Frame code
#[derive(Debug, Clone, PartialEq)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

impl Span {
    pub fn new(start: usize, end: usize) -> Self {
        Self { start, end }
    }
}

/// Type information for parameters and variables.
/// Frame has no type system — types are opaque strings passed through verbatim.
/// All user-written types are stored as Custom(original_text).
#[derive(Debug, Clone, PartialEq)]
pub enum Type {
    /// Any named type — stores the user's original type text verbatim
    Custom(String),
    /// Unknown/inferred type (no type annotation provided)
    Unknown,
}

// ============================================================================
// Section and Attribute Types (merged from old ast.rs)
// ============================================================================

/// Kinds of sections in a Frame system
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SystemSectionKind {
    Operations,
    Interface,
    Machine,
    Actions,
    Domain,
}

/// Section span tracking for validation (tracks where each section is located)
#[derive(Debug, Clone, Default)]
pub struct SystemSectionSpans {
    pub operations: Option<Span>,
    pub interface: Option<Span>,
    pub machine: Option<Span>,
    pub actions: Option<Span>,
    pub domain: Option<Span>,
}

/// Persistence attribute parsed from `@@persist` annotation
#[derive(Debug, Clone)]
pub struct PersistAttr {
    /// Optional custom save method name. When None, language-specific
    /// defaults are used (e.g., save_to_json / saveToJson).
    pub save_name: Option<String>,
    /// Optional custom restore method name. When None, language-specific
    /// defaults are used (e.g., restore_from_json / restoreFromJson).
    pub restore_name: Option<String>,
    /// Serialization library for Rust (e.g., "serde")
    pub library: Option<String>,
    pub span: Span,
}

/// Root AST node - either a system or a module
#[derive(Debug, Clone)]
pub enum FrameAst {
    System(SystemAst),
    Module(ModuleAst),
}

/// Module containing multiple systems
#[derive(Debug, Clone)]
pub struct ModuleAst {
    pub name: String,
    pub systems: Vec<SystemAst>,
    pub imports: Vec<Import>,
    pub span: Span,
}

/// Import statement
#[derive(Debug, Clone)]
pub struct Import {
    pub module: String,
    pub symbols: Vec<String>,
    pub alias: Option<String>,
    pub span: Span,
}

/// Frame system definition
#[derive(Debug, Clone)]
pub struct SystemAst {
    pub name: String,
    pub params: Vec<SystemParam>,
    /// Base classes/interfaces: `@@system Foo : Base1, Base2 { }`
    /// Passed through verbatim to the target language's inheritance syntax.
    pub bases: Vec<String>,
    pub interface: Vec<InterfaceMethod>,
    pub machine: Option<MachineAst>,
    pub actions: Vec<ActionAst>,
    pub operations: Vec<OperationAst>,
    pub domain: Vec<DomainVar>,
    pub span: Span,
    // NEW fields for unified AST:
    /// Section span tracking for validation
    pub section_spans: SystemSectionSpans,
    /// Optional persistence metadata from `@@persist`
    pub persist_attr: Option<PersistAttr>,
    /// Section order as encountered in source (may contain duplicates for validation)
    pub section_order: Vec<SystemSectionKind>,
    /// Visibility modifier: "private" overrides the public default.
    /// None or absent means public (the default).
    pub visibility: Option<String>,
    /// RFC-0014 module-level attributes attached via `@@[name(args?)]`
    /// immediately preceding a `@@system` declaration. The first
    /// recognized attribute is `@@[main]`, which marks the system as
    /// the file's primary for targets that privilege one class per
    /// file (GDScript, Java, etc.). RFC-0013's `@@[persist]` is
    /// special-cased into `persist_attr` for backwards compatibility
    /// — future attributes use this generic vec.
    pub attributes: Vec<Attribute>,
}

/// Which group a system header parameter belongs to.
///
/// The Frame language allows three groups of system parameters:
///   - Domain (bare `name`): becomes a constructor argument that is in
///     scope when the domain field initializers run.
///   - StateArg (`$(name)`): lands in the start state's
///     `compartment.state_args[name]` and is bound as a local at the
///     top of the state dispatch function.
///   - EnterArg (`$>(name)`): lands in the start state's
///     `compartment.enter_args[name]` and is bound by the existing
///     enter-handler dispatch code on the start state's `$>(name)` handler.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParamKind {
    Domain,
    StateArg,
    EnterArg,
}

/// System parameter (for parameterized systems)
#[derive(Debug, Clone)]
pub struct SystemParam {
    pub name: String,
    pub param_type: Type,
    pub default: Option<String>,
    /// Which group this param belongs to (domain, state-arg, or enter-arg).
    pub kind: ParamKind,
    pub span: Span,
}

/// RFC-0013 attribute. Carries the parsed `@@[name]` or
/// `@@[name(args)]` shape attached to an item (interface method,
/// domain field, handler).
///
/// Wave 2 introduces `@@[target("lang")]` for per-item conditional
/// emit. `args` is the raw bytes between the parens, NOT validated
/// here — codegen consumes whichever shape the attribute name
/// expects.
#[derive(Debug, Clone)]
pub struct Attribute {
    pub name: String,
    pub args: Option<String>,
    pub span: Span,
}

/// Interface method declaration
#[derive(Debug, Clone)]
pub struct InterfaceMethod {
    pub name: String,
    pub params: Vec<MethodParam>,
    pub return_type: Option<Type>,
    /// Default return value expression (e.g., `a1(): int = 10` has return_init = "10")
    pub return_init: Option<String>,
    /// Whether this method is declared async (triggers async dispatch chain)
    pub is_async: bool,
    /// Parsed but invalid on interface methods (E420)
    pub is_static: bool,
    /// Source comments encountered before this declaration in
    /// `interface:`. Captured by the lexer as
    /// `Lexer::take_pending_comments()` after each significant token;
    /// codegen emits them verbatim before the per-target wrapper
    /// definition. Empty for methods with no preceding comments.
    pub leading_comments: Vec<String>,
    /// RFC-0013 attributes attached via `@@[name(args?)]` immediately
    /// before this declaration. Wave 2 supports `@@[target("lang")]`
    /// for per-target conditional emit.
    pub attributes: Vec<Attribute>,
    pub span: Span,
}

/// Method parameter
#[derive(Debug, Clone)]
pub struct MethodParam {
    pub name: String,
    pub param_type: Type,
    pub default: Option<String>,
    pub span: Span,
}

/// State machine definition
#[derive(Debug, Clone)]
pub struct MachineAst {
    pub states: Vec<StateAst>,
    pub span: Span,
}

/// State variable declaration ($.varName: type = init)
#[derive(Debug, Clone)]
pub struct StateVarAst {
    pub name: String,
    pub var_type: Type,
    pub init: Option<Expression>,
    pub span: Span,
}

/// State definition
#[derive(Debug, Clone)]
pub struct StateAst {
    pub name: String,
    pub params: Vec<StateParam>,
    pub parent: Option<String>,       // For HSM parent state
    pub state_vars: Vec<StateVarAst>, // State-local variables ($.varName)
    pub handlers: Vec<HandlerAst>,
    pub enter: Option<EnterHandler>,
    pub exit: Option<ExitHandler>,
    /// State-level default forward to parent (bare `=> $^` at state level)
    pub default_forward: bool,
    /// Source comments encountered before this `$State { ... }`
    /// declaration in the `machine:` block. Captured by the lexer's
    /// `take_pending_comments()` and emitted by codegen before the
    /// state-dispatch function definition.
    pub leading_comments: Vec<String>,
    pub span: Span,
    /// Body span (inside braces only, for precise error reporting)
    pub body_span: Span,
}

/// State parameter
#[derive(Debug, Clone)]
pub struct StateParam {
    pub name: String,
    pub param_type: Type,
    pub span: Span,
}

/// Event handler in a state
#[derive(Debug, Clone)]
pub struct HandlerAst {
    pub event: String,
    pub params: Vec<EventParam>,
    pub return_type: Option<Type>,
    pub return_init: Option<String>,
    pub body: HandlerBody,
    /// Source comments encountered before this handler declaration in
    /// a `$State { ... }` block. Captured by the lexer's
    /// `take_pending_comments()` and emitted by codegen before the
    /// per-handler method definition.
    pub leading_comments: Vec<String>,
    /// RFC-0013 attributes (`@@[target("lang")]` etc.) attached
    /// immediately before this handler. Codegen consults the list
    /// to decide whether to emit the handler for the current target.
    pub attributes: Vec<Attribute>,
    pub span: Span,
}

/// Enter handler ($>)
#[derive(Debug, Clone)]
pub struct EnterHandler {
    pub params: Vec<EventParam>,
    pub body: HandlerBody,
    /// Same trivia plumbing as `HandlerAst.leading_comments`.
    pub leading_comments: Vec<String>,
    pub span: Span,
}

/// Exit handler ($<)
#[derive(Debug, Clone)]
pub struct ExitHandler {
    pub params: Vec<EventParam>,
    pub body: HandlerBody,
    /// Same trivia plumbing as `HandlerAst.leading_comments`.
    pub leading_comments: Vec<String>,
    pub span: Span,
}

/// Event parameter
#[derive(Debug, Clone)]
pub struct EventParam {
    pub name: String,
    pub param_type: Type,
    /// Optional default value for enter/exit handler params.
    /// Enables `$>(collected: list = [])` — the handler works both
    /// on initial entry (no args → default) and on pop return (with args).
    pub default_value: Option<String>,
    pub span: Span,
}

/// Handler body contains Frame statements only
/// Handler body containing an interleaved sequence of Frame statements and native code
#[derive(Debug, Clone)]
pub struct HandlerBody {
    /// Ordered sequence of Frame statements and NativeCode chunks
    pub statements: Vec<Statement>,
    /// Full span of handler body in source
    pub span: Span,
}

/// Statement in a handler body — Frame statements interleaved with native code
#[derive(Debug, Clone)]
pub enum Statement {
    /// Frame transition statement (->)
    Transition(TransitionAst),
    /// Frame transition-forward (-> => $State)
    /// Frame forward to parent (=>)
    Forward(ForwardAst),
    /// Frame stack push (push$)
    StackPush(StackPushAst),
    /// Frame stack pop (pop$)
    StackPop(StackPopAst),
    /// Frame return (return <expr>)
    Return(ReturnAst),
    /// Frame continue (deprecated)
    Continue(ContinueAst),
    /// Frame if statement
    If(IfAst),
    /// Frame loop statement
    Loop(LoopAst),
    /// A `{ ... }` block of statements. Used by RFC-0043 statement
    /// bodies (e.g. an `if` branch, an `@@fsm` action block). Carried
    /// as a single `Statement` so `IfAst`'s `Box<Statement>` branches
    /// can hold a multi-statement block.
    Block(BlockAst),
    /// Frame expression (assignments, calls, etc.)
    Expression(ExpressionAst),
    /// Native code chunk within handler body (V4 pipeline: Lexer extracts, Parser stores)
    NativeCode(String),

    // === Frame context constructs (mid-line and standalone) ===
    /// State variable read: $.varName
    StateVarRead { name: String, span: Span },
    /// State variable assignment: $.varName = expr
    StateVarAssign {
        name: String,
        expr: String,
        span: Span,
    },
    /// Context return: @@:return (bare read) or @@:return = expr (assignment)
    ContextReturn {
        assign_expr: Option<String>,
        span: Span,
    },
    /// Context return expression: @@:(expr)
    ContextReturnExpr { expr: String, span: Span },
    /// Return-call: @@:return(expr) — set return value AND exit handler
    ReturnCall { expr: String, span: Span },
    /// Context event: @@:event — interface event name (read-only)
    ContextEvent { span: Span },
    /// Context data read: @@:data["key"]
    ContextData { key: String, span: Span },
    /// Context data assignment: @@:data["key"] = expr
    ContextDataAssign {
        key: String,
        expr: String,
        span: Span,
    },
    /// Context params: @@:params["key"]
    ContextParams { key: String, span: Span },
    /// Self-call: @@:self.method(args) — reentrant interface call
    ContextSelfCall {
        method: String,
        args: String,
        span: Span,
    },
    /// Bare self reference: @@:self
    ContextSelf { span: Span },
    /// System state: @@:system.state — current state name
    ContextSystemState { span: Span },
    /// System instantiation:
    ///   - `@@SystemName(args)` — factory call, runs init code
    ///   - `@@!SystemName()` — RFC-0015 D7, allocates without calling init
    ///
    /// `kind` distinguishes the two; `args` is empty for `NoInitialization`.
    SystemInstantiation {
        system_name: String,
        args: String,
        kind: InstantiationKind,
        span: Span,
    },
}

/// Distinguishes the two flavors of `@@SystemName` call sites.
///
/// - `Factory` — the standard factory call (`@@Foo(args)`). Allocates, routes
///   args, fires `$Start` body + `$>` handler. This is what runs init code.
/// - `NoInitialization` — RFC-0015 D7 no-initialization allocation (`@@!Foo()`). Allocates
///   without calling init. Always zero-arg by definition. The user typically
///   pairs this with `inst.restore_state(data)` to load from saved bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstantiationKind {
    Factory,
    NoInitialization,
}

/// Transition statement (-> $State)
#[derive(Debug, Clone)]
pub struct TransitionAst {
    pub target: String,
    pub args: Vec<Expression>,
    /// Optional user-provided label (e.g., -> "Path A" $State).
    /// When present, replaces event name on GraphViz diagram edges.
    pub label: Option<String>,
    pub span: Span,
    /// Source indentation level (for proper code generation)
    pub indent: usize,
    /// Raw exit/enter/state arg strings from scanner (for codegen).
    /// These are populated by `regions_to_statements()`, not the parser.
    #[doc(hidden)]
    pub exit_args: Option<String>,
    #[doc(hidden)]
    pub enter_args: Option<String>,
    #[doc(hidden)]
    pub state_args: Option<String>,
    /// Pop-transition flag (-> pop$)
    #[doc(hidden)]
    pub is_pop: bool,
    /// Forward flag (-> => $State): dispatch current event to new state
    #[doc(hidden)]
    pub is_forward: bool,
}

/// Forward to parent (=> event)
#[derive(Debug, Clone)]
pub struct ForwardAst {
    pub event: String,
    pub args: Vec<Expression>,
    pub span: Span,
    /// Source indentation level (for proper code generation)
    pub indent: usize,
}

/// Stack push (push$)
#[derive(Debug, Clone)]
pub struct StackPushAst {
    pub span: Span,
    /// Source indentation level (for proper code generation)
    pub indent: usize,
    /// Target state of a `push$ -> $State` (push-with-transition), or `None`
    /// for a bare `push$`. The parser leaves this `None`; it's filled by
    /// `enrich_handler_body_metadata` from the scanner segment (like a
    /// Transition's args). Exposes the edge to AST-based passes such as the
    /// W414 reachability walker. (Codegen reads the target from the scanner
    /// metadata directly.)
    pub transition_target: Option<String>,
}

/// Stack pop (pop$)
#[derive(Debug, Clone)]
pub struct StackPopAst {
    pub span: Span,
    /// Source indentation level (for proper code generation)
    pub indent: usize,
}

/// Return statement (return <expr>)
#[derive(Debug, Clone)]
pub struct ReturnAst {
    pub value: Option<Expression>,
    pub span: Span,
}

/// Continue statement (^>)
#[derive(Debug, Clone)]
pub struct ContinueAst {
    pub span: Span,
}

/// If statement
#[derive(Debug, Clone)]
pub struct IfAst {
    pub condition: Expression,
    pub then_branch: Box<Statement>,
    pub else_branch: Option<Box<Statement>>,
    pub span: Span,
}

/// Loop statement
#[derive(Debug, Clone)]
pub struct LoopAst {
    pub kind: LoopKind,
    pub body: Box<Statement>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub enum LoopKind {
    While(Expression),
    For(String, Expression), // for var in expr
    Loop,                    // infinite loop
}

/// Expression AST
#[derive(Debug, Clone)]
pub struct ExpressionAst {
    pub expr: Expression,
    pub span: Span,
}

/// Expression types
#[derive(Debug, Clone)]
pub enum Expression {
    /// Variable reference
    Var(String),
    /// Literal value
    Literal(Literal),
    /// Binary operation
    Binary {
        left: Box<Expression>,
        op: BinaryOp,
        right: Box<Expression>,
    },
    /// Unary operation
    Unary { op: UnaryOp, expr: Box<Expression> },
    /// Method/function call
    Call { func: String, args: Vec<Expression> },
    /// Member access (obj.field)
    Member {
        object: Box<Expression>,
        field: String,
    },
    /// Index access (arr[idx])
    Index {
        object: Box<Expression>,
        index: Box<Expression>,
    },
    /// Assignment
    Assign {
        target: Box<Expression>,
        value: Box<Expression>,
    },
    /// Native expression - raw source passed through verbatim
    /// Used for language-specific expressions the parser doesn't understand
    NativeExpr(String),
}

/// Literal values
#[derive(Debug, Clone)]
pub enum Literal {
    Int(i64),
    Float(f64),
    String(String),
    Bool(bool),
    Null,
}

/// Binary operators
#[derive(Debug, Clone)]
pub enum BinaryOp {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    And,
    Or,
    BitAnd,
    BitOr,
    BitXor,
}

/// Unary operators
#[derive(Debug, Clone)]
pub enum UnaryOp {
    Not,
    Neg,
    BitNot,
}

/// Action definition
#[derive(Debug, Clone)]
pub struct ActionAst {
    pub name: String,
    pub params: Vec<ActionParam>,
    pub return_type: Type,
    pub body: ActionBody,
    /// Whether this action is declared async
    pub is_async: bool,
    /// Parsed but invalid on actions (E420)
    pub is_static: bool,
    /// Source comments encountered before this declaration in
    /// `actions:`. Captured by the lexer's `take_pending_comments()`
    /// after each significant token; codegen emits them verbatim
    /// before the per-target action method definition.
    pub leading_comments: Vec<String>,
    pub span: Span,
}

/// Action parameter
#[derive(Debug, Clone)]
pub struct ActionParam {
    pub name: String,
    pub param_type: Type,
    pub default: Option<String>,
    pub span: Span,
}

/// Action body - native code only, content preserved by splicer
#[derive(Debug, Clone)]
pub struct ActionBody {
    /// Span referencing original source
    pub span: Span,
    /// Native body content (extracted during parsing, used by codegen directly)
    pub code: Option<String>,
}

/// Operation definition (with return type)
#[derive(Debug, Clone)]
pub struct OperationAst {
    pub name: String,
    pub params: Vec<OperationParam>,
    pub return_type: Type,
    pub body: OperationBody,
    pub is_static: bool,
    /// Whether this operation is declared async
    pub is_async: bool,
    /// Source comments encountered before this declaration in
    /// `operations:`. Captured by the lexer's
    /// `take_pending_comments()` and emitted by codegen before the
    /// per-target operation method definition.
    pub leading_comments: Vec<String>,
    /// RFC-0013 attributes attached via `@@[name(args?)]` immediately
    /// before this declaration. RFC-0012 amendment 2026-05-02 uses
    /// this for `@@[save]` / `@@[load]` to mark persist endpoints
    /// whose bodies the framework generates.
    pub attributes: Vec<Attribute>,
    pub span: Span,
}

/// Operation parameter
#[derive(Debug, Clone)]
pub struct OperationParam {
    pub name: String,
    pub param_type: Type,
    pub default: Option<String>,
    pub span: Span,
}

/// Operation body - native code only, content preserved by splicer
#[derive(Debug, Clone)]
pub struct OperationBody {
    /// Span referencing original source
    pub span: Span,
    /// Native body content (extracted during parsing, used by codegen directly)
    pub code: Option<String>,
}

/// Domain variable
///
/// Domain fields are written in the target language's native syntax
/// (`int x = 5` for C, `var x: Int = 5` for Swift, `x = 5` for Erlang,
/// etc.). The Frame compiler parses each declaration into structured
/// Domain field declaration — first-class Frame syntax `name : type = init`.
/// Both type and init are opaque strings (Frame doesn't interpret them).
#[derive(Debug, Clone)]
pub struct DomainVar {
    pub name: String,
    /// `Type::Custom(s)` with the user's verbatim type text.
    /// `Type::Unknown` when type is omitted (bare form, dynamic targets).
    pub var_type: Type,
    /// Initializer expression as raw target-language text.
    /// Frame doesn't interpret this — codegen emits it verbatim.
    pub initializer_text: Option<String>,
    /// `const` modifier — field is immutable after construction.
    pub is_const: bool,
    /// Source comments encountered before this declaration in
    /// `domain:`. Captured by the lexer's
    /// `take_pending_comments()` and emitted by codegen before the
    /// generated struct/class field. Empty for fields with no
    /// preceding comments.
    pub leading_comments: Vec<String>,
    /// RFC-0013 attributes (`@@[target("lang")]` etc.) attached
    /// immediately before this domain field declaration.
    pub attributes: Vec<Attribute>,
    pub span: Span,
}

/// Target language for native blocks
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TargetLanguage {
    Python3,
    TypeScript,
    Rust,
    CSharp,
    C,
    Cpp,
    Java,
    Graphviz,
}

// Helper methods for AST nodes
impl SystemAst {
    /// Create a new minimal SystemAst (useful for tests and builder patterns)
    pub fn new(name: String, span: Span) -> Self {
        Self {
            name,
            params: vec![],
            bases: vec![],
            interface: vec![],
            machine: None,
            actions: vec![],
            operations: vec![],
            domain: vec![],
            span,
            section_spans: SystemSectionSpans::default(),
            persist_attr: None,
            section_order: vec![],
            visibility: None,
            attributes: vec![],
        }
    }

    /// True iff this system carries the RFC-0014 `@@[main]` attribute,
    /// marking it as the file's primary system for targets that
    /// privilege one class per file (GDScript, Java, etc.).
    pub fn is_main(&self) -> bool {
        self.attributes.iter().any(|a| a.name == "main")
    }

    /// RFC-0015: the user-supplied factory name from `@@[create(name)]`,
    /// or `None` if the attribute is absent or supplied without args.
    /// `None` means the codegen falls back to its per-backend default
    /// (locked in RFC-0015 § "D2").
    pub fn create_op_name(&self) -> Option<&str> {
        self.attributes
            .iter()
            .find(|a| a.name == "create")
            .and_then(|a| a.args.as_deref())
    }

    /// RFC-0015: the user-supplied save op name from `@@[save(name)]`,
    /// or `None` for the per-backend default. Signature is dictated
    /// by `@@[persist(<Format>)]` regardless.
    pub fn save_op_name_rfc0015(&self) -> Option<&str> {
        self.attributes
            .iter()
            .find(|a| a.name == "save")
            .and_then(|a| a.args.as_deref())
    }

    /// RFC-0015: the user-supplied load op name from `@@[load(name)]`,
    /// or `None` for the per-backend default. Signature is dictated
    /// by `@@[persist(<Format>)]` regardless.
    pub fn load_op_name_rfc0015(&self) -> Option<&str> {
        self.attributes
            .iter()
            .find(|a| a.name == "load")
            .and_then(|a| a.args.as_deref())
    }

    /// True iff this system carries any RFC-0015 lifecycle attribute.
    /// Used by codegen during the rollout to detect the new contract
    /// without scanning the full attributes list each time.
    pub fn has_rfc0015_lifecycle(&self) -> bool {
        self.attributes
            .iter()
            .any(|a| matches!(a.name.as_str(), "create" | "save" | "load"))
    }

    /// Get the start state of the machine (first state defined)
    pub fn start_state(&self) -> Option<&StateAst> {
        self.machine.as_ref()?.states.first()
    }

    /// Find a state by name
    pub fn find_state(&self, name: &str) -> Option<&StateAst> {
        self.machine
            .as_ref()?
            .states
            .iter()
            .find(|s| s.name == name)
    }

    /// Check if an interface method exists
    pub fn has_interface_method(&self, name: &str) -> bool {
        self.interface.iter().any(|m| m.name == name)
    }

    /// Check if an action exists
    pub fn has_action(&self, name: &str) -> bool {
        self.actions.iter().any(|a| a.name == name)
    }

    /// Check if an operation exists
    pub fn has_operation(&self, name: &str) -> bool {
        self.operations.iter().any(|o| o.name == name)
    }

    // ----------------------------------------------------------------
    // RFC-0012 amendment 2026-05-02: persist contract inspection.
    //
    // Three small lookups codegen uses to branch between the legacy
    // static-`restore_state` shape and the new instance-method shape
    // declared via `@@[save]` / `@@[load]` operation attributes.
    // ----------------------------------------------------------------

    /// Name of the save operation, sourced from either:
    /// - RFC-0012: an operation marked `@@[save]` (op-attribute form), or
    /// - RFC-0015: the system-level `@@[save(<name>)]` attribute.
    ///
    /// Codegen reads through this single accessor to pick up either
    /// surface form — backends don't need per-form branching.
    /// Returns `None` when neither form is present (system uses
    /// per-backend default name).
    pub fn save_op_name(&self) -> Option<&str> {
        // RFC-0012 op-attribute form (legacy)
        if let Some(op) = self
            .operations
            .iter()
            .find(|op| op.attributes.iter().any(|a| a.name == "save"))
        {
            return Some(op.name.as_str());
        }
        // RFC-0015 system-level form
        self.save_op_name_rfc0015()
    }

    /// Name of the load operation, sourced from either:
    /// - RFC-0012: an operation marked `@@[load]` (op-attribute form), or
    /// - RFC-0015: the system-level `@@[load(<name>)]` attribute.
    ///
    /// Codegen reads through this single accessor to pick up either
    /// surface form — backends don't need per-form branching.
    pub fn load_op_name(&self) -> Option<&str> {
        // RFC-0012 op-attribute form (legacy)
        if let Some(op) = self
            .operations
            .iter()
            .find(|op| op.attributes.iter().any(|a| a.name == "load"))
        {
            return Some(op.name.as_str());
        }
        // RFC-0015 system-level form
        self.load_op_name_rfc0015()
    }

    /// True iff the system uses the new persist contract — either via
    /// RFC-0012 op-attribute form (`@@[save]` / `@@[load]` operations)
    /// or RFC-0015 system-level form (`@@[save(name)]` / `@@[load(name)]`).
    /// Codegen uses this to switch between the legacy static-method
    /// persist shape and the new instance-method shape.
    pub fn uses_new_persist_contract(&self) -> bool {
        self.persist_attr.is_some()
            && (self.save_op_name().is_some() || self.load_op_name().is_some())
    }

    /// Name of the load operation parameter (always at most one).
    /// Codegen uses this as the parameter name in the framework-
    /// generated load body so it matches what the user declared in
    /// the operation signature (e.g., `unpickle(data: str)` →
    /// `"data"`).
    pub fn load_op_param_name(&self) -> Option<&str> {
        let op = self
            .operations
            .iter()
            .find(|op| op.attributes.iter().any(|a| a.name == "load"))?;
        op.params.first().map(|p| p.name.as_str())
    }

    /// Get the declared type of the load operation's first parameter
    /// (the one that receives the serialized data). Returns `None` if
    /// no @@[load] op exists or its param has no declared type.
    /// Backends that need a target-specific default (e.g. Rust's
    /// `&str`) should fall back to that when this returns `None`.
    pub fn load_op_param_type(&self) -> Option<String> {
        let op = self
            .operations
            .iter()
            .find(|op| op.attributes.iter().any(|a| a.name == "load"))?;
        let p = op.params.first()?;
        match &p.param_type {
            Type::Custom(s) => Some(s.clone()),
            Type::Unknown => None,
        }
    }

    /// Get section span for a given section kind
    pub fn get_section_span(&self, kind: SystemSectionKind) -> Option<&Span> {
        match kind {
            SystemSectionKind::Operations => self.section_spans.operations.as_ref(),
            SystemSectionKind::Interface => self.section_spans.interface.as_ref(),
            SystemSectionKind::Machine => self.section_spans.machine.as_ref(),
            SystemSectionKind::Actions => self.section_spans.actions.as_ref(),
            SystemSectionKind::Domain => self.section_spans.domain.as_ref(),
        }
    }

    /// Check if a section appears more than once (for duplicate detection)
    pub fn has_duplicate_sections(&self) -> Option<SystemSectionKind> {
        let mut seen = std::collections::HashSet::new();
        for kind in &self.section_order {
            if !seen.insert(*kind) {
                return Some(*kind);
            }
        }
        None
    }
}

impl StateAst {
    /// Create a new minimal StateAst
    pub fn new(name: String, span: Span) -> Self {
        Self {
            name,
            params: vec![],
            parent: None,
            state_vars: vec![],
            handlers: vec![],
            enter: None,
            exit: None,
            default_forward: false,
            leading_comments: Vec::new(),
            span: span.clone(),
            body_span: span,
        }
    }

    /// Get parameter count
    pub fn param_count(&self) -> usize {
        self.params.len()
    }

    /// Find handler by event name
    pub fn find_handler(&self, event: &str) -> Option<&HandlerAst> {
        self.handlers.iter().find(|h| h.event == event)
    }

    /// Check if state has a parent (HSM)
    pub fn has_parent(&self) -> bool {
        self.parent.is_some()
    }
}

impl HandlerBody {
    /// Create a new empty handler body
    pub fn empty(span: Span) -> Self {
        Self {
            statements: vec![],
            span,
        }
    }
}

impl SystemSectionSpans {
    /// Set the span for a given section kind
    pub fn set(&mut self, kind: SystemSectionKind, span: Span) {
        match kind {
            SystemSectionKind::Operations => self.operations = Some(span),
            SystemSectionKind::Interface => self.interface = Some(span),
            SystemSectionKind::Machine => self.machine = Some(span),
            SystemSectionKind::Actions => self.actions = Some(span),
            SystemSectionKind::Domain => self.domain = Some(span),
        }
    }

    /// Get the span for a given section kind
    pub fn get(&self, kind: SystemSectionKind) -> Option<&Span> {
        match kind {
            SystemSectionKind::Operations => self.operations.as_ref(),
            SystemSectionKind::Interface => self.interface.as_ref(),
            SystemSectionKind::Machine => self.machine.as_ref(),
            SystemSectionKind::Actions => self.actions.as_ref(),
            SystemSectionKind::Domain => self.domain.as_ref(),
        }
    }
}

// ============================================================================
// RFC-0042 — `@@fsm` AST nodes
// ============================================================================
//
// AST shapes produced by the new fsm parser at
// `framec/src/frame_c/compiler/fsm_parser/`. These types are entirely
// separate from the `@@system` AST above; per the design contract, no
// AST types are shared between `@@system` and `@@fsm`. The two
// construct families coexist as siblings in this module.
//
// Where RFC-0042 reuses RFC-0043-defined shapes (statement-body code,
// expressions), it does so via the existing `Statement`, `Expression`,
// `Literal`, `BinaryOp`, `UnaryOp`, and the new `BlockAst` defined at
// the bottom of this section. The fsm-specific types live above the
// shared `BlockAst`.

/// One complete `@@fsm` declaration. Output of the fsm parser's root
/// `FsmDeclParser` system. Per RFC-0042 §3.1.
#[derive(Debug, Clone)]
pub struct FsmDeclAst {
    /// Construct name — e.g., `M` in `@@fsm M(text: bytes) ...`.
    pub name: String,
    /// `@@[...]` decorations attached to the construct declaration
    /// (e.g., `@@[max_dfa_states(50)]`, `@@[multiline]`,
    /// `@@[dispatch(switch)]`).
    pub attributes: Vec<String>,
    /// Header parameter list. First parameter is the input source
    /// per §3.2 and determines the regex alphabet.
    pub params: Vec<FsmParameter>,
    /// Declared return type (e.g., `bool`, `int`, `Header`).
    pub return_type: Type,
    /// Mandatory default value for the return slot — used on failure
    /// paths and as the initial value before any bare-expression
    /// assignment fires. Stored as a raw expression string; later
    /// passes parse it.
    pub default_expr: String,
    /// State declarations in source order. First state is the start
    /// state per §3.4.
    pub states: Vec<FsmStateAst>,
    /// Optional `actions:` block of declared helper functions.
    pub actions: Option<FsmActionsBlock>,
    /// Optional `domain:` block of explicit (non-auto-promoted)
    /// fields.
    pub domain: Option<FsmDomainBlock>,
    pub span: Span,
}

impl FsmDeclAst {
    /// An empty placeholder, used as the default for a validator/codegen
    /// FSM's owned-AST domain field before the real AST is assigned.
    pub fn empty() -> Self {
        FsmDeclAst {
            name: String::new(),
            attributes: Vec::new(),
            params: Vec::new(),
            return_type: Type::Unknown,
            default_expr: String::new(),
            states: Vec::new(),
            actions: None,
            domain: None,
            span: Span::new(0, 0),
        }
    }
}

/// One parameter in the `@@fsm` header. Auto-promotes to a same-named
/// domain field on the constructed instance (§3.2).
#[derive(Debug, Clone)]
pub struct FsmParameter {
    pub name: String,
    pub param_type: Type,
    /// Optional default expression, parsed as a raw expression string.
    pub default: Option<String>,
    pub span: Span,
}

/// One state declaration inside an `@@fsm` body. Optional label;
/// one or more matches separated by `|` (ordered choice). The first
/// state declared is the start state.
///
/// Distinguished from `@@system`'s [`StateAst`] (defined earlier in
/// this module) — fsm states have no lifecycle handlers, no state
/// variables, no compartments.
#[derive(Debug, Clone)]
pub struct FsmStateAst {
    /// Optional `$label` — required if other code (transition target,
    /// stage-capture reference) needs to name this state. Unlabeled
    /// states are valid only as the start state (first body state).
    pub label: Option<String>,
    /// Ordered list of matches. Matches separated by `|` in source
    /// produce multiple elements here.
    pub matches: Vec<MatchAst>,
    pub span: Span,
}

/// One match inside a state. Sequence of elements (stages, action
/// blocks, bare expressions) optionally followed by a transition
/// clause. §3.5.1.
#[derive(Debug, Clone)]
pub struct MatchAst {
    pub elements: Vec<MatchElement>,
    pub transition: Option<FsmTransitionClauseAst>,
    pub span: Span,
}

/// One element inside a match. Per §3.5.1, an element is either a
/// match stage, an action block, or a bare expression (sugar for
/// `@@:return = expr`).
#[derive(Debug, Clone)]
pub enum MatchElement {
    Stage(StageAst),
    ActionBlock(BlockAst),
    BareExpression { expr: Expression, span: Span },
}

/// One match stage: optional `.label`, `/regex/`, zero or more
/// embedding actions. §3.5.2 and §3.5.5.
#[derive(Debug, Clone)]
pub struct StageAst {
    /// Optional `.label` — required to reference the stage from
    /// elsewhere.
    pub label: Option<String>,
    /// Parsed regex from [`crate::frame_c::compiler::fsm_regex`].
    /// Stored as the raw body string at parse time; the regex AST
    /// resolves later in the pipeline. (Final shape pinned when
    /// `fsm_regex` is wired in during Task 12 — see
    /// `_scratch/rfc_0043_parser_design.md`.)
    pub regex: String,
    /// Embedding actions attached after the stage's closing `/`.
    pub embedding_actions: Vec<EmbeddingActionAst>,
    pub span: Span,
}

/// One embedding action attached to a stage. §3.5.5.
#[derive(Debug, Clone)]
pub struct EmbeddingActionAst {
    pub op: EmbeddingOp,
    pub body: BlockAst,
    pub span: Span,
}

/// Where in the stage's compiled DFA the embedding action fires.
/// Maps to RFC-0042 §3.5.5's operator table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EmbeddingOp {
    /// `>{...}` — DFA start state entry.
    Start,
    /// `@{...}` — DFA accepting state entry.
    Accept,
    /// `${...}` — every DFA transition.
    EveryTransition,
    /// `%{...}` — leaving last accepting state.
    LeaveAccept,
    /// `@eof{...}` — end-of-input while mid-match.
    Eof,
}

/// Transition clause at the tail of a match. Success branch required
/// when present; failure branch optional. §3.5.4.
#[derive(Debug, Clone)]
pub struct FsmTransitionClauseAst {
    /// The success target (`-> $X`). `None` for a failure-only clause
    /// (`: -> $Err` with no success arrow): success leaves the match in
    /// its final position as an implicit-terminal match (§4.3).
    pub success: Option<FsmTransitionTarget>,
    pub failure: Option<FsmTransitionTarget>,
    pub span: Span,
}

/// Target of an fsm transition — either a static state/stage
/// reference or a runtime-chosen alternative from a
/// statically-enumerable set. §3.5.4.
#[derive(Debug, Clone)]
pub enum FsmTransitionTarget {
    /// `-> $State` or `-> $State.stage`.
    Static {
        state: String,
        stage: Option<String>,
        span: Span,
    },
    /// `-> ( $A when cond, $B when cond, ... )`. Each `cond_alt`
    /// requires a `when` guard per §3.5.4.1 (E715 if missing).
    Conditional(Vec<FsmCondAlt>),
}

/// One `cond_alt` inside a conditional target — a static target with
/// a `when` guard predicate. §3.5.4.1.
#[derive(Debug, Clone)]
pub struct FsmCondAlt {
    pub target: FsmTransitionTarget,
    pub condition: Expression,
    pub span: Span,
}

/// `actions:` block — declared helper functions callable from match
/// actions, embedding actions, and other actions. §3.7.
#[derive(Debug, Clone)]
pub struct FsmActionsBlock {
    pub actions: Vec<FsmActionDecl>,
    pub span: Span,
}

/// One declared action inside `actions:`. Constraint per §3.7:
/// actions cannot issue transitions or perform state-aware
/// operations.
#[derive(Debug, Clone)]
pub struct FsmActionDecl {
    pub name: String,
    pub params: Vec<FsmParameter>,
    pub return_type: Option<Type>,
    pub body: BlockAst,
    pub span: Span,
}

/// `domain:` block — explicit (non-auto-promoted) persistent fields
/// for the fsm instance. §3.8.
#[derive(Debug, Clone)]
pub struct FsmDomainBlock {
    pub vars: Vec<FsmDomainVar>,
    pub span: Span,
}

/// One declared `domain:` field. Mandatory typed declaration with a
/// default initializer (RFC-0042 §3.8).
#[derive(Debug, Clone)]
pub struct FsmDomainVar {
    pub name: String,
    pub var_type: Type,
    /// Default initializer — a parsed expression.
    pub default: Expression,
    pub span: Span,
}

/// A `{ ... }` block of RFC-0043 statements. Used by `@@fsm` action
/// bodies, embedding-action bodies, declared-action bodies, and the
/// branches of an `if/else` statement.
///
/// Reused by any future construct that adopts RFC-0043 statement
/// syntax; not specific to `@@fsm`.
#[derive(Debug, Clone)]
pub struct BlockAst {
    pub statements: Vec<Statement>,
    pub span: Span,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_system_ast_creation() {
        let mut system = SystemAst::new("TrafficLight".to_string(), Span::new(0, 100));
        system.machine = Some(MachineAst {
            states: vec![StateAst::new("Red".to_string(), Span::new(0, 10))],
            span: Span::new(0, 20),
        });

        assert_eq!(system.name, "TrafficLight");
        assert!(system.find_state("Red").is_some());
        assert!(system.find_state("Green").is_none());
    }

    #[test]
    fn test_rfc0015_lifecycle_helpers_absent() {
        // System with no RFC-0015 attributes — all helpers return None
        // and has_rfc0015_lifecycle() is false.
        let system = SystemAst::new("Foo".to_string(), Span::new(0, 10));
        assert_eq!(system.create_op_name(), None);
        assert_eq!(system.save_op_name_rfc0015(), None);
        assert_eq!(system.load_op_name_rfc0015(), None);
        assert!(!system.has_rfc0015_lifecycle());
    }

    #[test]
    fn test_rfc0015_lifecycle_helpers_present() {
        // System with all three RFC-0015 attributes — each helper
        // surfaces the user-supplied name.
        let mut system = SystemAst::new("Inner".to_string(), Span::new(0, 10));
        system.attributes.push(Attribute {
            name: "create".to_string(),
            args: Some("make".to_string()),
            span: Span::new(0, 0),
        });
        system.attributes.push(Attribute {
            name: "save".to_string(),
            args: Some("pickle".to_string()),
            span: Span::new(0, 0),
        });
        system.attributes.push(Attribute {
            name: "load".to_string(),
            args: Some("unpickle".to_string()),
            span: Span::new(0, 0),
        });
        assert_eq!(system.create_op_name(), Some("make"));
        assert_eq!(system.save_op_name_rfc0015(), Some("pickle"));
        assert_eq!(system.load_op_name_rfc0015(), Some("unpickle"));
        assert!(system.has_rfc0015_lifecycle());
    }

    #[test]
    fn test_rfc0015_lifecycle_no_arg_form() {
        // Bare `@@[create]` with no argument — args is None even
        // though has_rfc0015_lifecycle() reports true. Codegen falls
        // back to the per-backend default name (RFC-0015 § "D2").
        let mut system = SystemAst::new("Trivial".to_string(), Span::new(0, 10));
        system.attributes.push(Attribute {
            name: "create".to_string(),
            args: None,
            span: Span::new(0, 0),
        });
        assert_eq!(system.create_op_name(), None);
        assert!(system.has_rfc0015_lifecycle());
    }

    #[test]
    fn test_transition_ast() {
        let transition = TransitionAst {
            target: "Green".to_string(),
            args: vec![],
            label: None,
            span: Span::new(10, 20),
            indent: 8,
            exit_args: None,
            enter_args: None,
            state_args: None,
            is_pop: false,
            is_forward: false,
        };

        assert_eq!(transition.target, "Green");
        assert!(transition.args.is_empty());
        assert_eq!(transition.indent, 8);
    }

    #[test]
    fn test_section_spans() {
        let mut spans = SystemSectionSpans::default();
        spans.set(SystemSectionKind::Machine, Span::new(10, 50));
        spans.set(SystemSectionKind::Actions, Span::new(50, 80));

        assert!(spans.get(SystemSectionKind::Machine).is_some());
        assert!(spans.get(SystemSectionKind::Actions).is_some());
        assert!(spans.get(SystemSectionKind::Interface).is_none());
    }

    #[test]
    fn test_duplicate_sections() {
        let mut system = SystemAst::new("Test".to_string(), Span::new(0, 100));
        system.section_order = vec![
            SystemSectionKind::Machine,
            SystemSectionKind::Actions,
            SystemSectionKind::Machine, // duplicate!
        ];

        assert_eq!(
            system.has_duplicate_sections(),
            Some(SystemSectionKind::Machine)
        );
    }

    #[test]
    fn test_persist_attr() {
        let mut system = SystemAst::new("PersistentSystem".to_string(), Span::new(0, 100));
        system.persist_attr = Some(PersistAttr {
            save_name: Some("custom_save".to_string()),
            restore_name: None,
            library: None,
            span: Span::new(0, 20),
        });

        assert!(system.persist_attr.is_some());
        assert_eq!(
            system.persist_attr.as_ref().unwrap().save_name,
            Some("custom_save".to_string())
        );
    }

    // ----------------------------------------------------------------
    // RFC-0012 amendment 2026-05-02: persist contract inspection.
    // Tests for SystemAst::save_op_name / load_op_name /
    // uses_new_persist_contract / load_op_param_name.
    // ----------------------------------------------------------------

    fn make_op_with_attrs(name: &str, attr_names: &[&str]) -> OperationAst {
        OperationAst {
            name: name.to_string(),
            params: vec![],
            return_type: Type::Unknown,
            body: OperationBody {
                span: Span::new(0, 0),
                code: None,
            },
            is_static: false,
            is_async: false,
            leading_comments: vec![],
            attributes: attr_names
                .iter()
                .map(|n| Attribute {
                    name: n.to_string(),
                    args: None,
                    span: Span::new(0, 0),
                })
                .collect(),
            span: Span::new(0, 0),
        }
    }

    #[test]
    fn save_op_name_finds_marked_op() {
        let mut sys = SystemAst::new("Foo".to_string(), Span::new(0, 0));
        sys.operations.push(make_op_with_attrs("regular_op", &[]));
        sys.operations.push(make_op_with_attrs("pickle", &["save"]));
        sys.operations
            .push(make_op_with_attrs("unpickle", &["load"]));
        assert_eq!(sys.save_op_name(), Some("pickle"));
        assert_eq!(sys.load_op_name(), Some("unpickle"));
    }

    #[test]
    fn save_op_name_returns_none_when_no_op_marked() {
        let mut sys = SystemAst::new("Bare".to_string(), Span::new(0, 0));
        sys.operations.push(make_op_with_attrs("regular_op", &[]));
        assert_eq!(sys.save_op_name(), None);
        assert_eq!(sys.load_op_name(), None);
    }

    #[test]
    fn uses_new_persist_contract_requires_persist_and_save_or_load() {
        let mut sys = SystemAst::new("NoPersist".to_string(), Span::new(0, 0));
        sys.operations.push(make_op_with_attrs("pickle", &["save"]));
        // Has @@[save] but NOT @@[persist] → not the new contract.
        assert!(!sys.uses_new_persist_contract());

        sys.persist_attr = Some(PersistAttr {
            save_name: None,
            restore_name: None,
            library: None,
            span: Span::new(0, 0),
        });
        // Now has both → new contract active.
        assert!(sys.uses_new_persist_contract());
    }

    #[test]
    fn uses_new_persist_contract_active_with_only_load() {
        let mut sys = SystemAst::new("OnlyLoad".to_string(), Span::new(0, 0));
        sys.persist_attr = Some(PersistAttr {
            save_name: None,
            restore_name: None,
            library: None,
            span: Span::new(0, 0),
        });
        sys.operations
            .push(make_op_with_attrs("unpickle", &["load"]));
        // Either save or load is sufficient — system uses new contract.
        // (Validator E810 catches the asymmetric case separately.)
        assert!(sys.uses_new_persist_contract());
    }

    #[test]
    fn uses_new_persist_contract_inactive_with_no_save_load() {
        // Legacy persist systems (`@@[persist]` but no save/load ops)
        // stay on the static-method contract — codegen branches on
        // this flag.
        let mut sys = SystemAst::new("Legacy".to_string(), Span::new(0, 0));
        sys.persist_attr = Some(PersistAttr {
            save_name: None,
            restore_name: None,
            library: None,
            span: Span::new(0, 0),
        });
        sys.operations.push(make_op_with_attrs("regular_op", &[]));
        assert!(!sys.uses_new_persist_contract());
    }

    #[test]
    fn load_op_param_name_returns_user_chosen_name() {
        let mut sys = SystemAst::new("Foo".to_string(), Span::new(0, 0));
        let mut op = make_op_with_attrs("unpickle", &["load"]);
        op.params.push(OperationParam {
            name: "snap".to_string(),
            param_type: Type::Custom("str".to_string()),
            default: None,
            span: Span::new(0, 0),
        });
        sys.operations.push(op);
        assert_eq!(sys.load_op_param_name(), Some("snap"));
    }

    #[test]
    fn load_op_param_name_none_when_no_load_op() {
        let sys = SystemAst::new("Foo".to_string(), Span::new(0, 0));
        assert_eq!(sys.load_op_param_name(), None);
    }
}
