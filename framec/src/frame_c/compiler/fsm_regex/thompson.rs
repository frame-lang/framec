//! Thompson NFA construction.
//!
//! Builds an ε-NFA from a [`RegexAst`] following the standard Thompson
//! recipe (concat, alt, quantifier, group). The resulting NFA has
//! exactly one start state and one accept state per RFC-0042 §6.9
//! step 2.
//!
//! Lazy quantifiers and other non-greedy semantics are *not* handled
//! here — they were rejected by [`super::restrictions`] before reaching
//! Thompson. Anchors are encoded as zero-width [`TransitionLabel::Anchor`]
//! transitions; the subset construction ([`super::subset`]) carries them
//! onto DFA transitions as position assertions checked by the matcher.
//!
//! Shorthand classes (`\d`, `\w`, `\s`) use ASCII definitions in v0.1;
//! Unicode-aware shorthands are deferred to v0.2 (RFC-0042 §11.6).

use super::ast::{
    Anchor, CharClass, ClassMember, Literal, QuantifierKind, RegexAst, RegexNode, ShorthandKind,
    SpannedNode,
};
use super::Alphabet;

/// ε-NFA produced by Thompson construction.
#[derive(Debug, Clone)]
pub struct Nfa {
    pub states: Vec<NfaState>,
    pub start: NfaStateId,
    pub accept: NfaStateId,
    pub alphabet: Alphabet,
}

pub type NfaStateId = usize;

#[derive(Debug, Clone)]
pub struct NfaState {
    pub id: NfaStateId,
    pub transitions: Vec<NfaTransition>,
}

#[derive(Debug, Clone)]
pub struct NfaTransition {
    pub on: TransitionLabel,
    pub to: NfaStateId,
}

/// What an NFA transition consumes (or doesn't).
#[derive(Debug, Clone)]
pub enum TransitionLabel {
    /// ε — no input consumed. Used by Thompson for sequencing,
    /// alternation, and quantifier wiring.
    Epsilon,

    /// Match a single byte. Byte alphabet.
    Byte(u8),

    /// Match a byte range, low..=high inclusive. Byte alphabet, from
    /// character classes.
    ByteRange { low: u8, high: u8 },

    /// Match a Unicode code point. Char alphabet.
    CodePoint(char),

    /// Match a code-point range, low..=high inclusive. Char alphabet.
    CodePointRange { low: char, high: char },

    /// Match a single token kind by name. Token alphabet.
    Token(String),

    /// Zero-width position assertion. Treated as ε in subset
    /// construction; checked by the matcher at recognition time.
    Anchor(Anchor),
}

/// One sub-NFA: a single entry and a single exit state.
#[derive(Debug, Clone, Copy)]
struct Frag {
    start: NfaStateId,
    accept: NfaStateId,
}

struct Builder {
    states: Vec<NfaState>,
    alphabet: Alphabet,
}

impl Builder {
    fn new(alphabet: Alphabet) -> Self {
        Self {
            states: Vec::new(),
            alphabet,
        }
    }

    fn new_state(&mut self) -> NfaStateId {
        let id = self.states.len();
        self.states.push(NfaState {
            id,
            transitions: Vec::new(),
        });
        id
    }

    fn add(&mut self, from: NfaStateId, on: TransitionLabel, to: NfaStateId) {
        self.states[from].transitions.push(NfaTransition { on, to });
    }

    fn eps(&mut self, from: NfaStateId, to: NfaStateId) {
        self.add(from, TransitionLabel::Epsilon, to);
    }

    /// Build a fragment from a node.
    fn build(&mut self, node: &SpannedNode) -> Frag {
        match &node.node {
            RegexNode::Literal(lit) => self.literal_frag(lit),
            RegexNode::Class(class) => self.class_frag(class),
            RegexNode::Dot => self.dot_frag(),
            RegexNode::Anchor(a) => self.anchor_frag(*a),
            // The empty regex and empty concat both match the empty string:
            // a single ε edge.
            RegexNode::Empty => self.epsilon_frag(),
            RegexNode::Concat(items) => self.concat_frag(items),
            RegexNode::Alt(branches) => self.alt_frag(branches),
            RegexNode::Quantifier { inner, kind, .. } => self.quant_frag(inner, *kind),
            RegexNode::Group(inner) => self.build(inner),
            RegexNode::Forbidden(_) => {
                debug_assert!(
                    false,
                    "forbidden node reached Thompson; restrictions::check must run first"
                );
                self.epsilon_frag()
            }
        }
    }

    fn epsilon_frag(&mut self) -> Frag {
        let s = self.new_state();
        let a = self.new_state();
        self.eps(s, a);
        Frag {
            start: s,
            accept: a,
        }
    }

    fn single_label_frag(&mut self, on: TransitionLabel) -> Frag {
        let s = self.new_state();
        let a = self.new_state();
        self.add(s, on, a);
        Frag {
            start: s,
            accept: a,
        }
    }

    fn literal_frag(&mut self, lit: &Literal) -> Frag {
        let on = match lit {
            Literal::Byte(b) => TransitionLabel::Byte(*b),
            Literal::CodePoint(c) => TransitionLabel::CodePoint(*c),
            Literal::Token(t) => TransitionLabel::Token(t.clone()),
        };
        self.single_label_frag(on)
    }

    fn anchor_frag(&mut self, a: Anchor) -> Frag {
        self.single_label_frag(TransitionLabel::Anchor(a))
    }

    /// A class is the alternation of its members: one start, one accept,
    /// and a parallel labeled edge per resolved range.
    fn class_frag(&mut self, class: &CharClass) -> Frag {
        let s = self.new_state();
        let a = self.new_state();
        let ranges = resolve_class(class, self.alphabet);
        for (low, high) in ranges {
            let on = self.range_label(low, high);
            self.add(s, on, a);
        }
        Frag {
            start: s,
            accept: a,
        }
    }

    fn dot_frag(&mut self) -> Frag {
        let s = self.new_state();
        let a = self.new_state();
        // `.` matches any element except `\n` (0x0A) by default.
        for (low, high) in complement_ranges(&[(0x0A, 0x0A)], self.alphabet) {
            let on = self.range_label(low, high);
            self.add(s, on, a);
        }
        Frag {
            start: s,
            accept: a,
        }
    }

    /// Build a `Byte`/`CodePoint` (single) or `*Range` label for `low..=high`.
    fn range_label(&self, low: u32, high: u32) -> TransitionLabel {
        match self.alphabet {
            Alphabet::Bytes => {
                let lo = low.min(0xFF) as u8;
                let hi = high.min(0xFF) as u8;
                if lo == hi {
                    TransitionLabel::Byte(lo)
                } else {
                    TransitionLabel::ByteRange { low: lo, high: hi }
                }
            }
            Alphabet::Char | Alphabet::Token => {
                let lo = char::from_u32(low).unwrap_or('\u{FFFD}');
                let hi = char::from_u32(high).unwrap_or('\u{FFFD}');
                if lo == hi {
                    TransitionLabel::CodePoint(lo)
                } else {
                    TransitionLabel::CodePointRange { low: lo, high: hi }
                }
            }
        }
    }

    fn concat_frag(&mut self, items: &[SpannedNode]) -> Frag {
        if items.is_empty() {
            let s = self.new_state();
            let a = self.new_state();
            self.eps(s, a);
            return Frag {
                start: s,
                accept: a,
            };
        }
        let first = self.build(&items[0]);
        let mut accept = first.accept;
        for item in &items[1..] {
            let f = self.build(item);
            self.eps(accept, f.start);
            accept = f.accept;
        }
        Frag {
            start: first.start,
            accept,
        }
    }

    fn alt_frag(&mut self, branches: &[SpannedNode]) -> Frag {
        let s = self.new_state();
        let a = self.new_state();
        for b in branches {
            let f = self.build(b);
            self.eps(s, f.start);
            self.eps(f.accept, a);
        }
        Frag {
            start: s,
            accept: a,
        }
    }

    fn quant_frag(&mut self, inner: &SpannedNode, kind: QuantifierKind) -> Frag {
        match kind {
            QuantifierKind::ZeroOrOne => {
                let f = self.build(inner);
                let s = self.new_state();
                let a = self.new_state();
                self.eps(s, f.start);
                self.eps(f.accept, a);
                self.eps(s, a); // skip
                Frag {
                    start: s,
                    accept: a,
                }
            }
            QuantifierKind::ZeroOrMore => {
                let f = self.build(inner);
                let s = self.new_state();
                let a = self.new_state();
                self.eps(s, f.start);
                self.eps(s, a); // zero
                self.eps(f.accept, f.start); // loop
                self.eps(f.accept, a);
                Frag {
                    start: s,
                    accept: a,
                }
            }
            QuantifierKind::OneOrMore => {
                let f = self.build(inner);
                let s = self.new_state();
                let a = self.new_state();
                self.eps(s, f.start);
                self.eps(f.accept, f.start); // loop
                self.eps(f.accept, a);
                Frag {
                    start: s,
                    accept: a,
                }
            }
            QuantifierKind::Exact(n) => self.repeat(inner, n, n),
            QuantifierKind::Bounded { min, max } => self.repeat(inner, min, max),
            QuantifierKind::AtLeast(n) => {
                // n required copies followed by a star of one more copy.
                let req = self.repeat(inner, n, n);
                let star = self.quant_frag(inner, QuantifierKind::ZeroOrMore);
                self.eps(req.accept, star.start);
                Frag {
                    start: req.start,
                    accept: star.accept,
                }
            }
        }
    }

    /// `{min,max}` by unrolling: `min` mandatory copies then `max-min`
    /// optional copies. `repeat(_, 0, 0)` is the empty fragment.
    fn repeat(&mut self, inner: &SpannedNode, min: u32, max: u32) -> Frag {
        let s = self.new_state();
        let mut tail = s;
        // Mandatory copies.
        for _ in 0..min {
            let f = self.build(inner);
            self.eps(tail, f.start);
            tail = f.accept;
        }
        let a = self.new_state();
        // Optional copies, each able to skip straight to the accept.
        let optional = max.saturating_sub(min);
        for _ in 0..optional {
            let f = self.build(inner);
            self.eps(tail, f.start);
            self.eps(tail, a); // skip the rest
            tail = f.accept;
        }
        self.eps(tail, a);
        Frag {
            start: s,
            accept: a,
        }
    }
}

/// Build a Thompson ε-NFA from a validated [`RegexAst`].
///
/// **Precondition:** `ast` must have passed [`super::restrictions::check`].
/// Forbidden nodes still present in the AST trip a debug assertion.
pub fn build(ast: &RegexAst, alphabet: Alphabet) -> Nfa {
    let mut b = Builder::new(alphabet);
    let frag = b.build(&ast.root);
    Nfa {
        states: b.states,
        start: frag.start,
        accept: frag.accept,
        alphabet,
    }
}

/// Resolve a character class to a set of `(low, high)` scalar ranges,
/// applying negation against the alphabet's universe. Shorthands use
/// ASCII definitions (v0.1).
/// Resolve a `[...]` character class to its merged scalar ranges over
/// `alphabet`. Shared with the Pike VM compiler (`super::pike`), which emits
/// `Char` instructions directly from these ranges.
pub fn resolve_class(class: &CharClass, alphabet: Alphabet) -> Vec<(u32, u32)> {
    let mut ranges = Vec::new();
    for m in &class.members {
        match m {
            ClassMember::Single(v) => ranges.push((*v, *v)),
            ClassMember::Range { low, high } => ranges.push((*low, *high)),
            ClassMember::Shorthand { kind, negated } => {
                let base = shorthand_ranges(*kind);
                if *negated {
                    ranges.extend(complement_ranges(&base, alphabet));
                } else {
                    ranges.extend(base);
                }
            }
            // `\p{...}` members are rewritten to `Range`s by `super::unicode`
            // before Thompson runs, so none survive here.
            ClassMember::Unicode { .. } => {
                unreachable!("Unicode class members are resolved before Thompson construction")
            }
        }
    }
    let merged = merge_ranges(ranges);
    if class.negated {
        complement_ranges(&merged, alphabet)
    } else {
        merged
    }
}

/// The scalar ranges matched by `.` over `alphabet`: any element except `\n`
/// (0x0A). Shared with the Pike VM compiler (`super::pike`), mirroring
/// `Compiler::dot_frag`.
pub fn dot_ranges(alphabet: Alphabet) -> Vec<(u32, u32)> {
    complement_ranges(&[(0x0A, 0x0A)], alphabet)
}

/// ASCII definitions of the shorthand classes (v0.1).
fn shorthand_ranges(kind: ShorthandKind) -> Vec<(u32, u32)> {
    match kind {
        ShorthandKind::Digit => vec![(0x30, 0x39)],
        ShorthandKind::Word => vec![(0x30, 0x39), (0x41, 0x5A), (0x5F, 0x5F), (0x61, 0x7A)],
        ShorthandKind::Whitespace => vec![
            (0x09, 0x0A), // \t \n
            (0x0B, 0x0D), // \v \f \r
            (0x20, 0x20), // space
        ],
    }
}

/// The maximum scalar value in an alphabet's universe.
fn universe_max(alphabet: Alphabet) -> u32 {
    match alphabet {
        Alphabet::Bytes => 0xFF,
        // Char/Token: full Unicode scalar range. The surrogate gap is
        // handled by char::from_u32 returning None at label-build time.
        Alphabet::Char | Alphabet::Token => 0x10_FFFF,
    }
}

/// Sort + coalesce overlapping/adjacent ranges.
fn merge_ranges(mut ranges: Vec<(u32, u32)>) -> Vec<(u32, u32)> {
    ranges.retain(|(lo, hi)| lo <= hi);
    ranges.sort_by_key(|(lo, _)| *lo);
    let mut out: Vec<(u32, u32)> = Vec::new();
    for (lo, hi) in ranges {
        if let Some(last) = out.last_mut() {
            // Adjacent or overlapping (use saturating +1 to coalesce
            // touching ranges like 0..9 and 10..15).
            if lo <= last.1.saturating_add(1) {
                last.1 = last.1.max(hi);
                continue;
            }
        }
        out.push((lo, hi));
    }
    out
}

/// Complement a set of ranges against `[0, universe_max]`.
fn complement_ranges(ranges: &[(u32, u32)], alphabet: Alphabet) -> Vec<(u32, u32)> {
    let max = universe_max(alphabet);
    let merged = merge_ranges(ranges.to_vec());
    let mut out = Vec::new();
    let mut next = 0u32;
    for (lo, hi) in merged {
        if lo > next {
            out.push((next, lo - 1));
        }
        next = hi.saturating_add(1);
        if next > max {
            break;
        }
    }
    if next <= max {
        out.push((next, max));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frame_c::compiler::fsm_regex::parser;

    fn nfa(src: &str) -> Nfa {
        let ast = parser::parse(src, Alphabet::Bytes).expect("parse");
        build(&ast, Alphabet::Bytes)
    }

    /// Count transitions of a given predicate across all states.
    fn count<F: Fn(&TransitionLabel) -> bool>(nfa: &Nfa, pred: F) -> usize {
        nfa.states
            .iter()
            .flat_map(|s| &s.transitions)
            .filter(|t| pred(&t.on))
            .count()
    }

    #[test]
    fn single_literal() {
        let n = nfa("a");
        assert_eq!(count(&n, |l| matches!(l, TransitionLabel::Byte(b'a'))), 1);
        assert_ne!(n.start, n.accept);
    }

    #[test]
    fn concat_has_each_literal() {
        let n = nfa("abc");
        assert_eq!(count(&n, |l| matches!(l, TransitionLabel::Byte(_))), 3);
    }

    #[test]
    fn alternation_has_both() {
        let n = nfa("a|b");
        assert_eq!(count(&n, |l| matches!(l, TransitionLabel::Byte(_))), 2);
    }

    #[test]
    fn star_has_loop_epsilons() {
        let n = nfa("a*");
        // The single byte plus several ε edges (skip + loop wiring).
        assert_eq!(count(&n, |l| matches!(l, TransitionLabel::Byte(b'a'))), 1);
        assert!(count(&n, |l| matches!(l, TransitionLabel::Epsilon)) >= 3);
    }

    #[test]
    fn exact_repeat_clones_inner() {
        let n = nfa("a{3}");
        assert_eq!(count(&n, |l| matches!(l, TransitionLabel::Byte(b'a'))), 3);
    }

    #[test]
    fn bounded_repeat_clones_min_and_max() {
        let n = nfa("a{2,4}");
        assert_eq!(count(&n, |l| matches!(l, TransitionLabel::Byte(b'a'))), 4);
    }

    #[test]
    fn class_range_becomes_byterange() {
        let n = nfa("[a-z]");
        assert_eq!(
            count(&n, |l| matches!(
                l,
                TransitionLabel::ByteRange {
                    low: b'a',
                    high: b'z'
                }
            )),
            1
        );
    }

    #[test]
    fn digit_shorthand_is_0_9() {
        let n = nfa("\\d");
        assert_eq!(
            count(&n, |l| matches!(
                l,
                TransitionLabel::ByteRange {
                    low: 0x30,
                    high: 0x39
                }
            )),
            1
        );
    }

    #[test]
    fn negated_class_complements() {
        // [^a] over bytes → [0x00..0x60] and [0x62..0xFF].
        let n = nfa("[^a]");
        let ranges: Vec<_> = n
            .states
            .iter()
            .flat_map(|s| &s.transitions)
            .filter_map(|t| match t.on {
                TransitionLabel::ByteRange { low, high } => Some((low, high)),
                _ => None,
            })
            .collect();
        assert!(ranges.contains(&(0x00, 0x60)));
        assert!(ranges.contains(&(0x62, 0xFF)));
    }

    #[test]
    fn dot_excludes_newline() {
        let n = nfa(".");
        let ranges: Vec<_> = n
            .states
            .iter()
            .flat_map(|s| &s.transitions)
            .filter_map(|t| match t.on {
                TransitionLabel::ByteRange { low, high } => Some((low, high)),
                _ => None,
            })
            .collect();
        // Split around 0x0A.
        assert!(ranges.contains(&(0x00, 0x09)));
        assert!(ranges.contains(&(0x0B, 0xFF)));
    }

    #[test]
    fn anchor_is_zero_width_label() {
        let n = nfa("^a");
        assert_eq!(
            count(&n, |l| matches!(
                l,
                TransitionLabel::Anchor(Anchor::LineStart)
            )),
            1
        );
    }

    #[test]
    fn merge_coalesces_adjacent() {
        let merged = merge_ranges(vec![(0, 9), (10, 15), (20, 25)]);
        assert_eq!(merged, vec![(0, 15), (20, 25)]);
    }

    #[test]
    fn complement_round_trips() {
        // Complement of [a] over bytes.
        let c = complement_ranges(&[(b'a' as u32, b'a' as u32)], Alphabet::Bytes);
        assert_eq!(c, vec![(0x00, 0x60), (0x62, 0xFF)]);
    }
}
