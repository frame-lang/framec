//! Pike VM (priority NFA simulation) for `@@fsm` regex stages that contain a
//! lazy quantifier (RFC-0042 §11.1).
//!
//! A greedy-only stage compiles to a pure DFA (`super::subset`/`hopcroft`) and
//! takes the longest match. A *lazy* quantifier needs per-quantifier match-end
//! preference — leftmost-first / Perl semantics, where `a*?` is minimal but a
//! greedy `b+` after it is still maximal — which a single longest-match DFA
//! cannot express. Such stages instead compile to a **priority-ordered NFA
//! program** run by a small backtrack-free Pike VM. In a `Split`, the *first*
//! target is higher priority: a greedy quantifier prefers "repeat", a lazy one
//! prefers "exit", so the highest-priority thread that reaches `Match` wins.
//!
//! The program is anchored at the stage cursor (matches are leftmost by
//! construction); [`run`] returns the winning thread's end position. The same
//! program + VM is emitted into every backend (the program is a flat array of
//! integers; the VM is ~40 lines), exactly as the DFA path is.
//!
//! Scope: bytes/char alphabets (scalar element ranges). Token + lazy is gated
//! out earlier (no scalar notion); interior anchors are already rejected and
//! edge anchors are extracted before this runs, so the program is anchor-free.

use super::ast::{Laziness, Literal, QuantifierKind, RegexAst, RegexNode, SpannedNode};
use super::{thompson, Alphabet};

/// One Pike-VM instruction. Execution is sequential; `Char` consumes one input
/// element and falls through to the next instruction, `Split`/`Jmp` redirect.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Inst {
    /// Consume one input element if it lies in any `(lo, hi)` range; fall
    /// through to the next instruction.
    Char(Vec<(u32, u32)>),
    /// Fork: try thread `a` (higher priority) before `b`. Zero-width.
    Split(usize, usize),
    /// Unconditional jump.
    Jmp(usize),
    /// Accept — the match ends at the current position.
    Match,
}

/// A compiled Pike program. Entry point is instruction 0.
pub type Program = Vec<Inst>;

/// Does `ast` contain a lazy quantifier anywhere? Gates routing to the Pike
/// path in `super::compile`.
pub fn contains_lazy(ast: &RegexAst) -> bool {
    fn walk(n: &SpannedNode) -> bool {
        match &n.node {
            RegexNode::Quantifier {
                inner, laziness, ..
            } => *laziness == Laziness::Lazy || walk(inner),
            RegexNode::Group(inner) => walk(inner),
            RegexNode::Concat(items) | RegexNode::Alt(items) => items.iter().any(walk),
            _ => false,
        }
    }
    walk(&ast.root)
}

/// Compile a (boundary-anchor-free) regex AST to a Pike program over the
/// `alphabet`'s scalar element values.
pub fn compile(ast: &RegexAst, alphabet: Alphabet) -> Program {
    let mut c = Compiler {
        prog: Vec::new(),
        alphabet,
    };
    c.emit(&ast.root);
    c.prog.push(Inst::Match);
    c.prog
}

struct Compiler {
    prog: Program,
    alphabet: Alphabet,
}

impl Compiler {
    fn emit(&mut self, node: &SpannedNode) {
        match &node.node {
            RegexNode::Literal(lit) => {
                let v = match lit {
                    Literal::Byte(b) => *b as u32,
                    Literal::CodePoint(c) => *c as u32,
                    // Token + lazy is gated out before compilation.
                    Literal::Token(_) => unreachable!("token literal in a Pike program"),
                };
                self.prog.push(Inst::Char(vec![(v, v)]));
            }
            RegexNode::Class(class) => {
                let ranges = thompson::resolve_class(class, self.alphabet);
                self.prog.push(Inst::Char(ranges));
            }
            RegexNode::Dot => {
                self.prog
                    .push(Inst::Char(thompson::dot_ranges(self.alphabet)));
            }
            RegexNode::Concat(items) => {
                for it in items {
                    self.emit(it);
                }
            }
            RegexNode::Alt(items) => self.emit_alt(items),
            RegexNode::Group(inner) => self.emit(inner),
            RegexNode::Quantifier {
                inner,
                kind,
                laziness,
            } => self.emit_quant(inner, *kind, *laziness == Laziness::Lazy),
            // Empty matches the empty string; anchors never reach the Pike path
            // (edge anchors extracted, interior anchors already rejected).
            RegexNode::Empty | RegexNode::Anchor(_) => {}
            RegexNode::Forbidden(_) => {
                unreachable!("forbidden construct reached Pike compilation")
            }
        }
    }

    /// Alternation as a priority `Split` chain — earlier alternatives win
    /// (RE2 leftmost-first). Each branch jumps to a shared end.
    fn emit_alt(&mut self, items: &[SpannedNode]) {
        if items.is_empty() {
            return;
        }
        if items.len() == 1 {
            self.emit(&items[0]);
            return;
        }
        let mut jmp_ends: Vec<usize> = Vec::new();
        for (i, it) in items.iter().enumerate() {
            let last = i + 1 == items.len();
            if last {
                self.emit(it);
            } else {
                let split = self.push(Inst::Split(0, 0));
                let branch = self.prog.len();
                self.emit(it);
                jmp_ends.push(self.push(Inst::Jmp(0)));
                let next = self.prog.len();
                self.prog[split] = Inst::Split(branch, next);
            }
        }
        let end = self.prog.len();
        for j in jmp_ends {
            self.prog[j] = Inst::Jmp(end);
        }
    }

    fn emit_quant(&mut self, inner: &SpannedNode, kind: QuantifierKind, lazy: bool) {
        match kind {
            QuantifierKind::ZeroOrOne => self.emit_optional(inner, lazy),
            QuantifierKind::ZeroOrMore => self.emit_star(inner, lazy),
            QuantifierKind::OneOrMore => self.emit_plus(inner, lazy),
            QuantifierKind::Exact(n) => {
                for _ in 0..n {
                    self.emit(inner);
                }
            }
            QuantifierKind::AtLeast(n) => {
                for _ in 0..n.saturating_sub(1) {
                    self.emit(inner);
                }
                if n == 0 {
                    self.emit_star(inner, lazy);
                } else {
                    self.emit_plus(inner, lazy);
                }
            }
            QuantifierKind::Bounded { min, max } => {
                for _ in 0..min {
                    self.emit(inner);
                }
                for _ in min..max {
                    self.emit_optional(inner, lazy);
                }
            }
        }
    }

    /// `e?` — Split(body, after) greedy; Split(after, body) lazy.
    fn emit_optional(&mut self, inner: &SpannedNode, lazy: bool) {
        let split = self.push(Inst::Split(0, 0));
        let body = self.prog.len();
        self.emit(inner);
        let after = self.prog.len();
        self.prog[split] = if lazy {
            Inst::Split(after, body)
        } else {
            Inst::Split(body, after)
        };
    }

    /// `e*` — loop Split at the top; greedy prefers the body, lazy the exit.
    fn emit_star(&mut self, inner: &SpannedNode, lazy: bool) {
        let split = self.push(Inst::Split(0, 0));
        let body = self.prog.len();
        self.emit(inner);
        self.push(Inst::Jmp(split));
        let after = self.prog.len();
        self.prog[split] = if lazy {
            Inst::Split(after, body)
        } else {
            Inst::Split(body, after)
        };
    }

    /// `e+` — body once, then a Split deciding whether to repeat.
    fn emit_plus(&mut self, inner: &SpannedNode, lazy: bool) {
        let body = self.prog.len();
        self.emit(inner);
        let split = self.push(Inst::Split(0, 0));
        let after = self.prog.len();
        self.prog[split] = if lazy {
            Inst::Split(after, body)
        } else {
            Inst::Split(body, after)
        };
    }

    fn push(&mut self, inst: Inst) -> usize {
        let at = self.prog.len();
        self.prog.push(inst);
        at
    }
}

/// Run `prog` over `input` anchored at `start`, returning the end position of
/// the highest-priority (leftmost-first) match, or `None`. Reference VM; the
/// backends emit a transliteration of this exact algorithm.
pub fn run(prog: &Program, input: &[u32], start: usize) -> Option<usize> {
    let n = input.len();
    let mut clist = ThreadList::new(prog.len());
    let mut nlist = ThreadList::new(prog.len());
    let mut matched: Option<usize> = None;

    add_thread(prog, &mut clist, 0, start);
    let mut pos = start;
    loop {
        let mut i = 0;
        while i < clist.dense.len() {
            let pc = clist.dense[i];
            match &prog[pc] {
                Inst::Char(ranges) => {
                    if pos < n
                        && ranges
                            .iter()
                            .any(|&(lo, hi)| lo <= input[pos] && input[pos] <= hi)
                    {
                        add_thread(prog, &mut nlist, pc + 1, pos + 1);
                    }
                }
                Inst::Match => {
                    matched = Some(pos);
                    // Leftmost-first: threads after this one in `clist` are
                    // lower priority and cannot beat it at this position.
                    break;
                }
                // Split/Jmp are expanded by add_thread (never in `dense`).
                Inst::Split(..) | Inst::Jmp(_) => {}
            }
            i += 1;
        }
        if pos >= n {
            break;
        }
        pos += 1;
        std::mem::swap(&mut clist, &mut nlist);
        nlist.clear();
    }
    matched
}

/// A priority-ordered, deduplicated set of program counters (Thompson's
/// "dense/sparse" thread list — each pc added at most once per step, in
/// priority order).
struct ThreadList {
    dense: Vec<usize>,
    seen: Vec<bool>,
}

impl ThreadList {
    fn new(prog_len: usize) -> Self {
        Self {
            dense: Vec::new(),
            seen: vec![false; prog_len],
        }
    }
    fn clear(&mut self) {
        self.dense.clear();
        self.seen.iter_mut().for_each(|s| *s = false);
    }
}

/// Add `pc` to `list`, following `Split`/`Jmp` epsilon edges in priority order
/// so the resulting `dense` holds `Char`/`Match` pcs ready to execute.
fn add_thread(prog: &Program, list: &mut ThreadList, pc: usize, pos: usize) {
    if list.seen[pc] {
        return;
    }
    list.seen[pc] = true;
    match &prog[pc] {
        Inst::Jmp(x) => add_thread(prog, list, *x, pos),
        Inst::Split(a, b) => {
            add_thread(prog, list, *a, pos);
            add_thread(prog, list, *b, pos);
        }
        Inst::Char(_) | Inst::Match => list.dense.push(pc),
    }
}

/// Encode a program into two flat `i64` arrays for uniform emission across
/// all backends: `ops` holds 4 ints per instruction — `[opcode, a, b, _]` —
/// and `ranges` holds `lo, hi` pairs. Opcodes: `0` Char (`a` = pair index into
/// `ranges`, `b` = pair count), `1` Split (`a`/`b` = targets, `a` higher
/// priority), `2` Jmp (`a` = target), `3` Match. The emitted Pike VM is a
/// transliteration of [`run_encoded`] over these two arrays.
pub fn encode(prog: &Program) -> (Vec<i64>, Vec<i64>) {
    let mut ops: Vec<i64> = Vec::new();
    let mut ranges: Vec<i64> = Vec::new();
    for inst in prog {
        match inst {
            Inst::Char(rs) => {
                let start = (ranges.len() / 2) as i64;
                for &(lo, hi) in rs {
                    ranges.push(lo as i64);
                    ranges.push(hi as i64);
                }
                ops.extend([0, start, rs.len() as i64, 0]);
            }
            Inst::Split(a, b) => ops.extend([1, *a as i64, *b as i64, 0]),
            Inst::Jmp(x) => ops.extend([2, *x as i64, 0, 0]),
            Inst::Match => ops.extend([3, 0, 0, 0]),
        }
    }
    (ops, ranges)
}

/// Reference VM over the [`encode`]d arrays — the exact algorithm each backend
/// emits. Returns the leftmost-first match end from `start`, or `-1`.
pub fn run_encoded(ops: &[i64], ranges: &[i64], input: &[u32], start: usize) -> i64 {
    let ninst = ops.len() / 4;
    fn add(ops: &[i64], pc: usize, list: &mut Vec<usize>, seen: &mut [bool]) {
        if seen[pc] {
            return;
        }
        seen[pc] = true;
        match ops[pc * 4] {
            2 => add(ops, ops[pc * 4 + 1] as usize, list, seen),
            1 => {
                add(ops, ops[pc * 4 + 1] as usize, list, seen);
                add(ops, ops[pc * 4 + 2] as usize, list, seen);
            }
            _ => list.push(pc),
        }
    }
    let n = input.len();
    let mut clist: Vec<usize> = Vec::new();
    let mut cseen = vec![false; ninst];
    add(ops, 0, &mut clist, &mut cseen);
    let mut matched: i64 = -1;
    let mut pos = start;
    loop {
        let mut nlist: Vec<usize> = Vec::new();
        let mut nseen = vec![false; ninst];
        for &pc in &clist {
            match ops[pc * 4] {
                0 if pos < n => {
                    let v = input[pos];
                    let (rs, rc) = (ops[pc * 4 + 1] as usize, ops[pc * 4 + 2] as usize);
                    for k in 0..rc {
                        let lo = ranges[(rs + k) * 2] as u32;
                        let hi = ranges[(rs + k) * 2 + 1] as u32;
                        if lo <= v && v <= hi {
                            add(ops, pc + 1, &mut nlist, &mut nseen);
                            break;
                        }
                    }
                }
                3 => {
                    matched = pos as i64;
                    break;
                }
                _ => {}
            }
        }
        if pos >= n {
            break;
        }
        pos += 1;
        clist = nlist;
    }
    matched
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frame_c::compiler::fsm_regex::parser;

    fn prog(src: &str) -> Program {
        let ast = parser::parse(src, Alphabet::Bytes).expect("parse");
        compile(&ast, Alphabet::Bytes)
    }

    fn matched(src: &str, input: &str) -> Option<usize> {
        let bytes: Vec<u32> = input.bytes().map(|b| b as u32).collect();
        run(&prog(src), &bytes, 0)
    }

    #[test]
    fn greedy_star_is_longest() {
        // `a*` over "aaa" → longest (3).
        assert_eq!(matched("a*", "aaa"), Some(3));
        assert_eq!(matched("a*", "bbb"), Some(0)); // zero-width match
    }

    #[test]
    fn lazy_star_is_shortest() {
        // `a*?` prefers the empty match.
        assert_eq!(matched("a*?", "aaa"), Some(0));
    }

    #[test]
    fn lazy_to_first_delimiter() {
        // `.*?,` stops at the FIRST comma; greedy `.*,` would take the last.
        assert_eq!(matched(".*?,", "ab,cd,ef"), Some(3)); // "ab,"
        assert_eq!(matched(".*,", "ab,cd,ef"), Some(6)); // greedy: "ab,cd,"
    }

    #[test]
    fn mixed_lazy_then_greedy() {
        // `a*?b+` on "aabbb": a*? is minimal (must take both a's so b+ can
        // start), b+ is greedy (takes all three b's) → "aabbb" (5), NOT the
        // leftmost-shortest "aab" (3).
        assert_eq!(matched("a*?b+", "aabbb"), Some(5));
    }

    #[test]
    fn lazy_plus() {
        // `a+?` matches one (minimal), not all.
        assert_eq!(matched("a+?", "aaa"), Some(1));
        assert_eq!(matched("a+?", "b"), None);
    }

    #[test]
    fn lazy_optional_and_bounded() {
        // `a??` prefers empty; `a{2,4}?` takes the minimum 2.
        assert_eq!(matched("a??", "a"), Some(0));
        assert_eq!(matched("a{2,4}?", "aaaa"), Some(2));
    }

    #[test]
    fn alternation_leftmost_first() {
        // `a|ab` — leftmost alternative wins even when shorter.
        assert_eq!(matched("a|ab", "ab"), Some(1));
        assert_eq!(matched("ab|a", "ab"), Some(2));
    }

    /// The flat-int [`encode`]d VM agrees with the reference [`run`] — this is
    /// the form every backend emits, so they inherit the same semantics.
    #[test]
    fn encoded_vm_matches_reference() {
        for (src, input) in [
            ("a*?", "aaa"),
            (".*?,", "ab,cd,ef"),
            ("a*?b+", "aabbb"),
            ("a+?", "aaa"),
            ("a{2,4}?", "aaaa"),
            ("ab|a", "ab"),
            ("[0-9]*?x", "12x34"),
        ] {
            let p = prog(src);
            let (ops, ranges) = encode(&p);
            let bytes: Vec<u32> = input.bytes().map(|b| b as u32).collect();
            let want = run(&p, &bytes, 0).map(|m| m as i64).unwrap_or(-1);
            assert_eq!(
                run_encoded(&ops, &ranges, &bytes, 0),
                want,
                "src {src:?} input {input:?}"
            );
        }
    }
}
