---
title: "Recognition Recipes"
parent: Guides
nav_order: 1
---

# Recognition Recipes

*Situations in scanning and parsing, and the machines that resolve them.*

Every recipe here was paid for — each records a situation this project (or
the wider industry) actually hit, the options that exist, and the ruling
Frame's compiler follows. The format is fixed:

- **Fingerprint** — how to recognize you are in this situation. Read these
  first; the most expensive failures come from not noticing the situation,
  not from choosing badly once it's noticed.
- **Options** — the known resolutions, with trade-offs.
- **Precedent** — how established languages and tools handle it.
- **Ruling** — Frame's standing decision, where one exists.

One meta-rule above all of them, inherited from
[*Shadows on the Wall*](../articles/Shadows_on_the_Wall.md): a hard
recognition situation is never a reason to rule "not a machine problem" and
leave. Ambiguity, lookahead, and context-sensitivity are situations with
*more* machine structure, not less — forks, marks, adjudicators, and
registers. The recipes below are that structure, catalogued.

---

## 1. The sometimes-bracket

**Fingerprint.** A token pair that is *usually* brackets but *sometimes*
two unrelated operators — classically `<` and `>`, which are generic-type
delimiters in one reading and comparison operators in another. Symptom in
code: a depth counter whose alphabet includes `<`/`>`, or bug reports where
argument lists split in the wrong places.

**Options.**

1. *Always brackets* — count them unconditionally. Breaks every bare
   comparison (`f(a < b, c)` becomes one mangled argument; a `>=` drives
   the counter negative and silences everything after it).
2. *Never brackets* — ignore them. Breaks every unparenthesized generic
   (`new HashMap<Integer, String>()` splits at the interior comma).
3. *Local look-ahead* — classify each `<` by the token that follows
   (the C# specification's rule). Mostly right; carries a documented
   wrong case, and imports a guess into the scanner.
4. *Fork and adjudicate* — scan once with **two depth counters** (one per
   hypothesis); if their top-level comma sets agree, there was never an
   ambiguity; if they differ, emit both candidate splits as an explicit
   fork and let a downstream consumer with **semantic knowledge** pick.
   See recipe 7 for the general technique.

**Precedent.** C++'s template-`>` rules, Java's parser-level look-ahead,
C#'s spec-defined disambiguation, and Rust's turbofish (`::<>`) — a piece
of syntax that exists *entirely* because of this recipe.

**Ruling.** Frame instantiation arguments use **fork-and-adjudicate**: the
adjudicator is the target system's *declared parameter arity* — knowledge
Frame has in its own language, with no host-language name lookup. The two
candidate splits always differ in count and declared arity is fixed, so a
tie is impossible; if neither matches, the diagnostic shows both readings.
The bound is binary per argument list (all angles brackets, or none —
never per-occurrence speculation); mixed lists fail loudly and the escape
hatch is parenthesization, which hides an operator angle from the fork.
Transition arguments currently refuse to split on bare angles; they are
slated to converge on the same mechanism.

**Example.** Two depth-counter hypotheses scan in lockstep: `bracket`
counts commas only at angle-depth zero (angles are brackets), `flat`
counts them all (angles are operators). The adjudicator is declared
arity — knowledge the machine never guesses at. For `a<b, c>d, e` the
readings split into 2 and 3 arguments; arity 2 picks the bracket
reading, and a no-match falls to the loud arm.

```frame
@@[target("python_3")]

@@system AngleFork {
    interface:
        feed(ch: str)
        adjudicate(arity: int): str

    machine:
        $Scan {
            feed(ch: str) {
                if ch == "<":
                    self.depth = self.depth + 1
                if ch == ">":
                    self.depth = self.depth - 1
                if ch == ",":
                    self.flat = self.flat + 1
                    if self.depth == 0:
                        self.bracket = self.bracket + 1
            }
            adjudicate(arity: int): str {
                if self.bracket == self.flat:
                    @@:return("unambiguous")
                if arity == self.bracket + 1:
                    @@:return("angles are brackets")
                if arity == self.flat + 1:
                    @@:return("angles are operators")
                @@:return("no reading fits: report both")
            }
        }

    domain:
        depth = 0
        flat = 0
        bracket = 0
}

if __name__ == '__main__':
    m = @@AngleFork()
    for c in "a<b, c>d, e":
        m.feed(c)
    print(m.adjudicate(2))   # angles are brackets
```

## 2. String-blind counting

**Fingerprint.** A bare depth counter (`d += 1` on `(`, `d -= 1` on `)`)
walking text that can contain string literals, comments, or other opaque
regions. Symptom: a delimiter *inside* a string ("a `)` in a default
value, a `{` in a comment") corrupts the count. This is the single most
recurring bug class in this compiler's history.

**Options.** (1) Teach every counter about every string form — duplicated,
drifting string models everywhere. (2) Route all counting through one
shared balanced-delimiter machine that skips opaque regions via the one
shared opacity model.

**Ruling.** Option 2, always: counting composes the shared machines; no
private counter touches user text. Any bare counter near user text is a
defect fingerprint, whatever the tests say — the tests were written by
someone who also didn't think of strings.

**Example.** The mode is a Frame state, never a native flag: `$InString`
and `$Escape` are the shared opacity model, so the `)` inside the
literal never reaches the depth register `d`. Reading the register is
not an event — `depth()` is an operation, bypassing the machine.

```frame
@@[target("python_3")]

@@system StringSafeCount {
    operations:
        depth(): int {
            return self.d
        }

    interface:
        feed(ch: str)

    machine:
        $Code {
            feed(ch: str) {
                if ch == '"':
                    -> $InString
                if ch == "(":
                    self.d = self.d + 1
                if ch == ")":
                    self.d = self.d - 1
            }
        }
        $InString {
            feed(ch: str) {
                if ch == "\\":
                    -> $Escape
                if ch == '"':
                    -> $Code
            }
        }
        $Escape {
            feed(ch: str) {
                -> $InString
            }
        }

    domain:
        d = 0
}

if __name__ == '__main__':
    m = @@StringSafeCount()
    for c in 'f("a)b", (g)':
        m.feed(c)
    print(m.depth())   # 1 — the ")" in the string never counted
```

## 3. Islands in native water

**Fingerprint.** A grammar that owns *some* of the text (its own
constructs) embedded in text it must not interpret (a host language). The
temptation: parse everything. The failure: interpreting text you don't
own.

**Options.** Full parse of both languages (fragile, per-host work);
regex-carving (string-blind, recipe 2's disease); or **island grammar** —
recognize your islands precisely, treat everything else as opaque water
you *delimit but never interpret*, with the opacity model (strings,
comments, nesting) as the one piece of host knowledge you allow yourself.

**Ruling.** Island grammar, with the compiler's guarantee stated
positively: native water passes through byte-for-byte verbatim; the
scanner knows only enough host lexicography to avoid being fooled by it.

**Example.** The islands are `@@` markers; everything else is water the
machine delimits but never interprets. The only host knowledge is the
opacity model — the `$InString` state — so a marker inside a string
literal stays water. (In `@@fsm` regexes, `\@` escapes a literal `@`.)

```frame
@@[target("python_3")]

@@fsm IslandScan(src: bytes) : int = 0 {
    $Water:
        /\z/ self.islands
      | /\@\@/ { self.islands = self.islands + 1 } -> $Water
      | /"/ -> $InString
      | /[^"\@]/ -> $Water
      | /\@/ -> $Water
    $InString:
        /\\./ -> $InString
      | /"/ -> $Water
      | /[^"\\]/ -> $InString
    domain:
        islands: int = 0
}

if __name__ == '__main__':
    m = @@IslandScan('code @@island "water @@not-an-island" more @@island')
    print(m.accepted, m.return_value)   # True 2
```

## 4. Ordering-sensitive alternation

**Fingerprint.** Multiple recognizers tried in sequence where one's prefix
is another's whole match (`--[[` long comment vs `--` line comment;
longest-match vs first-match). Symptom: correctness silently depends on
table order, often documented only in a comment.

**Ruling.** Order-dependence is a transition rule of the dispatch machine
and MUST be enforced, not trusted: either structurally (longest-first
generation, a decisive first-byte partition) or by an assertion that fails
loudly when the invariant breaks. A comment that *promises* an assertion
which does not exist is worse than no comment — this project found exactly
that, years old.

**Example.** In `@@fsm`, ordered choice `|` is first-match-wins and the
order is structure, not a comment's promise: the `--[[` alternative is
declared — and therefore tried — before its own prefix `--`.

```frame
@@[target("python_3")]

@@fsm CommentKind(src: bytes) : str = "none" {
    /--\[\[/ "long"
  | /--/ "line"
}

if __name__ == '__main__':
    print(@@CommentKind("--[[ block ]]").return_value)   # long
    print(@@CommentKind("-- note").return_value)         # line
    m = @@CommentKind("x = 1")
    print(m.accepted, m.return_value)                    # False none
```

## 5. Tentative end (bounded lookahead)

**Fingerprint.** A boundary that can't be confirmed until later input —
"this newline ends the statement *unless* the next line continues it."
The naive fix reaches for arbitrary lookahead or cursor rewinding.

**Ruling.** Model it as a **mark register**: record the tentative boundary
in a domain variable, keep scanning forward monotonically, and either
promote the mark (boundary confirmed) or discard it (continuation found).
The cursor never moves backward; the "lookahead" is just a state with a
register. This is the standing pattern for line-continuation handling.

**Example.** The register is `mark`, the tentative boundary. The cursor
only moves forward: `$Pending` either promotes the mark (next line
starts flush) or discards it (leading space means continuation), then
forwards the very byte it inspected back to `$Line` with `-> => $Line`
so position tracking stays exact.

```frame
@@[target("python_3")]

@@system MarkRegister {
    interface:
        feed(ch: str)
        ends(): list

    machine:
        $Line {
            feed(ch: str) {
                self.pos = self.pos + 1
                if ch == "\n":
                    self.mark = self.pos - 1
                    -> $Pending
            }
            ends(): list {
                @@:(self.bounds)
            }
        }
        $Pending {
            feed(ch: str) {
                if ch != " ":
                    self.bounds.append(self.mark)
                -> => $Line
            }
            ends(): list {
                self.bounds.append(self.mark)
                @@:(self.bounds)
            }
        }

    domain:
        pos = 0
        mark = -1
        bounds = []
}

if __name__ == '__main__':
    m = @@MarkRegister()
    for c in "ab\n  cd\nef\n":
        m.feed(c)
    print(m.ends())   # [7, 10] — the newline at 2 was a continuation
```

## 6. Context-sensitivity and lexer feedback

**Fingerprint.** Recognition that requires knowing what a *name* means —
C's `(T)(x)` cast-or-call problem, which classic compilers solve by
feeding the symbol table back into the lexer.

**Ruling.** Frame refuses this category entirely: the compiler is
type-ignorant about host code by architectural boundary, so any recipe
requiring host name resolution is out of bounds. The honest alternatives
are recipe 1's escape hatches (syntax the user can add to disambiguate)
and recipe 7's adjudicators drawn from *Frame's own* declarations — never
from host semantics. When none applies, refuse loudly (recipe 8), don't
guess.

**Example.** By architectural boundary no Frame construct may ask what
`T` names, so `(T)(x)` stays locally undecidable — and the honest
machine says so, emitting the ambiguity as a verdict for a downstream
adjudicator (recipe 7) instead of embedding a guess.

```frame
@@[target("python_3")]

@@fsm CastOrCall(src: bytes) : str = "no-parse" {
    /\([A-Za-z_][A-Za-z0-9_]*\) *\(/ "ambiguous: cast-or-call"
  | /\([A-Za-z_][A-Za-z0-9_]*\)/ "parenthesized name"
}

if __name__ == '__main__':
    print(@@CastOrCall("(T)(x)").return_value)    # ambiguous: cast-or-call
    print(@@CastOrCall("(T) + 1").return_value)   # parenthesized name
```

## 7. Bounded speculation — fork and adjudicate

**Fingerprint.** Two legal readings of the same bytes, locally
undecidable, where guessing embeds a heuristic in a scanner. The general
technique behind recipe 1's ruling.

**The shape.** (1) Scan once, tracking every hypothesis in parallel —
usually just extra registers, not extra passes. (2) If all hypotheses
agree on the observable outcome, there was no ambiguity — the common case
must cost nothing. (3) On divergence, emit an **explicit fork** — both
outcomes, as data. (4) Adjudicate downstream where semantic knowledge
lives (declared arity, declared structure — whatever the language itself
knows). (5) **Choose the fork's bound by adjudication decisiveness, not
by simulation cost.** Per-occurrence branching does *not* blow up: threads
that agree on forward-determining state (position, depth, committed
count) merge, so live configurations stay linear even as paths go
exponential — a merged-set driver with witness back-pointers is a
miniature GLR engine. What the general form costs is *decisiveness*: with
several independent ambiguity sites, distinct readings can collide on the
same observable count, so the semantic adjudicator no longer picks
uniquely and a preference rule must be added. A binary hypothesis per
scope keeps the adjudicator provably decisive (two candidates always
differ in count) and the diagnostic two lines. Ship the decisive bound
first; instrument how often its loud failure actually fires; upgrade to
the merged-set form only when reality demands mixed readings. (6) Ties
and no-matches fail loudly, showing every reading; give the user an
escape hatch that makes their intent explicit.

**In automata terms:** this is bounded nondeterminism simulated in
parallel — the NFA/Pike-VM move (advance all live hypotheses in lockstep,
merging threads whose forward behavior coincides) — with existential
acceptance replaced by semantic adjudication. The branches may carry
registers (making them counter machines, not finite automata); the
simulation discipline is the same.

**Precedent.** Clang resolves C++'s declaration/expression ambiguities
(the "most vexing parse") by *tentative parsing* — the sequential cousin
of the parallel fork: speculate down one reading with a rollback guard,
commit or rewind. And the oldest shipped disambiguation rule is the
dangling `else`: every C-family grammar resolves "which `if` owns this
`else`?" by a declared preference (match the nearest) — proof that a
preference rule, stated once and documented, is a legitimate resolver
when semantic adjudication has nothing to grip.

**Why this over a guess:** the fork is honest machinery — a named state of
uncertainty with a named resolver — where a heuristic is a gloss with
good marketing.

**Example.** Tokens arrive as interface calls; the hypotheses are two
registers advanced in lockstep — `a` counts separators at depth zero
only, `b` counts them all. Agreement means the ambiguity never existed
and costs nothing; divergence emits the fork as data. Adjudication
happens downstream in the driver, where declared arity lives.

```frame
@@[target("python_3")]

@@system DualCounterFork {
    interface:
        sep()
        maybe_open()
        maybe_close()
        outcome(): str

    machine:
        $Scan {
            sep() {
                self.b = self.b + 1
                if self.depth == 0:
                    self.a = self.a + 1
            }
            maybe_open() {
                self.depth = self.depth + 1
            }
            maybe_close() {
                self.depth = self.depth - 1
            }
            outcome(): str {
                if self.a == self.b:
                    @@:return("agree: " + str(self.a + 1) + " parts")
                @@:return("fork: A=" + str(self.a + 1) + " parts, B=" + str(self.b + 1) + " parts")
            }
        }

    domain:
        depth = 0
        a = 0
        b = 0
}

if __name__ == '__main__':
    m = @@DualCounterFork()
    m.maybe_open(); m.sep(); m.maybe_close(); m.sep()
    print(m.outcome())   # fork: A=2 parts, B=3 parts
    arity = 2            # the adjudicator: declared arity, known downstream
    print("reading A wins" if m.a + 1 == arity else "reading B wins")
```

## 8. Terminal discipline — a refusal is never a guess

**Fingerprint.** A recognizer that "carries on" past malformed input: an
unterminated string treated as if it closed at end-of-line; an error
return silently converted to "no match here" by an `if let Ok(Some(..))`
at the call site; six failure modes merged into one boolean.

**Ruling.** Every distinct way a recognizer can refuse or fail is a
distinct terminal state and gets a distinct name — as an error variant, or
as a **register** recording the refusal cause when the machine must remain
total. Policies for unterminated input (fail hard vs tolerate-and-clamp)
are legitimate *per consumer* — but each is a stated, parameterized
decision, never an accident of which call site swallowed which error. A
conversion that replaces hand code must carry a **terminal ledger**: every
old exit accounted for, carried or fixed, none dropped silently.

**Example.** Every exit has a name: `closed`, `newline-in-string`,
`not-a-string` — and unterminated-at-EOF is the one refusal that keeps
the declared default with `accepted == False`. No two failure modes
share a boolean.

```frame
@@[target("python_3")]

@@fsm StringEnd(src: bytes) : str = "unterminated" {
    /"/ -> $Body : -> $not_a_string
    $Body:
        /[^"\\\n]+/ -> $Body
      | /\\./ -> $Body
      | /"/ -> $closed
      | /\n/ -> $newline_break
    $closed: "closed"
    $newline_break: "newline-in-string"
    $not_a_string: "not-a-string"
}

if __name__ == '__main__':
    for s in ['"ok"', '"a\\"b"', '"broken\nrest', '"never ends', 'x']:
        m = @@StringEnd(s)
        print(m.accepted, m.return_value)
```

## 9. Recursion placement

**Fingerprint.** Nested structure (arguments containing instantiations
containing arguments) tempting either a recursive machine or a machine
with an unbounded stack bolted on.

**Ruling.** Split by level: **within** one level, a flat machine (counters
and registers — recipe 2's shared machinery); **across** levels, the tree
recursion stays in a thin native driver whose call stack *is* the pushdown
stack — a machine deliberately left latent, with the plea and its void
condition (any need to suspend or resume mid-descent) recorded. Per-level
forks (recipe 7) stay independent when level extents are determined by
unambiguous pairs alone — state the independence argument when you rely
on it.

**Example.** Within one level the machine is flat — a depth register and
a parts counter (a deliberate step beyond regular; the counting composes
recipe 2's discipline). Across levels the native driver recurses, its
call stack serving as the pushdown: here it descends into `g(b, c)` and
reuses the same flat machine.

```frame
@@[target("python_3")]

@@fsm TopSplit(args: bytes) : int = 1 {
    $Scan:
        /\z/ self.parts
      | /\(/ { self.depth = self.depth + 1 } -> $Scan
      | /\)/ { self.depth = self.depth - 1 } -> $Scan
      | /,/  { if self.depth == 0 { self.parts = self.parts + 1 } } -> $Scan
      | /[^(),]/ -> $Scan
    domain:
        depth: int = 0
        parts: int = 1
}

if __name__ == '__main__':
    print(@@TopSplit("a, g(b, c), d").return_value)   # 3 — outer level
    print(@@TopSplit("b, c").return_value)            # 2 — driver descended one level
```

## 10. Layout — the offside rule

**Fingerprint.** Block structure carried by indentation instead of
delimiters (Python, Haskell, YAML). Symptom of doing it wrong: grammar
rules that try to "see" whitespace, or a parser that breaks on tabs.

**Options & precedent.** The industry answer is uniform: a stateful
pre-pass owns layout — an indent-stack machine that synthesizes explicit
`INDENT`/`DEDENT` tokens (Python's tokenizer, Haskell's layout algorithm)
— and the grammar proper consumes those tokens like any others. Layout in
the grammar itself is the anti-pattern.

**Ruling.** Frame's own syntax is delimiter-based by design and stays so.
Where scanning must coexist with layout-significant *host* code, the
opaque model already refuses to interpret host structure; if a future
need ever requires layout awareness, it enters as a synthesizing pre-pass
machine — never as grammar sensitivity to whitespace.

**Example.** The indent stack is the state stack: a deeper line pushes
the current `$Block(w)` compartment and enters a new one; a shallower
line synthesizes `D`, pops with `-> => pop$`, and re-dispatches the same
line event so cascaded dedents unwind one pop at a time. `I` and `D`
stand for the synthesized INDENT/DEDENT tokens.

```frame
@@[target("python_3")]

@@system IndentSynth($(w: int)) {
    interface:
        line(n: int)
        tokens(): str

    machine:
        $Block(w: int) {
            line(n: int) {
                if n > w:
                    self.out = self.out + "I"
                    push$
                    -> $Block(n)
                if n < w:
                    self.out = self.out + "D"
                    -> => pop$
            }
            tokens(): str { @@:(self.out) }
        }

    domain:
        out = ""
}

if __name__ == '__main__':
    m = @@IndentSynth($(0))
    for n in [0, 4, 8, 4, 0]:
        m.line(n)
    print(m.tokens())   # IIDD
```

## 11. Token splitting at the seam

**Fingerprint.** Maximal-munch lexing produces a token the parser needs
in halves — classically `>>` closing two nested generic scopes
(`Vec<Vec<T>>`), lexed as one shift operator.

**Options & precedent.** Re-lex with context (fragile); forbid the input
(C++ once required `> >` with a space); or **split the token at the
consumer** — C++11 changed the standard to mandate it; Java's parser does
it silently. The lexeme stays greedy; the consumer that knows better
splits.

**Ruling.** Frame's angle machinery counts byte-wise with explicit
digraph guards (`<=`, `>=`, `->`, `=>` never touch the depth register),
so a `>>` is two closes by construction — the split is structural rather
than corrective. The general rule stands for future seams: greedy lexeme,
consumer-side split, never a context-sensitive lexer.

**Example.** The digraph guards are ordered alternatives: `=>`, `>=`,
and `->` are consumed whole before a bare `>` can reach the close
register, so `>>` is two closes and `>=` is none — the split is
structural, not corrective.

```frame
@@[target("python_3")]

@@fsm CloseCounter(src: bytes) : int = 0 {
    $Scan:
        /\z/ self.closes
      | /=>/ -> $Scan
      | />=/ -> $Scan
      | /->/ -> $Scan
      | />/ { self.closes = self.closes + 1 } -> $Scan
      | /[^>]/ -> $Scan
    domain:
        closes: int = 0
}

if __name__ == '__main__':
    print(@@CloseCounter("Vec<Vec<T>>").return_value)    # 2 — >> is two closes
    print(@@CloseCounter("a >= b => c").return_value)    # 0 — digraphs guarded
```

## 12. Soft (contextual) keywords

**Fingerprint.** The language needs a new keyword, but reserving the word
breaks every existing program that used it as an identifier.

**Options & precedent.** Hard-reserve and break the world (Python's
`async` migration pain); version the grammar (Rust editions); or **soft
keywords** — lex as an identifier, give it meaning only in positions
where no identifier can appear (C#'s `var`/`await`, Java's `var`,
TypeScript throughout).

**Ruling.** Frame grows in sigil- and attribute-space by preference
(`@@[...]`, `$`, `->`) — syntax that *cannot* collide with user
identifiers, which is the strongest form of this recipe. Where a bare
word must someday become special, it enters as a soft keyword, never a
new reservation.

**Example.** The word `var` is never reserved: the same bytes are a
keyword in declaration position and a plain identifier in value
position. Position gives the word meaning; no existing identifier ever
breaks.

```frame
@@[target("python_3")]

@@fsm SoftKeyword(src: bytes) : str = "no-match" {
    /var +[A-Za-z]/ "keyword (declaration position)"
  | /var *=/ "identifier (value position)"
}

if __name__ == '__main__':
    print(@@SoftKeyword("var x = 1").return_value)   # keyword (declaration position)
    print(@@SoftKeyword("var = 1").return_value)     # identifier (value position)
```

## 13. Interpolation holes and mode stacks

**Fingerprint.** String literals containing full expressions containing
string literals (`f"{x['}'] }"`, template literals) — the lexer's modes
nest, so a flat lexer corrupts or a regex-carved one lies.

**Options & precedent.** Lexer mode *stacks* (ANTLR's pushMode/popMode);
or promote the construct into the parser itself — Python's PEP 701 moved
f-strings out of the tokenizer's hand-rolled special case and into the
grammar, the definitive case study that holes are *code*, not string
content.

**Ruling.** Standing and built: literal content is bytes, holes are code
— the hole's span is delimited by balanced scanning inside the literal
machine, and hole interiors re-enter the full grammar. The mode stack's
depth is the literal-nesting depth, carried where all counting is carried
(the shared balanced-delimiter machinery), never in a private flag.

**Example.** Literal content is bytes (`$Text`); holes are code
(`$Hole`), their spans delimited by balanced scanning inside the literal
machine. The mode stack collapses to its depth — the `depth` register —
carried where all counting is carried, never in a private flag.

```frame
@@[target("python_3")]

@@fsm HoleScan(lit: bytes) : int = 0 {
    /`/ -> $Text : -> $NotLit
    $Text:
        /\$\{/ { self.depth = 1 self.holes = self.holes + 1 } -> $Hole
      | /`/ -> $Done
      | /[^`$]/ -> $Text
    $Hole:
        /\{/ { self.depth = self.depth + 1 } -> $Hole
      | /\}/ { self.depth = self.depth - 1 }
            -> ( $Text when self.depth == 0, $Hole when self.depth > 0 ) : -> $NotLit
      | /[^{}]/ -> $Hole
    $Done: self.holes
    $NotLit: 0
    domain:
        depth: int = 0
        holes: int = 0
}

if __name__ == '__main__':
    m = @@HoleScan("`a ${ {'k': 1} } b ${x}`")
    print(m.accepted, m.return_value)   # True 2 — nested braces stayed in the hole
```

## 14. Close-word fixed at open

**Fingerprint.** The closing delimiter isn't knowable until the opening
one is read: Rust's `r##"..."##`, C++ raw strings' custom delimiter,
shell heredocs, Lua's `[==[...]==]`.

**Options & precedent.** These are **register machines** — capture the
close-word (hash count, tag bytes, equals level) at open time into a
register, then seek a literal match of the register's content. Every
industrial lexer implements exactly this; no finite table can, which is
the point: the construct sits one rung above what a DFA expresses.

**Ruling.** Standing and built (the raw-string and long-bracket
machines): open-time register, register-length close matching, distinct
refusal exits for malformed opens — and the register carried into the
error terminal, so an unterminated construct reports *which* one.

**Example.** The register is `hashes`, captured while the open delimiter
is read (input starts after the `r`); the close is a literal match of
the register's length, restarted on every fresh quote. The refusal exits
are distinct — `$BadOpen` for a malformed open — and the unterminated
verdicts carry the register, reporting exactly which close-word is
still owed.

```frame
@@[target("python_3")]

@@system RawStringScan {
    interface:
        feed(ch: str)
        verdict(): str = "unterminated: still in the opener"

    machine:
        $Open {
            feed(ch: str) {
                if ch == "#":
                    self.hashes = self.hashes + 1
                    return
                if ch == '"':
                    -> $Body
                -> $BadOpen
            }
        }
        $Body {
            feed(ch: str) {
                if ch == '"':
                    self.seen = 0
                    if self.seen == self.hashes:
                        -> $Done
                    -> $Close
            }
            verdict(): str { @@:('unterminated: needs "' + "#" * self.hashes) }
        }
        $Close {
            feed(ch: str) {
                if ch == "#":
                    self.seen = self.seen + 1
                    if self.seen == self.hashes:
                        -> $Done
                    return
                if ch == '"':
                    self.seen = 0
                    return
                -> $Body
            }
            verdict(): str { @@:("unterminated: needs " + "#" * (self.hashes - self.seen)) }
        }
        $Done {
            verdict(): str { @@:("closed") }
        }
        $BadOpen {
            verdict(): str { @@:("malformed open") }
        }

    domain:
        hashes = 0
        seen = 0
}

if __name__ == '__main__':
    for s in ['##"a"#b"##', '##"never "# closes', 'x']:
        m = @@RawStringScan()
        for c in s:
            m.feed(c)
        print(m.verdict())
```

## 15. Error recovery — always produce a tree

**Fingerprint.** A parser that returns nothing on malformed input — fatal
for IDEs, formatters, and diagnostics, which need structure for exactly
the code that's currently broken.

**Options & precedent.** Panic-mode with synchronization tokens (yacc
lineage); error productions; or the modern IDE discipline — **the tree
always exists**, with explicit `Missing` and `Skipped` nodes where input
failed (Roslyn), or typed error nodes spliced into an otherwise-valid
tree (Tree-sitter). Recovery quality is sync-point selection: statement
starts, section heads, close braces.

**Ruling.** The seed is recipe 8: refusals are registers, and the tree
partition owns every byte (malformed spans become named, typed nodes —
never dropped bytes). The full missing/skipped-node discipline binds when
the diagnostics pass lands; nothing may silently "repair" input in the
meantime.

**Example.** The partition owns every byte: clean spans become `item`
nodes, malformed spans become typed `skipped` nodes with their bytes
kept, and an empty slot is an explicit `missing` node. Malformed input
still produces structure — nothing is silently repaired or dropped.

```frame
@@[target("python_3")]

@@system TotalPartition {
    interface:
        feed(ch: str)
        end(): list

    machine:
        $Clean {
            feed(ch: str) {
                if ch == ",":
                    self.close("item")
                    return
                self.buf = self.buf + ch
                if not ch.isalnum():
                    -> $Dirty
            }
            end(): list {
                self.close("item")
                @@:(self.nodes)
            }
        }
        $Dirty {
            feed(ch: str) {
                if ch == ",":
                    self.close("skipped")
                    -> $Clean
                self.buf = self.buf + ch
            }
            end(): list {
                self.close("skipped")
                @@:(self.nodes)
            }
        }

    actions:
        close(kind) {
            if self.buf == "":
                self.nodes.append(("missing", ""))
            else:
                self.nodes.append((kind, self.buf))
            self.buf = ""
        }

    domain:
        buf = ""
        nodes = []
}

if __name__ == '__main__':
    m = @@TotalPartition()
    for c in "ab,!x,,c":
        m.feed(c)
    print(m.end())
    # [('item', 'ab'), ('skipped', '!x'), ('missing', ''), ('item', 'c')]
```

## 16. Incremental reparse

**Fingerprint.** Re-lexing the whole file per keystroke; editor tooling
that lags on large files.

**Options & precedent.** Tree-sitter reuses unchanged subtrees across
edits (its GLR core plus node reuse); Roslyn's red-green trees separate
position-independent structure (green: widths only) from positioned
facades (red), so an edit invalidates a spine, not a tree.

**Ruling.** No standing ruling — recorded as a *design constraint to
weigh now*: trees whose nodes carry absolute offsets resist reuse;
width-based (position-independent) interiors enable it. The cost of
retrofitting is famously high, so the choice is made consciously at tree
design time, not discovered at LSP time.

**Example.** There is no machine to rule on here — this recipe is a
tree-design constraint, so the example teaches the constraint rather
than a recognizer: the scanner emits width-only ("green") tokens, and
the driver re-derives offsets from any base. An edit upstream
invalidates positions, never the nodes.

```frame
@@[target("python_3")]

@@system WidthTokens {
    interface:
        feed(ch: str)
        end(): list

    machine:
        $Start {
            feed(ch: str) {
                self.w = 1
                if ch == " ":
                    -> $Gap
                -> $Word
            }
            end(): list { @@:(self.nodes) }
        }
        $Word {
            feed(ch: str) {
                if ch == " ":
                    self.nodes.append(("word", self.w))
                    self.w = 1
                    -> $Gap
                self.w = self.w + 1
            }
            end(): list {
                self.nodes.append(("word", self.w))
                @@:(self.nodes)
            }
        }
        $Gap {
            feed(ch: str) {
                if ch != " ":
                    self.nodes.append(("gap", self.w))
                    self.w = 1
                    -> $Word
                self.w = self.w + 1
            }
            end(): list {
                self.nodes.append(("gap", self.w))
                @@:(self.nodes)
            }
        }

    domain:
        w = 0
        nodes = []
}

if __name__ == '__main__':
    def offsets(base, toks):
        out = []
        for kind, w in toks:
            out.append((kind, base, w))
            base += w
        return out
    m = @@WidthTokens()
    for c in "let x":
        m.feed(c)
    green = m.end()
    print(green)             # [('word', 3), ('gap', 1), ('word', 1)] — widths only
    print(offsets(0, green))
    print(offsets(100, green))  # same green nodes, reused after an upstream edit
```

## 17. Resumable scanning

**Fingerprint.** Input arriving in pieces — network chunks, streams,
editor pipes — and a scanner that can only run start-to-finish.

**Options & precedent.** Reify the machine's state so scanning suspends
at a chunk boundary and resumes on the next: llhttp (Node's HTTP parser)
is literally a generated explicit-state machine for this reason;
simdjson's two-stage design separates structural indexing from parsing.
This is the foundational paper's observability payoff made industrial —
a program counter can't be suspended; a named state can.

**Ruling.** Generated recognizers already expose the resumable shape
(`over(...)` binding input and config, `scan_at(...)` advancing from a
cursor); the pluggable input-source design extends the same machines over
owned buffers, borrowed slices, and callbacks. New recognizers MUST NOT
assume whole-input availability in their design even when today's caller
has it.

**Example.** The named state is what survives the chunk boundary — the
CRLF split across two chunks below is no special case. One honest note:
a release `@@fsm` is construction-driven (it runs its whole input at
construction), so the resumable form in the release dialect is the
event-driven system; suspension is simply the space between interface
calls.

```frame
@@[target("python_3")]

@@system ResumableScan {
    operations:
        lines(): int {
            return self.n
        }

    interface:
        feed(ch: str)

    machine:
        $Text {
            feed(ch: str) {
                if ch == "\r":
                    -> $SawCR
            }
        }
        $SawCR {
            feed(ch: str) {
                if ch == "\n":
                    self.n = self.n + 1
                    -> $Text
                if ch != "\r":
                    -> $Text
            }
        }

    domain:
        n = 0
}

if __name__ == '__main__':
    m = @@ResumableScan()
    for chunk in ["one\r", "\ntwo\r\n", "tail"]:
        for c in chunk:
            m.feed(c)
    print(m.lines())   # 2 — the \r\n split across chunks still counted
```

## 18. Linear-time pattern matching

**Fingerprint.** A pattern engine that is fast on every input you tried
and exponential on one you didn't — catastrophic backtracking, the
classic production outage (a log line takes down the fleet).

**Options & precedent.** Backtracking engines with complexity cliffs
(PCRE-style), or the RE2 discipline: compile to an automaton, simulate
all threads in lockstep (Thompson/Pike VM), linear in the input,
guaranteed — at the cost of some conveniences (backreferences).

**Ruling.** Standing and in-house: Frame's own pattern engine is a Pike
VM with RE2-parity semantics. Backtracking pattern execution is not used,
ever; a pattern that seems to need it is redesigned or handled
structurally (recipe 7's forks are bounded and merged, never re-explored).

**Example.** `(a|aa)+` against a long run of `a`s with no `b` is the
classic catastrophic-backtracking bomb. Compiled by `@@fsm` to an
automaton and simulated in lockstep, the same pattern rejects 100,000
adversarial bytes instantly — linear by construction, not by luck.

```frame
@@[target("python_3")]

@@fsm LinearMatch(text: bytes) : bool = False {
    /(a|aa)+b/ True
}

if __name__ == '__main__':
    print(@@LinearMatch("aaab").accepted)         # True
    print(@@LinearMatch("a" * 100000).accepted)   # False — and instant
```

---

## Using this guide

For a design agent: load this alongside the foundational paper at
engagement start when the work touches recognition. The fingerprints are
scan targets — grep for bare counters, angle alphabets, `if let Ok(Some`
error-swallows, order-dependent dispatch tables. For a human: the recipes
are the difference between a scanner that works on the tests and one that
works on the language.
