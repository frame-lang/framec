//! Frame regex syntax → AST.
//!
//! Parses the body of a `/.../` regex literal into a [`RegexAst`]. The
//! parser is alphabet-aware so it can produce correct
//! [`Literal`](super::ast::Literal) variants. Forbidden constructs are
//! parsed structurally into [`ForbiddenConstruct`](super::ast::ForbiddenConstruct)
//! variants; rejection happens in
//! [`super::restrictions::check`].
//!
//! v0.1 implementation: hand-written recursive-descent over the grammar
//!
//! ```text
//!   alt        := concat ('|' concat)*
//!   concat     := quantified*
//!   quantified := atom quantifier?
//!   atom       := group | class | '.' | anchor | escape | literal
//!   quantifier := ('?' | '*' | '+' | '{' bound '}') '?'?
//! ```
//!
//! The parser never rejects a *forbidden-but-recognizable* construct
//! (backrefs, lookaround, `\p{}`, named captures, lazy quantifiers): it
//! builds the corresponding [`RegexNode`]/[`Laziness`] so
//! [`super::restrictions::check`] can report it with full span context.
//! Only genuinely malformed input (unbalanced `()`/`[]`, bad `{}`, bad
//! escapes) yields a [`ParseError`].

use super::ast::{
    Anchor, CharClass, ClassMember, ForbiddenConstruct, Laziness, Literal, QuantifierKind,
    RegexAst, RegexNode, ShorthandKind, SpannedNode,
};
use super::{Alphabet, Span};

/// Parse a regex literal body.
///
/// `source` is the text between the delimiting `/` characters (without
/// the slashes). `alphabet` controls how bare literals are interpreted.
///
/// Returns a complete AST including any forbidden constructs. Use
/// [`super::restrictions::check`] to validate.
pub fn parse(source: &str, alphabet: Alphabet) -> Result<RegexAst, ParseError> {
    if source.is_empty() {
        return Ok(RegexAst {
            root: SpannedNode {
                node: RegexNode::Empty,
                span: Span::new(0, 0),
            },
        });
    }
    let mut p = Parser::new(source, alphabet);
    let root = p.parse_alt()?;
    if !p.at_end() {
        // A stray `)` or `]` with no opener lands here.
        let c = p.peek().unwrap_or('?');
        return Err(p.error(ParseErrorKind::Unexpected(c)));
    }
    Ok(RegexAst { root })
}

/// Recursive-descent cursor over the regex body.
struct Parser {
    /// `(byte_offset, char)` for each source char, for span tracking.
    chars: Vec<(usize, char)>,
    /// Total byte length of the source (the span end past the last char).
    end: usize,
    /// Index into `chars`.
    idx: usize,
    alphabet: Alphabet,
}

impl Parser {
    fn new(source: &str, alphabet: Alphabet) -> Self {
        Self {
            chars: source.char_indices().collect(),
            end: source.len(),
            idx: 0,
            alphabet,
        }
    }

    // --- cursor primitives ---

    fn at_end(&self) -> bool {
        self.idx >= self.chars.len()
    }

    fn peek(&self) -> Option<char> {
        self.chars.get(self.idx).map(|&(_, c)| c)
    }

    fn peek2(&self) -> Option<char> {
        self.chars.get(self.idx + 1).map(|&(_, c)| c)
    }

    /// Byte offset of the current char (or `end` at EOF).
    fn pos(&self) -> usize {
        self.chars
            .get(self.idx)
            .map(|&(o, _)| o)
            .unwrap_or(self.end)
    }

    fn bump(&mut self) -> Option<char> {
        let c = self.peek();
        if c.is_some() {
            self.idx += 1;
        }
        c
    }

    /// Consume `c` if it is next; report whether it was.
    fn eat(&mut self, c: char) -> bool {
        if self.peek() == Some(c) {
            self.idx += 1;
            true
        } else {
            false
        }
    }

    fn error(&self, kind: ParseErrorKind) -> ParseError {
        let at = self.pos();
        ParseError {
            kind,
            span: Span::new(at, (at + 1).min(self.end.max(at + 1))),
        }
    }

    fn spanned(node: RegexNode, start: usize, end: usize) -> SpannedNode {
        SpannedNode {
            node,
            span: Span::new(start, end),
        }
    }

    // --- grammar ---

    /// In the token alphabet, whitespace separates token-kind references
    /// and is otherwise insignificant; in byte/char alphabets a space is a
    /// literal element, so this is a no-op there.
    fn skip_ws(&mut self) {
        if self.alphabet == Alphabet::Token {
            while matches!(
                self.peek(),
                Some(' ') | Some('\t') | Some('\n') | Some('\r')
            ) {
                self.bump();
            }
        }
    }

    /// `alt := concat ('|' concat)*`
    fn parse_alt(&mut self) -> Result<SpannedNode, ParseError> {
        let start = self.pos();
        let mut branches = vec![self.parse_concat()?];
        loop {
            self.skip_ws();
            if !self.eat('|') {
                break;
            }
            branches.push(self.parse_concat()?);
        }
        if branches.len() == 1 {
            Ok(branches.pop().unwrap())
        } else {
            let end = branches.last().map(|b| b.span.end).unwrap_or(start);
            Ok(Self::spanned(RegexNode::Alt(branches), start, end))
        }
    }

    /// `concat := quantified*` — terminated by `|`, `)`, or EOF.
    fn parse_concat(&mut self) -> Result<SpannedNode, ParseError> {
        let start = self.pos();
        let mut items = Vec::new();
        loop {
            self.skip_ws();
            match self.peek() {
                None | Some('|') | Some(')') => break,
                _ => {}
            }
            items.push(self.parse_quantified()?);
        }
        match items.len() {
            // An empty branch (`a|`, `(|a)`) is the empty regex.
            0 => Ok(Self::spanned(
                RegexNode::Concat(Vec::new()),
                start,
                self.pos(),
            )),
            1 => Ok(items.pop().unwrap()),
            _ => {
                let end = items.last().map(|i| i.span.end).unwrap_or(start);
                Ok(Self::spanned(RegexNode::Concat(items), start, end))
            }
        }
    }

    /// `quantified := atom quantifier?`
    fn parse_quantified(&mut self) -> Result<SpannedNode, ParseError> {
        let start = self.pos();
        let atom = self.parse_atom()?;
        let kind = match self.peek() {
            Some('?') => {
                self.bump();
                Some(QuantifierKind::ZeroOrOne)
            }
            Some('*') => {
                self.bump();
                Some(QuantifierKind::ZeroOrMore)
            }
            Some('+') => {
                self.bump();
                Some(QuantifierKind::OneOrMore)
            }
            Some('{') if self.looks_like_bound() => Some(self.parse_bound()?),
            _ => None,
        };
        match kind {
            None => Ok(atom),
            Some(kind) => {
                // A trailing `?` makes the quantifier lazy (v0.1 rejects it
                // in restrictions, but we record it precisely).
                let laziness = if self.eat('?') {
                    Laziness::Lazy
                } else {
                    Laziness::Greedy
                };
                let end = self.pos();
                Ok(Self::spanned(
                    RegexNode::Quantifier {
                        inner: Box::new(atom),
                        kind,
                        laziness,
                    },
                    start,
                    end,
                ))
            }
        }
    }

    /// Is the upcoming `{` a repetition bound (`{n}`, `{n,m}`, `{n,}`) and
    /// not a literal brace? A `{` not followed by a digit is a literal.
    fn looks_like_bound(&self) -> bool {
        matches!(self.peek2(), Some(d) if d.is_ascii_digit())
    }

    /// `{n}` | `{n,}` | `{n,m}` — the opening `{` is current.
    fn parse_bound(&mut self) -> Result<QuantifierKind, ParseError> {
        let open = self.pos();
        self.bump(); // `{`
        let min = self.parse_number().ok_or_else(|| ParseError {
            kind: ParseErrorKind::MalformedQuantifier,
            span: Span::new(open, self.pos()),
        })?;
        if self.eat('}') {
            return Ok(QuantifierKind::Exact(min));
        }
        if !self.eat(',') {
            return Err(ParseError {
                kind: ParseErrorKind::MalformedQuantifier,
                span: Span::new(open, self.pos()),
            });
        }
        // `{n,}` or `{n,m}`
        if self.eat('}') {
            return Ok(QuantifierKind::AtLeast(min));
        }
        let max = self.parse_number().ok_or_else(|| ParseError {
            kind: ParseErrorKind::MalformedQuantifier,
            span: Span::new(open, self.pos()),
        })?;
        if !self.eat('}') {
            return Err(ParseError {
                kind: ParseErrorKind::MalformedQuantifier,
                span: Span::new(open, self.pos()),
            });
        }
        Ok(QuantifierKind::Bounded { min, max })
    }

    fn parse_number(&mut self) -> Option<u32> {
        let mut digits = String::new();
        while let Some(c) = self.peek() {
            if c.is_ascii_digit() {
                digits.push(c);
                self.bump();
            } else {
                break;
            }
        }
        digits.parse().ok()
    }

    /// `atom := group | class | '.' | anchor | escape | literal`
    fn parse_atom(&mut self) -> Result<SpannedNode, ParseError> {
        let start = self.pos();
        let c = self.peek().expect("parse_atom called at EOF");
        match c {
            '(' => self.parse_group(),
            '[' => self.parse_class(),
            '.' => {
                self.bump();
                Ok(Self::spanned(RegexNode::Dot, start, self.pos()))
            }
            '^' => {
                self.bump();
                Ok(Self::spanned(
                    RegexNode::Anchor(Anchor::LineStart),
                    start,
                    self.pos(),
                ))
            }
            '$' => {
                self.bump();
                Ok(Self::spanned(
                    RegexNode::Anchor(Anchor::LineEnd),
                    start,
                    self.pos(),
                ))
            }
            '\\' => self.parse_escape(),
            '*' | '+' | '?' => {
                // A quantifier with nothing to quantify.
                Err(self.error(ParseErrorKind::Unexpected(c)))
            }
            // Token alphabet: an identifier run is one token-kind reference
            // (`/IDENT LPAREN/` → two Token literals, not 11 char literals).
            _ if self.alphabet == Alphabet::Token && (c.is_ascii_alphabetic() || c == '_') => {
                let mut name = String::new();
                while let Some(ch) = self.peek() {
                    if ch.is_ascii_alphanumeric() || ch == '_' {
                        name.push(ch);
                        self.bump();
                    } else {
                        break;
                    }
                }
                Ok(Self::spanned(
                    RegexNode::Literal(Literal::Token(name)),
                    start,
                    self.pos(),
                ))
            }
            _ => {
                self.bump();
                Ok(Self::spanned(
                    RegexNode::Literal(self.make_literal(c)),
                    start,
                    self.pos(),
                ))
            }
        }
    }

    /// `(` ... — a plain group, or a `(?...)` special form (all of which
    /// are forbidden in v0.1, captured structurally for diagnostics).
    fn parse_group(&mut self) -> Result<SpannedNode, ParseError> {
        let start = self.pos();
        self.bump(); // `(`
        if self.peek() == Some('?') {
            return self.parse_special_group(start);
        }
        let inner = self.parse_alt()?;
        if !self.eat(')') {
            return Err(ParseError {
                kind: ParseErrorKind::UnclosedGroup,
                span: Span::new(start, self.pos()),
            });
        }
        Ok(Self::spanned(
            RegexNode::Group(Box::new(inner)),
            start,
            self.pos(),
        ))
    }

    /// `(?...)` forms — lookaround, named capture/backref, recursion,
    /// non-capturing group. The leading `(` is consumed; `?` is current.
    fn parse_special_group(&mut self, start: usize) -> Result<SpannedNode, ParseError> {
        self.bump(); // `?`
        let forbidden = match self.peek() {
            Some(':') => {
                self.bump();
                let inner = self.parse_alt()?;
                self.expect_group_close(start)?;
                ForbiddenConstruct::NonCapturingGroup(Box::new(inner))
            }
            Some('=') => {
                self.bump();
                let inner = self.parse_alt()?;
                self.expect_group_close(start)?;
                ForbiddenConstruct::PositiveLookahead(Box::new(inner))
            }
            Some('!') => {
                self.bump();
                let inner = self.parse_alt()?;
                self.expect_group_close(start)?;
                ForbiddenConstruct::NegativeLookahead(Box::new(inner))
            }
            Some('<') if matches!(self.peek2(), Some('=') | Some('!')) => {
                self.bump(); // `<`
                let neg = self.bump() == Some('!');
                let inner = self.parse_alt()?;
                self.expect_group_close(start)?;
                if neg {
                    ForbiddenConstruct::NegativeLookbehind(Box::new(inner))
                } else {
                    ForbiddenConstruct::PositiveLookbehind(Box::new(inner))
                }
            }
            // `(?P<name>...)` named capture or `(?<name>...)`.
            Some('P') | Some('<') => {
                if self.peek() == Some('P') {
                    self.bump(); // `P`
                    if self.peek() == Some('=') {
                        // `(?P=name)` named backreference.
                        self.bump();
                        let name = self.read_name_until(')');
                        self.expect_group_close(start)?;
                        ForbiddenConstruct::NamedBackref(name)
                    } else {
                        // `(?P<name>...)`
                        self.eat('<');
                        let name = self.read_name_until('>');
                        self.eat('>');
                        let inner = self.parse_alt()?;
                        self.expect_group_close(start)?;
                        ForbiddenConstruct::NamedCapture {
                            name,
                            inner: Box::new(inner),
                        }
                    }
                } else {
                    // `(?<name>...)`
                    self.bump(); // `<`
                    let name = self.read_name_until('>');
                    self.eat('>');
                    let inner = self.parse_alt()?;
                    self.expect_group_close(start)?;
                    ForbiddenConstruct::NamedCapture {
                        name,
                        inner: Box::new(inner),
                    }
                }
            }
            // `(?R)` / `(?0)` / `(?-1)` recursion.
            Some('R') | Some('0') | Some('-') => {
                while !matches!(self.peek(), Some(')') | None) {
                    self.bump();
                }
                self.expect_group_close(start)?;
                ForbiddenConstruct::Recursion
            }
            _ => {
                // Inline flags `(?i)` etc. — unsupported; treat the rest as
                // an unrecognized special group (recover by skipping to `)`).
                while !matches!(self.peek(), Some(')') | None) {
                    self.bump();
                }
                self.expect_group_close(start)?;
                ForbiddenConstruct::Recursion
            }
        };
        Ok(Self::spanned(
            RegexNode::Forbidden(forbidden),
            start,
            self.pos(),
        ))
    }

    fn expect_group_close(&mut self, start: usize) -> Result<(), ParseError> {
        if self.eat(')') {
            Ok(())
        } else {
            Err(ParseError {
                kind: ParseErrorKind::UnclosedGroup,
                span: Span::new(start, self.pos()),
            })
        }
    }

    fn read_name_until(&mut self, term: char) -> String {
        let mut name = String::new();
        while let Some(c) = self.peek() {
            if c == term || c == ')' {
                break;
            }
            name.push(c);
            self.bump();
        }
        name
    }

    /// `[` ... `]` — a character class. The `[` is current.
    fn parse_class(&mut self) -> Result<SpannedNode, ParseError> {
        let start = self.pos();
        self.bump(); // `[`
        let negated = self.eat('^');
        let mut members = Vec::new();
        loop {
            match self.peek() {
                None => {
                    return Err(ParseError {
                        kind: ParseErrorKind::UnclosedClass,
                        span: Span::new(start, self.pos()),
                    });
                }
                Some(']') => {
                    self.bump();
                    break;
                }
                _ => {}
            }
            members.push(self.parse_class_member()?);
        }
        if members.is_empty() {
            return Err(ParseError {
                kind: ParseErrorKind::EmptyClass,
                span: Span::new(start, self.pos()),
            });
        }
        Ok(Self::spanned(
            RegexNode::Class(CharClass { negated, members }),
            start,
            self.pos(),
        ))
    }

    fn parse_class_member(&mut self) -> Result<ClassMember, ParseError> {
        // A shorthand (`\d`) or escaped scalar.
        if self.peek() == Some('\\') {
            return self.parse_class_escape();
        }
        let low = self.bump().expect("checked non-empty") as u32;
        // A range `a-z`, unless `-` is the last char before `]`.
        if self.peek() == Some('-') && self.peek2() != Some(']') && self.peek2().is_some() {
            self.bump(); // `-`
            let high = if self.peek() == Some('\\') {
                match self.parse_class_escape()? {
                    ClassMember::Single(v) => v,
                    // `[a-\d]` is malformed; treat the shorthand as the high
                    // bound's failure.
                    _ => {
                        return Err(ParseError {
                            kind: ParseErrorKind::MalformedQuantifier,
                            span: Span::new(self.pos(), self.pos()),
                        })
                    }
                }
            } else {
                self.bump().expect("range high present") as u32
            };
            return Ok(ClassMember::Range { low, high });
        }
        Ok(ClassMember::Single(low))
    }

    /// `\X` inside a class — a shorthand or an escaped scalar value.
    fn parse_class_escape(&mut self) -> Result<ClassMember, ParseError> {
        self.bump(); // `\`
        let c = self
            .bump()
            .ok_or_else(|| self.error(ParseErrorKind::UnknownEscape(String::new())))?;
        if let Some((kind, negated)) = shorthand(c) {
            return Ok(ClassMember::Shorthand { kind, negated });
        }
        let v = self.escaped_scalar(c)?;
        Ok(ClassMember::Single(v))
    }

    /// Parse an escape outside a class: anchors, shorthands (as a class),
    /// backrefs, `\p{}`, `\xNN`, `\u{...}`, control chars, escaped punct.
    fn parse_escape(&mut self) -> Result<SpannedNode, ParseError> {
        let start = self.pos();
        self.bump(); // `\`
        let c = self
            .bump()
            .ok_or_else(|| self.error(ParseErrorKind::UnknownEscape(String::new())))?;

        // Zero-width anchors.
        let anchor = match c {
            'b' => Some(Anchor::WordBoundary),
            'B' => Some(Anchor::NonWordBoundary),
            'A' => Some(Anchor::InputStart),
            'z' => Some(Anchor::InputEnd),
            _ => None,
        };
        if let Some(a) = anchor {
            return Ok(Self::spanned(RegexNode::Anchor(a), start, self.pos()));
        }

        // `\d` / `\w` / `\s` (and negated forms) — a single-shorthand class.
        if let Some((kind, negated)) = shorthand(c) {
            return Ok(Self::spanned(
                RegexNode::Class(CharClass {
                    negated: false,
                    members: vec![ClassMember::Shorthand { kind, negated }],
                }),
                start,
                self.pos(),
            ));
        }

        // `\1`..`\9` — backreference (forbidden).
        if c.is_ascii_digit() && c != '0' {
            let mut n = c.to_digit(10).unwrap();
            while let Some(d) = self.peek() {
                if d.is_ascii_digit() {
                    n = n * 10 + d.to_digit(10).unwrap();
                    self.bump();
                } else {
                    break;
                }
            }
            return Ok(Self::spanned(
                RegexNode::Forbidden(ForbiddenConstruct::Backref(n)),
                start,
                self.pos(),
            ));
        }

        // `\p{...}` / `\P{...}` — Unicode class (forbidden).
        if c == 'p' || c == 'P' {
            let mut body = String::new();
            if self.eat('{') {
                body = self.read_name_until('}');
                self.eat('}');
            } else if let Some(ch) = self.bump() {
                body.push(ch);
            }
            return Ok(Self::spanned(
                RegexNode::Forbidden(ForbiddenConstruct::UnicodeClass(body)),
                start,
                self.pos(),
            ));
        }

        // Otherwise an escaped scalar value (control char, hex, unicode, or
        // escaped punctuation).
        let v = self.escaped_scalar(c)?;
        Ok(Self::spanned(
            RegexNode::Literal(self.make_scalar_literal(v)),
            start,
            self.pos(),
        ))
    }

    /// Resolve `\X` (the `X` already consumed) to a scalar value, handling
    /// `\xNN`, `\u{...}`, control escapes, and escaped punctuation.
    fn escaped_scalar(&mut self, c: char) -> Result<u32, ParseError> {
        let v = match c {
            'n' => 0x0A,
            't' => 0x09,
            'r' => 0x0D,
            'f' => 0x0C,
            'v' => 0x0B,
            '0' => 0x00,
            'a' => 0x07,
            'e' => 0x1B,
            'x' => return self.parse_hex_escape(),
            'u' => return self.parse_unicode_escape(),
            // Escaped punctuation is the literal punctuation char.
            '\\' | '.' | '*' | '+' | '?' | '(' | ')' | '[' | ']' | '{' | '}' | '|' | '^' | '$'
            | '-' | '/' => c as u32,
            // Unknown alphabetic escape is an error; unknown punctuation is
            // treated as the literal char (lenient, RE2-ish).
            _ if c.is_ascii_alphabetic() => {
                return Err(self.error(ParseErrorKind::UnknownEscape(c.to_string())));
            }
            _ => c as u32,
        };
        Ok(v)
    }

    /// `\xNN` — exactly two hex digits.
    fn parse_hex_escape(&mut self) -> Result<u32, ParseError> {
        let mut hex = String::new();
        for _ in 0..2 {
            match self.peek() {
                Some(d) if d.is_ascii_hexdigit() => {
                    hex.push(d);
                    self.bump();
                }
                _ => return Err(self.error(ParseErrorKind::IncompleteHexEscape)),
            }
        }
        u32::from_str_radix(&hex, 16).map_err(|_| self.error(ParseErrorKind::IncompleteHexEscape))
    }

    /// `\u{...}` — braced hex code point.
    fn parse_unicode_escape(&mut self) -> Result<u32, ParseError> {
        if !self.eat('{') {
            return Err(self.error(ParseErrorKind::InvalidUnicodeEscape));
        }
        let body = self.read_name_until('}');
        if !self.eat('}') {
            return Err(self.error(ParseErrorKind::InvalidUnicodeEscape));
        }
        let v = u32::from_str_radix(&body, 16)
            .map_err(|_| self.error(ParseErrorKind::InvalidUnicodeEscape))?;
        if char::from_u32(v).is_none() {
            return Err(self.error(ParseErrorKind::InvalidUnicodeEscape));
        }
        Ok(v)
    }

    // --- literal construction (alphabet-aware) ---

    fn make_literal(&self, c: char) -> Literal {
        match self.alphabet {
            Alphabet::Bytes if c.is_ascii() => Literal::Byte(c as u8),
            // A non-ASCII literal in the bytes alphabet is wrong-alphabet;
            // emit a CodePoint so restrictions can flag E722.
            Alphabet::Bytes => Literal::CodePoint(c),
            Alphabet::Char => Literal::CodePoint(c),
            Alphabet::Token => Literal::Token(c.to_string()),
        }
    }

    fn make_scalar_literal(&self, v: u32) -> Literal {
        match self.alphabet {
            Alphabet::Bytes => Literal::Byte((v & 0xFF) as u8),
            Alphabet::Char => Literal::CodePoint(char::from_u32(v).unwrap_or('\u{FFFD}')),
            Alphabet::Token => {
                Literal::Token(char::from_u32(v).map(|c| c.to_string()).unwrap_or_default())
            }
        }
    }
}

/// Recognize a shorthand-class escape letter, returning its kind and
/// whether the uppercase (negated) form was used.
fn shorthand(c: char) -> Option<(ShorthandKind, bool)> {
    match c {
        'd' => Some((ShorthandKind::Digit, false)),
        'D' => Some((ShorthandKind::Digit, true)),
        'w' => Some((ShorthandKind::Word, false)),
        'W' => Some((ShorthandKind::Word, true)),
        's' => Some((ShorthandKind::Whitespace, false)),
        'S' => Some((ShorthandKind::Whitespace, true)),
        _ => None,
    }
}

/// Parse failure — a syntactically malformed regex that cannot be
/// recovered into an AST. Semantic restrictions (forbidden-but-parseable
/// constructs) are reported by [`super::restrictions`] instead.
#[derive(Debug, Clone)]
pub struct ParseError {
    pub kind: ParseErrorKind,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseErrorKind {
    /// Hit an unexpected character that no production accepts.
    Unexpected(char),

    /// `(` without matching `)`.
    UnclosedGroup,

    /// `[` without matching `]`.
    UnclosedClass,

    /// `{n}` / `{n,m}` quantifier malformed (e.g., `{,}`, `{a}`).
    MalformedQuantifier,

    /// `\X` where X is not a recognized escape.
    UnknownEscape(String),

    /// Class contains no members (`[]` or `[^]`).
    EmptyClass,

    /// `\xN` where N is incomplete (need exactly two hex digits).
    IncompleteHexEscape,

    /// `\u{...}` malformed or out of Unicode range.
    InvalidUnicodeEscape,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_bytes(src: &str) -> RegexAst {
        parse(src, Alphabet::Bytes).unwrap_or_else(|e| panic!("parse {:?} failed: {:?}", src, e))
    }

    fn root(src: &str) -> RegexNode {
        parse_bytes(src).root.node
    }

    #[test]
    fn empty_regex_is_empty_node() {
        assert!(matches!(root(""), RegexNode::Empty));
    }

    #[test]
    fn single_byte_literal() {
        match root("a") {
            RegexNode::Literal(Literal::Byte(b'a')) => {}
            other => panic!("got {:?}", other),
        }
    }

    #[test]
    fn concat_of_literals() {
        match root("abc") {
            RegexNode::Concat(items) => assert_eq!(items.len(), 3),
            other => panic!("got {:?}", other),
        }
    }

    #[test]
    fn alternation() {
        match root("a|b|c") {
            RegexNode::Alt(branches) => assert_eq!(branches.len(), 3),
            other => panic!("got {:?}", other),
        }
    }

    #[test]
    fn quantifiers() {
        assert!(matches!(
            root("a*"),
            RegexNode::Quantifier {
                kind: QuantifierKind::ZeroOrMore,
                laziness: Laziness::Greedy,
                ..
            }
        ));
        assert!(matches!(
            root("a+"),
            RegexNode::Quantifier {
                kind: QuantifierKind::OneOrMore,
                ..
            }
        ));
        assert!(matches!(
            root("a?"),
            RegexNode::Quantifier {
                kind: QuantifierKind::ZeroOrOne,
                ..
            }
        ));
    }

    #[test]
    fn lazy_quantifier_recorded() {
        assert!(matches!(
            root("a*?"),
            RegexNode::Quantifier {
                laziness: Laziness::Lazy,
                ..
            }
        ));
    }

    #[test]
    fn bounded_quantifiers() {
        assert!(matches!(
            root("a{3}"),
            RegexNode::Quantifier {
                kind: QuantifierKind::Exact(3),
                ..
            }
        ));
        assert!(matches!(
            root("a{2,5}"),
            RegexNode::Quantifier {
                kind: QuantifierKind::Bounded { min: 2, max: 5 },
                ..
            }
        ));
        assert!(matches!(
            root("a{2,}"),
            RegexNode::Quantifier {
                kind: QuantifierKind::AtLeast(2),
                ..
            }
        ));
    }

    #[test]
    fn brace_not_a_bound_is_literal() {
        // `{` not followed by a digit is a literal brace.
        match root("a{b") {
            RegexNode::Concat(items) => assert_eq!(items.len(), 3),
            other => panic!("got {:?}", other),
        }
    }

    #[test]
    fn char_class_with_range_and_shorthand() {
        match root("[a-z0-9\\d]") {
            RegexNode::Class(c) => {
                assert!(!c.negated);
                assert!(matches!(
                    c.members[0],
                    ClassMember::Range { low: 97, high: 122 }
                ));
                assert!(matches!(
                    c.members[1],
                    ClassMember::Range { low: 48, high: 57 }
                ));
                assert!(matches!(
                    c.members[2],
                    ClassMember::Shorthand {
                        kind: ShorthandKind::Digit,
                        negated: false
                    }
                ));
            }
            other => panic!("got {:?}", other),
        }
    }

    #[test]
    fn negated_class() {
        match root("[^abc]") {
            RegexNode::Class(c) => {
                assert!(c.negated);
                assert_eq!(c.members.len(), 3);
            }
            other => panic!("got {:?}", other),
        }
    }

    #[test]
    fn dot_and_anchors() {
        assert!(matches!(root("."), RegexNode::Dot));
        assert!(matches!(root("^"), RegexNode::Anchor(Anchor::LineStart)));
        assert!(matches!(root("$"), RegexNode::Anchor(Anchor::LineEnd)));
        assert!(matches!(
            root("\\b"),
            RegexNode::Anchor(Anchor::WordBoundary)
        ));
    }

    #[test]
    fn shorthand_class_atom() {
        match root("\\d") {
            RegexNode::Class(c) => assert!(matches!(
                c.members[0],
                ClassMember::Shorthand {
                    kind: ShorthandKind::Digit,
                    negated: false
                }
            )),
            other => panic!("got {:?}", other),
        }
        match root("\\W") {
            RegexNode::Class(c) => assert!(matches!(
                c.members[0],
                ClassMember::Shorthand {
                    kind: ShorthandKind::Word,
                    negated: true
                }
            )),
            other => panic!("got {:?}", other),
        }
    }

    #[test]
    fn hex_and_escaped_punct() {
        assert!(matches!(
            root("\\x41"),
            RegexNode::Literal(Literal::Byte(0x41))
        ));
        assert!(matches!(
            root("\\."),
            RegexNode::Literal(Literal::Byte(b'.'))
        ));
        assert!(matches!(
            root("\\n"),
            RegexNode::Literal(Literal::Byte(0x0A))
        ));
    }

    #[test]
    fn group() {
        match root("(ab)") {
            RegexNode::Group(inner) => {
                assert!(matches!(inner.node, RegexNode::Concat(_)));
            }
            other => panic!("got {:?}", other),
        }
    }

    #[test]
    fn forbidden_constructs_parse_structurally() {
        assert!(matches!(
            root("\\1"),
            RegexNode::Forbidden(ForbiddenConstruct::Backref(1))
        ));
        assert!(matches!(
            root("(?:ab)"),
            RegexNode::Forbidden(ForbiddenConstruct::NonCapturingGroup(_))
        ));
        assert!(matches!(
            root("(?=ab)"),
            RegexNode::Forbidden(ForbiddenConstruct::PositiveLookahead(_))
        ));
        assert!(matches!(
            root("(?!ab)"),
            RegexNode::Forbidden(ForbiddenConstruct::NegativeLookahead(_))
        ));
        assert!(matches!(
            root("(?<=ab)"),
            RegexNode::Forbidden(ForbiddenConstruct::PositiveLookbehind(_))
        ));
        assert!(matches!(
            root("(?P<year>ab)"),
            RegexNode::Forbidden(ForbiddenConstruct::NamedCapture { .. })
        ));
        match root("\\p{L}") {
            RegexNode::Forbidden(ForbiddenConstruct::UnicodeClass(b)) => assert_eq!(b, "L"),
            other => panic!("got {:?}", other),
        }
    }

    #[test]
    fn token_alphabet_parses_identifier_runs() {
        // `/IDENT LPAREN RPAREN/` → three token-kind literals, not chars.
        let ast = parse("IDENT LPAREN RPAREN", Alphabet::Token).unwrap();
        match ast.root.node {
            RegexNode::Concat(items) => {
                assert_eq!(items.len(), 3);
                for (i, name) in ["IDENT", "LPAREN", "RPAREN"].iter().enumerate() {
                    match &items[i].node {
                        RegexNode::Literal(Literal::Token(t)) => assert_eq!(t, name),
                        other => panic!("item {i} got {:?}", other),
                    }
                }
            }
            other => panic!("got {:?}", other),
        }
    }

    #[test]
    fn token_alphabet_alternation() {
        // `/A | B/` → alternation of two token literals (whitespace is
        // insignificant between tokens and around `|`).
        let ast = parse("A | B", Alphabet::Token).unwrap();
        match ast.root.node {
            RegexNode::Alt(branches) => assert_eq!(branches.len(), 2),
            other => panic!("got {:?}", other),
        }
    }

    #[test]
    fn char_alphabet_uses_codepoints() {
        let ast = parse("é", Alphabet::Char).unwrap();
        assert!(matches!(
            ast.root.node,
            RegexNode::Literal(Literal::CodePoint('é'))
        ));
    }

    #[test]
    fn errors() {
        assert!(matches!(
            parse("(a", Alphabet::Bytes).unwrap_err().kind,
            ParseErrorKind::UnclosedGroup
        ));
        assert!(matches!(
            parse("[abc", Alphabet::Bytes).unwrap_err().kind,
            ParseErrorKind::UnclosedClass
        ));
        assert!(matches!(
            parse("[]", Alphabet::Bytes).unwrap_err().kind,
            ParseErrorKind::EmptyClass
        ));
        assert!(matches!(
            parse("a)", Alphabet::Bytes).unwrap_err().kind,
            ParseErrorKind::Unexpected(')')
        ));
        assert!(matches!(
            parse("\\xZZ", Alphabet::Bytes).unwrap_err().kind,
            ParseErrorKind::IncompleteHexEscape
        ));
        assert!(matches!(
            parse("*", Alphabet::Bytes).unwrap_err().kind,
            ParseErrorKind::Unexpected('*')
        ));
    }

    #[test]
    fn nested_alt_in_group_with_quantifier() {
        // `(a|b)+` — group of alt, one-or-more.
        match root("(a|b)+") {
            RegexNode::Quantifier {
                kind: QuantifierKind::OneOrMore,
                inner,
                ..
            } => match inner.node {
                RegexNode::Group(g) => assert!(matches!(g.node, RegexNode::Alt(_))),
                other => panic!("got group inner {:?}", other),
            },
            other => panic!("got {:?}", other),
        }
    }
}
