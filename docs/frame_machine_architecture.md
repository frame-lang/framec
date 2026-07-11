---
title: "Frame Machine Architecture — choosing and designing computational machines"
nav_exclude: true
---

# Frame Machine Architecture

A reference for deciding **whether** a problem wants a machine, **which** kind of
machine it wants, and **how** to realize it in Frame. It is deliberately
decision-first: start at §1, and only descend into the taxonomy you land on.

The three orthogonal axes you are always choosing on:

1. **Computational power** (what class of problem it can *solve*): finite,
   pushdown, or Turing-complete — the Chomsky hierarchy (§3).
2. **Role** (what it *does* with that power): scanner, parser, recognizer,
   transducer, reactive/protocol controller (§4).
3. **Architecture** (how it is *structured*): flat, hierarchical, stacked,
   composed (§5).

A concrete design is one point on each axis, e.g. *"a pushdown transducer,
structured as a flat machine with a native counter"* (that is `ExprScannerFsm`).

---

## 1. First question: is this a machine problem at all?

A machine earns its keep when **behavior depends on accumulated history**, i.e.
the system is in *states* and the same input means different things in different
states. If output depends only on the current input, it is a function, not a
machine.

Reach for a machine when you see:
- **A stream to classify by structure** — tokens, nesting, grammar → scanner/parser (§3.1–3.2).
- **A stateful process/protocol/lifecycle** — connection, session, negotiation,
  UI mode, game entity, workflow → reactive state machine (§4.5, §5).
- **"It depends what happened before"** — modes, phases, "you can't X until Y".

Do **not** reach for a machine when (§8 expands):
- The transform is stateless (map/filter/reduce, a pure calculation) → a function.
- You need to parse a real, ambiguous grammar with backtracking → a parser
  generator / PEG / hand-written recursive-descent, not an ad-hoc DFA.
- The "states" are just data values with uniform behavior → a struct + logic.
- You are reaching for full Turing power to model something a plain program says
  more clearly → the machine framing is obscuring, not clarifying (§3.4, §8).

---

## 2. Second question: how much power does it need?

The decisive test is **what must be remembered**:

| If deciding the next step needs… | Power class | Machine |
|---|---|---|
| only the current state (bounded memory) | **Regular** | Finite automaton (§3.1) |
| a count / matched nesting to *unbounded* depth | **Context-free** | Pushdown automaton (§3.2) |
| cross-referencing unbounded material both ways | context-sensitive | LBA (rare, §3.3) |
| arbitrary computation over unbounded storage | **Turing-complete** | TM / general program (§3.4) |

The practical tell (the pumping-lemma intuition): **"do I need to count or match
nested structure without a fixed bound?"** If yes, it is *not* regular — a plain
DFA/`@@fsm` cannot do it, and you need a stack or counter. If you find yourself
adding a `depth` counter to an `@@fsm`, you have left the regular world on purpose
(§3.2) — say so.

---

## 3. Machines by computational power (the Chomsky hierarchy)

### 3.1 Finite automaton (FSM / DFA / NFA) — regular languages
FSM = finite states + transitions on input; **no memory beyond the current
state**. DFA (one transition per symbol) and NFA (many) are *equivalent in power*
(subset construction; NFA→DFA can blow up exponentially in state count).
Sub-flavors: **Moore** (output attached to states) vs **Mealy** (output on
transitions); **acceptor** (yes/no) vs **transducer** (emits output).

- **Can:** tokenize, match fixed patterns, enforce ordering/protocol phases with a
  bounded number of modes, character classes, anchors.
- **Cannot:** match balanced brackets, count `aⁿbⁿ`, recognize palindromes,
  anything needing unbounded memory.
- **Frame realization:** `@@fsm` (RFC-0042) — a regular lexer compiled to a Pike-VM
  (an NFA simulation with Assert opcodes for anchors/`\b`). Also every **flat
  `@@system`** whose behavior is finite-state.
- **framec examples:** the 15 `SyntaxSkipper` (comment/string), 15 `BodyCloser`
  (brace matching *by pattern*), `OutputBlockLexerFsm` (tokenizer).

### 3.2 Pushdown automaton (PDA) — context-free languages
FSM **+ a stack**. The stack gives unbounded memory with last-in-first-out
discipline — exactly what nesting needs. **DPDA** (deterministic) ⊊ PDA; most
practical parsers target the deterministic subset.

- **Can:** balanced `()[]{}`, arbitrary nesting depth, `aⁿbⁿ`, expression grammars,
  matched open/close.
- **Cannot:** `aⁿbⁿcⁿ`, cross-serial dependencies (that is context-sensitive).
- **Frame realizations — three, in increasing honesty:**
  1. **A counter augment:** an `@@fsm`/`@@system` with a `depth` domain var
     mutated in transition actions is a *counter automaton* — enough for
     single-kind bracket matching. This is a deliberate step **beyond regular**;
     call it out.
  2. **A native stack/counter in a handler:** `ExprScannerFsm` is "FSM-shaped" but
     its scan core is a native Rust `while` loop counting `()[]{}` depth — a PDA
     whose stack is a native integer. Honest and pragmatic; not a "real `@@fsm`".
  3. **Frame's own `push$`/`pop$` state stack:** this is a *genuine pushdown
     mechanism* over compartments. A `@@system` that pushes and pops states **is a
     PDA** — the state stack is the automaton's stack. Modal interrupts
     (pause→resume, dialog→return) are the natural fit.

### 3.3 Linear-bounded automaton — context-sensitive
TM with tape bounded by input size. Recognizes `aⁿbⁿcⁿ`, cross-serial
dependencies. **Rarely the right tool** in this codebase — if you think you need
it, you are usually better served by a Turing-complete program with explicit data
structures (§3.4). Listed for completeness.

### 3.4 Turing machine / Turing-complete system — unrestricted
FSM **+ unbounded read/write storage**. Any computable function.

- **Frame realization:** a `@@system` with unbounded domain data and arbitrary
  native actions is Turing-complete — you are using the machine as a *general
  program*, not a language recognizer.
- **The warning:** Turing-completeness is a description, not a goal. When the
  "states" stop being a small, nameable set of modes and become "wherever the data
  happens to be", the state-machine framing is **obscuring** the logic. Prefer a
  plain function/module; reserve `@@system` for genuinely stateful, mode-driven
  behavior (§8). A state machine used as a Turing tarpit is an anti-pattern.

---

## 4. Machine roles (orthogonal to power)

What the machine *does* — independent of how much power it needs.

- **Scanner / lexer** — split a stream into tokens. Usually regular (§3.1).
  *Frame:* skippers, `OutputBlockLexerFsm`.
- **Parser** — recover grammatical structure (nesting, arms, blocks). Context-free
  → PDA (§3.2). *Frame:* `OutputBlockParserFsm`, the context parser.
- **Recognizer / acceptor** — membership yes/no. *Frame:* validators phrased as
  machines.
- **Transducer / translator** — input→output transform (Mealy/Moore). Most framec
  codegen scanners are transducers.
- **Reactive / protocol controller** — model a stateful process reacting to events
  over time (§5). This is Frame's primary purpose: `@@system`.
- **Oracle — the anti-pattern, NOT a machine type.** A hand-rolled text probe that
  recovers *structure* from emitted or source text via ad-hoc
  `starts_with`/`contains`/manual brace counting, instead of a principled machine.
  The entire target of #123. If a "scanner" is really a pile of string probes
  reconstructing grammar, it is an oracle to be converted, not a machine to be
  kept. (Distinguish from *incidental* string ops — a type-string check, a
  single-token punctuation decision — which are fine.)

---

## 5. System architecture (structural composition)

How the states are organized. **These change compactness and maintainability, not
(mostly) computational power** — an HSM is equivalent to a flat FSM, just
exponentially smaller.

- **Flat machine** — one level of states. Best when modes are few and independent.
  *Frame:* a plain `@@system` with `$States`.
- **Hierarchical state machine (HSM)** — nested states with parent/child; a child
  inherits/forwards to its parent (`$Child => $Parent`, `=> $^`). Collapses the
  "shared behavior across many modes" explosion. **Same power as flat**, far more
  maintainable. Reach for it when many states share handling ("in any connected
  substate, `disconnect` does the same thing").
- **State stack / pushdown** — `push$`/`pop$`. Return-to-previous-state; modal
  interruptions that must resume exactly where they left. This is the construct
  that lifts a Frame machine to **PDA power** (§3.2.3). Use for pause menus,
  nested dialogs, interrupt-and-resume.
- **Composed / orthogonal machines** — several machines running and communicating
  (system-valued domain fields holding other `@@system`s; multi-system). The
  analogue of statecharts' orthogonal regions: independent concurrent aspects that
  would otherwise multiply into a combinatorial single machine. Use when aspects
  are genuinely independent (e.g. a connection's *transport* state × its *auth*
  state).
- **Async-gated machine** — RFC-0043 `@@[async]`: a single-driver gate wrapping the
  dispatch core so re-entrancy/concurrency is disciplined. An architectural overlay,
  not a new power class.

> **Statecharts** (Harel) = HSM + orthogonal regions + history + guarded
> transitions. Frame reaches statechart expressiveness through HSM (`=> $^`) +
> composition + the state stack (a form of history) + native guards.

---

## 6. Decision trees

### Tree A — is it a machine, and which power?
```
Does behavior depend on accumulated history / "what happened before"?
├─ No → NOT a machine. Use a function / data transform. (§8)
└─ Yes → is the input a language/stream to recognize or transform,
         or a process to model?
   ├─ Language/stream:
   │   Do I need to match nested structure or count without a fixed bound?
   │   ├─ No  → REGULAR → DFA / @@fsm lexer                     (§3.1)
   │   └─ Yes → CONTEXT-FREE → PDA: counter, native stack,
   │            or push$/pop$                                    (§3.2)
   │            (need cross-serial / aⁿbⁿcⁿ? → reconsider; §3.3/§8)
   └─ Stateful process → go to Tree B.
```

### Tree B — stateful process → which architecture?
```
How many modes, and how do they relate?
├─ Few, independent modes                → FLAT @@system            (§5)
├─ Many modes sharing common handling    → HSM (=> $^)              (§5)
├─ Must interrupt and RESUME a prior mode → STATE STACK (push$/pop$) (§5)
└─ Independent concurrent aspects         → COMPOSED / orthogonal    (§5)
Overlay: needs disciplined async/re-entrancy? → add @@[async]       (§5)
```

### Tree C — "I think I need a parser"
```
Is the grammar…
├─ Regular (tokens, no nesting)          → lexer / @@fsm            (§3.1)
├─ Deterministic context-free (nesting,
│  matched delimiters, no ambiguity)     → PDA / DPDA               (§3.2)
├─ Ambiguous / needs backtracking / full
│  CFG                                    → parser generator, PEG,
│                                            recursive descent — NOT
│                                            a hand-rolled machine  (§8)
└─ "Just recover structure from text I
   emitted myself"                        → you have an ORACLE; the
                                            real fix is to keep the
                                            structure, not re-scan   (§4, §8)
```

---

## 7. Problem-domain → architecture map

| Problem domain | Power | Role | Architecture | Frame |
|---|---|---|---|---|
| Comment/string skipping | Regular | Scanner | Flat | `@@fsm` skipper |
| Brace/delimiter matching | Context-free | Recognizer | Counter/stack | `@@fsm`+counter or native |
| Expression / nesting scan | Context-free | Transducer | Flat + native stack | `ExprScannerFsm` |
| Tokenize then structure | Reg → CF | Lexer → Parser | Two-stage | Lexer FSM → Parser FSM |
| Network/session protocol | Regular* | Reactive | Flat or HSM | `@@system` (+HSM) |
| Connection w/ nested sub-states | Regular | Reactive | HSM | `$Child => $Parent` |
| UI with modal overlays | Context-free | Reactive | State stack | `push$`/`pop$` |
| Game entity AI | Regular | Reactive | HSM (+stack) | `@@system` HSM |
| Independent concurrent aspects | — | Reactive | Composed | system-valued fields |
| Config/data transform | (none) | — | — | **not a machine** — a function |
| Ambiguous language parse | CF+ | Parser | — | **not hand-rolled** — parser gen |

\* Most protocols are finite-state; they become context-free only if they carry an
unbounded nesting/return discipline (then use the state stack).

---

## 8. When NOT to use a machine (red flags)

- **Stateless transform.** No meaningful states → a function. A machine adds
  ceremony and hides the logic.
- **Structure you already have.** Recovering grammar from text *you emitted* is an
  **oracle** (§4). The fix is to carry the structure through (keep the AST/IR/token
  stream), not to build a machine to re-derive it. This is the #123 lesson.
- **Real grammar parsing.** Ambiguity, backtracking, precedence-heavy expression
  grammars → a parser generator, PEG, or recursive descent. A hand DFA/PDA will be
  wrong in the corners (the else-if-without-else class of bug).
- **Turing tarpit.** If the state set is unbounded/data-shaped rather than a small
  set of named modes, you are writing a general program inside a state-machine
  costume (§3.4). Use a plain module.
- **Unbounded lookahead/lookback as the core mechanism.** Machines stream; if the
  problem fundamentally needs whole-input random access, a machine is a poor fit.

State the rejection explicitly and name the better tool — recognizing a
non-machine problem is as valuable as designing a machine.

---

## 9. Glossary

- **DFA / NFA** — deterministic / non-deterministic finite automaton; equivalent
  power (regular languages). NFA→DFA via subset construction.
- **Regular language** — recognizable with bounded memory; closed under
  union/intersection/complement/concatenation/star. No unbounded counting/nesting.
- **PDA / DPDA** — pushdown automaton (FSM + stack) / its deterministic subset;
  recognizes context-free / deterministic-context-free languages.
- **Context-free** — grammar expressible with a stack; balanced nesting.
- **Context-sensitive / LBA** — needs bounded two-way unbounded reference; rarely
  the right tool here.
- **Turing-complete** — FSM + unbounded storage; any computable function. A
  *description* of expressive power, not a design goal.
- **Chomsky hierarchy** — regular ⊂ context-free ⊂ context-sensitive ⊂
  recursively-enumerable; each level needs strictly more machine.
- **Moore / Mealy** — output on states / output on transitions.
- **Acceptor / transducer** — recognizes membership / emits transformed output.
- **HSM** — hierarchical state machine; nested states with inheritance/forwarding.
  Same power as a flat FSM, exponentially more compact.
- **Statechart** — Harel's HSM + orthogonal regions + history + guards.
- **Orthogonal region / composition** — independent concurrent machines.
- **State stack (`push$`/`pop$`)** — Frame's pushdown mechanism; the construct that
  gives a Frame machine PDA power.
- **Oracle (framec sense)** — an anti-pattern: recovering *structure* from emitted
  or source text via ad-hoc string ops instead of a principled machine (#123). Not
  a machine type.
- **Pumping-lemma intuition** — the practical tell that a language is beyond
  regular: "must I count / match nesting to unbounded depth?"

---

## 10. Frame capability map (construct → power/architecture)

| Frame construct | Adds | Power / role |
|---|---|---|
| `@@fsm` (RFC-0042) | regular lexer (Pike-VM) | Regular scanner/recognizer |
| flat `@@system` | finite states + events | Regular reactive transducer |
| HSM (`=> $^`, `$Child => $Parent`) | state nesting/inheritance | Same power, compact structure |
| `push$` / `pop$` | genuine stack of states | **Lifts to PDA** (context-free) |
| domain var counter in actions | a counter | Counter automaton (restricted PDA) |
| unbounded domain data + native actions | read/write storage | **Turing-complete** (use sparingly) |
| system-valued fields / multi-system | concurrent machines | Composition / orthogonal regions |
| `@@[async]` | single-driver gate | Async overlay (not a new power class) |

When proposing an architecture: fix the point on each axis (power → role →
structure), map it to the constructs above, and if the problem sits outside the
machine world, say so and name the better tool (§8).
