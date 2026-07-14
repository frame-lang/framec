//! The handler body. **This is the tree the old compiler did not have.**
//!
//! Below a handler's opening brace, the old compiler had a flat segment stream with
//! native code as an opaque `String`. Every downstream pass therefore re-derived
//! structure by reading text — its own output, or the user's — and **all twenty-five
//! shipped bugs came from exactly that.** It is not a collection of mistakes. It is
//! one missing tree, observed twenty-five times.
//!
//! # A native statement is a CONTAINER, not a leaf
//!
//! The first draft of this rebuild had `NativeStmt { span, terminated, block_depth }`
//! and a proud comment: *"no `text` field — the text is the span."* That tree
//! **cannot represent the language framec already ships.** Both of these compile
//! correctly today:
//!
//! ```frame
//! let total = $.count + compute(@@:self.factor, 2) * 3;   // TWO refs inside ONE native stmt
//! print(f"count is {$.count}")                            // a ref inside a STRING LITERAL
//! ```
//!
//! Neither had a field to live in. The island-grammar literature says plainly that
//! **the water is still tokenized** — the first draft quoted that and then stopped at
//! *delimited*, which is not the same thing.
//!
//! So a native statement is a sequence of [`NativePart`]s, and the parts partition it.
//!
//! # Holes are code. Content is not.
//!
//! ```text
//! f"count is {$.count}"     the HOLE is an expression position  ->  framec looks
//! "a literal $.x here"      the CONTENT is bytes                 ->  framec does not
//! ```
//!
//! An interpolation hole (`{…}` in an f-string, `${…}` in a template, `\(…)` in Swift)
//! is an **expression position in the target's own grammar**. The target compiler will
//! treat those bytes as code, so framec may too — and nowhere else.
//!
//! The old compiler gave **two different answers** to this depending on which code
//! path arrived: its scanner said a sigil in a string is not a reference; its
//! expression byte-loop (string-blind by design) said it is. Both shipped (#224). Here
//! the answer is not a rule anyone has to remember — it is the **shape of the type**.
//! A [`FrameRef`] can only exist as a `NativePart` or inside a [`Hole`]. There is no
//! variant that puts one in string content, so the wrong answer is unrepresentable.
//!
//! # Why `literals` are nodes
//!
//! Because framec must know where they are in order to **leave them alone**. The old
//! compiler's `normalize_indentation` stripped the left margin off every emitted line
//! *including lines inside a string literal* — so the user's string had a different
//! value at runtime than in their source (#215). It could not have known better: it
//! had no idea where the literals were. Now it does, and that is the same fact that
//! makes the hole rule expressible. One node, two bugs.

use crate::tree::{Node, TriviaNode};
use crate::Span;

/// A handler's body. **Partitions the span between its braces.**
#[derive(Debug)]
pub struct Body {
    pub span: Span,
    pub stmts: Vec<Stmt>,
}

/// A statement in a handler body.
///
/// Each Frame construct is **its own variant**. An earlier attempt collapsed ~20 of
/// them into one blob carrying its identity in a `kind` *field* — and an exhaustive
/// match caught it instantly while a byte-diff of the emitted output showed
/// **nothing**. Only the *type* caught it. Identity lives in the type, never in a
/// field.
#[derive(Debug)]
pub enum Stmt {
    /// Whitespace and comments *between* statements.
    ///
    /// This is the part everyone skips, and it is exactly where the terminator bug
    /// lived: framec spliced a `;` **inside a trailing comment**, because the comment
    /// was not a node and nothing knew it was there.
    Trivia(TriviaNode),

    /// Native code. **Delimited, never interpreted** — but *tokenized*.
    Native(NativeStmt),

    /// `-> $State(args)`, `-> (exit) $S`, `-> => $S`
    Transition(TransitionStmt),
    /// `push$ -> $State(args)`
    StackPush(TransitionStmt),
    /// `-> pop$`
    StackPop(SimpleStmt),
    /// `=> $^`
    Forward(SimpleStmt),

    /// **`@@:self.x = <expr>` / `$.x = <expr>` / `@@:data.k = <expr>` / `@@:return = <expr>`**
    ///
    /// A FRAME statement — Frame's own assignment syntax — not native code with a ref
    /// spliced into it. framec owns it end to end, **including its terminator**.
    ///
    /// The old compiler had no node for this at all: `@@:self` was a *reference*, and
    /// the ` = expr` fell out as untyped native text. So nothing could ask whether the
    /// statement was terminated, and framec resorted to searching its own emitted string
    /// for the last non-whitespace byte — which landed a `;` inside a comment (#173), and
    /// on other paths emitted no `;` at all (#229).
    ///
    /// **One missing node; a bug on seven backends.**
    ///
    /// A trailing target terminator (`;`) in the source is **part of Frame's statement**
    /// and is consumed by the scanner (see `terminator`), then re-emitted by the backend
    /// in that target's own spelling. Both corpus forms therefore work unchanged, and no
    /// pass ever re-reads emitted text to decide.
    Assign(AssignStmt),

    /// **`@@:return(<expr>)`** — set the return value AND exit the handler.
    ///
    /// A Frame statement. framec owns it, terminates it, and knows it is TERMINAL — so
    /// nothing after it is emitted. The old compiler had to work that out by reading its
    /// own output.
    ReturnCall(ReturnCallStmt),

    /// **`@@:self.method(<args>)`** — a reentrant call back into the system's interface.
    ///
    /// A Frame statement, not native code with a ref in it: framec authored the call, so
    /// framec terminates it. (The old compiler lowered the `@@:self` part to a reference
    /// and left `.report()` as native text with no terminator — #229.)
    SelfCall(SelfCallStmt),
}

#[derive(Debug)]
pub struct ReturnCallStmt {
    pub span: Span,
    /// The statement's COLUMN in the source.
    ///
    /// An indent-delimited target (Python, GDScript) must reproduce the user's nesting:
    /// a `@@:return` inside an `if x:` has to be indented under it. A brace target does
    /// not care. So this is a fact on the node, and what to DO with it is a spelling.
    pub col: u32,
    /// `@@:return(` or `@@:(` — Frame's syntax.
    pub head: TriviaNode,
    /// `)` plus any terminator and trailing whitespace.
    pub tail: TriviaNode,
    /// Brace nesting DEPTH within the handler body. **0 = top level.**
    ///
    /// A terminal statement only terminates the BODY when it is at depth 0. A
    /// `@@:return` inside an `if` block returns from that branch; the code after the
    /// block is still reachable.
    ///
    /// The scanner records this because it is LEXING and already knows. Without it the
    /// emitter dropped the `if` block's closing brace and every statement after it —
    /// emitting a file with unbalanced braces.
    pub depth: u32,
    /// The expression, tokenized. NOT split, NOT interpreted.
    pub expr: Vec<NativePart>,
    pub expr_span: Span,
}

#[derive(Debug)]
pub struct SelfCallStmt {
    pub span: Span,
    /// The statement's COLUMN in the source.
    ///
    /// An indent-delimited target (Python, GDScript) must reproduce the user's nesting:
    /// a `@@:return` inside an `if x:` has to be indented under it. A brace target does
    /// not care. So this is a fact on the node, and what to DO with it is a spelling.
    pub col: u32,
    pub method: String,
    /// The args, verbatim, **as one blob**. framec does not split them; the target
    /// compiler does, correctly and for free.
    pub args_text: String,
}

/// `<frame-ref> = <native expr>`
#[derive(Debug)]
pub struct AssignStmt {
    pub span: Span,
    /// The statement's COLUMN in the source.
    ///
    /// An indent-delimited target (Python, GDScript) must reproduce the user's nesting:
    /// a `@@:return` inside an `if x:` has to be indented under it. A brace target does
    /// not care. So this is a fact on the node, and what to DO with it is a spelling.
    pub col: u32,
    /// What is being assigned TO. A Frame reference — framec's own syntax.
    pub lhs: FrameRef,
    /// The right-hand side: native code, **tokenized** (its literals and its own Frame
    /// refs are nodes), and NOT including any trailing terminator.
    pub rhs: Vec<NativePart>,
    pub rhs_span: Span,
    /// The ` = ` between them. Frame's own syntax — and a NODE, because every byte is.
    pub op: TriviaNode,
    /// Everything after the RHS: the terminator and any trailing whitespace.
    pub tail: Option<TriviaNode>,
    /// The trailing terminator the user wrote, if any — `;`.
    ///
    /// It is consumed as part of Frame's statement and **re-emitted by the backend in
    /// its own spelling**. It is not "did the user terminate their native code?" (that
    /// question is the user's business and framec has no opinion); it is "where does
    /// Frame's statement end?", which is delimitation and is framec's job.
    pub terminator: Option<Span>,
}

/// A native statement: **a container**, not a leaf.
#[derive(Debug)]
pub struct NativeStmt {
    pub span: Span,
    /// The parts. **They partition `span`.**
    pub parts: Vec<NativePart>,
    /// Column relative to the handler body's base — RENDER's re-indent basis.
    pub logical_indent: u32,
    /// Brace nesting DEPTH — **never block KIND**.
    ///
    /// Only two consumers exist and neither cares what kind of block it is:
    /// unreachable-code suppression after a transition's implicit `return` (Java is
    /// essentially alone — the only target where dead code is a *compile error*), and
    /// Python/GDScript indentation. Both want a number.
    ///
    /// `None` where a lexer cannot honestly compute it. In Ruby, `x = 1 if y` (a
    /// modifier, no `end`) and `if y … end` (a block) are the **same token sequence**
    /// in different grammatical positions — no lexer can tell them apart, and framec
    /// does not parse Ruby. Ruby does not consume this field, so Ruby never needs it.
    /// **Where framec cannot know, it says so. It does not guess.** A guess is what
    /// produced the bug family.
    pub block_depth: Option<u32>,
}

/// A piece of a native statement.
#[derive(Debug)]
pub enum NativePart {
    /// Opaque target bytes. framec carries these and never asks what they mean.
    Text(TriviaNode),
    /// A string/comment/raw literal — **an extent framec must never touch**, plus the
    /// code holes inside it.
    Literal(LiteralNode),
    /// A Frame reference spliced mid-expression: `$.count`, `@@:self.factor`.
    Ref(FrameRef),
    /// `@@SystemName(args)` — Frame's own instantiation syntax (spec §1103), captured as
    /// a STRUCTURED call. Emit matches the call args against the target system's declared
    /// params — filling defaults, ordering for the constructor, routing state/enter args —
    /// so the `(...)` is never opaque water when the system takes params.
    Instantiate(Instantiation),
    /// `@@:self.<field>.<method>(args)` — an embedded-system interface call (RFC-0046). If
    /// `<field>` is a domain field whose type is a defined system, framec emits the call in
    /// the target's idiom (on C the cross-system free-function form `Sys_method(self->field,
    /// args)`); otherwise it is a native method call on a scalar field's value. Which one is
    /// decided at emit from the field's declared type, so the scanner just captures shape.
    EmbedCall(EmbedCall),
}

/// A `@@:self.<field>.<method>(args)` call site (RFC-0046).
#[derive(Debug)]
pub struct EmbedCall {
    pub span: Span,
    /// The domain field the call receives on — `inner` in `@@:self.inner.ping()`.
    pub field: String,
    /// The method invoked — `ping`.
    pub method: String,
    /// The args, verbatim, as one blob (may be empty). framec does not split them.
    pub args: String,
}

/// A `@@SystemName(...)` call site. The args are captured as parsed groups; matching them
/// to the declared params (order, defaults, sigil routing) happens at emit, where the
/// symbol table is in scope.
#[derive(Debug)]
pub struct Instantiation {
    pub span: Span,
    pub name: String,
    /// The call-site args, in source order. Empty for `@@Name()`.
    pub args: Vec<InstArg>,
    /// `@@Name(x=1, y=2)` (named) vs `@@Name(1, 2)` (positional). Spec §1108: a single
    /// call may not mix the two.
    pub named: bool,
}

/// One call-site argument: its group (from the sigil), an optional name (named form), and
/// the verbatim value expression.
#[derive(Debug)]
pub struct InstArg {
    pub group: ParamGroup,
    /// `Some("x")` in the named form (`$(x=7)` / `name="R2D2"`); `None` positionally.
    pub name: Option<String>,
    /// The value expression, verbatim.
    pub value: String,
}

/// Which header group a param/arg belongs to, decided by its call-site (or declaration)
/// sigil: `$(...)` state, `$>(...)` enter, bare domain.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParamGroup {
    State,
    Enter,
    Domain,
}

/// A string, comment, or raw literal inside native code.
#[derive(Debug)]
pub struct LiteralNode {
    pub span: Span,
    /// The delimiter byte. Carried as a FACT rather than re-derived at 39 sites — in
    /// the old compiler that re-derivation got the wrong answer on 8 targets, because
    /// `'x'` is a **char** in C#/Java/Kotlin/Swift/C/C++/Go/Rust, not a string (#221).
    pub delim: u8,
    /// The parts. **They partition `span`** — content, holes, content, …
    pub parts: Vec<LiteralPart>,
}

#[derive(Debug)]
pub enum LiteralPart {
    /// String CONTENT. Bytes. framec does not look here — ever.
    Content(TriviaNode),
    /// An interpolation HOLE: an expression position. framec looks here.
    Hole(Hole),
}

/// An interpolation hole — `{…}` / `${…}` / `\(…)`. **Code, not content.**
#[derive(Debug)]
pub struct Hole {
    pub span: Span,
    /// The hole's parts. Frame refs may live here; that is the whole point.
    pub parts: Vec<NativePart>,
}

/// A Frame reference: `$.x`, `@@:self.f`, `@@:data.k`, `@@:params.k`, `@@:return`, …
#[derive(Debug)]
pub struct FrameRef {
    pub span: Span,
    pub kind: RefKind,
    /// The name after the sigil: `count` for `$.count`, `factor` for `@@:self.factor`,
    /// `k` for `@@:params.k`.
    ///
    /// A fact **framec** put here — it is Frame's own syntax. So EMIT may ask for it,
    /// and does not have to re-derive it by re-reading the span (RULE 1). This is the
    /// difference between a compiler and a pile of string oracles.
    pub name: String,
}

/// Which reference this is.
///
/// This *is* a `kind` field, and that is deliberate and different: a `FrameRef` is a
/// **leaf** — it has no interior structure that varies by kind. The B1b lesson was
/// about *statements*, which have different children per construct and therefore must
/// carry identity in the type. A ref carries a name and nothing else.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefKind {
    StateVar,
    ContextSelf,
    ContextData,
    ContextParams,
    ContextReturn,
    ContextEvent,
    ContextSystemState,
    SelfCall,
}

#[derive(Debug)]
pub struct TransitionStmt {
    pub span: Span,
    /// The statement's COLUMN in the source.
    ///
    /// An indent-delimited target (Python, GDScript) must reproduce the user's nesting:
    /// a `@@:return` inside an `if x:` has to be indented under it. A brace target does
    /// not care. So this is a fact on the node, and what to DO with it is a spelling.
    pub col: u32,
    /// Brace nesting DEPTH within the handler body. **0 = top level.**
    ///
    /// A terminal statement only terminates the BODY when it is at depth 0. A
    /// `@@:return` inside an `if` block returns from that branch; the code after the
    /// block is still reachable.
    ///
    /// The scanner records this because it is LEXING and already knows. Without it the
    /// emitter dropped the `if` block's closing brace and every statement after it —
    /// emitting a file with unbalanced braces.
    pub depth: u32,
    pub target: Option<String>,
    /// Args, if any. **Not split.**
    ///
    /// framec does NOT compute the arity. It cannot: in C++, `f(a < b, c > d)` (two
    /// comparisons) and `f(std::map<int, int>())` (one generic) are the same token
    /// shape, and telling them apart needs name lookup over the user's types that
    /// C++'s own grammar cannot do (#218). Thirteen of sixteen backends already got
    /// this right by **never splitting** — they hand the blob to a variadic and let
    /// the target compiler do it, which also hands the arity diagnostic back for free.
    ///
    /// The cheapest number of times to compute a fact is sometimes **zero**.
    pub args: Option<Span>,
    /// The state args, verbatim, **as one blob**. Never split. `-> $T(these)`.
    pub args_text: Option<String>,
    /// Exit args — `(these) -> $T` — delivered to the SOURCE state's `<$` exit handler.
    pub exit_args: Option<String>,
    /// Enter args — `-> (these) $T` — delivered to the TARGET state's `$>` enter handler.
    pub enter_args: Option<String>,
}

#[derive(Debug)]
pub struct SimpleStmt {
    pub span: Span,
    /// Exit args on `(reason) -> pop$` — delivered to the current state's `<$` handler
    /// before the pop. (Forward ignores this.)
    pub exit_args: Option<String>,
    /// The statement's COLUMN in the source.
    ///
    /// An indent-delimited target (Python, GDScript) must reproduce the user's nesting:
    /// a `@@:return` inside an `if x:` has to be indented under it. A brace target does
    /// not care. So this is a fact on the node, and what to DO with it is a spelling.
    pub col: u32,
    /// Brace nesting DEPTH within the handler body. **0 = top level.**
    ///
    /// A terminal statement only terminates the BODY when it is at depth 0. A
    /// `@@:return` inside an `if` block returns from that branch; the code after the
    /// block is still reachable.
    ///
    /// The scanner records this because it is LEXING and already knows. Without it the
    /// emitter dropped the `if` block's closing brace and every statement after it —
    /// emitting a file with unbalanced braces.
    pub depth: u32,
}

// ---------------------------------------------------------------- Node impls

impl Node for Body {
    fn span(&self) -> Span {
        self.span
    }
    fn children(&self) -> Vec<&dyn Node> {
        self.stmts.iter().map(|s| s as &dyn Node).collect()
    }
    fn kind(&self) -> &'static str {
        "Body"
    }
    fn is_leaf_on_purpose(&self) -> bool {
        self.stmts.is_empty() // an empty body `{ }` is genuinely empty
    }
}

impl Node for Stmt {
    fn span(&self) -> Span {
        match self {
            Stmt::Trivia(t) => t.span,
            Stmt::Native(n) => n.span,
            Stmt::Transition(t) | Stmt::StackPush(t) => t.span,
            Stmt::StackPop(s) | Stmt::Forward(s) => s.span,
            Stmt::Assign(a) => a.span,
            Stmt::ReturnCall(r) => r.span,
            Stmt::SelfCall(c) => c.span,
        }
    }
    fn children(&self) -> Vec<&dyn Node> {
        match self {
            Stmt::Native(n) => n.parts.iter().map(|p| p as &dyn Node).collect(),
            // An ASSIGNMENT has an RHS — full of literals and Frame refs. Claiming it is
            // a leaf would hide exactly the structure this tree exists to hold. (I did
            // claim that, to get it compiling, and the granularity census caught it.)
            // EVERY byte of the assignment is a node: the LHS ref, the `=`, the RHS
            // parts, and the terminator. The first version made only the RHS a child and
            // the recursive-totality check caught it instantly — the LHS and the `;`
            // belonged to nothing.
            Stmt::Assign(a) => {
                let mut v: Vec<&dyn Node> = vec![&a.lhs, &a.op];
                v.extend(a.rhs.iter().map(|p| p as &dyn Node));
                if let Some(t) = &a.tail {
                    v.push(t);
                }
                v
            }
            Stmt::ReturnCall(r) => {
                let mut v: Vec<&dyn Node> = vec![&r.head];
                v.extend(r.expr.iter().map(|p| p as &dyn Node));
                v.push(&r.tail);
                v
            }
            _ => Vec::new(),
        }
    }
    fn kind(&self) -> &'static str {
        match self {
            Stmt::Trivia(_) => "Trivia",
            Stmt::Native(_) => "NativeStmt",
            Stmt::Transition(_) => "Transition",
            Stmt::StackPush(_) => "StackPush",
            Stmt::StackPop(_) => "StackPop",
            Stmt::Forward(_) => "Forward",
            Stmt::Assign(_) => "Assign",
            Stmt::ReturnCall(_) => "ReturnCall",
            Stmt::SelfCall(_) => "SelfCall",
        }
    }
    fn is_leaf_on_purpose(&self) -> bool {
        match self {
            Stmt::Trivia(_) => true,
            // Frame statements framec authored. Their interior is held in typed fields.
            Stmt::Transition(_) | Stmt::StackPush(_) | Stmt::StackPop(_) | Stmt::Forward(_) => true,
            // A SelfCall's args are one opaque blob by design (framec does not split
            // them — the target compiler does). It is a genuine leaf.
            Stmt::SelfCall(_) => true,
            // These are NOT leaves. Their RHS/expr is a tree.
            Stmt::Assign(_) | Stmt::ReturnCall(_) => false,
            // A native statement has parts. It is NOT a leaf — that was the whole bug.
            Stmt::Native(n) => n.parts.is_empty(),
        }
    }
}

impl Node for NativePart {
    fn span(&self) -> Span {
        match self {
            NativePart::Text(t) => t.span,
            NativePart::Literal(l) => l.span,
            NativePart::Ref(r) => r.span,
            NativePart::Instantiate(i) => i.span,
            NativePart::EmbedCall(e) => e.span,
        }
    }
    fn children(&self) -> Vec<&dyn Node> {
        match self {
            NativePart::Literal(l) => l.parts.iter().map(|p| p as &dyn Node).collect(),
            _ => Vec::new(),
        }
    }
    fn kind(&self) -> &'static str {
        match self {
            NativePart::Text(_) => "NativeText",
            NativePart::Literal(_) => "Literal",
            NativePart::Ref(_) => "FrameRef",
            NativePart::Instantiate(_) => "Instantiation",
            NativePart::EmbedCall(_) => "EmbedCall",
        }
    }
    fn is_leaf_on_purpose(&self) -> bool {
        match self {
            // OPAQUE TARGET BYTES. "No structure framec is entitled to know" is exactly
            // right, and it is the Oceans model in one line. The extent is known; the
            // meaning never is.
            NativePart::Text(_) => true,
            NativePart::Ref(_) => true,
            // `@@Name(...)` — a leaf whose span is fully accounted; its args are parsed
            // fields (Frame's own syntax), not sub-Nodes framec must re-cover.
            NativePart::Instantiate(_) => true,
            // `@@:self.field.method(...)` — likewise a leaf; field/method/args are parsed.
            NativePart::EmbedCall(_) => true,
            NativePart::Literal(l) => l.parts.is_empty(),
        }
    }
}

impl Node for LiteralPart {
    fn span(&self) -> Span {
        match self {
            LiteralPart::Content(c) => c.span,
            LiteralPart::Hole(h) => h.span,
        }
    }
    fn children(&self) -> Vec<&dyn Node> {
        match self {
            LiteralPart::Hole(h) => h.parts.iter().map(|p| p as &dyn Node).collect(),
            LiteralPart::Content(_) => Vec::new(),
        }
    }
    fn kind(&self) -> &'static str {
        match self {
            LiteralPart::Content(_) => "StringContent",
            LiteralPart::Hole(_) => "Hole",
        }
    }
    fn is_leaf_on_purpose(&self) -> bool {
        match self {
            // STRING CONTENT. framec does not look here. This leaf claim is the
            // language decision, made structural: a `$.x` in string content is NOT a
            // Frame reference, and there is no variant that could make it one.
            LiteralPart::Content(_) => true,
            LiteralPart::Hole(h) => h.parts.is_empty(),
        }
    }
}


/// A Frame reference is a leaf, and a node.
impl Node for FrameRef {
    fn span(&self) -> Span {
        self.span
    }
    fn children(&self) -> Vec<&dyn Node> {
        Vec::new()
    }
    fn kind(&self) -> &'static str {
        "FrameRef"
    }
    fn is_leaf_on_purpose(&self) -> bool {
        true
    }
}

/// `TriviaNode` is a leaf: a run of bytes with no further structure framec may know.
/// Used for Frame's own punctuation (`=`, `;`, `@@:return(`) and for opaque target text.
impl Node for TriviaNode {
    fn span(&self) -> Span {
        self.span
    }
    fn children(&self) -> Vec<&dyn Node> {
        Vec::new()
    }
    fn kind(&self) -> &'static str {
        "Trivia"
    }
    fn is_leaf_on_purpose(&self) -> bool {
        true
    }
}
