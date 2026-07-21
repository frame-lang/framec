//! What each target's literals and comments look like.
//!
//! # Why this is a table
//!
//! The old compiler had **fifteen hand-written brace-counters** (`body_closer/*.frs`)
//! plus **thirteen hand-written skippers**, and they learned different subsets of
//! their own languages. The divergence shipped as bugs (#219): all of these are
//! ordinary, legal target code, and all of them were **rejected**:
//!
//! ```text
//! JS      let re = /[}]/;              PHP     $s = <<<EOT ... EOT;
//! Ruby    =begin ... } ... =end        Lua     local s = [==[ } ]==]
//! Ruby    a = %w[} foo]                Ruby    s = <<~EOS ... EOS
//! ```
//!
//! Each one is a literal form containing a `}` that the closer did not know about,
//! so it counted a brace that wasn't there and the file died at E002 — *while the
//! scanner sitting next to it knew perfectly well* (`php_skipper` **calls**
//! `skip_php_heredoc`; `body_closer/php.frs` has never heard of heredocs). Two
//! recognizers, diverged, in the water.
//!
//! So: **one lexer, parameterized by this table.** A target cannot know less about
//! its own strings than another target knows about its.
//!
//! # Why it is not *only* a table
//!
//! Some literal forms are not expressible as a delimiter pair, and pretending
//! otherwise is how you get a scanner that is quietly wrong:
//!
//! * Rust `r##"..."##` — the delimiter length is **counted at the open**.
//! * C++ `R"delim(...)delim"` — the delimiter is **named at the open**.
//! * Lua `[==[ ... ]==]` — same idea, third spelling.
//! * PHP/Ruby heredocs — the terminator is an **identifier chosen by the user**.
//! * JS/TS template literals — they **nest**: `` `${ `${x}` }` ``.
//! * JS/TS regex literals — `/` is regex-or-division depending on whether the
//!   previous token was an **operand or an operator**. This needs the token before
//!   it. It is the one form that genuinely needs lexer *state*, not just a pattern.
//!
//! These get code. But the *set is closed and named* — which is the whole difference
//! between this and fifteen scanners that each forgot something different.

/// A literal or comment form. The set is **closed**: if a target has a form that is
/// not in this enum, it is not supported, and it says so — it does not silently
/// mis-scan.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Form {
    /// `// ...`, `# ...`, `-- ...` — to end of line.
    LineComment(&'static str),
    /// `/* ... */`, `=begin ... =end`, `--[[ ... ]]`.
    BlockComment {
        open: &'static str,
        close: &'static str,
        /// Rust's `/* /* */ */` nest. C's do not.
        nests: bool,
    },
    /// `"..."` / `'...'` — a simple quoted string with backslash escapes.
    Quoted {
        delim: u8,
        /// C/C++/Java/Go char literals do not span lines; Python's `'...'` doesn't
        /// either. A string that cannot contain a newline lets us bail early on an
        /// unterminated one instead of eating the rest of the file.
        multiline: bool,
        escapes: bool,
    },
    /// `"""..."""` / `'''...'''`.
    TripleQuoted { delim: u8 },
    /// Rust `r"..."`, `r#"..."#`, `r##"..."##` — hash count fixed at the open.
    RustRaw,
    /// C++ `R"delim( ... )delim"` — delimiter named at the open.
    CppRaw,
    /// Lua `[[ ... ]]`, `[=[ ... ]=]`, `[==[ ... ]==]` — level fixed at the open.
    /// Also Lua's long *comments*: `--[==[ ... ]==]`.
    LuaLongBracket,
    /// PHP `<<<EOT ... EOT;` and `<<<'EOT' ... EOT;` (nowdoc).
    PhpHeredoc,
    /// Ruby `<<~EOS`, `<<-EOS`, `<<EOS`.
    RubyHeredoc,
    /// Ruby `%w[...]`, `%q(...)`, `%i{...}`, `%{...}` — delimiter chosen at the open,
    /// and it *nests* for bracket pairs.
    RubyPercent,
    /// `` `...${expr}...` `` — nests, and the `${}` holes contain real code.
    Template { delim: u8 },
    /// `/.../flags` — regex, but ONLY where an operand is expected. This is the
    /// single context-sensitive form: after `=`, `(`, `,`, `return`, an operator, it
    /// is a regex; after an identifier, a literal, `)`, `]` it is division.
    RegexLiteral,
}

/// Everything the lexer needs to know about one target.
#[derive(Debug, Clone, Copy)]
pub struct Literals {
    pub forms: &'static [Form],
}

/// Every target framec emits. Adding a target means adding a row here — and the
/// compiler will not let you forget, because this match is exhaustive.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Target {
    Python3,
    TypeScript,
    JavaScript,
    Rust,
    C,
    Cpp,
    Java,
    CSharp,
    Go,
    Php,
    Kotlin,
    Swift,
    Ruby,
    Lua,
    Dart,
    GdScript,
}

const C_FAMILY: &[Form] = &[
    Form::LineComment("//"),
    Form::BlockComment {
        open: "/*",
        close: "*/",
        nests: false,
    },
    Form::Quoted {
        delim: b'"',
        multiline: false,
        escapes: true,
    },
    Form::Quoted {
        delim: b'\'',
        multiline: false,
        escapes: true,
    },
];

impl Target {
    /// The literal forms of this target.
    ///
    /// **Exhaustive by construction.** A new target cannot be added without deciding
    /// what its strings look like — which is exactly the decision the old compiler
    /// let fifteen different files make fifteen different ways.
    pub fn literals(self) -> Literals {
        use Form::*;
        let forms: &'static [Form] = match self {
            // The C family, unmodified. `'x'` is a CHAR here, not a string — which is
            // why the old compiler's Python-only quote-swap emitted `state_vars['n']`
            // and broke C#, Kotlin and Swift (#221).
            Target::C | Target::Java | Target::CSharp | Target::Kotlin | Target::Go => C_FAMILY,

            Target::Cpp => &[
                LineComment("//"),
                BlockComment { open: "/*", close: "*/", nests: false },
                CppRaw, // R"delim(...)delim" — MUST precede Quoted; it starts with R"
                Quoted { delim: b'"', multiline: false, escapes: true },
                Quoted { delim: b'\'', multiline: false, escapes: true },
            ],

            Target::Rust => &[
                LineComment("//"),
                BlockComment { open: "/*", close: "*/", nests: true }, // Rust's DO nest
                RustRaw, // r#"..."# — MUST precede Quoted
                Quoted { delim: b'"', multiline: true, escapes: true },
                Quoted { delim: b'\'', multiline: false, escapes: true }, // char / lifetime
            ],

            Target::Swift => &[
                LineComment("//"),
                BlockComment { open: "/*", close: "*/", nests: true }, // Swift's DO nest
                TripleQuoted { delim: b'"' },
                Quoted { delim: b'"', multiline: false, escapes: true },
            ],

            Target::Python3 | Target::GdScript => &[
                LineComment("#"),
                TripleQuoted { delim: b'"' }, // MUST precede Quoted
                TripleQuoted { delim: b'\'' },
                Quoted { delim: b'"', multiline: false, escapes: true },
                Quoted { delim: b'\'', multiline: false, escapes: true },
            ],

            Target::JavaScript | Target::TypeScript => &[
                LineComment("//"),
                BlockComment { open: "/*", close: "*/", nests: false },
                Template { delim: b'`' },
                RegexLiteral, // /[}]/ — #219. Context-sensitive; see lex.rs.
                Quoted { delim: b'"', multiline: false, escapes: true },
                Quoted { delim: b'\'', multiline: false, escapes: true },
            ],

            Target::Dart => &[
                LineComment("//"),
                BlockComment { open: "/*", close: "*/", nests: true },
                TripleQuoted { delim: b'"' },
                TripleQuoted { delim: b'\'' },
                Quoted { delim: b'"', multiline: false, escapes: true },
                Quoted { delim: b'\'', multiline: false, escapes: true },
            ],

            Target::Php => &[
                LineComment("//"),
                LineComment("#"),
                BlockComment { open: "/*", close: "*/", nests: false },
                PhpHeredoc, // <<<EOT — #219
                Quoted { delim: b'"', multiline: true, escapes: true },
                Quoted { delim: b'\'', multiline: true, escapes: true },
            ],

            Target::Ruby => &[
                BlockComment { open: "=begin", close: "=end", nests: false }, // #219
                LineComment("#"),
                RubyHeredoc, // <<~EOS — #219
                RubyPercent, // %w[...] — #219
                Quoted { delim: b'"', multiline: true, escapes: true },
                Quoted { delim: b'\'', multiline: true, escapes: true },
            ],

            Target::Lua => &[
                LuaLongBracket, // [==[...]==] and --[==[...]==] — #219. MUST precede
                                // LineComment, since `--[[` is a long COMMENT.
                LineComment("--"),
                Quoted { delim: b'"', multiline: false, escapes: true },
                Quoted { delim: b'\'', multiline: false, escapes: true },
            ],
        };
        Literals { forms }
    }

    pub fn name(self) -> &'static str {
        match self {
            Target::Python3 => "python",
            Target::TypeScript => "typescript",
            Target::JavaScript => "javascript",
            Target::Rust => "rust",
            Target::C => "c",
            Target::Cpp => "cpp",
            Target::Java => "java",
            Target::CSharp => "csharp",
            Target::Go => "go",
            Target::Php => "php",
            Target::Kotlin => "kotlin",
            Target::Swift => "swift",
            Target::Ruby => "ruby",
            Target::Lua => "lua",
            Target::Dart => "dart",
            Target::GdScript => "gdscript",
        }
    }

    pub const ALL: &'static [Target] = &[
        Target::Python3,
        Target::TypeScript,
        Target::JavaScript,
        Target::Rust,
        Target::C,
        Target::Cpp,
        Target::Java,
        Target::CSharp,
        Target::Go,
        Target::Php,
        Target::Kotlin,
        Target::Swift,
        Target::Ruby,
        Target::Lua,
        Target::Dart,
        Target::GdScript,
    ];
}
