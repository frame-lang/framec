---
title: "Shadows on the Wall — The Latent Machine"
parent: Articles
nav_order: 4
---

# Shadows on the Wall — The Latent Machine

*Computation as the unwitting construction of state machines*

*Prompt Engineer and Editor: Mark Truluck <mark@frame-lang.org>*

> "For quite a while, I've been disturbed by the emphasis on language in computer
> science." — Leslie Lamport, *Computation and State Machines* [7]

## Abstract

This paper defends a strong claim: **every program is a state machine, and every
programmer is a state-machine author** — most of them unknowingly, in languages
that provide no way to say so. We establish the claim on three independent
grounds — the formal semantics of programming languages, a mechanical
construction that exhibits the machine in any program, and the accumulated
engineering record — while being precise about their differing force: the
semantic ground *identifies* execution with a transition system by the very way
meaning is defined (a settled identity, not a surprising theorem), the
construction is a genuine result, and the record is the weight of practice. That
the machine *exists* is therefore not the paper's real news; its consequence is.
We then draw the claim's one honest boundary — the class of artifacts that genuinely are not state machines, which
turn out to be precisely the artifacts that are not computations — and sharpen
that boundary from a vague intuition ("data things") into a precise criterion
(the value/process distinction, cut at the spec/engine seam) — and name the
third role that boundary hides, the **predicate**: a law over a machine's
behaviors, neither process nor data, where the paper's own verifiability and
alignment payoffs live. Drawn correctly the ontology is a trichotomy —
machine, value, predicate. We catalog the
disguises under which machines hide in ordinary code, name the two symmetric
failure modes of state-machine authorship (glossing and costuming), and derive
the design discipline that follows: the existence of the machine is never in
question; the only engineering decision is whether to *name* it, and the burden
of proof belongs to not naming it. Finally, we give a practical method — top-down
and iterative — for recovering the latent machines in any specification or
codebase. The intended audience is both human and artificial: this paper is
written to retrain the working worldview of anyone, person or model, who reads
and writes programs.

---

## 1. Introduction

In Plato's allegory, the prisoners of the cave face a wall, watching shadows
cast by things they cannot turn to see — and, knowing nothing else, they take
the shadows for the things themselves. The working programmer occupies a
similar position through no fault of their own: **our languages face us toward
the wall.** What we read and write all day — the functions, branches, flags,
and early returns — are shadows. The thing casting them, always, is a state
machine. This paper is about turning around.

Ask a working programmer what they did today and they will answer in the
vocabulary of their language: they wrote functions, classes, handlers, queries.
Ask what those artifacts *are*, operationally — what happens when they run — and
the answer is uniform across every language, paradigm, and era:
**a machine moved through a sequence of states.**

This is not a figure of speech. It is the literal content of how we define what
programs do, how processors execute them, and how the operational account of
computation has modeled them since 1936. Yet almost no mainstream language lets
the programmer *write down* the state structure of what they are building. The
states are there — they are always there — but they live encoded in program
counters, boolean flags, enum columns, early returns, call stacks, and the
positions of instruction pointers inside suspended coroutines. The machine is
real; only its *name* is missing.

The thesis of this paper is that this situation should invert the default
posture of software design. The question practitioners habitually ask — *"is
this a state machine problem?"* — is malformed, because it presumes the machine
is sometimes absent. The machine is never absent from computation. The
well-formed question is: *"here is the machine I am necessarily building — does
it pay to write it down?"* That inversion, from *whether* to *whether to name*,
changes what code review looks for, what languages should provide, and how both
humans and AI systems should be trained to read code.

We proceed as follows. Section 2 makes the case on three independent grounds. Section 3 makes
it precise — what exactly is a state, what is a transition, and why the claim
is scale-invariant. Section 4 draws the boundary: what genuinely is not a
state machine, and why that class is exactly the class of non-computations.
Section 5 catalogs where machines hide in ordinary code. Section 6 derives the
naming discipline and its two failure modes. Section 7 gives a method for
finding the machines in an arbitrary codebase. Section 8 draws implications.

---

## 2. Three Independent Grounds

The claim — *all computation is state machinery* — rests on three independent
grounds: the semantics of how programs are given meaning, a construction that
exhibits the machine in any program, and the practice of tooling that reifies
it. Their force differs, and this section is careful to say so. The semantic
ground is an *identification* — meaning is defined as a transition relation, so
this settles the machine's existence rather than discovering it. The
construction is a genuine result about any sequential program. The record is
corroboration from the field. Together they settle that the machine is always
present — which is the least interesting thing about it. The interesting thing,
taken up in §6, is that its presence being settled leaves exactly one open
decision: whether to name it.

### 2.1 The semantic ground: meaning is defined as a transition system

When the field formalized what a program *means*, the formalism it converged on
was a transition system. Gordon Plotkin's structural operational semantics [2]
— the standard method for specifying language behavior — defines the meaning of
a program as a relation between *configurations*:

```
⟨ statement, store ⟩  →  ⟨ statement′, store′ ⟩
```

A configuration pairs a control point with a memory state; the semantics is the
set of permitted transitions between configurations. There is no rival account
of what it is to *run* a program. The denotational account — which assigns a
program a timeless mathematical value — is not a rival but the value pole of
the boundary this paper will draw in §4, and it is answerable to the transition
system through adequacy results. To execute is to traverse the transition
relation; a run of the program *is* a path through a state space. This is
likewise the picture the hardware presents: a processor is a finite-state
control — the pipeline and its program counter — acting on a store. (One can go
further and note that, physical memory being finite, every real execution is
*literally* a walk through a finite, astronomically large state machine. But
that is the degenerate, true-but-uninformative reading §3 is at pains to
quarantine — a machine with 2^(billions) states is not usefully "finite-state,"
and nothing in this paper leans on it. The load-bearing claim is the semantic
one above, that meaning *is* a transition relation, not the counting one.)

The lineage runs straight back to the founding document. Turing's machine [1]
is a finite table of *m-configurations* — his term for states — governing a
head over a tape. The finite control was not incidental apparatus; it was
Turing's model of the human computer's "state of mind." Computation was born as
state machinery, and the operational semantics of every language since has
preserved that shape.

> **The Turing machine, in one breath.** A finite table of states; an
> unbounded tape of symbols; a head reading one symbol at a time. Each table
> entry says: *in this state, reading this symbol — write that symbol, move
> one cell left or right, enter that state.* That is the entire model, and
> everything computable is computable by it. Every machine in this paper is,
> at bottom, a descendant of this table.

### 2.2 The constructive ground: the machine is mechanically recoverable

A skeptic might grant that execution is *describable* as a state machine while
denying that any particular program *contains* one in a recoverable sense.
John Reynolds closed that escape in 1972 [3]. The two-step transformation he
introduced — convert a program to continuation-passing style, then
**defunctionalize** the continuations — takes any *sequential* program, in any
style, and mechanically produces a first-order state machine: a data type of
continuations, whose constructors are the machine's control states and whose
values are its stack, together with a first-order step function. (A concurrent
program defunctionalizes thread by thread, the scheduler joining the
composition as one more machine; §3 completes that picture.) Danvy and Nielsen
later showed how routine and general the technique is [4]. Nothing about the
source program needs to look "machine-like" for the transformation to succeed,
because the machine was never absent; it was merely encoded in the program's
control structure. Defunctionalization is not an
analysis that sometimes finds a machine. It is a change of representation that
always exhibits one.

> **Two terms, quickly.** *Continuation-passing style* rewrites a program so
> that "what happens next" — the continuation — is passed along as an explicit
> value rather than living implicitly in the call stack.
> *Defunctionalization* then replaces those continuation values with plain
> data: an enumeration of the finitely many shapes "what happens next" can
> take, plus a first-order step function that interprets them. An enumeration
> of nexts is a set of states; the function that steps through them is a
> transition table.

The same closure applies to the classic objection from recursion. A recursive
descent parser "isn't a state machine," the folklore says — it's a program. But
a recursive parser is precisely a pushdown machine — a finite-state control
augmented with a stack — whose stack is the host language's call stack. The machine did not disappear into the recursion; its
most important component was silently outsourced to the runtime. This
distinction matters in practice: the moment such a parser must suspend, resume,
or report *where it was* when input ran out, the hidden stack must be dug back
out — which is defunctionalization again, performed under deadline pressure.

### 2.3 The engineering record: compilers already reify the machine

The third ground is empirical: wherever software *needs* the machine to be a
first-class value, tooling **reifies** it — that is, makes the abstract
concrete: turns structure that until then existed only in behavior into an
explicit object the program can hold and inspect. The recovery is mechanical,
performed on "ordinary" code — demonstrating that the machine was there all
along.

- **Coroutines and `async/await`.** When a function must suspend mid-body and
  resume later, the toolchain must hold its machine explicitly. Compilers for
  C# (a synthesized state field driving `MoveNext`), Rust (an enum whose
  discriminant *is* the state, resumed via `poll`), Kotlin (a label-dispatching
  `invokeSuspend`), and JavaScript (switch-dispatch machines in engines and
  transpilers alike) reify the linear-looking function into an object with an
  explicit state field and a resume method; CPython instead suspends the
  interpreter frame itself, storing the resume offset — the same machine, held
  by the runtime rather than synthesized by the compiler. Either way, a state
  machine emerges from code whose author may never have thought the word
  "state," which is possible only because the machine was already present in
  the control structure.
- **Regular expressions.** A pattern is compiled to an automaton and executed
  as one; automaton-based engines descend from Ken Thompson's 1968
  construction [5].
- **Reactive interfaces.** The front-end ecosystem spent a decade rediscovering
  that interface logic scattered across callbacks and flags is an unmanaged
  state machine, and converged on tools that reify it — a practical vindication
  of David Harel's argument, three decades earlier, that complex reactive
  behavior demands an explicit state formalism [6]. Harel's *statecharts* —
  the notation that later became UML's state diagrams — are that formalism:
  plain state machines extended with **hierarchy** (states nested inside
  states, so one parent transition serves a whole family) and
  **orthogonality** (independent regions running side by side) — the two
  devices that let a dozen drawn states govern behavior that flat code spreads
  across hundreds of branches.
- **Distributed systems.** The entire theory of fault-tolerant replication
  rests on modeling a service *as* a deterministic state machine, replicating
  it, and ensuring every replica applies the same requests in the same order
  [8]. The approach extends to arbitrary services because any service can be
  *rendered as* such a machine — the engineering content of the method is
  exactly that determinization, which presupposes, and thereby evidences, the
  underlying machine.
- **Specification.** Leslie Lamport — Turing laureate for the foundations of
  distributed computing, and for decades the field's most insistent advocate
  of the state-machine view — specifies programs, protocols, and hardware
  alike as state machines in TLA+ [9]: a behavior is a sequence of states, and
  a specification is the machine that generates the permitted behaviors.
  Lamport has pressed the general point throughout, arguing that our fixation
  on *language* obscures the underlying invariant: that what a program
  describes is a state machine, and that computer science pays a price for
  teaching syntax before teaching this [7].

Three grounds, one conclusion. The state machine is not a design pattern to be
selected when appropriate; it is what computation *is*. That much is settled —
and, settled, it is nearly uninteresting. What varies between programs, and what
the rest of this paper is about, is only whether the machine is written down.

---

## 3. Precision: States, Transitions, and the Tower of Quotients

A claim this strong must be stated exactly, or it degrades into slogan. Three
refinements make it rigorous.

**Statements are transitions; program points are states.** In the operational
picture of §2.1, an executable statement is an *edge*: it transforms one
configuration into the next. The *nodes* are the program points between
statements. A straight-line program of *n* statements is therefore an
(*n*+1)-state machine — a linear chain in which each state has exactly one
outgoing transition. This is the degenerate pole of the claim: perfectly true,
and perfectly uninformative, because the chain's state structure carries no
information beyond the program counter, which the language already maintains
for free. The degenerate case is important not because anyone should reify it
but because it establishes that machine-hood is *never* the question. Even the
blandest code is a machine; the interesting property lies elsewhere.

**The structure is fractal.** Each transition, examined closely, decomposes
into a finer machine. A single expression is itself a fine-grained process —
which is why C must legislate sequence points, the instants by which its
micro-steps' side effects must have completed, and why an `await` can suspend
execution *inside* an expression. Below that, the processor's own pipeline is a machine
executing micro-transitions. There is no bottom at which the machine view
stops applying; there are only levels of description.

**Abstraction is quotienting.** If the structure is fractal, which machine is
*the* machine of a program? The answer is that every coarser machine is a
**quotient** of a finer one: a partitioning of many low-level configurations
into a few named modes, with transitions inherited across the partition. When a
designer says a connection is `CONNECTING`, `OPEN`, or `CLOSED`, they have
quotiented billions of byte-level configurations into three modes that predict
behavior. Not every partition qualifies: the induced transitions predict
behavior only when the partition respects the underlying dynamics — when
configurations grouped together move to the same groups. **Choosing the
quotient — a partition that is both meaningful and faithful — is the design
act.** It is the same
intellectual move as choosing an abstraction — indeed it *is* choosing an
abstraction, stated operationally.

> **Quotient, in one breath.** The word is the mathematician's: divide a
> set by a "counts as the same" rule, and the result's elements are the
> lumps themselves. A 12-hour clock is the integers divided by "differs by
> a multiple of twelve" — infinitely many numbers, twelve classes, and
> arithmetic still works on the classes because addition respects the
> lumping. A named machine state is exactly such a lump of configurations,
> and it is honest on exactly the clock's condition: transitions must
> respect the partition. The two failure modes of §6 are the two ways a
> partition goes wrong — glossing lumps too much (one name hiding two
> futures), costuming too little (two names sharing one). And a register
> is the coordinate that refuses to be lumped: the part of the
> configuration the partition cannot absorb, carried along as data. The skill of state-machine authorship is not
inventing states; the states exist at every granularity. The skill is selecting
the level at which the mode structure is meaningful: few modes governing much
behavior, with boundaries that fall where the *observable* differences fall.

**Concurrency multiplies machines; it does not escape them.** A concurrent or
distributed program is not one sequential walk but a composition of component
machines whose steps interleave nondeterministically. The composition is
itself a state machine — its configurations drawn from the product of its
components', its transition relation the union of their steps — which is
precisely how TLA+ specifies such systems [9] and how the replication
literature exploits them [8]. Concurrency adds machines (one per thread, plus
the scheduler) and adds nondeterminism to the walk; it widens the tower rather
than standing outside it.

With these refinements, the claim of §2 can be restated in its exact form:
*every program is a tower of state machines related by quotients; the
programmer's control structures select one level of the tower and then encode
it namelessly.* What remains is to ask what, if anything, stands outside the
tower.

---

## 4. The Boundary: What Is Not a Machine

An honest maximalism must locate its own limit. There are artifacts in software
practice that resist the machine description — a database schema, a type
definition, a configuration file, a SQL query, an algebraic identity. The
tempting move is to carve out a domain exemption: *data things* aren't state
machines. The intuition is pointing at something real, but domain is the wrong
axis to cut along, and finding the right one sharpens the entire thesis.

### 4.1 The trichotomy: process, value, and law

Begin from Lamport's identification: a computation is a sequence of steps —
in the state-based view he adopts, a sequence of states [7]. Take it seriously
in both directions. If computation is state sequence, then whatever genuinely
escapes the machine view escapes *by not being a computation*. Two different
kinds of thing do, and both deserve a name.

The first is a **value** — or a **space** of values, or a **description**
awaiting an engine: data at rest, with neither transitions nor a time axis.
This is the machine's complement, and it arrives in three familiar shapes:

- A **schema** or **type definition** is not a machine — and not a computation.
  It *defines a state space*: the set of configurations an entity may occupy.
  It has no transitions and no time axis. Note carefully that this makes data
  modeling not the *rival* of the machine view but its **other half**: a
  machine is a state space plus a transition structure, and data modeling
  supplies the first component. (A state-machine language reflects this
  directly: a machine declaration contains its data model — the variables whose
  values, jointly with the named mode, constitute a configuration.)
- A **pure total function**, viewed denotationally, is a timeless mapping from
  inputs to outputs — a value in the mathematical sense. Its *evaluation* is a
  computation (and thus a machine, per §2.1), but when no intermediate state of
  that evaluation is observable — it cannot fail partway, suspend, or be
  interrupted in any way the rest of the system can detect — the mapping view
  is honest, and the artifact may be treated as a value.
- A **specification** — a regex pattern, a SQL query, a build file, a grammar —
  is data that *describes* behavior without performing it.

The value side of the boundary is sharpened in §4.2–4.3. But there is a
*second* kind of thing that is not a machine, and the machine-versus-value
picture drawn so far — the picture most attempts at this ontology stop at —
misses it entirely. It is a **predicate**: a *law over a machine's behaviors*.
"The agent never executes without approval"; "every request is eventually
answered"; a type read as the proposition it obligates. A predicate is not data
at rest, because it *judges*; and it is not a process, because it has no states
of its own. It is a third role — developed in §4.4 — and it is precisely where
this paper's strongest payoffs, verifiability and alignment, turn out to live.
The ontology in full is therefore not a dichotomy but a **trichotomy —
machine | value | predicate**: a process, the data it moves, and the laws that
say whether it moved rightly. These are *roles*, not disjoint substances — one
artifact can play more than one. A type, by Curry–Howard, is at once a **value**
(the space of its inhabitants) and a **predicate** (the proposition those
inhabitants prove); the trichotomy classifies what an artifact is *doing*, not a
partition of syntactic objects.

### 4.2 The right cut: spec versus engine

The specification case exposes the correct boundary. Every one of those
"non-machine" artifacts is animated, somewhere, by an engine — **and the engine
is always a machine:**

| The value (not a machine, not a computation) | Its engine (always a machine) |
|---|---|
| Regular-expression pattern | The matcher: a finite automaton [5] |
| SQL query (relational algebra) | The executor: per-operator `open`/`next`/`close` state protocols [10] |
| Stream pipeline `map f ∘ filter p` | The consumer: a transducer (a machine that emits output as it consumes input) with buffering, end-of-stream, error, and backpressure states |
| Build file | The build executor: a scheduler over task states |
| Type/schema definition | The validator, migrator, and every lifecycle that moves instances through time |
| State-machine *description* itself | The generated or interpreted machine that runs it |

The cut that separates machine from non-machine therefore falls not along
*domain* ("data-ish things are exempt") but along *role*: the **spec/engine
seam** — equivalently, value versus process, the denotational view versus the
operational one. If you are writing the description, you are writing data. If
you are writing the thing that animates the description, you are writing a
machine, whatever your language calls it.

### 4.3 Streaming, and data across time

Streaming is the case that breaks the domain exemption and confirms the
role-based one. A "stream" sounds like data — and the pipeline *algebra* (the
composition of maps and filters, as an expression) is indeed a value. But a
stream is data with a **time axis**, and the moment time enters, machines
return: the consumer that processes arriving chunks is a transducer; a
resumable stream protocol is an automaton with a position register; every
backpressure scheme is a protocol machine. The same reintroduction happens
inside the most data-centric practice: a schema *migration* is a transition
between versions of a state space; an entity *lifecycle* (`draft → review →
published → archived`) is a machine that manages data; and the ubiquitous
`status` column — consulted by conditionals scattered across a dozen handlers —
is among the most common latent machines in industrial software.

The boundary, stated as a maxim: **data at rest is a state; data across time
is a machine.** The exemption from the machine view is real, but it is narrow
and must be *earned* — by showing the artifact has no observable intermediate
states, no time axis, and no failure/suspension/resumption structure; that is,
by showing it is a value, a space, or a spec whose engine lives elsewhere. An
exemption claimed on those grounds should also say *where* the engine lives,
because someone owns it, and that someone is writing a state machine.

### 4.4 The predicate: the law that judges

The value is what the machine moves; the predicate is what says whether it
moved rightly. The idea is old and thoroughly worked. An **assertion** attached
to a point in a program — a proposition required to hold whenever control
reaches it — is a predicate; Turing used such assertions to check a routine in
1949, and Floyd and Hoare made them the basis of what a program *means* [12, 13].
These are neither data at rest nor processes with states of their own. They are
**laws**, and what a law judges can be any of the other roles:

- a **value** — a schema, a well-formedness rule, or a refinement type
  `{v | φ(v)}` that carries an explicit predicate on its inhabitants [17]; by the
  Curry–Howard correspondence a type simply *is* a proposition, and its values
  are the proofs [16];
- a **function** — its type, or a pre-/post-condition contract judging the
  mapping from inputs to outputs (the Hoare triple `{P} f {Q}` made syntactic as
  `require`/`ensure`, or an algebraic law the function must obey) [13, 15];
- a **machine** — a safety invariant over its reachable states or a liveness
  property over its traces, in the temporal logic of programs [18, 19, 20], up
  to a reachability law like "every path to `$Executing` passes through
  `$Approved`." This is the load-bearing case for this paper, because a *named*
  machine can be **model-checked** against such a law [21] — but the predicate is
  the same kind of thing in every case: a proposition the thing it judges can
  satisfy or **violate**.

**Why the predicate is irreducible: the argument from violation.** A value a
machine computes cannot be *wrong* relative to that machine — its output simply
is whatever the machine makes it. But a law can be false *of* the very machine
built to satisfy it; the machine can *break* it. Sharpen this until it bites:
if a predicate were merely "what the machine calculates or sets," there could
be **no bugs** — the machine would define its own correctness by fiat, and
every trace would be correct because *correct* would mean *whatever it did*.
Incorrectness exists at all only because there is a law whose authority is
independent of the machine it judges. **The bug is the daylight between the
predicate and the machine** — and daylight has no home in an ontology of
machines and values alone.

**The deep form: is and ought.** Sections 2 through 4.3 are a *physics of
computation* — they say what programs **are** (machines moving values), in the
declarative register of a natural law. But correctness, verification, and
alignment are **oughts**, and no accumulation of *is* — no operational
semantics, however complete — yields "this trace is wrong." This is Hume's old
observation that a normative conclusion cannot be drawn from purely descriptive
premises [22], turned on program semantics; and it is why philosophers of
computing classify a *specification* as prescriptive rather than descriptive —
the criterion by which a system is judged correct or malfunctioning, not a
report of what it does [23]. The predicate is where the *ought* lives, and a
purely descriptive ontology, which is what a two-category picture is,
structurally has no room for it. This is not a
decorative point: the paper's own headline payoffs depend on it. **Verifiability**
(§6.1) and any **alignment** guarantee are stated as laws over the transition
graph — "every route to `$Executing` passes through `$Approved`" is neither a
state nor a transition — which is exactly why, until the third category is
admitted, those payoffs float free of the very foundation meant to support them.

**The reduction, and why it fails.** The tempting dissolution: a predicate is
just "what some machine computes" — a checker evaluates it, so it is a machine
(the checker) plus a value (the verdict). This conflates three things: the
**checker** (a machine, granted), the **verdict** (a value, granted), and the
**predicate itself** — the law — which is neither. The paper already refuses
this exact move for values (§4.1: a pure function's *evaluation* is a machine,
yet the mapping may be treated as a value); by the same parity a predicate's
*checking* is a machine and its *verdict* a value, but the law is irreducible
to either. There is one consistent escape — a predicate is extensionally a
*set* of behaviors, hence a value-about-machines — but it does not rescue the
two-category picture: a **machine's** denotation is *also* a set of behaviors,
so the very move that folds predicate into value folds machine into value too,
and lands in denotational **monism** (everything is a value) — coherent,
useless for design, and flatly not this paper's position. What one cannot
consistently do is reduce `predicate → value` while shielding `machine → value`
from the identical argument. That asymmetric halfway house just *is* the
two-bucket ontology, and it does not stand.

**An emblem, and where it breaks.** A predicate sits at the `machine | value`
boundary much as a **virus** sits at biology's `alive | inert` boundary, and
the fit is close. A virus is an obligate parasite — inert on its own, acting
only through a host's machinery — which is exactly why "predicates are only ever
computed by machines" is true and yet does *not* dissolve the category. It is
made of the same substance as the two primary kinds — a virus is ordinary
biomolecules; a predicate is, extensionally, an ordinary set — but arranged into
a role neither anticipated. It is identified only *relative* to a host, and it
is handled by a parallel taxonomy rather than forced into the tree. Then the
analogy breaks, in the one place that matters. A virus is *causal*: it hijacks
the cell for its own replication and has no opinion about what the cell *ought*
to do. A predicate is *normative*: it **judges** the machine and can find it
guilty. Biology has parasites but no oughts; that residue — **normativity** — is
precisely what earns the predicate its own category, and precisely what a
descriptive physics of computation has no word for.

**The predicate in force: the constraint.** A predicate, like the virus, is
inert until it is *applied*. In a comment or a design note, "balance ≥ 0" does
nothing; the law acquires force only when it is bound to a site in a machine — a
guard on a transition (Dijkstra's guarded command, where a predicate gates which
step may fire [14]), an `assert` at a program point, a validation gate, a pre-
or post-condition — at which point it becomes an **invariant**: the law in force
over the machine's runs. This applied form earns a name, because it is what one
actually meets in code. Call it the **constraint** — the predicate's engine,
exactly as an interpreter is the engine of a spec (§4.2). The parallel is exact:

|          | at rest       | in force     |
|----------|---------------|--------------|
| **data** | value         | machine      |
| **law**  | predicate     | constraint   |

A value is data at rest; a machine is that data given a transition structure. A
predicate is a law at rest; a constraint is that law given a site to act at. In
both rows the animator is the same — a machine — which is why the constraint is
a **second-order** aspect, not a fourth primitive: it is `predicate ⊗ machine`,
the way a computation is `value ⊗ machine`, and the trichotomy of primitives
stays clean. But the working purpose of this paper — training a reader, human or
model, to categorize real code — makes the constraint something that must be
**distinguished so it can be identified**. Finding a bare predicate (a law
stated), finding the machine it ranges over, and finding the constraint that
binds them (the `if` guard, the `assert`, the validation check, the contract)
are three different acts of recognition. An inventory that knows only the
machine reads a guard as mere control flow and an assertion as a dead statement,
and misses the law each one enforces. The constraint seam is where correctness
becomes visible in the source — which is exactly where an analyst, having found
the machine, looks next.

Taken seriously, the third category asks for a first-class place to *write the
law*, not only the machine — and a language that means this thesis would provide
one, alongside the machine and the value it already names. That is the direction
the boundary points. This paper's task is only to draw the boundary correctly,
and drawn correctly it has three sides, not two: the machine, the value it
moves, and the law that judges it — with the constraint as the seam, second-order
but not second-class, where that law is bound to the machine and correctness
enters the code.

---

## 5. Where Machines Hide: A Field Guide to Latent State

If every program encodes a machine, ordinary language features must be the
encoding. They are. Structured programming itself can be read this way: the
Böhm–Jacopini result [11] showed that sequencing, selection, and iteration
suffice to express any computable control flow — which is to say, **structured
control flow is a complete notation for writing automata without ever naming
their states.** An `if` is a two-way branch between anonymous states; a loop is
an anonymous cycle; a function call pushes a frame of an unnamed pushdown
machine. The historical triumph of structured programming was the taming of
explicit control transfer — and its side effect was to render state structure
invisible at exactly the moment it became universal. (Appendix B draws each
control statement as the automaton fragment it denotes.)

The disguises are few and recur everywhere. A reader trained to see them can
recover the machine from almost any code:

| Disguise | What it actually is |
|---|---|
| Boolean flag (`connected`, `dirty`, `initialized`) | One bit of the mode register — a state distinction demoted to data |
| Enum/`status`/`phase`/`mode` field | The mode register itself, with transitions scattered as assignments across the codebase |
| `Option`/nullable return, or a `Result` with a *merged* error | A fork to distinct terminal states, thinned into the value channel (but a `Result` with a *rich* error type is the opposite — see the rival discussed below) |
| Early `return` / `break` / `continue` | An unnamed transition, often to an unnamed terminal state |
| Exception / `try–catch` | A non-local transition into an error state whose existence the happy path never acknowledges |
| Loop counter / depth counter | The register of a counter automaton tracking the walk |
| The call stack | The stack of a pushdown machine outsourced to the runtime |
| Callback / async continuation | A suspension state, reified by the compiler but unnamed in the source |
| Retry/backoff/timeout logic | A protocol machine's error-recovery states, inlined |
| Constructor/setup ordering (fields that must be set before others are valid) | An initialization phase — states the object passes through before its steady modes, encoded as call order |
| A timestamp or version column | A time axis — the signature that a machine governs this data (§4.3) |

(Appendix A draws each row of this table: the code shape, and beneath it the
machine it encodes.)

Two entries in this table deserve emphasis because they name the most costly
gloss in practice — the failure mode §6.2 will name *glossing*. **Initialization and error conditions are states** — and
they are the states most consistently flattened into flags, early returns, and
merged failure enums. A parser is mostly its edge cases: end-of-input,
malformed constructs, the unterminated string. Code that models its steady-state loop
carefully while compressing six distinct failure modes into one boolean has not
avoided building a machine; it has built one with its most operationally
important states unlabeled. When such code is later "converted" to an explicit
machine by faithfully transcribing its control flow, the gloss survives the
conversion — the machine is *code-faithful* without being *state-faithful*. The
test of a faithful machine is that its init and terminal structure is as
articulated as its processing structure.

**The strongest rival: the rich sum type.** Listing `Result` as a disguise is
only half the truth, and the other half is this paper's most serious rival. A
`Result<T, E>` whose error type `E` is a *rich, well-named sum* — the
functional-programming discipline of *making illegal states unrepresentable* —
does not erase the terminal states; it **names** them, in the value channel,
exactly as an explicit machine would name them in the state channel. That
discipline is a genuine rival reification of this paper's insight, and it is
important to concede where it wins. Where the artifact is a **pure, total
mapping** — a function whose only interesting structure is its set of outcomes,
with no ordering and no intermediate modes — the tagged sum is the *right*
reification, and the machine is correctly left latent. That is not a defeat for
the thesis; it is §4's value plea, arrived at from the other direction: the sum
type wins exactly where §4's boundary says the machine should not be named.
What the sum cannot express is *dynamics*. It names the codomain — which
outcomes exist — but not the path to them: it cannot say "every route to
`$Executing` passes through `$Approved`," cannot order the modes a value passes
through before it is valid, cannot make reachability a checkable property of a
transition graph. A `Result` names the exits; a machine names the exits *and*
the trajectory. So the two are not competitors but a division along §4's own
seam — name the value with a rich sum when the artifact is a mapping; name the
machine when it is a process. The failure the table indicts is never *using*
`Result`; it is using a *flattened* one (or a bare `Option`) for something that
is a process — thinning a lifecycle's several distinct terminals into one
anonymous error.

**A companion field guide: where laws hide.** The table above fingerprints the
machine; the same source carries the other two roles, and an analyst who names
only the machine will read past them. A **predicate** (a law stated, inert) and
a **constraint** (that law bound to a site, in force — §4.4) have their own
recurring shapes:

| Code shape | Role | Judges | How to tell |
|---|---|---|---|
| `assert P` / `debug_assert!` | predicate, applied → constraint | data / machine | a boolean about the state *here*; erasable on the happy path (`NDEBUG`, `-O`), aborts only on violation — a law, not a branch you route into |
| rejecting guard: `if !valid(x): return err / raise` | constraint | machine | a predicate that *gates* the step (Dijkstra's guarded command [14]); remove it and behavior changes — it decides transition enablement, not an arbitrary branch |
| `require` / `ensures`, `@pre` / `@post` | predicate (constraint if monitored) | function | judges the routine by relating entry to exit — the Hoare triple `{P} f {Q}` made syntactic [13], a design-by-contract clause [15]; a proof obligation, not a datum that flows on |
| loop / object `invariant P` | predicate | machine | asserted at every step boundary — implicitly quantified over *all* reachable states; re-established each step, yet selects no step |
| refinement / subset type `{v: Int \| v > 0}`, `x: Nat` | predicate (constraint where the checker enforces it) | data / function | a predicate carried *by the type* [17]; type-checking is that predicate in force |
| a type read as a proposition; `Option<T>`, `@NonNull` | predicate compiled into the type | data | Curry–Howard: the type *is* the law, enforced by construction, no runtime check [16] |
| validator fn: `is_valid`, `validate_*`, SQL `CHECK` | predicate (inert), constraint at a gate | data | a side-effect-free boolean — a law reified as a reusable function; does nothing until planted at a site |
| parser / smart constructor: `Email.parse(s) -> Result<…>` | constraint fusing predicate + type | data | emits a value that *carries its own proof* — "parse, don't validate" [25]; the type distinguishes checked from unchecked |
| property test: `prop_*`, `forAll(gen)`, an algebraic law | predicate over a function, in force over samples | function | a universally-quantified proposition about the function's behavior [24] |
| sum type "make illegal states unrepresentable" | predicate compiled into a value's type | data | the law enforced by construction, so a class of bugs cannot be written (Minsky; Wlaschin) |
| exhaustiveness / total-match obligation | constraint (compiler obliges all cases) | function | a law about the code, discharged at compile time |
| `raise` / `throw` / `panic` on a broken law | constraint firing | data / function | the observable effect of a law-in-force detecting a breach — but an `Err(ExpectedAlt)` is a plain machine terminal; disambiguate by whether the failure denotes a *violation* or an expected outcome |

Three tells settle the ambiguous cases. (1) A predicate is *erasable on the
happy path* — compile out an `assert` or an unmonitored `require` and the
algorithm is unchanged; a machine transition cannot be. (2) A constraint is
*load-bearing at a gate* — remove a guard, a validating branch, or a type-check
and behavior changes, because it decides whether a step fires [14]. (3) The
subject names what the law judges — the *state at a point* (a data predicate),
a routine's *entry-to-exit* (a function contract [13, 15]), or *all reachable
states / all traces* (a machine invariant or temporal property [18, 19, 20]).
The most treacherous shape is the reusable **validator**: the very same pure
boolean is a bare predicate where it is *defined* and a constraint where it is
*planted at a gate* — the law and its enforcement are distinct artifacts, and a
faithful inventory records both.

---

## 6. When to Name the Machine

Everything above establishes that the machine's *existence* is never at issue.
The engineering question — the only one — is whether to **name** it: to reify
states, transitions, and events as first-class, inspectable structure rather
than leaving them encoded in control flow and flags. This question has a
disciplined answer.

### 6.1 The three payoffs

Naming the machine pays on exactly three grounds, and a proposal to reify
should be able to point to at least one:

1. **Compression.** When a few modes govern many statements — when the
   quotient is steep — the named machine is a shorter, clearer description of
   behavior than the code that encodes it. This is Harel's original argument
   for statecharts [6]: the formalism earns its keep by compressing complex
   reactive behavior into hierarchy and orthogonality that flat code (and flat
   machines) cannot express legibly.
2. **Observability.** When intermediate states are operationally significant —
   when the system must suspend and resume, persist and restore, report where
   it is, or distinguish its failure modes — the states must be values, not
   positions in control flow. A program counter cannot be serialized to disk,
   displayed on a dashboard, or pattern-matched in an error handler; a named
   state can.
3. **Verifiability.** A named machine can be checked: its state space
   enumerated, its transitions tested one by one, unreachable modes detected,
   illegal transitions made unrepresentable, and its behavior compared
   state-for-state against an oracle. A latent machine offers no such surface —
   its states can be reached but not enumerated, exercised but not asserted on.
   Checking, precisely, is holding the machine against a **predicate** (§4.4) —
   a law over its behaviors; naming the machine is what gives that law a
   surface to bind to, which is why this payoff, unlike the first two, is
   really a payoff of the third category resting on the machine the first two
   reify.

### 6.2 The two failure modes: glossing and costuming

State-machine authorship fails in two symmetric ways, and both are errors of
**quotient selection** (§3).

**Glossing** is under-naming: writing the machine but hiding its states — the
boolean that is really a mode, the merged error terminals, the initialization
phase living in a constructor's implicit sequencing. Glossed code passes tests
on the happy path and fails at the edge cases, because the edge cases are
precisely the states that were never named. Glossing selects a quotient too
coarse for the observable behavior — and "observable" is the load-bearing
word: a partition is lawful only relative to a chosen set of observations,
and requirements, not syntax, choose them. A gloss is typically a partition
that is valid for the observations its author checked and unlawful for the
ones the system actually owes — which is the formal reason glossed code
passes its own tests: the missing observations are exactly where the names
lie.

**Costuming** is over-naming: reifying states that carry no information — the
degenerate (*n*+1)-chain of §3 dressed in state syntax, `Step1 → Step2 →
Step3` wrapped around straight-line code with no re-entry, no branching, no
observable intermediates, and no failure structure. A costume adds ceremony
without adding a single bit beyond the program counter. Costuming selects a
quotient too fine to be meaningful.

The discriminator between a real machine and a costume — and between honest
data and a gloss — is that **named states must be load-bearing**: each must
carry information about future behavior that is not already explicit in the
code's structure. A state that could be deleted, with its transitions fused,
without changing any observable behavior or any reader's understanding, was a
costume. A flag whose value changes which transitions are possible was always
a state.

### 6.3 The inversion of the burden of proof

Here is the discipline in one sentence. Under the traditional posture, a
designer asks *"is this a state machine problem?"* and the default answer is
no; machinery must argue its way in. Under the posture this paper defends, the
machine's presence is settled — by the grounds of §2 — so the question becomes
*"this is a state machine — justify leaving it latent,"* and the admissible
pleas are exactly three. The first is the plea §4 established: *this artifact is a
value, a space, or a spec, and its engine lives elsewhere.* The second, for a
fragment of genuine process: *it is pure, total, and none of its intermediate
states are observable.* The third, licensed by §6.2: *the machine at this
level is degenerate — its states carry nothing beyond the program counter, so
naming it would be costume.* That settled presence bears on existence, not significance;
significance is settled by the load-bearing test, and the burden of
justification therefore attaches where the signatures of §5 — the flags, the
mode fields, the merged terminals, the retry logic — evidence a non-degenerate
quotient. Where they do, and no plea holds, the latent machine is a design
decision that was made silently, and silence is the wrong way to make it.

This inversion is not maximalism about reification — §6.2's costume warning
stands. It is maximalism about **honesty**: the machine may reasonably remain
latent, but only as a *stated* decision with a *stated* justification, exactly
as one would justify any other abstraction choice.

---

## 7. Finding the Machines: A Method

The worldview implies a practice: given an arbitrary specification or
codebase, recover its latent machines. The method that follows is **top-down
in its structure and iterative in its execution** — top-down because the outermost
machine (the system's lifecycle) is the quotient under which every inner
machine is a refinement, and iterative because in real code one usually
encounters *evidence* of machines long before the machines themselves become
identifiable. The method embraces that: it gathers symptoms first, relates
them, and only then names the machine that manages them.

**Phase 1 — Symptoms.** Survey the artifact for the signatures of latent state
(the field guide of §5): flags, `status`/`mode`/`phase` fields, `Option`/
`Result` returns, early returns and breaks, exception structure, retry and
timeout logic, counters and depths, recursion, suspension points, timestamps
and version fields. Record each *symptom* with its location and the data it
reads or writes. At this stage, resist the urge to declare machines; a
symptom is not a diagnosis.

**Phase 2 — Relations.** Cluster the symptoms. Which flags co-vary? Which
functions consult the same mode data? Which error paths belong to the same
lifecycle? Which symptoms share a time axis? The clusters are hypotheses of the
form *"these symptoms are governed by one machine."* Some symptoms will resist
clustering — hold them; their machine has not yet surfaced, and forcing them
into the nearest cluster is how false machines get drawn.

**Phase 3 — Machines.** For each cluster, name the machine that manages it:
its states — **including, with particular care, its initialization states and
its full set of distinct terminal and error states**, since these are the
most-glossed (§5); its transitions and the events that drive them; its
classification (a mode dispatcher, a counter automaton, a transducer, a
pushdown machine, a protocol controller); its current encoding (which flags,
returns, and control structures presently carry it); and its position on the
spec/engine seam (§4.2) — is this code the description or the animator? Then
choose the quotient deliberately: the level at which the mode structure is
load-bearing.

> **The classifications, one line each.** A *mode dispatcher* selects among
> behaviors by a current mode. A *counter automaton* is finite control plus a
> counter or two — a depth, a retry budget. A *transducer* emits output as it
> consumes input. A *pushdown machine* is finite control plus a stack — the
> shape of recursion. A *protocol controller* sequences an interaction with
> another party, its timeouts and retries included.

**Iterate.** Descend: each named machine's states may themselves decompose
(§3), and the scan repeats within them. Ascend: newly named machines may
reveal that previously orphaned symptoms belong to a larger lifecycle not yet
drawn. The scan converges when every symptom is either owned by a named machine
or covered by an earned exemption (§4.3's plea: value, space, or
spec-with-engine-elsewhere).

The deliverable of the method is a **machine inventory**: the census of a
system's machines. Each entry has a fixed shape:

- **Machine** — its name;
- **Evidence** — the symptoms that betray it, with locations;
- **States** — initialization, steady, and the *full* set of distinct
  terminal/error states, latent ones included;
- **Events and transitions** — what drives movement between them;
- **Classification** — dispatcher, counter, transducer, pushdown, protocol
  controller;
- **Current encoding** — which flags, returns, and control structures carry it
  today;
- **Disposition** — *reify*, naming which of §6.1's payoffs, or *leave
  latent*, with the plea §6.3 demands.

An inventory of this kind is, in a precise sense, the operational truth of a
codebase: it answers what the system *does* in the only vocabulary computation
actually has.

### A worked miniature

The method deserves to be seen once at full magnification. Here is an entirely
ordinary helper — no machine in sight:

```python
def upload(path, client):
    connected = False
    retries = 0
    data = read_file(path)          # may raise
    while retries < 3:
        if not connected:
            client.open()
            connected = True
        try:
            client.send(data)
            return True             # success
        except TransientError:
            retries += 1
            connected = False
        except Exception:
            return False            # failure
    return False                    # gave up
```

**Phase 1 — Symptoms.** `connected` (a boolean flag, written on two paths);
`retries` (a bounded counter); three exits (`return True` once, `return False`
twice); a two-armed `try/except` (two error families treated differently); and
one symptom that is easy to miss — `read_file` can raise *before the loop*, an
exit that appears nowhere in the function's visible returns.

**Phase 2 — Relations.** `connected` and `retries` co-vary: both are written
on the `TransientError` path, so one lifecycle governs them. The three
`return` statements are not one outcome in three places but three *distinct*
outcomes — success, permanent failure, retries exhausted. The unguarded
`read_file` refuses to join any cluster: it is an orphan symptom, held until
the machine surfaces.

**Phase 3 — The machine.** Now the cluster names itself:

- **Machine:** `UploadSession`
- **Evidence:** the `connected` flag, the `retries` counter, three returns,
  two `except` arms, the unguarded `read_file`
- **States:** init `Reading`, `Connecting`; steady `Sending`; terminals
  `Done`, `Failed`, `RetriesExhausted` — and the latent **`ReadFailed`**, an
  exit the code takes (an exception escaping the function) but never
  acknowledges: the orphan symptom was an unnamed terminal all along
- **Events/transitions:** read ok → `Connecting`; send ok → `Done`;
  `TransientError` → `Connecting` (incrementing the bound; at 3 →
  `RetriesExhausted`); any other error → `Failed`
- **Classification:** protocol controller with a bounded-retry counter
- **Current encoding:** two mutable locals, three returns, exception structure
- **Disposition:** *reify* — on observability (four distinct terminals
  currently collapsed into `True`/`False`/an uncaught raise) and verifiability
  (a retry bound that can be asserted per state)

The point of the miniature is not that fifteen lines deserved a formalism. It
is that the machine, its four terminals, and its unacknowledged
initialization failure were all already there. The inventory added no
structure. It disclosed it.

---

## 8. Implications

**For programmers.** This reframing asks nothing new of practice except
honesty about what practice already is. Every working programmer is a prolific author
of state machines; the flags, enums, and early returns of their daily work are
machine notation, unnamed. What changes under it is stewardship:
error paths are read as terminal states and articulated rather than merged;
initialization is read as a phase with structure rather than a prologue;
`status` fields are recognized as mode registers whose transition rules
deserve one home instead of a dozen scattered conditionals. Nothing about this
requires a new language — only a new reading of the old one.

**For language design.** The engineering record (§2.3) shows compilers
routinely *reifying* machines from unannotated code the moment the runtime needs
them. It is tempting to read this as a simple argument — if the machine can be
recovered downstream, it could have been written upstream — but that reading
proves too much, and the record itself refutes it. `async/await`, generators,
and regular expressions are cases where the machine is deliberately *not* written
by hand: a programmer writes linear or declarative source, a lowering pass builds
the automaton, and this is agreed to be the better design — a hand-drawn
coroutine state machine would be worse than the `await`-annotated function that
denotes it. So the record argues, honestly, for *latency plus tooling*: leave
the machine implicit and let the compiler reify it. That is not a rebuttal of
this paper; it is §6's *leave-latent* disposition exercised at the level of
language design, and for those cases it is exactly right.

The genuine language-design argument is narrower, and follows from §6 rather
than overriding it. Most languages do not offer the *choice* the disposition
presumes; they force the machine latent, because they provide no way to name it
at all. A `status` field's transition rules cannot be gathered into one declared
machine even when scattering them is the mistake; a safety or alignment property
over a lifecycle cannot be *stated* even when it is the whole point. What a
machine-aware language adds is not a mandate to author every machine by hand —
the `await` case shows that would be wrong — but the *option* to name the machine
when naming pays, so that §6's decision can be made deliberately instead of
settled by default. Frame occupies that position: the machine is a first-class
source artifact one may reach for, *alongside* — not in place of — the
latent-and-lowered forms the record already vindicates. (Companion pieces on
this site survey the history of such attempts and the pathologies of state's
absence; this paper's argument stands independently of them, on the primary
literature.)

**For artificial intelligence.** Language models learn to program from corpora
in which virtually every machine is latent — and so, we conjecture and our
experience suggests, they tend to reproduce the gloss: generated code that
handles the happy path and flattens the edge cases, flags where there should
be modes, merged failures where there should be terminals. A model trained — or
instructed — to hold this paper's worldview should read and write differently:
asking *where is the machine* before asking *what is the code*, treating an
`Option` return as a fork in a lifecycle, and regarding an unarticulated error
path as an unfinished design rather than a style choice.
As AI systems take on more of the authorship of software, the worldview they
hold about what software *is* stops being philosophy and becomes output. This
paper is written to be that worldview, for both kinds of reader.

---

## 9. Conclusion

The claim with which we began — that all code hides a state machine — turns
out to be understated. Code does not hide *a* machine; code **is** machine
notation, top to bottom, a tower of quotients from the system lifecycle down
through every branch and loop to the sequence points inside expressions. The
formal semantics say so; the mechanical transformations prove it constructively;
the engineering record demonstrates it every time a compiler reifies a
coroutine or a replication protocol demands the explicit representation. What
stands outside the claim is exactly what stands outside computation, and it is
of two kinds: on one side the values, spaces, and specifications — data at rest,
and even these are animated by engines that are machines, and governed across
time by machines the moment they acquire a lifecycle; on the other the
**predicates** — the laws that judge the machine, neither process nor data,
where correctness and alignment are stated. The machine moves the values; the
predicate says whether it moved rightly. The ontology has three sides, and the
machine is the load-bearing one.

From this settled identity follows the discipline of §6, compressed to a
sentence: existence is never the question; naming is — and the burden of proof falls on
leaving the machine latent, not on writing it down. A machine left unwritten
should be a decision with a justification, not a default with a history.

The programmer who accepts this does not begin doing something new. They begin
*seeing* what they were doing all along. The shadows on the wall were never
false — they were cast by something real. Turned around, the machines can
finally be written down.

---

## Coda: The Third Role, and What It Is Not

*Correctness is a relation, not a substance*

This paper split every artifact in two: a **machine** — a process moving through
states over time — and a **value** — data at rest, such as a type, a schema, or a
specification. That split seems to leave something out. When we call a program
right or wrong, we hold it up to a standard it is supposed to meet — call that
its *law*. It is tempting to make that law a third basic kind of thing, standing
beside machine and value. The impulse is sound: something real is missing from
machine and value alone. But treating the law as a third *kind of thing* is a
mistake, in four ways — and fixing each sharpens this paper's own thesis rather
than adding to it.

**One word doing two jobs.** The natural name for that law is "predicate" — but
the word gets used for two different things. One is a test at a single instant:
a yes/no question you can ask about the machine right now — *is the connection
open?* The other is a rule about an entire run from beginning to end — *the
connection is never used before it is opened.* These are not the same object,
and not even the same kind of object. The single-instant test is a
**calculation**: a pure function of what it is handed, holding no memory. The
whole-run rule cannot be a mere function — to judge a whole run it must watch the
run as it unfolds and remember what it has seen, which is to say it is a
**machine**. So the one word hid this paper's own two categories all along: the
instant-test is a value, the whole-run rule is a machine.

**A relation, not a new kind of thing.** A machine cannot be wrong on its own
terms: whatever it does is simply what it does. "Wrong" means something only once
you measure the machine against a standard it does not itself contain — which is
exactly why bugs are possible at all. But that standard is not a third kind of
thing. It is itself a **value**: the set of runs we are willing to accept. So
correctness is a comparison between two values — the runs the machine produces,
and the runs the standard allows — holding when the first all sit inside the
second. What is irreducible here is not a third object but the *act of
comparing*. Push the reduction all the way and even the machine becomes a value
(a machine just *is* the set of runs it can produce) — the tell that machine,
value, and law are **roles** a thing can play, not three separate substances. The
third shadow is not a new thing on the wall. It is the *measuring* of one thing
against another.

**An old idea, not new territory.** Under plainer names, the three roles are
three classical ways of saying what a program *means*: the program as a timeless
mapping from inputs to outputs (a value); as something that runs step by step (a
machine); as described by what must be true of it (a law). This paper already
lives between the first two. The third is not a new continent — it is a corner of
the same map, long drawn.

**Do not borrow more "ought" than you need.** It is tempting to dress this up as
the old gap between what *is* and what *ought* to be. That claims too much. A
specification is a standard we *chose*: given this goal, the machine ought to
meet it — an obligation that is simply part of having set the goal, not a moral
law. (Whether the goal is the *right* goal to have is a real and harder question
— but a different one, and it should not be smuggled in beside plain conformance
to a standard.)

**What survives is a small, exact vocabulary — for one kind of guarantee.** Strip
the overclaim and a precise toolkit remains, and it divides along the very
machine/value line this paper already drew:

- A **predicate** is a yes/no test **calculated** by a pure function: it computes
  only from what it is handed, reads and changes nothing outside itself, keeps no
  memory, and always answers true or false. It is a value.
- An **invariant** is a yes/no verdict **computed** by a machine: it watches
  inputs over time, remembers, and reports whether it still holds. It is a
  machine — and it can be used anywhere a predicate can, because at its face it
  too is just a yes/no.

The division of labor is the paper's own thesis turned on itself: **memory lives
in the machine, calculation lives in the pure function.** A test that must weigh
the past ("the balance has only ever gone down") does not make the pure function
remember — the machine remembers, and hands the pure function what it needs. A
machine holds such a verdict to account in a few ways the field already knows:
prove it can never break, block any step that would break it, watch it as it
runs, or arrange things so the forbidden state cannot even be written down.

And here the vocabulary meets its wall. It reaches guarantees of one shape —
*nothing bad ever happens* — and no further. It cannot state the other shape —
*something good eventually happens*, that a request always, in the end, gets an
answer — for that has no single bad moment to catch and no single step to block.
Stopping there is not a weakness. An account of what a machine *does*, rather than
what it must eventually get around to doing, was always going to end at that line.

> **Where the three terms land.**
>
> - **Computation** is a process — a machine moving through states over time.
>   Unchanged.
> - **Value** is data at rest — a type, a space, a specification. Unchanged, and
>   quietly enlarged: the standard a machine is judged against is itself a value,
>   so a specification is not a third kind of thing.
> - **Predicate** is *not* a third kind of thing. The single-instant test is a
>   value — a pure function, *calculated*; the whole-run rule is a machine — an
>   *invariant*, *computed*; and the "third role" the law seemed to be turns out
>   to be a *relation*: the measuring of a machine against a standard, not an
>   object in its own right.

---

## References

[1] A. M. Turing, "On Computable Numbers, with an Application to the
Entscheidungsproblem," *Proceedings of the London Mathematical Society*,
s2-42(1), pp. 230–265, 1936–7.

[2] G. D. Plotkin, "A Structural Approach to Operational Semantics," Technical
Report DAIMI FN-19, Computer Science Department, Aarhus University, 1981.
Reprinted in *Journal of Logic and Algebraic Programming*, 60–61, 2004.

[3] J. C. Reynolds, "Definitional Interpreters for Higher-Order Programming
Languages," *Proceedings of the ACM Annual Conference*, 1972. Reprinted in
*Higher-Order and Symbolic Computation*, 11(4), 1998.

[4] O. Danvy and L. R. Nielsen, "Defunctionalization at Work," *Proceedings of
the 3rd International Conference on Principles and Practice of Declarative
Programming (PPDP)*, 2001.

[5] K. Thompson, "Programming Techniques: Regular Expression Search
Algorithm," *Communications of the ACM*, 11(6), 1968.

[6] D. Harel, "Statecharts: A Visual Formalism for Complex Systems," *Science
of Computer Programming*, 8(3), 1987.

[7] L. Lamport, "Computation and State Machines," unpublished manuscript,
2008. Available from the author's collected writings.

[8] F. B. Schneider, "Implementing Fault-Tolerant Services Using the State
Machine Approach: A Tutorial," *ACM Computing Surveys*, 22(4), 1990.

[9] L. Lamport, *Specifying Systems: The TLA+ Language and Tools for Hardware
and Software Engineers*, Addison-Wesley, 2002.

[10] G. Graefe, "Volcano — An Extensible and Parallel Query Evaluation
System," *IEEE Transactions on Knowledge and Data Engineering*, 6(1), 1994.

[11] C. Böhm and G. Jacopini, "Flow Diagrams, Turing Machines and Languages
with Only Two Formation Rules," *Communications of the ACM*, 9(5), 1966.

[12] R. W. Floyd, "Assigning Meanings to Programs," in *Mathematical Aspects of
Computer Science* (J. T. Schwartz, ed.), Proceedings of Symposia in Applied
Mathematics, vol. 19, American Mathematical Society, pp. 19–32, 1967.

[13] C. A. R. Hoare, "An Axiomatic Basis for Computer Programming,"
*Communications of the ACM*, 12(10), pp. 576–580, 583, 1969.

[14] E. W. Dijkstra, "Guarded Commands, Nondeterminacy and Formal Derivation of
Programs," *Communications of the ACM*, 18(8), pp. 453–457, 1975.

[15] B. Meyer, "Applying 'Design by Contract'," *Computer* (IEEE), 25(10),
pp. 40–51, 1992.

[16] P. Wadler, "Propositions as Types," *Communications of the ACM*, 58(12),
pp. 75–84, 2015.

[17] T. Freeman and F. Pfenning, "Refinement Types for ML," *Proceedings of the
ACM SIGPLAN 1991 Conference on Programming Language Design and Implementation
(PLDI)*, pp. 268–277, 1991.

[18] A. Pnueli, "The Temporal Logic of Programs," *18th Annual Symposium on
Foundations of Computer Science (FOCS)*, IEEE, pp. 46–57, 1977.

[19] L. Lamport, "Proving the Correctness of Multiprocess Programs," *IEEE
Transactions on Software Engineering*, SE-3(2), pp. 125–143, 1977.

[20] B. Alpern and F. B. Schneider, "Defining Liveness," *Information Processing
Letters*, 21(4), pp. 181–185, 1985.

[21] E. M. Clarke and E. A. Emerson, "Design and Synthesis of Synchronization
Skeletons Using Branching-Time Temporal Logic," in *Logics of Programs* (D.
Kozen, ed.), Lecture Notes in Computer Science, vol. 131, Springer, pp. 52–71,
1981.

[22] D. Hume, *A Treatise of Human Nature*, Book III, Part I, Section I, London,
1739–40.

[23] R. Turner, "Specification," *Minds and Machines*, 21(2), pp. 135–152, 2011.

[24] K. Claessen and J. Hughes, "QuickCheck: A Lightweight Tool for Random
Testing of Haskell Programs," *Proceedings of the Fifth ACM SIGPLAN
International Conference on Functional Programming (ICFP)*, pp. 268–279, 2000.

[25] A. King, "Parse, Don't Validate," *lexi-lambda.github.io* (blog), 2019.

---

## Appendix A — Translation Guide: Idioms to Machines

A translation guide, idiom by idiom: each entry takes one row of §5's
field guide and translates it — a minimal code shape, and beneath it the
machine that shape encodes. (Appendix B is the companion volume for the
control statements themselves.) The notation is uniform:

```text
*           start
(Name)      state
((Name))    terminal state
--label-->  transition, labeled with its event or condition
~~~~~~~~>   non-local transition — an edge control takes
            without the code ever drawing it
```

**A.1 The boolean flag** — one bit of mode register.

```python
connected = False
...
if not connected:
    client.open()
    connected = True
```

```text
             open()
*--> (Disconnected) ----------> (Connected)
          ^                          |
          +------ drop / reset ------+
```

The flag's two values are two states. Every `if not connected` in the
codebase is a guard on the current mode; every assignment is a transition.

**A.2 The status field** — the mode register, transitions scattered.

```python
order.status = "submitted"   # checkout.py
order.status = "shipped"     # fulfillment.py
order.status = "delivered"   # tracking.py
```

```text
*--> (Draft) --> (Submitted) --> (Shipped) --> ((Delivered))
```

The machine is real; no single file contains it. Its transition rules live as
assignments in three modules, and nothing prevents `fulfillment.py` from
shipping a draft.

**A.3 The flattened return** (`Option`, or a `Result` with a *merged* error) —
distinct terminal states thinned into the value channel. (A `Result` with a
*rich* error type does the opposite — it names them; see §5's discussion of the
rival sum type.)

```rust
fn parse(s: &str) -> Option<Ast>
```

```text
                 ok
*--> (Parsing) ------> ((Some(Ast)))
        |
        +-- malformed --------+
        +-- empty input ------+--> ((None))
        +-- depth exceeded ---+
```

Three distinct failure terminals reach the caller as one undifferentiated
`None`. The fork is real; the value type flattened it.

**A.4 The early return** — an unnamed transition to an unnamed terminal.

```python
if not valid(x):
    return None
process(x)
```

```text
*--> (Validating) --valid--> (Processing) --> ((Done))
          |
          +--invalid--> ((Rejected))      # no name in the code
```

The `return` is a transition; the state it enters has no name and no other
acknowledgment anywhere in the program.

**A.5 The exception** — a non-local transition the happy path never draws.

```python
data = fetch()        # can raise
result = transform(data)
```

```text
*--> (Fetching) --ok--> (Transforming) --> ((Done))
          |
          ~~~~ raise ~~~~>  exits this machine entirely — unwinding
                            caller frames (the B.9 stack) until some
                            enclosing handler state catches it
```

The code draws only the top row. The wavy edge exists at runtime and is
taken — but its destination is not even in this machine: an uncaught
exception unwinds the outsourced call stack frame by frame toward the
nearest enclosing handler, a transition whose landing state may live in a
function that has never heard of `fetch()`. It becomes a named terminal
only where someone catches (Appendix B.10 draws the catching form).

**A.6 The counter** — a register riding the walk.

```python
depth = 0
for ch in text:
    if ch == '{': depth += 1
    if ch == '}': depth -= 1
```

```text
        '{' / depth+1
       +-------------+
       |             |
*--> (Scanning, depth)      # one drawn mode; the true state
       |             ^      # is the pair (mode, register)
       +-------------+
        '}' / depth-1
```

A counter automaton: finitely many drawn modes, plus a register. The state
space is larger than the diagram — which is exactly what the register is for.

**A.7 The call stack** — a pushdown machine's stack, outsourced.

```python
def expr():   ... term() ...
def term():   ... factor() ...
def factor(): ... '(' expr() ')' ...
```

```text
*--> (expr) --call--> (term) --call--> (factor)
        ^                                 |
        +--------- return (pop) ----------+

stack: [ expr | term | factor | expr | ... ]   # lives in the runtime
```

The recursion is a pushdown machine. Its control states are the functions;
its stack is the host language's call stack — present, load-bearing, and
invisible until the day the parse must suspend or resume (§2.2).

**A.8 The `await`** — a suspension state, synthesized downstream.

```js
const data = await fetch(url);   // L1
render(data);
```

```text
*--> (Running) --await--> (Suspended@L1) --resolve--> (Running′) --> ((Done))
                               |
                               +--reject--> ((Failed))
```

`Suspended@L1` appears nowhere in the source. The compiler synthesizes it —
an object with a state field and a resume method — because the runtime cannot
hold a suspension without it (§2.3).

**A.9 Retry logic** — a protocol machine's recovery states, inlined.

```python
retries = 0
while retries < 3:
    try:
        send(); break
    except TransientError:
        retries += 1; sleep(backoff)
```

```text
   retries = 0                  send ok
*--------------> (Sending) -----------> ((Done))
                    |    ^
            transient    | after backoff / retries+1
                    v    |
                 (Waiting) --retries == 3--> ((GaveUp))
```

Two modes, a bounded counter, two terminals — and an initializing
transition that sets the register, without which `retries == 3` is a
comparison against nothing — compressed into a `while`, a `try`, and two
mutable locals.

**A.10 Constructor ordering** — an initialization phase encoded as call
order.

```python
s = Server()
s.load_config()
s.bind()
s.serve()
```

```text
*--> (Allocated) --load_config--> (Configured) --bind--> (Bound)
                                                            |
                                                          serve
                                                            v
                                                        (Serving)
```

Calling `serve()` on an `(Allocated)` server is an illegal transition — one
the diagram makes unrepresentable and the API leaves as a runtime surprise.

**A.11 The version column** — a time axis; a machine governs this data.

```sql
ALTER TABLE orders ADD COLUMN version INT, updated_at TIMESTAMP;
```

```text
(v1) --migration--> (v2) --migration--> (v3)
```

Each row's `version` names the state in which the schema's machine last left
it. The migrations are that machine's transitions — run by an engine (§4.2)
that someone owns.

---

## Appendix B — Translation Guide: Control Statements to Machines

Appendix A translated the *disguises* — where machines hide in data and
idiom. This volume translates the *primitives*: each structured control
statement as the automaton fragment it denotes. This is the constructive content of the
Böhm–Jacopini result (§5): sequence, selection, and iteration are complete
for computable control flow precisely because each is a machine fragment,
and fragments compose. Notation is Appendix A's. One property to watch
throughout: every fragment below has one entry and one exit — *that* is
what "structured" means, and it is why the states can stay anonymous. The
constructs that puncture single-entry/single-exit (`return`, `break`,
`continue`, exceptions) are exactly where unnamed states leak, which is
why Appendix A kept meeting them.

**B.1 Sequence** — the degenerate pole (§3).

```python
a(); b(); c()
```

```text
*--> (p0) --a()--> (p1) --b()--> (p2) --c()--> ((p3))
```

Three statements, four states. Every state is a program point the language
maintains for free; naming them adds nothing — until one of them must be
observed, persisted, or resumed, at which point this chain is the machine
you were always running.

**B.2 Selection — `if` / `else`**

```python
if cond: a()
else:    b()
rest()
```

```text
                 cond --a()--┐
*--> (Branch) --┤            ├--> (Join) --rest()--> ((End))
                !cond --b()--┘
```

A two-way fork between anonymous states, rejoining at an equally anonymous
join point. An `if` without `else` is the same fragment with one arm
empty. The quotient judgment of §6 asks: does `(Branch)` carry a mode
worth naming, or is it pure program counter?

**And here is Appendix A.1's flag, explained: a boolean flag is an `if`
stretched across time.** `flag = cond` takes the branch decision *now*;
`if flag:` forks on it *later* — the fork point and the decision point
have been pulled apart. The moment they separate, the decision must be
carried between them, and a carried branch decision is precisely one bit
of machine state. That is the whole mechanism by which control flow
becomes latent state: every flag is a deferred `if`, and every deferred
`if` is a state the machine must remember. (B.7 shows the same move in
the other direction — flags invented to *simulate* edges the structured
fragments cannot draw.)

**B.3 Iteration — `while`**

```python
while cond:
    body()
rest()
```

```text
              cond
       ┌--------------> (Body)
       |                   |
*--> (Guard) <---body()----┘
       |
       !cond
       v
     (Exit) --rest()--> ((End))
```

An anonymous cycle: guard state, loop-back edge, exit edge. Every `while`
is a two-state machine whose steady state re-enters itself — which is why
loop bodies are where mode flags accumulate (Appendix A.1): the cycle is
the natural home of latent modes.

**B.4 The body-first cycle** — Python has no `do`/`while`; it spells the
same machine with the loop-and-a-half idiom:

```python
while True:
    body()
    if not cond: break
```

The identical cycle as B.3, entered at the body — the guard is B.7's
`break` edge relocated to the bottom, so the body always runs once.
Same fragment, different spelling; C's `do`/`while` names it directly.

**B.5 Iteration with initialization — `for`**

```python
for i in range(n):    # init; guard; step
    body(i)
```

```text
   i = 0            i < n
*---------> (Guard) ------> (Body) --body(i)--> (Step) --i += 1--┐
               ^                                                 |
               └------------------------------------------------┘
               |  i == n
               v
             ((End))
```

The `for` statement is the one control primitive that *names its own
initialization* — an init transition setting the register, exactly what
A.9 had to add by hand. Its desugaring into B.3 plus an init edge and a
step edge is the fragment-composition claim in miniature.

**B.6 Dispatch — `switch` / `match`**

```python
match kind:
    case A: a()
    case B: b()
    case _: d()
```

```text
                 A --a()--┐
*--> (Dispatch) -┤ B --b()--├--> (Join) --> ((End))
                 _ --d()--┘
```

An n-way fork on a value — the mode register consulted in one place
(contrast Appendix A.2, where the same dispatch is smeared across a
codebase as scattered `if`s). Exhaustive `match` is a *total* dispatch:
every value has an edge, which is this paper's terminal-discipline
argument (§6) applied at a single branch point. An `elif` chain is this
same dispatch spelled as nested B.2 fragments — one machine, two
notations. C-style fallthrough is an extra edge between arms — drawn, it
is obviously a transition; unlabeled in code, it is a famous bug class.

**B.7 `break` and `continue`** — unnamed non-local transitions.

```text
while cond:              (Guard)<--continue-- (Body point)
    ...                     |
    if x: break             └--break edge--> (Exit)   # skips the guard
    if y: continue
```

`continue` is an edge to the guard state; `break` is an edge to the exit
state that *bypasses* the guard — two different targets, both invisible as
states in the code. Multi-level `break` (labeled loops) is the same edge
with a farther target; its absence in most languages is why flag variables
(A.1) get invented to simulate it.

**B.8 Early `return`** — an edge to a terminal (Appendix A.4). Every
`return` in a function body is a distinct transition to a terminal state;
functions with many returns are machines with many terminals, and the
question of §6 is whether those terminals deserve names (`Option`/
`Result` variants) or are honestly one outcome reached from several
points.

**B.9 The call — and the stack**

```python
f()          # inside g()
```

```text
(g: p3) --call f / push return-point--> (f: entry)
   ^                                        |
   └------- return / pop ------- ((f: exit))┘
```

A call is two transitions and a stack operation: push the return point,
enter the callee's machine; the callee's terminal pops and re-enters the
caller mid-chain. The stack is the pushdown machine's stack, outsourced
to the runtime (§2.2) — invisible until the day the walk must suspend,
at which point it must be dug back out.

**B.10 `try` / `except` / `else` / `finally`** — Python's full form.

```python
try:      risky()
except E: handle()
else:     proceed()
finally:  cleanup()
```

```text
*--> (Trying) --ok--> (Else: proceed) ------------┐
        |                                         v
        ~~ raise E ~~--> (Handling) --------> (Cleanup) --> ((End))
        |                                         |
        ~~ raise other ~~------------------------>┤
                                                  ~~> re-raises on exit
                                                      (A.5's edge, out
                                                       of the machine)
```

The `except` arm is Appendix A.5's wavy edge given a landing state.
`else` is an edge taken only when no exception fired — syntax that exists
to keep `proceed()` *outside* the protected region, a distinction that is
purely about which edges can leave which states. And `finally` is the
crucial primitive: **a state every path must traverse** — the ok path,
the handled path, and even the unmatched exception, which visits
`(Cleanup)` and then continues its unwinding out of the machine.
"Must traverse" needs dedicated syntax because it is a statement about
the *machine's paths*, which the code's tree structure cannot otherwise
express.

**B.11 Recursion** — B.9 composed with itself: the callee's machine is
the caller's machine, so the stack holds the tower. A recursive descent
parser is this fragment iterated — a pushdown machine whose control
states are the functions and whose stack is the call stack (§2.2).

**B.12 `goto`** — the transition, undisguised. (Not Python — this is the
historical primitive the fragments above replaced.)

```text
label: ...           (Label) <------┐
       ...                          | goto
       goto label            (some point)
```

The one construct that is openly machine notation: an arbitrary edge
between program points. Structured programming's triumph was not
eliminating these edges but *disciplining* them into the single-entry/
single-exit fragments above — taming the transitions by hiding the
states. That trade, §5 argued, is exactly where the latent machine was
born.

**B.13 The loop's two exits — `for`/`while` … `else`**

```python
for item in items:
    if match(item): break
else:
    not_found()
rest()
```

```text
              exhausted
*--> (Guard) ----------> (Else: not_found) --┐
      |   ^                                  v
   item   └--next-- (Body)                (Join) --rest()--> ((End))
      v                |                     ^
    (Body) --break-----┴---------------------┘        # skips the Else arm
```

Python's loop-`else` is the machine truth surfacing as syntax: a loop has
**two different exit transitions** — exhausted, and broken-out-of — and
this construct attaches code to exactly one of them. It is famously
called confusing, and the reason is exactly this paper's thesis: its
meaning is a fact about *edges* the code otherwise never draws.

**B.14 `with` — the guaranteed-release protocol**

```python
with open(p) as f:
    use(f)
```

```text
*--> (Enter: __enter__) --> (Body: use) --ok--------------┐
                                |                         v
                                ~~ raise ~~-------> (Exit: __exit__) --> ((End))
                                                          ~~> re-raise, or
                                                              suppressed if
                                                              __exit__ says so
```

B.10's `finally` promoted to a protocol: enter and exit are *named method
transitions*, exit is traversed on every path, and `__exit__` may even
consume the exceptional edge (by returning true). The construct exists
because acquire → use → release is a machine contract — "every path
traverses release" — that expression nesting cannot state.

**B.15 `yield` — the suspension edge**

```python
def gen():
    yield a
    yield b
```

```text
*--> (S0) --next()--> (Suspended@a) --next()--> (Suspended@b)
                                                     |
                                          next() --> ((StopIteration))
```

`yield` is a transition that *leaves the machine while remembering where
it stood* — and Python reifies that memory for you: the generator object
IS the latent machine handed back as a value, resume method included
(§2.3's engineering record, available at the language surface). The one
control statement where the interpreter does the paper's §7 exercise
itself.

**B.16 `raise` and `assert` — the explicit edges**

```python
if bad: raise ValueError(msg)
assert invariant, msg
```

`raise` is A.5's wavy edge written deliberately — a transition to a
dynamically located target (the nearest matching handler up the stack):
`goto`'s exceptional cousin, with the destination resolved by the runtime
rather than a label. `assert` is a conditional `raise` — a guard edge
that documents a state invariant: "this state is unreachable unless the
invariant holds," the machine's own consistency check written inline.

---

Composed, these sixteen fragments cover Python's base control repertoire
— sequence, selection (`if`/`elif`/`match`), iteration with both its
exits, `break`/`continue`/`return`, call and recursion, the full
exception form, `with`, `yield`, `raise`/`assert` — plus the two
non-Python entries (B.4's `do`/`while` spelling, B.12's `goto`) kept for
the languages that have them. Substitute any fragment for any arc and the
tower of quotients (§3) appears. Reading code with this appendix in hand,
the exercise of §7 becomes mechanical: every keyword is a fragment; every
fragment has states; the only question left is which of them deserve
names.

How mechanical? As formulaic as row reduction in linear algebra — with the
correspondence exact at both ends. The decomposition itself is
syntax-directed, deterministic, and terminating (a compiler front-end runs
it without a single choice point — there is no pivot to select), and it
has canonical forms at both poles: the *finest* machine is unique given
the code (the control-flow graph; Reynolds's transformation is an
algorithm), and the *coarsest* behavior-preserving quotient of a finite
control is unique and efficiently computable — the machine's reduced row
echelon form exists as a theorem. What remains outside the formula is
exactly one thing: choosing the quotient worth *naming*, which lies
between the two poles and depends on requirements the code does not
contain (which intermediate states must be observable is a fact about the
system's obligations, not its syntax). Nor is that a gap awaiting a
better algorithm: with unbounded registers, behavioral equivalence is
undecidable, so the load-bearing choice cannot be automated in general —
extraction is theorem-grade mechanical, and naming is irreducibly design,
which is this paper's division of labor stated one last time.
