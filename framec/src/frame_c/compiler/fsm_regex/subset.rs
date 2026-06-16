//! Subset construction — NFA → DFA.
//!
//! Standard powerset algorithm: each DFA state corresponds to the
//! ε-closure of a set of NFA states; DFA transitions are computed by
//! taking, over a disjoint partition of the input alphabet, the
//! ε-closure of the union of NFA targets. RFC-0042 §6.9 step 3.
//!
//! The result is a DFA that is *not yet minimal*. Minimization happens
//! in [`super::hopcroft`].
//!
//! # Anchors (v0.1 status)
//!
//! Zero-width anchors (`^ $ \A \z \b \B`) are not yet folded into the
//! DFA: a correct construction partitions transitions by a position-flag
//! context the matcher evaluates per position, which needs a richer
//! transition model than the current [`DfaTransition::assertions`]
//! conjunction can express (it cannot represent "must NOT be at a word
//! boundary"). Anchor support is a dedicated follow-up increment. Until
//! then the engine boundary rejects anchor-bearing regexes via
//! [`nfa_has_anchors`] rather than miscompiling them. `construct`
//! therefore assumes an anchor-free NFA (debug-asserted).

use super::thompson::{Nfa, NfaStateId, TransitionLabel};
use super::Alphabet;
use std::collections::{BTreeSet, HashMap};

/// DFA produced by subset construction.
#[derive(Debug, Clone)]
pub struct Dfa {
    pub states: Vec<DfaState>,
    pub start: DfaStateId,
    pub alphabet: Alphabet,
}

pub type DfaStateId = usize;

#[derive(Debug, Clone)]
pub struct DfaState {
    pub id: DfaStateId,
    pub transitions: Vec<DfaTransition>,
    /// `true` if any of the NFA states this DFA state represents is the
    /// NFA's accept state.
    pub is_accept: bool,
}

#[derive(Debug, Clone)]
pub struct DfaTransition {
    /// Input condition under which this transition fires.
    pub label: DfaLabel,
    /// Destination DFA state.
    pub to: DfaStateId,
    /// Position assertions (anchors) that must also hold. Empty until the
    /// anchor increment lands (see the module note).
    pub assertions: Vec<super::ast::Anchor>,
}

/// What input a DFA transition consumes. Deterministic: at most one
/// transition per (state, input) pair after subset construction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DfaLabel {
    /// Byte alphabet.
    Byte(u8),
    ByteRange {
        low: u8,
        high: u8,
    },

    /// Char alphabet.
    CodePoint(char),
    CodePointRange {
        low: char,
        high: char,
    },

    /// Token alphabet.
    Token(String),
}

/// Does the NFA contain any zero-width anchor transition? The engine
/// boundary uses this to defer anchor-bearing regexes (module note).
pub fn nfa_has_anchors(nfa: &Nfa) -> bool {
    nfa.states
        .iter()
        .flat_map(|s| &s.transitions)
        .any(|t| matches!(t.on, TransitionLabel::Anchor(_)))
}

/// Run subset construction on an **anchor-free** `nfa`. Returns a DFA
/// with the same alphabet.
pub fn construct(nfa: &Nfa) -> Dfa {
    debug_assert!(
        !nfa_has_anchors(nfa),
        "subset::construct requires an anchor-free NFA (gate with nfa_has_anchors)"
    );

    let mut builder = DfaBuilder::new(nfa);
    let start_set = builder.epsilon_closure(&[nfa.start]);
    let start = builder.intern(start_set);

    let mut work = vec![start];
    while let Some(dfa_id) = work.pop() {
        let nfa_set = builder.set_of(dfa_id).to_vec();
        let moves = builder.moves(&nfa_set);
        for (label, targets) in moves {
            let closure = builder.epsilon_closure(&targets);
            let (to, is_new) = builder.intern_tracked(closure);
            if is_new {
                work.push(to);
            }
            builder.states[dfa_id].transitions.push(DfaTransition {
                label,
                to,
                assertions: Vec::new(),
            });
        }
    }

    Dfa {
        states: builder.states,
        start,
        alphabet: nfa.alphabet,
    }
}

struct DfaBuilder<'a> {
    nfa: &'a Nfa,
    /// Interned DFA states, keyed by their sorted NFA-state set.
    index: HashMap<Vec<NfaStateId>, DfaStateId>,
    sets: Vec<Vec<NfaStateId>>,
    states: Vec<DfaState>,
}

impl<'a> DfaBuilder<'a> {
    fn new(nfa: &'a Nfa) -> Self {
        Self {
            nfa,
            index: HashMap::new(),
            sets: Vec::new(),
            states: Vec::new(),
        }
    }

    /// ε-closure of a seed set (follows `Epsilon` edges only; the NFA is
    /// anchor-free here). Returns a sorted, deduped state vector.
    fn epsilon_closure(&self, seed: &[NfaStateId]) -> Vec<NfaStateId> {
        let mut seen: BTreeSet<NfaStateId> = seed.iter().copied().collect();
        let mut stack: Vec<NfaStateId> = seed.to_vec();
        while let Some(s) = stack.pop() {
            for t in &self.nfa.states[s].transitions {
                if matches!(t.on, TransitionLabel::Epsilon) && seen.insert(t.to) {
                    stack.push(t.to);
                }
            }
        }
        seen.into_iter().collect()
    }

    fn intern(&mut self, set: Vec<NfaStateId>) -> DfaStateId {
        self.intern_tracked(set).0
    }

    /// Intern a state set, returning its id and whether it was newly
    /// created (so the caller can enqueue it).
    fn intern_tracked(&mut self, set: Vec<NfaStateId>) -> (DfaStateId, bool) {
        if let Some(&id) = self.index.get(&set) {
            return (id, false);
        }
        let id = self.states.len();
        let is_accept = set.contains(&self.nfa.accept);
        self.index.insert(set.clone(), id);
        self.sets.push(set);
        self.states.push(DfaState {
            id,
            transitions: Vec::new(),
            is_accept,
        });
        (id, true)
    }

    fn set_of(&self, id: DfaStateId) -> &[NfaStateId] {
        &self.sets[id]
    }

    /// Compute the DFA moves out of an NFA-state set: a deterministic map
    /// from a disjoint input label to the union of NFA targets.
    fn moves(&self, nfa_set: &[NfaStateId]) -> Vec<(DfaLabel, Vec<NfaStateId>)> {
        match self.nfa.alphabet {
            Alphabet::Token => self.token_moves(nfa_set),
            Alphabet::Bytes | Alphabet::Char => self.range_moves(nfa_set),
        }
    }

    /// Token alphabet: each distinct token name is its own symbol.
    fn token_moves(&self, nfa_set: &[NfaStateId]) -> Vec<(DfaLabel, Vec<NfaStateId>)> {
        let mut by_token: HashMap<String, BTreeSet<NfaStateId>> = HashMap::new();
        for &s in nfa_set {
            for t in &self.nfa.states[s].transitions {
                if let TransitionLabel::Token(name) = &t.on {
                    by_token.entry(name.clone()).or_default().insert(t.to);
                }
            }
        }
        let mut out: Vec<_> = by_token
            .into_iter()
            .map(|(name, set)| (DfaLabel::Token(name), set.into_iter().collect()))
            .collect();
        // Stable order for reproducible codegen.
        out.sort_by_key(|(label, _)| label_key(label));
        out
    }

    /// Byte / Char alphabet: split overlapping ranges into a disjoint
    /// partition, then union the targets covering each piece.
    fn range_moves(&self, nfa_set: &[NfaStateId]) -> Vec<(DfaLabel, Vec<NfaStateId>)> {
        // Collect every (low, high, target) consuming edge.
        let mut edges: Vec<(u32, u32, NfaStateId)> = Vec::new();
        for &s in nfa_set {
            for t in &self.nfa.states[s].transitions {
                if let Some((lo, hi)) = scalar_range(&t.on) {
                    edges.push((lo, hi, t.to));
                }
            }
        }
        if edges.is_empty() {
            return Vec::new();
        }

        // Boundary points that split the line into disjoint intervals.
        let mut bounds: BTreeSet<u32> = BTreeSet::new();
        for &(lo, hi, _) in &edges {
            bounds.insert(lo);
            // The point just past `hi` starts a new interval (guard the
            // 0xFFFF_FFFF edge, which never occurs for our universes).
            bounds.insert(hi.saturating_add(1));
        }
        let points: Vec<u32> = bounds.into_iter().collect();

        // For each disjoint interval [points[i], points[i+1]-1], gather the
        // targets of every edge covering it.
        let mut out: Vec<(DfaLabel, Vec<NfaStateId>)> = Vec::new();
        for win in points.windows(2) {
            let lo = win[0];
            let hi = win[1] - 1;
            let mut targets: BTreeSet<NfaStateId> = BTreeSet::new();
            for &(elo, ehi, to) in &edges {
                if elo <= lo && hi <= ehi {
                    targets.insert(to);
                }
            }
            if targets.is_empty() {
                continue;
            }
            out.push((self.scalar_label(lo, hi), targets.into_iter().collect()));
        }
        out
    }

    fn scalar_label(&self, lo: u32, hi: u32) -> DfaLabel {
        match self.nfa.alphabet {
            Alphabet::Bytes => {
                if lo == hi {
                    DfaLabel::Byte(lo as u8)
                } else {
                    DfaLabel::ByteRange {
                        low: lo as u8,
                        high: hi as u8,
                    }
                }
            }
            _ => {
                let l = char::from_u32(lo).unwrap_or('\u{FFFD}');
                let h = char::from_u32(hi).unwrap_or('\u{FFFD}');
                if l == h {
                    DfaLabel::CodePoint(l)
                } else {
                    DfaLabel::CodePointRange { low: l, high: h }
                }
            }
        }
    }
}

/// The scalar `(low, high)` an input-consuming label covers, or `None`
/// for ε / anchor / token labels.
fn scalar_range(label: &TransitionLabel) -> Option<(u32, u32)> {
    match label {
        TransitionLabel::Byte(b) => Some((*b as u32, *b as u32)),
        TransitionLabel::ByteRange { low, high } => Some((*low as u32, *high as u32)),
        TransitionLabel::CodePoint(c) => Some((*c as u32, *c as u32)),
        TransitionLabel::CodePointRange { low, high } => Some((*low as u32, *high as u32)),
        TransitionLabel::Token(_) | TransitionLabel::Epsilon | TransitionLabel::Anchor(_) => None,
    }
}

/// A total order key for deterministic transition ordering.
fn label_key(label: &DfaLabel) -> (u8, u32, u32, String) {
    match label {
        DfaLabel::Byte(b) => (0, *b as u32, *b as u32, String::new()),
        DfaLabel::ByteRange { low, high } => (0, *low as u32, *high as u32, String::new()),
        DfaLabel::CodePoint(c) => (1, *c as u32, *c as u32, String::new()),
        DfaLabel::CodePointRange { low, high } => (1, *low as u32, *high as u32, String::new()),
        DfaLabel::Token(t) => (2, 0, 0, t.clone()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frame_c::compiler::fsm_regex::{parser, thompson};

    fn dfa(src: &str) -> Dfa {
        let ast = parser::parse(src, Alphabet::Bytes).expect("parse");
        let nfa = thompson::build(&ast, Alphabet::Bytes);
        construct(&nfa)
    }

    /// Run the DFA over input bytes; return whether it ends in an accept
    /// state having consumed all input (full-match semantics for tests).
    fn full_match(dfa: &Dfa, input: &[u8]) -> bool {
        let mut cur = dfa.start;
        for &b in input {
            let next = dfa.states[cur].transitions.iter().find_map(|t| {
                let hit = match &t.label {
                    DfaLabel::Byte(x) => *x == b,
                    DfaLabel::ByteRange { low, high } => *low <= b && b <= *high,
                    _ => false,
                };
                hit.then_some(t.to)
            });
            match next {
                Some(n) => cur = n,
                None => return false,
            }
        }
        dfa.states[cur].is_accept
    }

    #[test]
    fn single_literal() {
        let d = dfa("a");
        assert!(full_match(&d, b"a"));
        assert!(!full_match(&d, b"b"));
        assert!(!full_match(&d, b"aa"));
        assert!(!full_match(&d, b""));
    }

    #[test]
    fn concat() {
        let d = dfa("abc");
        assert!(full_match(&d, b"abc"));
        assert!(!full_match(&d, b"ab"));
        assert!(!full_match(&d, b"abcd"));
    }

    #[test]
    fn alternation() {
        let d = dfa("cat|dog");
        assert!(full_match(&d, b"cat"));
        assert!(full_match(&d, b"dog"));
        assert!(!full_match(&d, b"cot"));
    }

    #[test]
    fn star() {
        let d = dfa("a*");
        assert!(full_match(&d, b""));
        assert!(full_match(&d, b"a"));
        assert!(full_match(&d, b"aaaa"));
        assert!(!full_match(&d, b"b"));
    }

    #[test]
    fn plus() {
        let d = dfa("a+");
        assert!(!full_match(&d, b""));
        assert!(full_match(&d, b"a"));
        assert!(full_match(&d, b"aaa"));
    }

    #[test]
    fn digit_class_plus() {
        let d = dfa("[0-9]+");
        assert!(full_match(&d, b"0"));
        assert!(full_match(&d, b"12345"));
        assert!(!full_match(&d, b""));
        assert!(!full_match(&d, b"12a"));
    }

    #[test]
    fn overlapping_ranges_partition_correctly() {
        // `[a-z]|c` — overlapping; `c` must still be accepted, and so must
        // any other letter.
        let d = dfa("[a-m]x|cy");
        assert!(full_match(&d, b"ax"));
        assert!(full_match(&d, b"cx")); // c is in [a-m]
        assert!(full_match(&d, b"cy")); // c via the literal branch
        assert!(!full_match(&d, b"zx")); // z not in [a-m]
    }

    #[test]
    fn bounded_repeat() {
        let d = dfa("a{2,3}");
        assert!(!full_match(&d, b"a"));
        assert!(full_match(&d, b"aa"));
        assert!(full_match(&d, b"aaa"));
        assert!(!full_match(&d, b"aaaa"));
    }

    #[test]
    fn determinism_one_transition_per_disjoint_label() {
        // After subset construction every state's transitions cover
        // disjoint input — verify no two byte-transitions overlap.
        let d = dfa("[a-z]+|[0-9]+");
        for st in &d.states {
            let ranges: Vec<(u8, u8)> = st
                .transitions
                .iter()
                .filter_map(|t| match &t.label {
                    DfaLabel::Byte(b) => Some((*b, *b)),
                    DfaLabel::ByteRange { low, high } => Some((*low, *high)),
                    _ => None,
                })
                .collect();
            for (i, a) in ranges.iter().enumerate() {
                for b in &ranges[i + 1..] {
                    assert!(
                        a.1 < b.0 || b.1 < a.0,
                        "overlapping transitions {a:?} and {b:?} in state {}",
                        st.id
                    );
                }
            }
        }
    }

    #[test]
    fn anchor_detection() {
        let ast = parser::parse("^a", Alphabet::Bytes).unwrap();
        let nfa = thompson::build(&ast, Alphabet::Bytes);
        assert!(nfa_has_anchors(&nfa));
        let ast2 = parser::parse("a", Alphabet::Bytes).unwrap();
        let nfa2 = thompson::build(&ast2, Alphabet::Bytes);
        assert!(!nfa_has_anchors(&nfa2));
    }
}
