//! **ParamScan — the declaration-site header param parser — fixes F5 #1/#3/#4 and CARRIES #2,
//! proven by running.**
//!
//! `param_scan::parse_decl` is generated from `param_scan.frs` (a `@@[scan(u8)]` dual-counter angle
//! fork, `"`-only by composition with StringScan). It replaces the string-BLIND + sigil-BLIND hand
//! split (the retired `ParamSplit` counter + the native `strip_prefix`/`trim_end_matches` sigil parse
//! that used to live in `scan/mod.rs::split_system_params`). From the ALREADY-`(`-balanced interior it
//! emits, per param, its GROUP (`$(`=state / `$>(`=enter / bare=domain) and its TRIMMED body.
//!
//! F5 fixes proven here:
//!   * #3 — a `$>(` enter-sigil's `>` is consumed as part of the 3-byte sigil, NEVER bracket-counted,
//!     so a TRAILING domain param after an enter group is no longer silently dropped
//!     (`f5_sigil_gt_miscount_drops_trailing_param`, UN-`#[ignore]`d, must PASS);
//!   * #1 — an operator/shift `<`/`>` in a top-level default no longer merges params (the angle fork
//!     clears `g_viable` and falls back to the operator reading);
//!   * #4 — a group's BALANCED `)` is found by the walk, so `$(g: int = f(1))` keeps `f(1)` (the hand
//!     `trim_end_matches(')')` truncation to `f(1` is GONE).
//! F5 #2 CARRIED (recorded, not assumed): a `'…'` char/string default is still miscounted — StringScan
//! is `"`-only (dodging the Rust `'a`-lifetime hazard AND agreeing with ParenBalance's interior
//! boundary). The double-quote form of the SAME structure IS fixed — proving the gap is exactly the
//! `"`-only limit.
//!
//! Coverage strategy:
//!   * a RESTRICTED carried differential + fuzz (`split_extents_hand`) over inputs with NO
//!     `< > $( $>( "` — where the angle fork, sigil recognition and opacity are ALL inert, so ParamScan
//!     must split at exactly the string-blind hand's top-level commas (the boring cases did not regress);
//!   * the FIXED teeth (#1/#3/#4) and the combined named-fix case, asserted directly on `parse_decl`
//!     and end-to-end through the real `segment()` pipeline;
//!   * the #2 CARRIED pin (single-quote) with its double-quote FIXED contrast;
//!   * the MILESTONE — a `@@system` header whose default carries a `,` inside a `"…"`-string splits
//!     into the correct param count.
//!
//! Every test here is SCAFFOLDING (it depends on the restricted string-blind oracle `split_extents_hand`
//! or the cleanroom-only `@@[scan(u8)]`-on-`@@system` capability / the internal `segment()` tree API);
//! it NEVER promotes to the cross-language corpus.

use frame_compiler::text::scan::{param_scan, segment};
use frame_compiler::tree::body::ParamGroup;
use frame_compiler::tree::{Item, SystemParams};
use frame_compiler::Source;

// ============================================================================
// END-TO-END HELPER — build a minimal `@@system S(<interior>) { ... }`, run the real `segment()`
// pipeline, and return its SystemParams. This is the production path (split_system_params →
// param_scan::parse_decl), not a direct FSM poke.
// ============================================================================

fn sysparams(interior: &str) -> SystemParams {
    let text = format!(
        "@@system S({interior}) {{\n\
         \x20   interface:\n\
         \x20       go()\n\
         \x20   machine:\n\
         \x20       $A {{ go() {{ }} }}\n\
         }}\n"
    );
    let src = Source::new("t.frm", text.into_bytes()).unwrap();
    let ast = segment(&src, frame_compiler::text::scan::literals::Target::Rust).unwrap();
    ast.items
        .iter()
        .find_map(|it| match it {
            Item::System(s) => Some(s.params.clone()),
            _ => None,
        })
        .expect("expected exactly one @@system")
}

// ============================================================================
// RESTRICTED DIFFERENTIAL ORACLE — the exact pre-conversion string-BLIND depth-0 comma loop, used
// ONLY on inputs with NO `< > $( $>( "`, where the angle fork / sigil recognition / opacity are all
// inert and ParamScan must split at exactly these commas. Deleted at C-final; until then it keeps the
// no-regression claim on the boring cases honest.
// ============================================================================

fn split_extents_hand(bytes: &[u8]) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut depth = 0i32;
    let mut part_start = 0usize;
    let mut i = 0usize;
    while i < bytes.len() {
        let b = bytes[i];
        if b == b',' && depth == 0 {
            out.push((part_start, i));
            part_start = i + 1;
        }
        if b == b'(' || b == b'[' || b == b'<' || b == b'{' {
            depth += 1;
        }
        if b == b')' || b == b']' || b == b'>' || b == b'}' {
            depth -= 1;
        }
        i += 1;
    }
    if part_start < bytes.len() {
        out.push((part_start, bytes.len()));
    }
    out
}

/// True iff `s` is in the restricted alphabet (no `< > $( $>( "` — the differential is only valid
/// where the angle fork, sigil recognition, and `"`-opacity are all inert).
fn is_restricted(s: &[u8]) -> bool {
    !s.iter()
        .any(|&b| b == b'<' || b == b'>' || b == b'$' || b == b'"')
}

/// True iff the merged-Dyck depth over `([{` / `)]}` never goes negative — i.e. no stray closer.
/// This is the ADDITIONAL precondition for the string-blind hand to be a valid oracle: on a stray
/// closer, ParamScan fires the StrayCloser REFUSAL (named degradation, inherited from ArgScan and
/// taken verbatim to end), whereas the hand drives depth negative and keeps splitting — a DELIBERATE
/// divergence, pinned separately in `stray_closer_refuses_to_verbatim`. The production interior is
/// always `()`-balanced (ParenBalance) with `>` off the depth counter, so a depth-negative interior
/// only arises from genuinely malformed `]`/`}` input.
fn hand_well_nested(bytes: &[u8]) -> bool {
    let mut depth = 0i32;
    for &b in bytes {
        if b == b'(' || b == b'[' || b == b'{' {
            depth += 1;
        }
        if b == b')' || b == b']' || b == b'}' {
            depth -= 1;
            if depth < 0 {
                return false;
            }
        }
    }
    true
}

/// On a restricted input, ParamScan's parts (all Domain) must split at exactly the string-blind
/// hand's top-level commas — same trimmed, non-empty bodies, in order.
fn assert_restricted_agrees(s: &str) {
    let b = s.as_bytes();
    assert!(is_restricted(b), "assert_restricted_agrees given a non-restricted input {s:?}");
    assert!(
        hand_well_nested(b),
        "assert_restricted_agrees given a stray-closer input {s:?} — ParamScan refuses there \
         (see stray_closer_refuses_to_verbatim); the string-blind hand is not a valid oracle"
    );
    let got: Vec<String> = param_scan::parse_decl(b)
        .into_iter()
        .map(|(g, body)| {
            assert_eq!(g, ParamGroup::Domain, "restricted input yielded a non-Domain group: {s:?}");
            body
        })
        .collect();
    let want: Vec<String> = split_extents_hand(b)
        .into_iter()
        .map(|(lo, hi)| s[lo..hi].trim().to_string())
        .filter(|t| !t.is_empty()) // ParamScan drops empty comma-segments (the old `if raw.is_empty() { continue }`)
        .collect();
    assert_eq!(got, want, "restricted differential divergence on {s:?}");
}

// ============================================================================
// RESTRICTED CARRIED CORPUS — no `< > $ "`. ParamScan splits at exactly the hand's top-level commas.
// (The angle/sigil inputs that used to live here have MOVED to the FIXED section below.)
// ============================================================================

#[test]
fn restricted_carried_agrees() {
    for s in [
        "",
        ",",
        ",,",
        "a",
        "a,",
        ",a",
        "a,b",
        "a,,b",
        "a, b, c",
        "  a  ,  b  ",
        "f(a, b), g(c, d)",     // () protects interior commas
        "[a, b], {c, d}",       // [] and {} protect
        "f(g(h(1,2),3),4), y",  // deep nesting
        "(((",                  // unbalanced opens
        "a(b,c",                // unbalanced open → comma protected
        "a: T = 0, b: U = 1",   // realistic domain params
        "n: N = 0",             // single typed default
    ] {
        assert_restricted_agrees(s);
    }
}

// ============================================================================
// F5 #3 — the un-`#[ignore]`d pin. A `$>(` enter-sigil's `>` is NO LONGER bracket-counted, so the
// trailing domain param `name: String` is kept. MUST PASS.
// ============================================================================

#[test]
fn f5_sigil_gt_miscount_drops_trailing_param() {
    let p = sysparams("$(slot: int), $>(timeout: int), name: String");
    assert_eq!(p.state.len(), 1, "one $() state param (slot)");
    assert_eq!(p.state[0].name, "slot");
    assert_eq!(p.state[0].ty.as_deref(), Some("int"));
    assert_eq!(p.enter.len(), 1, "one $>() enter param (timeout)");
    assert_eq!(p.enter[0].name, "timeout");
    assert_eq!(
        p.enter[0].ty.as_deref(),
        Some("int"),
        "the enter type must be `int`, not the swallowed `int), name: String`"
    );
    assert_eq!(p.domain.len(), 1, "the trailing `name: String` domain param must be KEPT (F5 #3)");
    assert_eq!(p.domain[0].name, "name");
    assert_eq!(p.domain[0].ty.as_deref(), Some("String"));
}

// ============================================================================
// The old carried `"$(x), $>(y), z"` case — MOVED here from the carried corpus (it pinned the buggy
// 2-part behavior). Now it is the correct 3-group parse.
// ============================================================================

#[test]
fn sigil_forms_three_groups() {
    let parts = param_scan::parse_decl(b"$(x), $>(y), z");
    assert_eq!(
        parts,
        vec![
            (ParamGroup::State, "x".to_string()),
            (ParamGroup::Enter, "y".to_string()),
            (ParamGroup::Domain, "z".to_string()),
        ],
        "sigil forms must parse to state x / enter y / domain z"
    );

    // End-to-end shape too.
    let p = sysparams("$(x), $>(y), z");
    assert_eq!(p.state.len(), 1);
    assert_eq!(p.state[0].name, "x");
    assert_eq!(p.enter.len(), 1);
    assert_eq!(p.enter[0].name, "y");
    assert_eq!(p.domain.len(), 1);
    assert_eq!(p.domain[0].name, "z");
}

// ============================================================================
// F5 #4 — the nested-paren group default. A group's BALANCED `)` is found, so `$(g: int = f(1))`
// keeps `f(1)`; the hand `trim_end_matches(')')` truncated it to `f(1`.
// ============================================================================

#[test]
fn f5_nested_paren_group_default_kept() {
    let parts = param_scan::parse_decl(b"$(g: int = f(1))");
    assert_eq!(
        parts,
        vec![(ParamGroup::State, "g: int = f(1)".to_string())],
        "the group's balanced `)` must be found — `f(1)` intact, not truncated to `f(1`"
    );

    let p = sysparams("$(g: int = f(1))");
    assert_eq!(p.state.len(), 1);
    assert_eq!(p.state[0].name, "g");
    assert_eq!(p.state[0].ty.as_deref(), Some("int"));
    assert_eq!(
        p.state[0].default.as_deref(),
        Some("f(1)"),
        "the default must be `f(1)` — NOT the hand's truncated `f(1`"
    );
}

// ============================================================================
// F5 #1 — an operator/shift `<`/`>` in a top-level default. The angle fork clears `g_viable` and
// falls back to the operator reading, so the params split correctly (they used to MERGE / underflow).
// ============================================================================

#[test]
fn f5_operator_angle_in_default_splits() {
    // Unclosed `<` (relational/shift): g_viable cleared at end-of-interior → operator reading.
    let p = sysparams("a: int = x < y, b: int");
    assert_eq!(p.domain.len(), 2, "operator `<` must not merge the two params (F5 #1)");
    assert_eq!(p.domain[0].name, "a");
    assert_eq!(p.domain[0].default.as_deref(), Some("x < y"));
    assert_eq!(p.domain[1].name, "b");
    assert_eq!(p.domain[1].ty.as_deref(), Some("int"));

    // Lone `>` (would UNDERFLOW the old merged counter): g_viable cleared immediately → operator reading.
    let q = sysparams("a: int = p > q, b: int");
    assert_eq!(q.domain.len(), 2, "operator `>` must not underflow / merge (F5 #1)");
    assert_eq!(q.domain[0].name, "a");
    assert_eq!(q.domain[0].default.as_deref(), Some("p > q"));
    assert_eq!(q.domain[1].name, "b");
}

// ============================================================================
// GENERIC PRESERVED — `Map<K, V>` is a self-consistent angle pair (g_viable), so the interior comma
// is protected and the type is kept whole. (Carried-correct before by luck; now correct by the fork.)
// ============================================================================

#[test]
fn generic_type_default_preserved() {
    let p = sysparams("c: Map<K, V> = d");
    assert_eq!(p.domain.len(), 1, "the `,` inside `Map<K, V>` must be protected (one param)");
    assert_eq!(p.domain[0].name, "c");
    assert_eq!(p.domain[0].ty.as_deref(), Some("Map<K, V>"));
    assert_eq!(p.domain[0].default.as_deref(), Some("d"));
}

// ============================================================================
// THE COMBINED NAMED-FIX CASE — `$(a: T), $>(b: T = "x,y"), c: Map<K,V> = d`. Sigil groups + a
// string-protected comma + a self-consistent generic, all in one. Full `parse_decl` + end-to-end.
// ============================================================================

#[test]
fn named_fix_case_full_parse() {
    let parts = param_scan::parse_decl(br#"$(a: T), $>(b: T = "x,y"), c: Map<K,V> = d"#);
    assert_eq!(
        parts,
        vec![
            (ParamGroup::State, "a: T".to_string()),
            (ParamGroup::Enter, r#"b: T = "x,y""#.to_string()),
            (ParamGroup::Domain, "c: Map<K,V> = d".to_string()),
        ],
        "combined sigil + string-protected comma + generic must parse to exactly 3 groups"
    );

    let p = sysparams(r#"$(a: T), $>(b: T = "x,y"), c: Map<K,V> = d"#);
    assert_eq!(p.state.len(), 1);
    assert_eq!(p.state[0].name, "a");
    assert_eq!(p.state[0].ty.as_deref(), Some("T"));
    assert_eq!(p.enter.len(), 1);
    assert_eq!(p.enter[0].name, "b");
    assert_eq!(p.enter[0].ty.as_deref(), Some("T"));
    assert_eq!(
        p.enter[0].default.as_deref(),
        Some("\"x,y\""),
        "the `\"x,y\"` string default must be intact (the interior comma was protected)"
    );
    assert_eq!(p.domain.len(), 1);
    assert_eq!(p.domain[0].name, "c");
    assert_eq!(p.domain[0].ty.as_deref(), Some("Map<K,V>"));
    assert_eq!(p.domain[0].default.as_deref(), Some("d"));
}

// ============================================================================
// F5 #2 CARRIED PIN — the `"`-only boundary, recorded not assumed. A single-quote `= ')'` default is
// STILL miscounted (StringScan is `"`-only): the `)` inside `')'` is a StrayCloser, so the param count
// is a carried MISS (2, not the single-quote-aware 3). The double-quote form of the SAME structure IS
// fixed — proving the gap is exactly the `"`-only limit, not a deeper defect.
// ============================================================================

#[test]
fn carried_gap_single_quote_still_miscounts() {
    // `a, b = ')', c` — single-quote-aware would be 3 domain params; `"`-only yields a carried MISS.
    let p = sysparams("a, b = ')', c");
    assert_eq!(
        p.domain.len(),
        2,
        "the `)` in `')'` is miscounted — a semantic MISS, carried by design (`\"`-only)"
    );
    assert!(p.state.is_empty());
    assert!(p.enter.is_empty());

    // Contrast: the SAME structure with DOUBLE quotes IS fixed — `")"` is skipped, the comma splits.
    let q = sysparams("a, b = \")\", c");
    assert_eq!(q.domain.len(), 3, "double-quote `)` is skipped — the comma splits (FIXED)");
    assert_eq!(q.domain[0].name, "a");
    assert_eq!(q.domain[1].name, "b");
    assert_eq!(q.domain[1].default.as_deref(), Some("\")\""));
    assert_eq!(q.domain[2].name, "c");
}

// ============================================================================
// MILESTONE — end to end through `segment()`. A `@@system` header whose param default carries a `,`
// inside a `"…"`-string must split into the CORRECT param count: the string comma is protected.
// ============================================================================

#[test]
fn milestone_system_header_comma_in_string_default_is_protected() {
    let p = sysparams("a: A, msg: String = \"x,y\", c: C");
    assert!(p.state.is_empty(), "no $() state params expected");
    assert!(p.enter.is_empty(), "no $>() enter params expected");
    assert_eq!(
        p.domain.len(),
        3,
        "param split corrupted (string comma not protected): {:#?}",
        p.domain
    );
    assert_eq!(p.domain[0].name, "a");
    assert_eq!(p.domain[0].ty.as_deref(), Some("A"));
    assert_eq!(p.domain[1].name, "msg");
    assert_eq!(p.domain[1].ty.as_deref(), Some("String"));
    assert_eq!(
        p.domain[1].default.as_deref(),
        Some("\"x,y\""),
        "the middle default was truncated — the string comma was miscounted"
    );
    assert_eq!(p.domain[2].name, "c");
    assert_eq!(p.domain[2].ty.as_deref(), Some("C"));
}

// ============================================================================
// DETERMINISTIC FUZZ — xorshift64*, random RESTRICTED interiors (no `< > $ "`), so the angle fork /
// sigil recognition / opacity are inert and the string-blind hand is a valid oracle. Every case must
// (a) be deterministic (a leaked register would break this) and (b) agree with the hand's top-level
// comma split. Plus a coverage gate: the fuzz must reach multi-part splits (else the arm is vacuous).
// ============================================================================

struct Rng(u64);
impl Rng {
    fn new(seed: u64) -> Rng {
        let mut s = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15).wrapping_add(0x1234_5678);
        if s == 0 {
            s = 0xDEAD_BEEF;
        }
        Rng(s)
    }
    fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next_u64() % (n as u64)) as usize
    }
}

// Restricted fragments only: idents, `:`, `=`, the THREE non-angle bracket kinds, commas, whitespace.
// No `< > $ "` — so the differential oracle is valid.
const FRAGMENTS: &[&[u8]] = &[
    b"a", b"bb", b"x1", b"_id", b"0", b"42", b":", b" = ", b", ", b",", b" ", b"\t", b"(", b")",
    b"[", b"]", b"{", b"}", b"()", b"[]", b"{}", b"f(1, 2)", b"T", b"K", b"V",
];

fn gen_interior(rng: &mut Rng, max_frags: usize) -> Vec<u8> {
    let n = rng.below(max_frags + 1);
    let mut v: Vec<u8> = Vec::new();
    for _ in 0..n {
        v.extend_from_slice(FRAGMENTS[rng.below(FRAGMENTS.len())]);
    }
    v
}

#[test]
fn fuzz_restricted_differential() {
    let mut multi = 0usize;
    let mut nested = 0usize;
    for seed in 0u64..12000 {
        let mut rng = Rng::new(seed ^ 0x9A5C_0000);
        let b = gen_interior(&mut rng, 10);
        debug_assert!(is_restricted(&b));
        // Determinism holds on EVERY input (a leaked register would break this).
        let m1 = param_scan::parse_decl(&b);
        let m2 = param_scan::parse_decl(&b);
        assert_eq!(m1, m2, "nondeterminism: seed {seed} of {b:?}");
        // Differential against the string-blind hand — valid ONLY on well-nested inputs (a stray
        // closer triggers ParamScan's intended StrayCloser refusal, pinned separately).
        if hand_well_nested(&b) {
            nested += 1;
            let s = String::from_utf8(b.clone()).unwrap();
            assert_restricted_agrees(&s);
            if m1.len() >= 2 {
                multi += 1;
            }
        }
    }
    assert!(nested > 2000, "fuzz reached too few well-nested inputs ({nested}) — the arm is thin");
    assert!(multi > 200, "fuzz reached too few multi-part splits ({multi}) — the arm is vacuous");
}

// ============================================================================
// STRAY-CLOSER REFUSAL (the intended divergence from the string-blind hand, RECORDED). A closer with
// no matching opener drives the old string-blind loop's depth negative (and it keeps splitting);
// ParamScan instead fires the named StrayCloser refusal (inherited from ArgScan) and takes the rest
// VERBATIM. Only reachable on malformed `]`/`}` input — the production interior is `()`-balanced
// (ParenBalance) with `>` off the depth counter.
// ============================================================================

#[test]
fn stray_closer_refuses_to_verbatim() {
    // The string-blind hand splits at the depth-0 comma (`]` → -1, `(` → 0, `,` at 0) → 2 parts.
    let hand = split_extents_hand(b"a](b, c");
    assert_eq!(hand.len(), 2, "the string-blind hand splits after recovering depth to 0");
    // ParamScan refuses at the stray `]` and takes the rest verbatim → 1 part.
    let parts = param_scan::parse_decl(b"a](b, c");
    assert_eq!(parts.len(), 1, "stray closer → one verbatim part (named StrayCloser refusal)");
    assert_eq!(parts[0], (ParamGroup::Domain, "a](b, c".to_string()));
}

// ============================================================================
// KNOWN LIMITATION #248 — param-list `<>` disambiguation FAVORS THE TEMPLATE (the old,
// angle-aware `parse_type` reading: count `<`/`>`, merge when they balance). It is correct for
// every generic type and for the associated-type binding `x: Map<K, Item = V>`. The ONE case it
// gets wrong is a BALANCED comparison-operator straddle across adjacent defaults: the `<` of one
// default and the `>` of the next balance across the comma, so the two params MERGE into one
// (the second is folded into the first's default). ACCEPTED — favor the template when there is
// no clean signal (owner decision 2026-07-21). Stakes are bounded: it is rare, and the mis-merge
// emits mal-formed native the target compiler rejects LOUDLY (not silent); the workaround is to
// parenthesize (`= (x < y)`). Future work — type-hint extraction + an ambiguity warning — is
// tracked in https://github.com/frame-lang/framec/issues/248. Two fixtures document it: the
// CURRENT accepted behavior (pinned, runs) and the IDEAL (skipped until #248 lands).
// ============================================================================

#[test]
fn limitation_248_operator_straddle_current_favors_template() {
    // CURRENT ACCEPTED BEHAVIOR (favor the template): the balanced `<`…`>` merges the two
    // comparison-default params into ONE (the second, `b`, folds into `a`'s default) — the old
    // angle-aware `parse_type` reading. Pinned so any change to the policy is noticed.
    let parts = param_scan::parse_decl(b"a: int = x < y, b: int = z > w");
    assert_eq!(
        parts.len(),
        1,
        "favor-the-template: the operator straddle merges to one param (accepted; see #248)"
    );
    assert_eq!(
        parts[0],
        (ParamGroup::Domain, "a: int = x < y, b: int = z > w".to_string())
    );
}

#[test]
#[ignore = "KNOWN LIMITATION #248: the param-list `<>` split FAVORS THE TEMPLATE, so a balanced \
            comparison-operator straddle across adjacent defaults merges into one param. Accepted \
            (favor the template — the old way); a real fix needs type-hint extraction. This asserts \
            the IDEAL 2-param split — un-ignore when #248 lands."]
fn limitation_248_operator_straddle_ideal_two_params() {
    // IDEAL: `a: int = x < y, b: int = z > w` is TWO params, each a comparison default. Favoring
    // the template loses this (see the current-behavior test above). Tracked in issue #248.
    let parts = param_scan::parse_decl(b"a: int = x < y, b: int = z > w");
    assert_eq!(parts.len(), 2, "#248: the operator straddle should be two params");
    assert_eq!(parts[0], (ParamGroup::Domain, "a: int = x < y".to_string()));
    assert_eq!(parts[1], (ParamGroup::Domain, "b: int = z > w".to_string()));
}
