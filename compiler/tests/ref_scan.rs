//! **The Frame-reference recognizer, as a system, agrees with the hand `frame_ref_at_hand`.**
//!
//! `ref_scan::scan` is generated from `ref_scan.frs`, a `@@[scan(u8)]` Frame system. This
//! proves — by running — that it yields the SAME (kind, name, end) as the hand recognizer
//! at EVERY position of a corpus, across all the context kinds and the reject cases. As of
//! Item 4 Commit C the system holds the LAST production seat (the statement scanner's
//! assign-LHS); the hand fn is oracle-only, deleted at C-final.

use frame_compiler::text::scan::parts::frame_ref_at_hand;
use frame_compiler::text::scan::ref_scan;

fn hand(bytes: &[u8], i: usize) -> Option<(String, String, usize)> {
    frame_ref_at_hand(bytes, i, bytes.len())
        .map(|r| (format!("{:?}", r.kind), r.name, r.span.end))
}

fn machine(bytes: &[u8], i: usize) -> Option<(String, String, usize)> {
    ref_scan::scan(bytes, i).map(|(k, n, e)| (format!("{k:?}"), n, e))
}

fn agree(src: &str) {
    let bytes = src.as_bytes();
    for i in 0..bytes.len() {
        assert_eq!(
            machine(bytes, i),
            hand(bytes, i),
            "disagreement at byte {i} of {src:?}"
        );
    }
}

#[test]
fn state_vars_agree() {
    agree("x = $.count + $.n; y = $.total_amount;");
    agree("$.a $.b $.c");
}

#[test]
fn every_context_kind_agrees() {
    agree("@@:self.factor");
    agree("@@:data.k");
    agree("@@:params.arg");
    agree("@@:return");
    agree("@@:event");
    agree("@@:system");
    agree("a = @@:self.x + @@:params.y - @@:data.z;");
}

#[test]
fn the_reject_cases_agree() {
    // A bare `$` or `@@` without the ref shape, and identifiers that merely start like one.
    agree("$ alone, $x no-dot, @@ pair, @@system decl, plain words");
    agree("email@@host $money");
    agree("");
}

#[test]
fn refs_packed_against_neighbours_agree() {
    agree("f($.a,@@:self.b)+$.c");
    agree("@@:self.a.b.c");   // dotted context path
    agree("$.x$.y");          // adjacent state vars
}

/// Δ5 (T-R1/T-R2) TEETH — the differential PARTITIONS here: the SYSTEM segment-matches and
/// REFUSES an unknown context word (`RefKind::Unknown`, whole word as the name), while the hand
/// `frame_ref_at_hand` oracle stays buggy — it `starts_with`-prefix-matches and defaults an
/// unrecognized word to `ContextSelf`. Pin the divergence + `oracle_stayed_buggy` so the fix
/// can never be agreed vacuously. (The `agree` corpus above is all KNOWN contexts, which
/// segment-match identically — those stay carried.)
#[test]
fn delta5_unknown_and_prefix_diverge_from_the_oracle() {
    use frame_compiler::tree::body::RefKind;
    // (input, the oracle's WRONG kind, the oracle's name = rest after the first `.`)
    for (src, oracle_kind, oracle_name) in [
        ("@@:wat.x", RefKind::ContextSelf, "x"), // T-R1: unknown → oracle defaults ContextSelf
        ("@@:database.k", RefKind::ContextData, "k"), // T-R2: prefix → oracle ContextData
        ("@@:selfish.y", RefKind::ContextSelf, "y"), // T-R2: prefix → oracle ContextSelf
        ("@@:paramsX.z", RefKind::ContextParams, "z"), // T-R2: prefix → oracle ContextParams
    ] {
        let b = src.as_bytes();
        let word = src.strip_prefix("@@:").unwrap();
        // System: Unknown (refusal as data), the WHOLE word as the name.
        let (k, n, _) = ref_scan::scan(b, 0).expect("system recognizes the shape");
        assert_eq!(k, RefKind::Unknown, "system must refuse on {src:?}");
        assert_eq!(n, word, "the whole word is the name on {src:?}");
        // oracle_stayed_buggy: the hand still guesses (prefix / ContextSelf default).
        let r = frame_ref_at_hand(b, 0, b.len()).expect("oracle recognizes");
        assert_eq!(
            (r.kind, r.name.as_str()),
            (oracle_kind, oracle_name),
            "the hand oracle was fixed — the Δ5 teeth are vacuous on {src:?}"
        );
        assert_ne!(k, r.kind, "the system and oracle must DIVERGE (fix teeth) on {src:?}");
    }
}
