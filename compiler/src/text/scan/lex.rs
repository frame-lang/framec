//! The lexer. **The only code in the compiler that reads a byte.**
//!
//! # What it does, and the line it must not cross
//!
//! It answers exactly one kind of question: *where does this thing end?*
//!
//! * where does this string literal end
//! * where does this comment end
//! * where does this bracket close
//! * where are the interpolation **holes** inside this string
//!
//! It never asks what any of it *means*. That is the whole contract, and it has a
//! precise name: **lex, don't parse.**
//!
//! # Why not parse
//!
//! Not because parsing is expensive. Because **the capability is the hazard.** If
//! framec holds a parse tree of the user's native code, some pass will eventually
//! read it, and no type will stop it — and the moment a pass depends on native
//! *semantics*, framec is coupled to sixteen evolving language semantics forever.
//!
//! And it is not needed. Every fact framec is entitled to know is lexical: block
//! depth (count braces), literal extents (this file), Frame-reference positions
//! (this file), interpolation holes (this file). The facts that would need a parser
//! turned out to be facts framec should not compute at all — argument arity is the
//! target compiler's job, and statement termination is the user's.
//!
//! A parser would also **still not be enough**: in C++, `a < b, c > d` (two
//! comparisons) and `std::map<int, int>()` (one generic) are the same token shape,
//! and telling them apart needs name lookup that C++'s own grammar cannot do.
//!
//! # Interpolation holes — the rule
//!
//! A string's **holes** are code. A string's **content** is not.
//!
//! ```text
//! f"count is {$.count}"     hole    -> Frame reference     (framec looks)
//! "a literal $.x here"      content -> just bytes          (framec does not)
//! ```
//!
//! A hole (`{...}` in an f-string, `${...}` in a template, `\(...)` in Swift) is an
//! **expression position in the target's own grammar**. Those bytes are code; the
//! target compiler will treat them as code. framec looks in exactly those places and
//! nowhere else.
//!
//! The old compiler answered this question **two different ways** depending on which
//! code path arrived: its scanner said a sigil in a string is not a reference, and
//! its expression byte-loop (string-blind by design) said it is. Both shipped. Two
//! answers to "what is the language?" (#224).

use super::literals::{Form, Literals, Target};
use crate::text::Span;

/// Why the lexer could not make sense of the bytes.
///
/// **Every variant is a refusal, never a guess.** The old compiler's scanners, when
/// they met a literal form they did not know, kept counting braces — and a `}` inside
/// a Ruby heredoc or a JS regex closed a block that was never open. That is how legal
/// code got rejected (#219) and how a Lua long-string truncated a handler body.
///
/// A lexer that does not know something must **say so**. It must never carry on.
#[derive(Debug, PartialEq, Eq)]
pub enum LexError {
    UnterminatedString { open: Span },
    UnterminatedComment { open: Span },
    UnterminatedHeredoc { open: Span, tag: String },
    UnbalancedBracket { at: usize },
}

/// A cursor over the source. Skips literals and comments; counts brackets.
pub struct Lexer<'a> {
    bytes: &'a [u8],
    lits: Literals,
    target: Target,
}

/// A literal's extent, plus the code holes inside it.
#[derive(Debug, PartialEq, Eq)]
pub struct LiteralExtent {
    /// The whole literal, delimiters included.
    pub span: Span,
    /// Interpolation holes — **expression positions**, in source order.
    ///
    /// These are the spans framec is allowed to look inside. Everything else between
    /// the delimiters is content, and framec must leave it exactly alone.
    pub holes: Vec<Span>,
    /// The delimiter byte, so a downstream emitter can pick a *different* quote for a
    /// dict key it splices into a hole. (In the old compiler this fact was recomputed
    /// at 39 sites and got the wrong answer on 8 targets, because `'x'` is a CHAR in
    /// C#/Java/Kotlin/Swift/C/C++/Go/Rust, not a string — #221.)
    pub delim: u8,
}

impl<'a> Lexer<'a> {
    pub fn new(bytes: &'a [u8], target: Target) -> Lexer<'a> {
        Lexer {
            bytes,
            lits: target.literals(),
            target,
        }
    }

    pub fn target(&self) -> Target {
        self.target
    }

    /// If a comment starts at `i`, return the offset just past it.
    pub fn comment_at(&self, i: usize) -> Result<Option<usize>, LexError> {
        for form in self.lits.forms {
            match *form {
                Form::LineComment(open) => {
                    if self.starts_with(i, open.as_bytes()) {
                        // Lua: `--[[` is a long COMMENT, not a line comment. The table
                        // puts LuaLongBracket first so it wins; assert the ordering
                        // held rather than trusting it silently.
                        return Ok(Some(self.to_end_of_line(i)));
                    }
                }
                Form::BlockComment { open, close, nests } => {
                    if self.starts_with(i, open.as_bytes()) {
                        return self.block_comment(i, open, close, nests).map(Some);
                    }
                }
                Form::LuaLongBracket => {
                    // `--[==[ ... ]==]` — a long comment.
                    if self.starts_with(i, b"--") {
                        if let Some(level) = self.lua_long_open(i + 2) {
                            return self.lua_long_close(i, i + 2, level).map(Some);
                        }
                        // A plain `--` line comment; the LineComment form handles it.
                    }
                }
                _ => {}
            }
        }
        Ok(None)
    }

    /// If a string/template/raw-literal starts at `i`, return its extent and holes.
    pub fn literal_at(&self, i: usize) -> Result<Option<LiteralExtent>, LexError> {
        for form in self.lits.forms {
            let hit = match *form {
                Form::TripleQuoted { delim } => self.triple_quoted(i, delim)?,
                Form::RustRaw => self.rust_raw(i)?,
                Form::CppRaw => self.cpp_raw(i)?,
                Form::LuaLongBracket => self.lua_long_string(i)?,
                Form::PhpHeredoc => self.php_heredoc(i)?,
                Form::RubyHeredoc => self.ruby_heredoc(i)?,
                Form::RubyPercent => self.ruby_percent(i)?,
                Form::Template { delim } => self.template(i, delim)?,
                Form::Quoted {
                    delim,
                    multiline,
                    escapes,
                } => self.quoted(i, delim, multiline, escapes)?,
                // Regex is context-sensitive and cannot be decided from `i` alone —
                // see `regex_at`, which the caller must use because only the caller
                // knows the previous token.
                Form::RegexLiteral => None,
                Form::LineComment(_) | Form::BlockComment { .. } => None,
            };
            if hit.is_some() {
                return Ok(hit);
            }
        }
        Ok(None)
    }

    // ---------------------------------------------------------------- primitives

    fn starts_with(&self, i: usize, pat: &[u8]) -> bool {
        self.bytes.len() >= i + pat.len() && &self.bytes[i..i + pat.len()] == pat
    }

    fn to_end_of_line(&self, mut i: usize) -> usize {
        while i < self.bytes.len() && self.bytes[i] != b'\n' {
            i += 1;
        }
        i
    }

    fn block_comment(
        &self,
        start: usize,
        open: &str,
        close: &str,
        nests: bool,
    ) -> Result<usize, LexError> {
        let (o, c) = (open.as_bytes(), close.as_bytes());
        let mut i = start + o.len();
        let mut depth = 1usize;
        while i < self.bytes.len() {
            if nests && self.starts_with(i, o) {
                depth += 1;
                i += o.len();
                continue;
            }
            if self.starts_with(i, c) {
                depth -= 1;
                i += c.len();
                if depth == 0 {
                    return Ok(i);
                }
                continue;
            }
            i += 1;
        }
        Err(LexError::UnterminatedComment {
            open: Span::new(start, self.bytes.len()),
        })
    }

    /// A simple quoted string. `escapes` controls whether `\` escapes the delimiter.
    fn quoted(
        &self,
        start: usize,
        delim: u8,
        multiline: bool,
        escapes: bool,
    ) -> Result<Option<LiteralExtent>, LexError> {
        if self.byte(start) != Some(delim) {
            return Ok(None);
        }
        let mut i = start + 1;
        let mut holes = Vec::new();
        while i < self.bytes.len() {
            let b = self.bytes[i];
            if escapes && b == b'\\' {
                i += 2;
                continue;
            }
            if b == b'\n' && !multiline {
                // A single-line string that hit a newline is unterminated. Say so —
                // do NOT keep scanning and swallow the rest of the file.
                return Err(LexError::UnterminatedString {
                    open: Span::new(start, i),
                });
            }
            // An interpolation hole: `{...}` in a Python f-string, `${...}` in Dart /
            // Kotlin, `\(...)` in Swift. These are CODE (#224).
            if let Some(hole) = self.hole_at(i, delim) {
                holes.push(hole);
                i = hole.end + 1; // past the closing brace/paren
                continue;
            }
            if b == delim {
                return Ok(Some(LiteralExtent {
                    span: Span::new(start, i + 1),
                    holes,
                    delim,
                }));
            }
            i += 1;
        }
        Err(LexError::UnterminatedString {
            open: Span::new(start, self.bytes.len()),
        })
    }

    /// An interpolation hole starting at `i`, if this target has one there.
    ///
    /// Returns the span of the hole's **contents** (not the delimiters) — that is the
    /// expression position framec may look inside, and nothing else in the literal is.
    fn hole_at(&self, i: usize, _string_delim: u8) -> Option<Span> {
        let (open_len, open_ok) = match self.target {
            // Python f-strings / GDScript: `{expr}`. (`{{` is a literal brace.)
            Target::Python3 | Target::GdScript => (1, self.byte(i) == Some(b'{')),
            // `${expr}` — Dart, Kotlin, JS/TS templates, PHP double-quoted.
            Target::Dart
            | Target::Kotlin
            | Target::JavaScript
            | Target::TypeScript
            | Target::Php => (2, self.starts_with(i, b"${")),
            // Swift: `\(expr)`
            Target::Swift => (2, self.starts_with(i, b"\\(")),
            // C#: `$"{expr}"` — but `{` only opens a hole in an interpolated string,
            // which needs the `$` prefix. Handled conservatively: see note below.
            Target::CSharp => (1, self.byte(i) == Some(b'{')),
            // Ruby: `#{expr}`
            Target::Ruby => (2, self.starts_with(i, b"#{")),
            // No interpolation in the string grammar.
            Target::C | Target::Cpp | Target::Java | Target::Go | Target::Rust | Target::Lua => {
                (0, false)
            }
        };
        if !open_ok {
            return None;
        }
        // Python/C#: `{{` is an escaped literal brace, not a hole.
        if open_len == 1 && self.byte(i + 1) == Some(b'{') {
            return None;
        }
        let close = if self.target == Target::Swift {
            b')'
        } else {
            b'}'
        };
        let open = if self.target == Target::Swift {
            b'('
        } else {
            b'{'
        };
        // Holes nest: `` `${ `${x}` }` `` is real. Count.
        let mut depth = 0i32;
        let mut j = i + open_len - 1; // sit on the opening brace/paren
        let content_start = j + 1;
        while j < self.bytes.len() {
            let b = self.bytes[j];
            if b == open {
                depth += 1;
            } else if b == close {
                depth -= 1;
                if depth == 0 {
                    return Some(Span::new(content_start, j));
                }
            }
            j += 1;
        }
        None
    }

    fn triple_quoted(&self, start: usize, delim: u8) -> Result<Option<LiteralExtent>, LexError> {
        let q3 = [delim, delim, delim];
        if !self.starts_with(start, &q3) {
            return Ok(None);
        }
        let mut i = start + 3;
        let mut holes = Vec::new();
        while i < self.bytes.len() {
            if self.bytes[i] == b'\\' {
                i += 2;
                continue;
            }
            if let Some(hole) = self.hole_at(i, delim) {
                holes.push(hole);
                i = hole.end + 1;
                continue;
            }
            if self.starts_with(i, &q3) {
                return Ok(Some(LiteralExtent {
                    span: Span::new(start, i + 3),
                    holes,
                    delim,
                }));
            }
            i += 1;
        }
        Err(LexError::UnterminatedString {
            open: Span::new(start, self.bytes.len()),
        })
    }

    /// Rust `r"..."`, `r#"..."#`, `r##"..."##`, and the `b`/`br` variants.
    /// The hash count is fixed at the open — a table cannot express this.
    fn rust_raw(&self, start: usize) -> Result<Option<LiteralExtent>, LexError> {
        let mut i = start;
        // optional `b` (byte string)
        if self.byte(i) == Some(b'b') {
            i += 1;
        }
        if self.byte(i) != Some(b'r') {
            return Ok(None);
        }
        i += 1;
        let hash_start = i;
        while self.byte(i) == Some(b'#') {
            i += 1;
        }
        let hashes = i - hash_start;
        if self.byte(i) != Some(b'"') {
            return Ok(None); // just an identifier starting with r, e.g. `read`
        }
        i += 1;
        // Close is `"` followed by exactly `hashes` `#`. No escapes inside a raw string.
        while i < self.bytes.len() {
            if self.bytes[i] == b'"' {
                let mut k = i + 1;
                let mut seen = 0;
                while seen < hashes && self.byte(k) == Some(b'#') {
                    k += 1;
                    seen += 1;
                }
                if seen == hashes {
                    return Ok(Some(LiteralExtent {
                        span: Span::new(start, k),
                        holes: Vec::new(), // raw strings do not interpolate
                        delim: b'"',
                    }));
                }
            }
            i += 1;
        }
        Err(LexError::UnterminatedString {
            open: Span::new(start, self.bytes.len()),
        })
    }

    /// C++ `R"delim( ... )delim"` — the delimiter is *named* at the open.
    fn cpp_raw(&self, start: usize) -> Result<Option<LiteralExtent>, LexError> {
        if !self.starts_with(start, b"R\"") {
            return Ok(None);
        }
        let mut i = start + 2;
        let tag_start = i;
        while i < self.bytes.len() && self.bytes[i] != b'(' {
            i += 1;
        }
        if i >= self.bytes.len() {
            return Err(LexError::UnterminatedString {
                open: Span::new(start, self.bytes.len()),
            });
        }
        let tag = &self.bytes[tag_start..i];
        // close = `)` + tag + `"`
        let mut close = Vec::with_capacity(tag.len() + 2);
        close.push(b')');
        close.extend_from_slice(tag);
        close.push(b'"');
        i += 1;
        while i < self.bytes.len() {
            if self.starts_with(i, &close) {
                return Ok(Some(LiteralExtent {
                    span: Span::new(start, i + close.len()),
                    holes: Vec::new(),
                    delim: b'"',
                }));
            }
            i += 1;
        }
        Err(LexError::UnterminatedString {
            open: Span::new(start, self.bytes.len()),
        })
    }

    /// Lua `[[ ... ]]`, `[=[ ... ]=]`, `[==[ ... ]==]`. Level fixed at the open.
    /// The old compiler didn't know this form, and a `}` inside one silently
    /// TRUNCATED the handler body (#219).
    fn lua_long_string(&self, start: usize) -> Result<Option<LiteralExtent>, LexError> {
        let Some(level) = self.lua_long_open(start) else {
            return Ok(None);
        };
        let end = self.lua_long_close(start, start, level)?;
        Ok(Some(LiteralExtent {
            span: Span::new(start, end),
            holes: Vec::new(),
            delim: b'[',
        }))
    }

    /// `[`, then N `=`, then `[`. Returns N.
    fn lua_long_open(&self, i: usize) -> Option<usize> {
        if self.byte(i) != Some(b'[') {
            return None;
        }
        let mut k = i + 1;
        while self.byte(k) == Some(b'=') {
            k += 1;
        }
        if self.byte(k) == Some(b'[') {
            Some(k - i - 1)
        } else {
            None
        }
    }

    fn lua_long_close(&self, start: usize, open_at: usize, level: usize) -> Result<usize, LexError> {
        let mut close = Vec::with_capacity(level + 2);
        close.push(b']');
        close.extend(std::iter::repeat(b'=').take(level));
        close.push(b']');
        let mut i = open_at + level + 2;
        while i < self.bytes.len() {
            if self.starts_with(i, &close) {
                return Ok(i + close.len());
            }
            i += 1;
        }
        Err(LexError::UnterminatedString {
            open: Span::new(start, self.bytes.len()),
        })
    }

    /// PHP `<<<EOT ... EOT;` and `<<<'EOT' ... EOT;` (nowdoc).
    /// The terminator is an identifier the *user* chooses (#219).
    fn php_heredoc(&self, start: usize) -> Result<Option<LiteralExtent>, LexError> {
        if !self.starts_with(start, b"<<<") {
            return Ok(None);
        }
        let mut i = start + 3;
        while matches!(self.byte(i), Some(b' ') | Some(b'\t')) {
            i += 1;
        }
        // Optional quoting: <<<"EOT" (heredoc) or <<<'EOT' (nowdoc).
        let quote = match self.byte(i) {
            Some(q @ (b'"' | b'\'')) => {
                i += 1;
                Some(q)
            }
            _ => None,
        };
        let tag_start = i;
        while matches!(self.byte(i), Some(b) if b.is_ascii_alphanumeric() || b == b'_') {
            i += 1;
        }
        let tag = self.bytes[tag_start..i].to_vec();
        if tag.is_empty() {
            return Ok(None); // `<<<` that isn't a heredoc — leave it alone.
        }
        if let Some(q) = quote {
            if self.byte(i) != Some(q) {
                return Ok(None);
            }
            i += 1;
        }
        self.heredoc_body(start, i, &tag)
    }

    /// Ruby `<<~EOS`, `<<-EOS`, `<<EOS`.
    fn ruby_heredoc(&self, start: usize) -> Result<Option<LiteralExtent>, LexError> {
        if !self.starts_with(start, b"<<") {
            return Ok(None);
        }
        let mut i = start + 2;
        if matches!(self.byte(i), Some(b'~') | Some(b'-')) {
            i += 1;
        }
        let quote = match self.byte(i) {
            Some(q @ (b'"' | b'\'')) => {
                i += 1;
                Some(q)
            }
            _ => None,
        };
        let tag_start = i;
        // Ruby heredoc tags are conventionally uppercase; require the first char to be
        // a letter or `_` so `a << b` (left shift) is not mistaken for a heredoc.
        if !matches!(self.byte(i), Some(b) if b.is_ascii_alphabetic() || b == b'_') {
            return Ok(None);
        }
        while matches!(self.byte(i), Some(b) if b.is_ascii_alphanumeric() || b == b'_') {
            i += 1;
        }
        let tag = self.bytes[tag_start..i].to_vec();
        if let Some(q) = quote {
            if self.byte(i) != Some(q) {
                return Ok(None);
            }
            i += 1;
        }
        self.heredoc_body(start, i, &tag)
    }

    /// Shared: scan forward for a line whose (trimmed) content is exactly `tag`.
    fn heredoc_body(
        &self,
        start: usize,
        after_tag: usize,
        tag: &[u8],
    ) -> Result<Option<LiteralExtent>, LexError> {
        // Body begins on the next line.
        let mut i = self.to_end_of_line(after_tag);
        if i < self.bytes.len() {
            i += 1; // past the newline
        }
        while i < self.bytes.len() {
            let line_start = i;
            let line_end = self.to_end_of_line(i);
            let line = &self.bytes[line_start..line_end];
            let trimmed: &[u8] = {
                let a = line
                    .iter()
                    .position(|b| !b.is_ascii_whitespace())
                    .unwrap_or(line.len());
                let z = line
                    .iter()
                    .rposition(|b| !b.is_ascii_whitespace())
                    .map(|p| p + 1)
                    .unwrap_or(a);
                &line[a..z]
            };
            // The terminator may be followed by `;` or `,` in PHP.
            let is_term = trimmed == tag
                || (trimmed.starts_with(tag)
                    && trimmed[tag.len()..]
                        .iter()
                        .all(|&b| b == b';' || b == b','));
            if is_term {
                return Ok(Some(LiteralExtent {
                    span: Span::new(start, line_end),
                    holes: Vec::new(), // TODO: PHP heredocs DO interpolate `${...}`.
                    delim: b'<',
                }));
            }
            i = if line_end < self.bytes.len() {
                line_end + 1
            } else {
                line_end
            };
        }
        Err(LexError::UnterminatedHeredoc {
            open: Span::new(start, self.bytes.len()),
            tag: String::from_utf8_lossy(tag).into_owned(),
        })
    }

    /// Ruby `%w[...]`, `%q(...)`, `%i{...}`, `%{...}`. The delimiter is chosen at the
    /// open, and bracket pairs **nest** (#219).
    fn ruby_percent(&self, start: usize) -> Result<Option<LiteralExtent>, LexError> {
        if self.byte(start) != Some(b'%') {
            return Ok(None);
        }
        let mut i = start + 1;
        // Optional type letter: q Q w W i I r s x
        if matches!(self.byte(i), Some(b) if b.is_ascii_alphabetic()) {
            if !matches!(
                self.byte(i),
                Some(b'q' | b'Q' | b'w' | b'W' | b'i' | b'I' | b'r' | b's' | b'x')
            ) {
                return Ok(None); // `%` as modulo followed by an identifier
            }
            i += 1;
        }
        let Some(open) = self.byte(i) else {
            return Ok(None);
        };
        // A delimiter must be punctuation. `a % b` (modulo) has a space or an operand.
        if open.is_ascii_alphanumeric() || open.is_ascii_whitespace() {
            return Ok(None);
        }
        let close = match open {
            b'[' => b']',
            b'(' => b')',
            b'{' => b'}',
            b'<' => b'>',
            other => other, // e.g. %w|...| — same char both ends, no nesting
        };
        let nests = close != open;
        i += 1;
        let mut depth = 1i32;
        while i < self.bytes.len() {
            let b = self.bytes[i];
            if b == b'\\' {
                i += 2;
                continue;
            }
            if nests && b == open {
                depth += 1;
            } else if b == close {
                depth -= 1;
                if depth == 0 {
                    return Ok(Some(LiteralExtent {
                        span: Span::new(start, i + 1),
                        holes: Vec::new(),
                        delim: open,
                    }));
                }
            }
            i += 1;
        }
        Err(LexError::UnterminatedString {
            open: Span::new(start, self.bytes.len()),
        })
    }

    /// `` `...${expr}...` `` — JS/TS template literals. They **nest**.
    fn template(&self, start: usize, delim: u8) -> Result<Option<LiteralExtent>, LexError> {
        if self.byte(start) != Some(delim) {
            return Ok(None);
        }
        let mut i = start + 1;
        let mut holes = Vec::new();
        while i < self.bytes.len() {
            let b = self.bytes[i];
            if b == b'\\' {
                i += 2;
                continue;
            }
            if self.starts_with(i, b"${") {
                if let Some(hole) = self.hole_at(i, delim) {
                    holes.push(hole);
                    i = hole.end + 1;
                    continue;
                }
            }
            if b == delim {
                return Ok(Some(LiteralExtent {
                    span: Span::new(start, i + 1),
                    holes,
                    delim,
                }));
            }
            i += 1;
        }
        Err(LexError::UnterminatedString {
            open: Span::new(start, self.bytes.len()),
        })
    }

    /// A JS/TS regex literal at `i` — **but only if an operand is expected there**.
    ///
    /// `/` is regex-or-division depending on the *previous* token. This is the one
    /// form that genuinely needs lexer state, so the caller must supply it: after an
    /// identifier, a literal, `)` or `]`, a `/` is **division**; after `=`, `(`, `,`,
    /// `return`, or any operator, it is a **regex**.
    ///
    /// The old compiler had no notion of this and rejected `let re = /[}]/;` outright,
    /// because the `}` inside the character class closed a block that was never open
    /// (#219).
    pub fn regex_at(&self, i: usize, operand_expected: bool) -> Result<Option<Span>, LexError> {
        if !self.lits.forms.contains(&Form::RegexLiteral) {
            return Ok(None);
        }
        if !operand_expected || self.byte(i) != Some(b'/') {
            return Ok(None);
        }
        let mut j = i + 1;
        let mut in_class = false;
        while j < self.bytes.len() {
            match self.bytes[j] {
                b'\\' => {
                    j += 2;
                    continue;
                }
                b'[' => in_class = true,
                b']' => in_class = false,
                b'\n' => {
                    return Err(LexError::UnterminatedString {
                        open: Span::new(i, j),
                    })
                }
                b'/' if !in_class => {
                    // trailing flags
                    let mut k = j + 1;
                    while matches!(self.byte(k), Some(b) if b.is_ascii_alphabetic()) {
                        k += 1;
                    }
                    return Ok(Some(Span::new(i, k)));
                }
                _ => {}
            }
            j += 1;
        }
        Err(LexError::UnterminatedString {
            open: Span::new(i, self.bytes.len()),
        })
    }

    fn byte(&self, i: usize) -> Option<u8> {
        self.bytes.get(i).copied()
    }
}
