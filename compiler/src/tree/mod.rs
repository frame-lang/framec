//! The tree. **Total: every byte of the file belongs to exactly one item.**
//!
//! # The two invariants
//!
//! The old compiler had an AST of the system *skeleton* and **no AST of handler
//! bodies**. Below the handler's opening brace it was a flat segment stream with
//! native code as an opaque string — so every downstream pass re-derived structure
//! by reading text, and twenty-five shipped bugs came from exactly that. It is not a
//! collection of mistakes; it is one missing tree, observed twenty-five times.
//!
//! So the tree is **total**. And totality is checked by *two* invariants, not one,
//! because the obvious one is a trap:
//!
//! ### I1 — Coverage (necessary, and weak)
//!
//! > `unparse(parse(src)) == src`, byte for byte.
//!
//! Cheap, and it does stop dropped trivia and quietly-widened spans. But it has a
//! **trivial satisfying assignment: classify the whole file as water.** Coverage
//! cannot distinguish *"understood the file"* from *"understood nothing."*
//!
//! That is not hypothetical. In the old compiler a UTF-8 BOM made the segmenter
//! classify an entire `@@system` as native text, emit it verbatim, and exit 0
//! (#214). Coverage held **perfectly**. The compiler understood nothing.
//!
//! ### I2 — Island coverage (the dual — this is the one with teeth)
//!
//! > For every offset at which a Frame construct exists, the tree has a Frame node.
//! > **A file that parses to nothing but water is an ERROR, not a success.**
//!
//! The failure that ships is not *forgetting a byte*. It is **classifying an island
//! as water**. I1 is blind to that; I2 is not.

pub mod body;
pub mod node;

pub use node::{census, check_total, Defect, Node};

use crate::Span;

/// A parsed file. Items are in source order and **partition the file exactly**.
#[derive(Debug)]
pub struct FileAst {
    pub items: Vec<Item>,
    /// Total length of the source, so coverage is checkable without the `Source`.
    pub source_len: usize,
}

/// A top-level item.
///
/// Each variant is its own type. That is deliberate: an earlier attempt collapsed
/// ~20 Frame constructs into one blob that carried its identity in a `kind` *field*,
/// and an exhaustive-match test caught it instantly while a byte-diff of the output
/// showed **nothing** — the emitted code was identical. Only the *type* caught it.
/// Identity lives in the type, never in a field.
#[derive(Debug)]
pub enum Item {
    /// A leading UTF-8 BOM.
    ///
    /// It **is** a byte, it is **not** native code, and in the old compiler it
    /// destroyed the compile (#214). So it is a node: `unparse` reproduces it (I1
    /// holds with no special case), and codegen ignores it (a BOM belongs to the
    /// file it arrived in, not to the file we generate).
    Bom(BomItem),
    /// The ocean. Delimited, **never interpreted**.
    Native(NativeItem),
    Pragma(PragmaItem),
    System(SystemItem),
    /// A sibling item. Its *compiler* is separate and is not part of this rebuild.
    Efsm(EfsmItem),
}

#[derive(Debug)]
pub struct BomItem {
    pub span: Span,
}

#[derive(Debug)]
pub struct NativeItem {
    pub span: Span,
    /// The water, decomposed into parts so `@@SystemName()` islands (spec §1103) are
    /// lowered even in top-level native code. Non-island parts render verbatim.
    pub parts: Vec<crate::tree::body::NativePart>,
}

#[derive(Debug)]
pub struct PragmaItem {
    pub span: Span,
    /// The attribute name: `@@[async]` -> "async", `@@[persist]` -> "persist".
    ///
    /// Frame's own vocabulary, read by the scanner. A later pass may ask for it (RULE 1);
    /// nothing re-derives it from text.
    pub attr: Option<String>,
}

#[derive(Debug)]
pub struct SystemItem {
    pub span: Span,
    /// The system's name, as written. A fact framec put here (RULE 1: a pass may
    /// interrogate a node about facts *framec* put there, never facts the *user*
    /// put there — a system's name is ours; the body of a native statement is not).
    pub name: String,
    /// The system's interior. **Partitions `span`.**
    ///
    /// This is the line the old compiler never crossed: it had an AST of the system
    /// SKELETON and nothing below it. Every one of the 25 bugs lives down here.
    pub sections: Vec<Section>,
    /// Header params — `@@system Name($(state), $>(enter), domain)` (spec §203).
    /// Domain params become constructor args (in scope for domain inits); state/enter
    /// params seed the start compartment. Verbatim types; framec reorders, never parses.
    pub params: SystemParams,
    /// `@@system private Name` — the system is emitted with reduced (package/module-private)
    /// visibility on targets that have class-level visibility (Java: `class` not `public
    /// class`). `public` is the default; the redundant keyword and `private` on a target
    /// without class visibility are diagnosed at resolve.
    pub private: bool,
    /// The redundant `@@system public Name` keyword was written explicitly. Systems are public
    /// by default, so this is diagnosed (E730) — kept distinct from the no-modifier default.
    pub public_keyword: bool,
}

/// The three header param groups of `@@system Name(...)`.
#[derive(Debug, Default, Clone)]
pub struct SystemParams {
    /// `$(name: type = default)` — start state's `state_args`.
    pub state: Vec<Param>,
    /// `$>(name: type = default)` — start state's `enter_args`.
    pub enter: Vec<Param>,
    /// bare `name: type = default` — constructor args, in scope for domain inits.
    pub domain: Vec<Param>,
}

#[derive(Debug, Clone)]
pub struct Param {
    pub name: String,
    /// Verbatim type text (the user's), or None if untyped.
    pub ty: Option<String>,
    /// Verbatim default expression, or None.
    pub default: Option<String>,
}

/// A section of a system. Together these **partition the system's span** — including
/// the header, the trivia between sections, and the closing brace. Nothing is "just
/// formatting"; a byte with no node is a byte a later pass will have to guess about.
#[derive(Debug)]
pub enum Section {
    /// `@@system Name(params) {` — everything up to and including the opening brace.
    Header(HeaderSection),
    Interface(DeclSection),
    /// The one section with real depth: states -> handlers -> statements.
    Machine(MachineSection),
    Domain(DeclSection),
    Actions(DeclSection),
    Operations(DeclSection),
    /// Whitespace and comments **inside** the system — Frame's own trivia, which
    /// framec MAY reformat. (Distinct from water, which it may never touch.)
    Trivia(TriviaNode),
    /// The system's closing `}`.
    Close(TriviaNode),
}

#[derive(Debug)]
pub struct HeaderSection {
    pub span: Span,
}

/// A section whose members are declarations: `interface:`, `domain:`, `actions:`,
/// `operations:`.
#[derive(Debug)]
pub struct DeclSection {
    pub span: Span,
    /// The `interface:` / `domain:` keyword.
    pub keyword_node: FrameSpan,
    /// Members. **They partition the span after the keyword.**
    pub members: Vec<Decl>,
}

/// One declaration, or the trivia between two.
#[derive(Debug)]
pub enum Decl {
    Trivia(TriviaNode),
    /// A signature or field: `go()`, `n: int = 0`, `doThing(a: int): bool`.
    ///
    /// Frame's own vocabulary — framec authored the shape of this line, so it may
    /// interrogate it (RULE 1). Its type annotation, though, is the USER's text and
    /// passes through verbatim.
    Member(MemberDecl),
    /// An `actions:` / `operations:` member with a NATIVE body.
    ///
    /// The signature is Frame's; the body is the user's, and is decomposed into parts
    /// like any other native code.
    WithBody(BodyDecl),
}

#[derive(Debug, PartialEq)]
pub struct MemberDecl {
    pub span: Span,
    /// The declared name: `go`, `n`, `count`.
    ///
    /// A fact **framec** put here — Frame's own vocabulary — so a later pass may ask
    /// for it (RULE 1). The scanner extracts it; nothing downstream re-derives it from
    /// text, because nothing downstream *can*: RESOLVE lives outside `crate::text` and
    /// cannot open a byte. The wall forced this design rather than merely allowing it.
    pub name: String,
    /// The type annotation, **exactly as the user wrote it**.
    ///
    /// Carried verbatim and **never parsed**. framec has no type system; a type is the
    /// user's text and it passes straight through to the target. Parsing it would mean
    /// sixteen type grammars — and that is the "never parse native code" rule broken
    /// one level up.
    pub type_text: Option<String>,
    /// Parameters, verbatim, if this is a signature (`go(a: int, b: str)`).
    pub params_text: Option<String>,
    /// If the initializer is a Frame system instantiation — `= @@Inner()` — the name
    /// of that system.
    ///
    /// **This is how framec learns a field holds a system, and it is the RIGHT way**,
    /// because `@@Inner()` is *Frame's own syntax* — a fact framec put there — while
    /// the type annotation `Inner*` is the *user's* text (RULE 1).
    ///
    /// Reading the type to recover this fact was reading the user's code to learn
    /// something framec's own code already said. It is also unreliable: `Inner*` is not
    /// a wrapper a C user chose, it is **C's mandatory spelling** for a system instance
    /// (C has no references; `create` returns a pointer). Telling them to "just write
    /// `Inner`" would be telling them to write something that is not C.
    pub init_system: Option<String>,
    /// `async fetch(...)` — the event is asynchronous.
    ///
    /// A MODIFIER, carried as a fact on the node. Not part of the name — reading it as
    /// one emitted `def async(self):`, and `async` is a Python keyword.
    pub is_async: bool,
    /// The initializer expression after `=`, VERBATIM. `count: int = 0` -> "0";
    /// `cache: Cache = Cache` -> "Cache". The user's native expression — emitted
    /// unchanged, never interpreted. Ignoring it (emitting a default) is a verbatim
    /// violation that only hid because scalars default to the same value.
    pub init_text: Option<String>,
}

#[derive(Debug)]
pub struct BodyDecl {
    pub span: Span,
    /// The action's name — Frame's vocabulary.
    pub name: String,
    /// Its parameters, verbatim.
    pub params_text: String,
    /// Its declared return type, verbatim (the user's text).
    pub return_text: Option<String>,
    pub signature_node: FrameSpan,
    pub body: body::Body,
    /// The member's closing `}`.
    ///
    /// It has to be a CHILD, not a sibling. When it was neither, the recursive
    /// totality check caught it immediately: `BodyDecl`'s children left its last byte
    /// uncovered (a gap) and the sibling trivia I pushed to compensate overlapped it.
    /// One mistake, reported twice, before it could reach anything.
    pub close_node: FrameSpan,
}

/// `machine:` — states, and the trivia between them.
#[derive(Debug)]
pub struct MachineSection {
    pub span: Span,
    pub keyword_node: FrameSpan,
    pub members: Vec<MachineMember>,
}

#[derive(Debug)]
pub enum MachineMember {
    Trivia(TriviaNode),
    State(StateNode),
}

/// `$Name(params) { …handlers… }`
#[derive(Debug)]
pub struct StateNode {
    pub span: Span,
    pub name: String,
    /// The state's declared parameters: `$B(n: int)` -> ["n"].
    ///
    /// Names only. The TYPES are the user's text and stay in the header span — framec
    /// does not need them, because it never splits the ARGS either (it hands the blob
    /// to the target compiler, which splits it correctly and for free). One fact
    /// framec does not compute is one fact framec cannot get wrong.
    pub params: Vec<String>,
    /// Their declared types, verbatim (the user's text).
    pub param_types: std::collections::HashMap<String, String>,
    /// **The parent state.** `$Awake => $Live` -> `Some("Live")`.
    ///
    /// This is what makes the machine HIERARCHICAL. An event a child does not handle is
    /// handled by its nearest ancestor that does. Ignoring it does not fail to compile —
    /// it produces a FLAT machine that silently drops events, which is the worst kind of
    /// wrong: it looks like it works.
    pub parent: Option<String>,
    /// `$Name(params) {` — Frame's, up to and including the opening brace.
    pub header_node: FrameSpan,
    pub members: Vec<StateMember>,
    /// The state's closing `}`.
    pub close_node: FrameSpan,
}

#[derive(Debug)]
pub enum StateMember {
    Trivia(TriviaNode),
    Handler(HandlerNode),
    /// A state variable: `$.n: int = 0`
    StateVar(MemberDecl),
}

/// `event(params) { …body… }` — or `$>()` / `<$()` for enter/exit.
#[derive(Debug)]
pub struct HandlerNode {
    pub span: Span,
    pub event: String,
    /// The handler's parameters, verbatim.
    pub params_text: String,
    /// The declared return type, verbatim: `decide(score: int): String`.
    ///
    /// The TYPE is the user's text and passes through untouched. The `: T` *syntax* is
    /// Frame's, so reading it is RULE 1-clean.
    pub return_text: Option<String>,
    /// `event(params) {` — Frame's.
    pub header_node: FrameSpan,
    /// **The tree the old compiler did not have.**
    pub body: body::Body,
    /// The handler's closing `}`.
    pub close_node: FrameSpan,
}

#[derive(Debug)]
pub struct TriviaNode {
    pub span: Span,
}

#[derive(Debug)]
pub struct EfsmItem {
    pub span: Span,
    pub name: String,
}

impl Item {
    pub fn span(&self) -> Span {
        match self {
            Item::Bom(i) => i.span,
            Item::Native(i) => i.span,
            Item::Pragma(i) => i.span,
            Item::System(i) => i.span,
            Item::Efsm(i) => i.span,
        }
    }

    /// Is this an **island** (something framec actually understood), as opposed to
    /// water or an encoding marker?
    pub fn is_island(&self) -> bool {
        match self {
            Item::Pragma(_) | Item::System(_) | Item::Efsm(_) => true,
            Item::Bom(_) | Item::Native(_) => false,
        }
    }
}

/// Why a tree failed its invariants. These are **compiler bugs**, not user errors,
/// and they are named as such.
#[derive(Debug, PartialEq, Eq)]
pub enum TreeDefect {
    /// I1: a byte belongs to no item.
    Gap { from: usize, to: usize },
    /// I1: a byte belongs to two items.
    Overlap { at: usize },
    /// I1: items are not in source order.
    OutOfOrder { at: usize },
    /// I2: the file is nothing but water. Coverage would call this a success.
    /// It is the BOM bug, and it is the one that ships.
    AllWater,
}

impl std::fmt::Display for TreeDefect {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TreeDefect::Gap { from, to } => write!(
                f,
                "COMPILER BUG: bytes {from}..{to} belong to no item — the tree is not total"
            ),
            TreeDefect::Overlap { at } => write!(
                f,
                "COMPILER BUG: byte {at} belongs to two items — spans overlap"
            ),
            TreeDefect::OutOfOrder { at } => {
                write!(f, "COMPILER BUG: items are not in source order at byte {at}")
            }
            TreeDefect::AllWater => write!(
                f,
                "COMPILER BUG: the file parsed to nothing but native text — no Frame \
                 construct was recognized anywhere. Byte coverage would call this a \
                 success; it is how a whole `@@system` silently became water (#214)."
            ),
        }
    }
}

impl FileAst {
    /// **I1 — Coverage.** The items partition `[0, source_len)` exactly: sorted, no
    /// gaps, no overlaps.
    pub fn check_coverage(&self) -> Result<(), TreeDefect> {
        let mut cursor = 0usize;
        for item in &self.items {
            let s = item.span();
            if s.start < cursor {
                return Err(if s.start < cursor && !self.items.is_empty() {
                    TreeDefect::Overlap { at: s.start }
                } else {
                    TreeDefect::OutOfOrder { at: s.start }
                });
            }
            if s.start > cursor {
                return Err(TreeDefect::Gap {
                    from: cursor,
                    to: s.start,
                });
            }
            cursor = s.end;
        }
        if cursor != self.source_len {
            return Err(TreeDefect::Gap {
                from: cursor,
                to: self.source_len,
            });
        }
        Ok(())
    }

    /// **I2 — Island coverage.** At least one Frame construct was recognized.
    ///
    /// This is the invariant coverage structurally cannot express. A tree of one
    /// `Native` item spanning the whole file satisfies I1 *perfectly* and means the
    /// compiler understood nothing.
    pub fn check_islands(&self) -> Result<(), TreeDefect> {
        if self.items.iter().any(Item::is_island) {
            Ok(())
        } else {
            Err(TreeDefect::AllWater)
        }
    }

    /// Both invariants. Every parse must pass this.
    pub fn check(&self) -> Result<(), TreeDefect> {
        self.check_coverage()?;
        self.check_islands()
    }

    /// **I1, constructively.** Reassemble the source from the tree. If this is not
    /// byte-identical to the input, a span is wrong.
    ///
    /// Note this reads bytes — so it takes them from the caller rather than reaching
    /// for them, and it lives here rather than in a pass. It is a *test oracle*, not
    /// a compiler stage.
    pub fn unparse(&self, original: &[u8]) -> Vec<u8> {
        let mut out = Vec::with_capacity(original.len());
        for item in &self.items {
            let s = item.span();
            out.extend_from_slice(&original[s.start..s.end]);
        }
        out
    }

    pub fn islands(&self) -> impl Iterator<Item = &Item> {
        self.items.iter().filter(|i| i.is_island())
    }
}

// ===========================================================================
// Node impls — the recursive-totality contract.
//
// Note what each `is_leaf_on_purpose` is CLAIMING. It is not a formality: it is the
// difference between "these bytes have no structure framec may know" and "these bytes
// have structure and I haven't parsed it." The old compiler never had to make that
// claim out loud about a handler body, and so it never noticed it wasn't making it.
// ===========================================================================

impl Node for FileAst {
    fn span(&self) -> Span {
        Span::new(0, self.source_len)
    }
    fn children(&self) -> Vec<&dyn Node> {
        self.items.iter().map(|i| i as &dyn Node).collect()
    }
    fn kind(&self) -> &'static str {
        "File"
    }
}

impl Node for Item {
    fn span(&self) -> Span {
        Item::span(self)
    }
    fn children(&self) -> Vec<&dyn Node> {
        match self {
            Item::System(s) => s.children(),
            _ => Vec::new(),
        }
    }
    fn kind(&self) -> &'static str {
        match self {
            Item::Bom(_) => "Bom",
            Item::Native(_) => "Native",
            Item::Pragma(_) => "Pragma",
            Item::System(_) => "System",
            Item::Efsm(_) => "Efsm",
        }
    }
    fn is_leaf_on_purpose(&self) -> bool {
        match self {
            // A BOM is three bytes with no interior. True leaf.
            Item::Bom(_) => true,
            // WATER. framec must never interpret these bytes — "no structure framec is
            // entitled to know" is exactly right here, and it is the whole Oceans model
            // in one line. (Its *extent* is known; its meaning never is.)
            Item::Native(_) => true,
            // A pragma line. Will gain interior structure when attributes are parsed.
            Item::Pragma(_) => true,
            // The @@efsm compiler is a separate construct, out of scope for this
            // rebuild (see REUSE.md). Its interior is not ours to decompose.
            Item::Efsm(_) => true,
            Item::System(_) => false,
        }
    }
}

impl Node for SystemItem {
    fn span(&self) -> Span {
        self.span
    }
    fn children(&self) -> Vec<&dyn Node> {
        self.sections.iter().map(|s| s as &dyn Node).collect()
    }
    fn kind(&self) -> &'static str {
        "System"
    }
}

impl Node for Section {
    fn span(&self) -> Span {
        match self {
            Section::Header(h) => h.span,
            Section::Interface(d) | Section::Domain(d) | Section::Actions(d) | Section::Operations(d) => d.span,
            Section::Machine(m) => m.span,
            Section::Trivia(t) | Section::Close(t) => t.span,
        }
    }
    fn children(&self) -> Vec<&dyn Node> {
        match self {
            Section::Interface(d) | Section::Domain(d) | Section::Actions(d) | Section::Operations(d) => {
                let mut v: Vec<&dyn Node> = vec![&d.keyword_node];
                v.extend(d.members.iter().map(|m| m as &dyn Node));
                v
            }
            Section::Machine(m) => {
                let mut v: Vec<&dyn Node> = vec![&m.keyword_node];
                v.extend(m.members.iter().map(|x| x as &dyn Node));
                v
            }
            _ => Vec::new(),
        }
    }
    fn kind(&self) -> &'static str {
        match self {
            Section::Header(_) => "Header",
            Section::Interface(_) => "Interface",
            Section::Machine(_) => "Machine",
            Section::Domain(_) => "Domain",
            Section::Actions(_) => "Actions",
            Section::Operations(_) => "Operations",
            Section::Trivia(_) => "Trivia",
            Section::Close(_) => "Close",
        }
    }
    fn is_leaf_on_purpose(&self) -> bool {
        match self {
            // Frame's own whitespace, and the system's closing brace.
            Section::Trivia(_) | Section::Close(_) => true,
            // The header will decompose into name + param groups. Not yet — and it
            // says so by being a leaf that has not earned the claim... except it has:
            // there is no user code in a header, so nothing can hide in it.
            Section::Header(_) => true,
            Section::Interface(d) | Section::Domain(d) | Section::Actions(d) | Section::Operations(d) => d.members.is_empty(),
            Section::Machine(m) => m.members.is_empty(),
        }
    }
}

impl Node for Decl {
    fn span(&self) -> Span {
        match self {
            Decl::Trivia(t) => t.span,
            Decl::Member(m) => m.span,
            Decl::WithBody(b) => b.span,
        }
    }
    fn children(&self) -> Vec<&dyn Node> {
        match self {
            // An action/operation: its SIGNATURE is Frame's; its BODY is the user's;
            // and its closing brace is Frame's too.
            Decl::WithBody(b) => vec![&b.signature_node, &b.body, &b.close_node],
            _ => Vec::new(),
        }
    }
    fn kind(&self) -> &'static str {
        match self {
            Decl::Trivia(_) => "Trivia",
            Decl::Member(_) => "Decl",
            Decl::WithBody(_) => "DeclWithBody",
        }
    }
    fn is_leaf_on_purpose(&self) -> bool {
        match self {
            Decl::Trivia(_) => true,
            // A signature line: `go()`, `n: int = 0`. Frame's shape; the type
            // annotation is the user's text, carried verbatim, never parsed (framec
            // has no type system, and adding one to read `Rc<RefCell<Child>>` would
            // mean parsing 16 type grammars).
            Decl::Member(_) => true,
            Decl::WithBody(_) => false,
        }
    }
}

impl Node for MachineMember {
    fn span(&self) -> Span {
        match self {
            MachineMember::Trivia(t) => t.span,
            MachineMember::State(s) => s.span,
        }
    }
    fn children(&self) -> Vec<&dyn Node> {
        match self {
            MachineMember::State(s) => {
                let mut v: Vec<&dyn Node> = vec![&s.header_node];
                v.extend(s.members.iter().map(|m| m as &dyn Node));
                v.push(&s.close_node);
                v
            }
            MachineMember::Trivia(_) => Vec::new(),
        }
    }
    fn kind(&self) -> &'static str {
        match self {
            MachineMember::Trivia(_) => "Trivia",
            MachineMember::State(_) => "State",
        }
    }
    fn is_leaf_on_purpose(&self) -> bool {
        matches!(self, MachineMember::Trivia(_))
    }
}

impl Node for StateMember {
    fn span(&self) -> Span {
        match self {
            StateMember::Trivia(t) => t.span,
            StateMember::Handler(h) => h.span,
            StateMember::StateVar(v) => v.span,
        }
    }
    fn children(&self) -> Vec<&dyn Node> {
        match self {
            StateMember::Handler(h) => vec![&h.header_node, &h.body, &h.close_node],
            _ => Vec::new(),
        }
    }
    fn kind(&self) -> &'static str {
        match self {
            StateMember::Trivia(_) => "Trivia",
            StateMember::Handler(_) => "Handler",
            StateMember::StateVar(_) => "StateVar",
        }
    }
    fn is_leaf_on_purpose(&self) -> bool {
        match self {
            StateMember::Trivia(_) | StateMember::StateVar(_) => true,
            // *** A handler is NEVER a leaf. ***
            // This is the exact node the old compiler kept as a String. Every one of
            // the 25 bugs lives inside it.
            StateMember::Handler(_) => false,
        }
    }
}

/// A span of Frame's own text (a keyword, a header, a closing brace) — a true leaf.
#[derive(Debug)]
pub struct FrameSpan {
    pub span: Span,
    pub kind: &'static str,
}

impl Node for FrameSpan {
    fn span(&self) -> Span {
        self.span
    }
    fn children(&self) -> Vec<&dyn Node> {
        Vec::new()
    }
    fn kind(&self) -> &'static str {
        self.kind
    }
    fn is_leaf_on_purpose(&self) -> bool {
        true
    }
}
