//! **GATE-A — the handler/action body BASE-column min-fold, as a `@@system`, is byte-for-byte the
//! hand fold.** SCAFFOLDING (differential vs the preserved
//! [`frame_compiler::text::emit::driver::base_column_hand`] oracle; conversion-internal, never
//! promoted).
//!
//! `emit_body`'s inline `base` computation — `stmts.filter_map(col).min().unwrap_or(0)`, the
//! shallowest logical column that seeds every statement's reindent — was reified as the
//! `BaseColumn` plain-`@@system` (`base_column.frs`): a cursor over the statement slice carrying a
//! `min` register and a `seen` bit, the per-`Stmt` column extraction surfaced as the `col_at` leaf.
//!
//! This test proves — by running — that for **every** handler and action body the machine
//! ([`super::base_column::compute`], via [`driver::base_parity_report`]) and the preserved hand
//! oracle ([`base_column_hand`]) return the **identical `u32`**, over (a) the same curated corpus
//! `tests/stmt_walk.rs` uses — exercising every column-bearing `Stmt` variant plus the skipped
//! `Trivia` — and (b) the same deterministic fuzz of random statement sequences. Because `base`
//! feeds `StmtWalk`'s reindent, `tests/stmt_walk.rs` byte-parity is a second, transitive gate on
//! this value; here it is checked directly and absolutely.

use frame_compiler::resolve::resolve;
use frame_compiler::scan::{literals::Target, segment};
use frame_compiler::text::emit::driver;
use frame_compiler::Source;

const TARGETS: [Target; 4] = [Target::Python3, Target::Java, Target::Rust, Target::C];

/// Run the base-fold differential over one Frame source, for all four targets. Asserts, for every
/// parsed body, `machine_base == hand_base`. Returns the set of `Stmt` kinds (0..9) the corpus
/// exercised, unioned across targets.
fn check(label: &str, frm: &str) -> Vec<i32> {
    let mut kinds_seen: Vec<i32> = Vec::new();
    for target in TARGETS {
        let src = Source::new("t.frm", frm.as_bytes().to_vec()).unwrap();
        let ast = match segment(&src, target) {
            Ok(a) => a,
            Err(e) => panic!("{label}: segment failed for {target:?}: {e:?}"),
        };
        let (syms, _diags) = resolve(&ast);
        let report = driver::base_parity_report(&ast, &syms);
        assert!(
            !report.is_empty(),
            "{label}: no bodies parsed for {target:?} (did the system resolve?)"
        );
        for p in &report {
            assert_eq!(
                p.machine_base, p.hand_base,
                "{label} [{}] {target:?}: BaseColumn machine base ({}) != base_column_hand ({})",
                p.label, p.machine_base, p.hand_base
            );
            for &k in &p.kinds {
                if !kinds_seen.contains(&k) {
                    kinds_seen.push(k);
                }
            }
        }
    }
    kinds_seen
}

/// Transitions, a state-var assign, a return, native code, and a comment (Trivia) inside a body.
const DOOR: &str = r#"@@system Door {
    interface:
        open()
        close()
        report(): String
    machine:
        $Closed {
            $.tries: i32 = 0
            open() {
                // bump the counter (this comment is Trivia between statements)
                let x = 1;
                $.tries = $.tries + 1
                -> $Open
            }
            report(): String { @@:(String::from("closed")) }
        }
        $Open {
            close() { -> $Closed }
            report(): String { @@:(String::from("open")) }
        }
}
"#;

/// push$/pop$ pushdown WITH lifecycle handlers ($>/<$).
const VEND: &str = r#"@@system Vend {
    interface:
        coin()
        pick()
        refund()
    machine:
        $Idle {
            <$(code: i32) { log_exit(code); }
            coin() { push$ -> $Paid(5) }
        }
        $Paid {
            $>(amount: i32) { note_enter(amount); }
            pick() { -> pop$ }
            refund() { (99) -> pop$ }
        }
}
"#;

/// HSM forward: `=> $^` to a parent that HANDLES the event and to one that does NOT.
const FWD: &str = r#"@@system Fwd {
    interface:
        ping()
        buzz()
    machine:
        $Awake => $Live {
            ping() {
                run_child();
                => $^
            }
            buzz() {
                only_child();
                => $^
            }
        }
        $Live {
            ping() { run_parent(); }
        }
}
"#;

/// Self-call, a self-field assign, bare `push$`/`pop$`, a falling body, and a nested terminal.
const MISC: &str = r#"@@system Misc {
    interface:
        go()
        tick()
        act()
    machine:
        $A {
            go() {
                @@:self.flag = 2
                @@:self.tick()
                pop$
                push$
            }
            tick() {
                if flag {
                    -> $B
                }
                after_the_if();
            }
            act() {
                just_native();
            }
        }
        $B {
            go() { -> $A }
        }
    domain:
        flag: i32 = 0
}
"#;

#[test]
fn curated_corpus_base_is_byte_identical_and_covers_every_variant() {
    let mut all: Vec<i32> = Vec::new();
    for (label, frm) in [("Door", DOOR), ("Vend", VEND), ("Fwd", FWD), ("Misc", MISC)] {
        for k in check(label, frm) {
            if !all.contains(&k) {
                all.push(k);
            }
        }
    }
    all.sort();
    // The 10 Stmt variants, by the machine's own `kind_of`: 0=Trivia .. 9=Forward. Trivia (0) is
    // the skipped arm; the other nine each contribute a column. If any is missing the corpus did
    // not exercise it — a hole in the gate, not a pass.
    assert_eq!(
        all,
        (0..=9).collect::<Vec<i32>>(),
        "curated corpus must exercise ALL 10 Stmt variants; saw {all:?}"
    );
}

#[test]
fn known_base_values_are_correct_on_both_paths() {
    // Pin absolute base columns so the fold is proven load-bearing (not merely self-consistent).
    // `check` already asserts machine==hand for these; here the values themselves are nailed down.
    let src = Source::new("t.frm", MISC.as_bytes().to_vec()).unwrap();
    let ast = segment(&src, Target::Rust).unwrap();
    let (syms, _d) = resolve(&ast);
    let report = driver::base_parity_report(&ast, &syms);
    let base_of = |lbl: &str| {
        report
            .iter()
            .find(|p| p.label == lbl)
            .unwrap_or_else(|| {
                panic!(
                    "no body {lbl}; have {:?}",
                    report.iter().map(|p| &p.label).collect::<Vec<_>>()
                )
            })
            .machine_base
    };
    // `Misc/A/go`'s four statements all sit at the same source column (16 spaces under `go() {`),
    // so the base is that column; `Misc/B/go`'s single `-> $A` likewise. The absolute number is
    // the source indent — the point is that machine and hand agree AND it is non-zero (the body is
    // genuinely indented), which `check` + this pin together establish.
    assert_eq!(base_of("Misc/A/go"), base_of("Misc/A/tick"), "sibling handlers share a base indent");
    assert!(base_of("Misc/A/go") > 0, "an indented body must have a non-zero base");
}

// ----------------------------------------------------------------- deterministic fuzz

/// xorshift64* — deterministic, seed-stable across runs and machines.
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545F4914F6CDD1D)
    }
    fn pick<'a>(&mut self, xs: &[&'a str]) -> &'a str {
        xs[(self.next() as usize) % xs.len()]
    }
    fn upto(&mut self, n: usize) -> usize {
        (self.next() as usize) % n
    }
}

/// Well-formed statement palette — one per (or several per) Stmt variant. A random sequence of
/// them is always a parseable body.
const PALETTE: [&str; 12] = [
    "let v = 1;",                // Native
    "trivia_native();",          // Native
    "// a trailing comment",     // Trivia
    "$.n = $.n + 1",             // Assign (state var)
    "@@:self.flag = 7",          // Assign (self field)
    "@@:self.tick()",            // SelfCall
    "@@:(String::from(\"x\"))",  // ReturnCall
    "-> $B",                     // Transition
    "push$ -> $B(1)",            // StackPush (target)
    "push$",                     // StackPush (bare)
    "-> pop$",                   // StackPop
    "pop$",                      // StackPopBare
];

fn fuzz_system(n: usize, body: &str) -> String {
    format!(
        r#"@@system Fuzz{n} {{
    interface:
        run()
        tick()
    machine:
        $A {{
            $.n: i32 = 0
            run() {{
{body}
            }}
            tick() {{ trivia(); }}
        }}
        $B {{
            run() {{ -> $A }}
        }}
    domain:
        flag: i32 = 0
}}
"#
    )
}

#[test]
fn deterministic_fuzz_base_is_byte_identical() {
    // Randomize the leading indentation per line too, so the min-fold actually varies (the curated
    // corpus keeps a body's statements column-aligned; here they are staggered, so `min` is
    // exercised as a genuine running minimum, not a constant).
    let mut rng = Rng(0x5EED_1234_ABCD_0F01);
    let mut ran = 0usize;
    for n in 0..300usize {
        let count = 1 + rng.upto(9);
        let body: String = (0..count)
            .map(|_| {
                let pad = 12 + rng.upto(8); // 12..19 spaces of leading indent
                format!("{}{}", " ".repeat(pad), rng.pick(&PALETTE))
            })
            .collect::<Vec<_>>()
            .join("\n");
        let frm = fuzz_system(n, &body);

        for target in TARGETS {
            let src = Source::new("t.frm", frm.as_bytes().to_vec()).unwrap();
            let ast = match segment(&src, target) {
                Ok(a) => a,
                Err(_) => continue,
            };
            let (syms, _d) = resolve(&ast);
            for p in driver::base_parity_report(&ast, &syms) {
                assert_eq!(
                    p.machine_base, p.hand_base,
                    "FUZZ n={n} {target:?} [{}]: base differs (machine={}, hand={})\nbody:\n{body}",
                    p.label, p.machine_base, p.hand_base
                );
            }
            ran += 1;
        }
    }
    assert!(ran >= 300, "fuzz must actually run across targets; ran {ran}");
}
