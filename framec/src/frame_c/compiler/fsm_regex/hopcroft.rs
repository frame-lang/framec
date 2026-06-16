//! DFA minimization.
//!
//! Partition-refinement (Moore's algorithm) over a global disjoint
//! partition of the input alphabet. Splits the state set into
//! distinguishable equivalence classes; the resulting DFA has the
//! minimum number of states recognizing the same language. RFC-0042
//! §6.9 step 4.
//!
//! Range DFAs don't share a common finite symbol set, so we first derive
//! one: every transition range across the whole DFA contributes boundary
//! points, yielding a set of disjoint *symbol classes*. Two states are
//! equivalent iff they agree on acceptance and, for every symbol class,
//! transition to equivalent states (a missing transition goes to an
//! implicit dead block). After refinement converges we coalesce adjacent
//! symbol classes that share a target block back into ranges.

use super::subset::{Dfa, DfaLabel, DfaState, DfaStateId, DfaTransition};
use std::collections::BTreeSet;

/// A symbol class used during refinement: a disjoint scalar interval or a
/// single token name.
#[derive(Debug, Clone, PartialEq, Eq)]
enum SymClass {
    Range { low: u32, high: u32 },
    Token(String),
}

/// Sentinel "block" for a missing (dead) transition target.
const DEAD: usize = usize::MAX;

/// Minimize a DFA. Returns a new DFA recognizing the same language with
/// the minimum possible state count. State IDs are renumbered.
pub fn minimize(dfa: &Dfa) -> Dfa {
    if dfa.states.is_empty() {
        return dfa.clone();
    }

    let classes = symbol_classes(dfa);

    // target(state, class) → Some(DfaStateId) or None (dead).
    let target = |s: DfaStateId, class: &SymClass| -> Option<DfaStateId> {
        for t in &dfa.states[s].transitions {
            if label_covers(&t.label, class) {
                return Some(t.to);
            }
        }
        None
    };

    // Initial partition: accept vs non-accept.
    let mut block: Vec<usize> = dfa
        .states
        .iter()
        .map(|s| usize::from(s.is_accept))
        .collect();

    // Refine until stable.
    loop {
        // Signature of each state under the current partition.
        let mut sig: Vec<(usize, Vec<usize>)> = Vec::with_capacity(dfa.states.len());
        for s in 0..dfa.states.len() {
            let row: Vec<usize> = classes
                .iter()
                .map(|c| target(s, c).map(|t| block[t]).unwrap_or(DEAD))
                .collect();
            sig.push((block[s], row));
        }

        // Assign a fresh block id per distinct signature.
        let mut new_id: Vec<usize> = vec![0; dfa.states.len()];
        let mut seen: Vec<(usize, Vec<usize>)> = Vec::new();
        for s in 0..dfa.states.len() {
            let id = match seen.iter().position(|x| *x == sig[s]) {
                Some(i) => i,
                None => {
                    seen.push(sig[s].clone());
                    seen.len() - 1
                }
            };
            new_id[s] = id;
        }

        if new_id == block {
            break;
        }
        block = new_id;
    }

    rebuild(dfa, &block, &classes)
}

/// Build the minimized DFA from the converged partition.
fn rebuild(dfa: &Dfa, block: &[usize], classes: &[SymClass]) -> Dfa {
    let num_blocks = block.iter().copied().max().map(|m| m + 1).unwrap_or(0);

    // A representative original state for each block.
    let mut rep: Vec<Option<DfaStateId>> = vec![None; num_blocks];
    for (s, &b) in block.iter().enumerate() {
        rep[b].get_or_insert(s);
    }

    let target = |s: DfaStateId, class: &SymClass| -> Option<DfaStateId> {
        for t in &dfa.states[s].transitions {
            if label_covers(&t.label, class) {
                return Some(t.to);
            }
        }
        None
    };

    let mut states: Vec<DfaState> = Vec::with_capacity(num_blocks);
    for b in 0..num_blocks {
        let r = rep[b].expect("every block has a representative");

        // Per-class target block; coalesce adjacent ranges sharing a block.
        let mut transitions: Vec<DfaTransition> = Vec::new();
        let mut pending: Option<(u32, u32, usize)> = None; // (low, high, to_block)

        let mut flush = |pending: &mut Option<(u32, u32, usize)>,
                         transitions: &mut Vec<DfaTransition>| {
            if let Some((low, high, to)) = pending.take() {
                transitions.push(DfaTransition {
                    label: range_label(dfa, low, high),
                    to,
                    assertions: Vec::new(),
                });
            }
        };

        for c in classes {
            match c {
                SymClass::Range { low, high } => match target(r, c).map(|t| block[t]) {
                    None => flush(&mut pending, &mut transitions),
                    Some(to) => match pending {
                        Some((plow, phigh, pto)) if pto == to && phigh + 1 == *low => {
                            pending = Some((plow, *high, pto));
                        }
                        _ => {
                            flush(&mut pending, &mut transitions);
                            pending = Some((*low, *high, to));
                        }
                    },
                },
                SymClass::Token(name) => {
                    flush(&mut pending, &mut transitions);
                    if let Some(to) = target(r, c).map(|t| block[t]) {
                        transitions.push(DfaTransition {
                            label: DfaLabel::Token(name.clone()),
                            to,
                            assertions: Vec::new(),
                        });
                    }
                }
            }
        }
        flush(&mut pending, &mut transitions);

        states.push(DfaState {
            id: b,
            transitions,
            is_accept: dfa.states[r].is_accept,
        });
    }

    Dfa {
        states,
        start: block[dfa.start],
        alphabet: dfa.alphabet,
    }
}

/// Build a single-or-range label for `low..=high` in the DFA's alphabet.
fn range_label(dfa: &Dfa, low: u32, high: u32) -> DfaLabel {
    match dfa.alphabet {
        super::Alphabet::Bytes => {
            if low == high {
                DfaLabel::Byte(low as u8)
            } else {
                DfaLabel::ByteRange {
                    low: low as u8,
                    high: high as u8,
                }
            }
        }
        _ => {
            let l = char::from_u32(low).unwrap_or('\u{FFFD}');
            let h = char::from_u32(high).unwrap_or('\u{FFFD}');
            if l == h {
                DfaLabel::CodePoint(l)
            } else {
                DfaLabel::CodePointRange { low: l, high: h }
            }
        }
    }
}

/// Derive the global disjoint symbol classes from every transition.
fn symbol_classes(dfa: &Dfa) -> Vec<SymClass> {
    let mut bounds: BTreeSet<u32> = BTreeSet::new();
    let mut tokens: BTreeSet<String> = BTreeSet::new();
    for st in &dfa.states {
        for t in &st.transitions {
            match &t.label {
                DfaLabel::Byte(b) => {
                    bounds.insert(*b as u32);
                    bounds.insert(*b as u32 + 1);
                }
                DfaLabel::ByteRange { low, high } => {
                    bounds.insert(*low as u32);
                    bounds.insert(*high as u32 + 1);
                }
                DfaLabel::CodePoint(c) => {
                    bounds.insert(*c as u32);
                    bounds.insert(*c as u32 + 1);
                }
                DfaLabel::CodePointRange { low, high } => {
                    bounds.insert(*low as u32);
                    bounds.insert(*high as u32 + 1);
                }
                DfaLabel::Token(name) => {
                    tokens.insert(name.clone());
                }
            }
        }
    }

    let points: Vec<u32> = bounds.into_iter().collect();
    let mut classes: Vec<SymClass> = points
        .windows(2)
        .map(|w| SymClass::Range {
            low: w[0],
            high: w[1] - 1,
        })
        .collect();
    classes.extend(tokens.into_iter().map(SymClass::Token));
    classes
}

/// Does a transition label cover an entire symbol class?
fn label_covers(label: &DfaLabel, class: &SymClass) -> bool {
    match (label, class) {
        (DfaLabel::Byte(b), SymClass::Range { low, high }) => {
            (*b as u32) <= *low && *high <= (*b as u32)
        }
        (DfaLabel::ByteRange { low: l, high: h }, SymClass::Range { low, high }) => {
            (*l as u32) <= *low && *high <= (*h as u32)
        }
        (DfaLabel::CodePoint(c), SymClass::Range { low, high }) => {
            (*c as u32) <= *low && *high <= (*c as u32)
        }
        (DfaLabel::CodePointRange { low: l, high: h }, SymClass::Range { low, high }) => {
            (*l as u32) <= *low && *high <= (*h as u32)
        }
        (DfaLabel::Token(a), SymClass::Token(b)) => a == b,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frame_c::compiler::fsm_regex::{parser, subset, thompson, Alphabet};

    fn minimal(src: &str) -> Dfa {
        let ast = parser::parse(src, Alphabet::Bytes).expect("parse");
        let nfa = thompson::build(&ast, Alphabet::Bytes);
        minimize(&subset::construct(&nfa))
    }

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
    fn language_preserved_after_minimization() {
        let d = minimal("[0-9]+");
        assert!(full_match(&d, b"7"));
        assert!(full_match(&d, b"12345"));
        assert!(!full_match(&d, b""));
        assert!(!full_match(&d, b"1a"));
    }

    #[test]
    fn alternation_language_preserved() {
        let d = minimal("cat|dog");
        assert!(full_match(&d, b"cat"));
        assert!(full_match(&d, b"dog"));
        assert!(!full_match(&d, b"cot"));
    }

    #[test]
    fn minimization_collapses_equivalent_states() {
        // `a*` minimizes to a single accepting state with a self-loop.
        let d = minimal("a*");
        assert_eq!(d.states.len(), 1, "a* should minimize to one state");
        assert!(d.states[0].is_accept);
        assert!(full_match(&d, b""));
        assert!(full_match(&d, b"aaaa"));
    }

    #[test]
    fn already_minimal_unchanged_language() {
        let d = minimal("abc");
        assert!(full_match(&d, b"abc"));
        assert!(!full_match(&d, b"ab"));
        // `abc` needs 4 states (after each prefix) — already minimal.
        assert_eq!(d.states.len(), 4);
    }

    #[test]
    fn coalesces_adjacent_ranges_to_one_transition() {
        // `[0-9]` then accept: the minimized start state has a single
        // range transition, not ten byte transitions.
        let d = minimal("[0-9]");
        let byte_like = d.states[d.start]
            .transitions
            .iter()
            .filter(|t| matches!(t.label, DfaLabel::Byte(_) | DfaLabel::ByteRange { .. }))
            .count();
        assert_eq!(byte_like, 1);
    }

    #[test]
    fn minimal_is_at_most_subset_size() {
        let ast = parser::parse("(ab|ac)*", Alphabet::Bytes).unwrap();
        let nfa = thompson::build(&ast, Alphabet::Bytes);
        let unmin = subset::construct(&nfa);
        let min = minimize(&unmin);
        assert!(min.states.len() <= unmin.states.len());
        assert!(full_match(&min, b"abacab"));
        assert!(!full_match(&min, b"aba"));
    }
}
