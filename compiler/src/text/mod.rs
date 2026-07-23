//! The text types — and the two modules allowed to touch them.
//!
//! `scan` (which may read bytes) and `emit` (which may unwrap text) are **submodules
//! of this one**, and that nesting is the whole enforcement mechanism. Rust's
//! `pub(in path)` can only restrict visibility to an ANCESTOR module — so "only the
//! scanner may see a byte" and "only the emitter may unwrap text" are *only sayable*
//! if the scanner and the emitter live underneath the types they are privileged over.
//!
//! Everything else in the compiler — the tree, the symbol table, resolve, validate,
//! lower — sits OUTSIDE `crate::text` and therefore **cannot call `Source::open` and
//! cannot call `NativeText::finish`**. Not "should not". *Cannot.* rustc says so.
//!
//! That is the answer to "does codegen only source from the AST?" — it is not a
//! promise, it is a link error.
//!
//! # Why this module exists
//!
//! Every one of the twenty-five bugs in the old compiler has the same shape:
//!
//! > framec knows a fact while generating, encodes the fact into **text**, throws
//! > the fact away, and then re-reads its own output (or the user's) to recover it.
//!
//! A `String` is what makes that possible. `&str` in Rust carries `.contains()`,
//! `.starts_with()`, `.find()` — so *any* pass, anywhere, can ask text a structural
//! question. The old compiler had 71 such probes. Review caught none of them; three
//! separate backends learned the same lesson independently and still shipped it.
//!
//! So the fix is not a rule. The fix is a **type**.
//!
//! # The design
//!
//! * [`Source`] holds the bytes **privately**. There is no accessor.
//! * The single escape is [`Source::open`], which is `pub(in crate::scan)` — so
//!   **only the scanner can see a byte**. Not "should only"; *can* only. Rust's
//!   module privacy is the enforcement, and it is not negotiable at review time.
//! * Everything downstream of the scanner holds [`Span`]s (a range) and
//!   [`NativeText`] (opaque). Neither can be asked a question about its contents.
//! * [`NativeText`] has no `Deref`, no `AsRef<str>`, no `Display`, no
//!   `PartialEq<&str>`, and a **hand-written `Debug` that prints the span and never
//!   the content**. Each of those is an escape hatch a competent author adds on day
//!   one without malice — `format!("{:?}", t).contains(..)` is a whole-text oracle,
//!   and `#[derive(Debug)]` is enough to open it.
//! * The only way out of [`NativeText`] is [`NativeText::finish`], which **consumes
//!   `self`** and lives at the sink. After `finish` there is no compiler value left
//!   to run a pass over.
//!
//! That last point is the good one: **"emission is one-way" stops being a policy and
//! becomes a borrow-check error.** A pass that tries to re-read emitted output does
//! not fail review. It fails to compile.

pub mod emit;
pub mod scan;

use std::fmt;

/// A byte range in a [`Source`]. Half-open: `[start, end)`.
///
/// Spans are **absolute** into the source buffer, always. A pass that needs to know
/// *where* something is gets a `Span`. A pass that needs to know *what it says*
/// mostly does not, and cannot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

impl Span {
    pub fn new(start: usize, end: usize) -> Span {
        debug_assert!(start <= end, "inverted span {start}..{end}");
        Span { start, end }
    }

    pub fn len(&self) -> usize {
        self.end - self.start
    }

    pub fn is_empty(&self) -> bool {
        self.start == self.end
    }
}

/// Verbatim target-language text — **opaque by construction**.
///
/// framec must never know what this *means*. It is carried, moved, and eventually
/// emitted; it is never interrogated. There is deliberately no way to ask it a
/// question:
///
/// ```compile_fail
/// # use frame_compiler::source::NativeText;
/// # fn f(t: NativeText) {
/// t.contains("co_await");   // no Deref, no AsRef<str> — does not compile
/// # }
/// ```
///
/// That single line is the bug that shipped on seven backends. It cannot be written
/// here.
#[derive(Clone, PartialEq, Eq)]
pub struct NativeText {
    text: String,
    span: Span,
}

impl NativeText {
    /// Only the scanner may mint native text, and only from a span it delimited.
    pub(in crate::text) fn new(text: String, span: Span) -> NativeText {
        NativeText { text, span }
    }

    /// Where it came from. A pass may ask *where*; never *what*.
    pub fn span(&self) -> Span {
        self.span
    }

    /// How many bytes. Useful for diagnostics and layout; reveals nothing.
    pub fn len(&self) -> usize {
        self.text.len()
    }

    pub fn is_empty(&self) -> bool {
        self.text.is_empty()
    }

    /// The **sink**. Consumes `self` — this is the last thing that ever happens to
    /// a piece of native text.
    ///
    /// This is RFC-0056 P6 ("emission is one-way") expressed as a *move*. A pass
    /// that wanted to re-read emitted output would have to hold a value it has
    /// already given away. That is not a policy violation; it is a borrow-check
    /// error, and it is reported by rustc rather than by a reviewer who is tired.
    pub(in crate::text) fn finish(self) -> String {
        self.text
    }
}

/// Prints the **span, never the content**.
///
/// `#[derive(Debug)]` here would hand every pass a whole-text oracle via
/// `format!("{:?}", t).contains(..)`, and it is the derive everyone reaches for on
/// day one to get `--dump-ast` and `assert_eq!` working. So it is hand-written, and
/// it says nothing.
impl fmt::Debug for NativeText {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "NativeText({}..{}, {} bytes)",
            self.span.start,
            self.span.end,
            self.text.len()
        )
    }
}

// Deliberately NOT implemented on NativeText, each for a specific reason:
//
//   Display            — `format!("{t}")` yields a String; the newtype is gone.
//   Deref<Target=str>  — leaks the entire &str API, including `contains`.
//   AsRef<str> / Borrow<str> — same leak, one keystroke.
//   PartialEq<&str>    — `t == "co_await"` IS a whole-text oracle.
//   serde::Serialize   — derive -> JSON -> String. The serializer belongs at the
//                        sink, past `finish()`, not on the value.
//
// If you are here because you want one of these: you want `finish()`, and you are
// probably not at the sink. Stop.

/// A source file. **Owns the bytes; does not share them.**
pub struct Source {
    /// PRIVATE. The only reader is [`Source::open`], and that is `pub(in crate::scan)`.
    bytes: Vec<u8>,
    path: String,
    /// Length of a leading UTF-8 BOM, if any (0 or 3).
    bom_len: usize,
}

#[derive(Debug)]
pub enum SourceError {
    /// framec's input alphabet is UTF-8 scalar values, not arbitrary bytes. Say so
    /// plainly, rather than failing deep in a scanner with a byte offset.
    NotUtf8 { path: String, at: usize },
}

impl fmt::Display for SourceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SourceError::NotUtf8 { path, at } => write!(
                f,
                "{path}: not valid UTF-8 (first bad byte at offset {at}). \
                 Frame source must be UTF-8; a Latin-1 comment in an otherwise \
                 fine file will land here."
            ),
        }
    }
}

impl Source {
    pub fn new(path: impl Into<String>, bytes: Vec<u8>) -> Result<Source, SourceError> {
        let path = path.into();

        // framec's input alphabet is UTF-8 scalar values. Establish that ONCE, here,
        // at the boundary — so no downstream pass has to wonder, and so the error
        // names the file instead of surfacing as a panic in a byte loop.
        if let Err(e) = std::str::from_utf8(&bytes) {
            return Err(SourceError::NotUtf8 {
                path,
                at: e.valid_up_to(),
            });
        }

        // A leading UTF-8 BOM is an ENCODING MARKER, not content. It belongs to the
        // file it arrived in, not to the file we generate. rustc, clang and javac all
        // consume it silently.
        //
        // The old compiler had no idea it existed: the start-of-line pragma probe saw
        // 0xEF instead of '@' at byte 0, decided line 1 held no pragma, and classified
        // the ENTIRE `@@system` as native text — emitting it verbatim, generating no
        // class at all, and exiting 0 (#214).
        //
        // Note what that bug does to a naive "we cover every byte" invariant:
        // `unparse(parse(src)) == src` holds *perfectly* when you classify the whole
        // file as water. Coverage cannot tell "understood everything" from "understood
        // nothing". That is why coverage is only half the contract (see `tree.rs`).
        const BOM: [u8; 3] = [0xEF, 0xBB, 0xBF];
        let bom_len = if bytes.starts_with(&BOM) { 3 } else { 0 };

        Ok(Source {
            bytes,
            path,
            bom_len,
        })
    }

    pub fn path(&self) -> &str {
        &self.path
    }

    pub fn len(&self) -> usize {
        self.bytes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }

    /// Where the *content* starts — past any BOM.
    pub fn content_start(&self) -> usize {
        self.bom_len
    }

    /// **The only door to the bytes, and it opens into exactly one room.**
    ///
    /// `pub(in crate::scan)` — the scanner, and nothing else in this compiler, can
    /// call this. Not "should only call". *Can* only call. Every other pass in every
    /// other module holds spans and opaque text, and no amount of enthusiasm will
    /// get it a `&[u8]` to run `.windows(4) == b"-> $"` over.
    ///
    /// (`&[u8]` is the fatal escape and must be named: `slice::contains`,
    /// `starts_with`, `ends_with`, `windows`. Handing a `&[u8]` to a pass is a
    /// *complete* bypass of every other protection in this file. So it is handed to
    /// one module, and that module is reviewed as the security boundary it is.)
    pub(in crate::text) fn open(&self) -> &[u8] {
        &self.bytes
    }

    /// Byte offset -> (1-based line, 1-based column). For diagnostics.
    pub fn line_col(&self, offset: usize) -> (usize, usize) {
        let upto = &self.bytes[..offset.min(self.bytes.len())];
        let line = 1 + upto.iter().filter(|&&b| b == b'\n').count();
        let col = 1 + upto.iter().rev().take_while(|&&b| b != b'\n').count();
        (line, col)
    }

    /// Does `span` cross a newline — i.e. is the spanned source more than one logical
    /// line? A trailing newline (only whitespace past the last content byte) does NOT
    /// count: `(True\n)` is one line; `(True\n and False)` is two.
    ///
    /// A POSITION question, answered from the bytes here at the boundary. The emit
    /// passes ask *whether the source expression spans lines* — Python needs wrapping
    /// parens for implicit line continuation (bug R2) — without ever seeing the bytes
    /// or the opaque native content.
    pub(in crate::text) fn span_is_multiline(&self, span: Span) -> bool {
        let end = span.end.min(self.bytes.len());
        let start = span.start.min(end);
        let slice = &self.bytes[start..end];
        // Trailing whitespace/newlines are layout, not a second line.
        let content_end = slice
            .iter()
            .rposition(|&b| !b.is_ascii_whitespace())
            .map_or(0, |p| p + 1);
        slice[..content_end].contains(&b'\n')
    }
}

/// `Source` prints its path and size — never its content. Same reasoning as
/// `NativeText`: a derived `Debug` is a text oracle wearing a disguise.
impl fmt::Debug for Source {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Source({:?}, {} bytes, bom={})",
            self.path,
            self.bytes.len(),
            self.bom_len
        )
    }
}
