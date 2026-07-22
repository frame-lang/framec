//! **Parser-bug corpus — durable regression fixtures for the scan/parse layer.**
//!
//! This file is the committed home of a just-completed empirical audit (workflow
//! `wf_dabc5851-68d`): 31 empirically-confirmed candidates, each run against the real
//! cleanroom scanners (`ran=True`), here deduplicated and pinned so that the exact CURRENT
//! behavior is a named test — a green run means "no drift", not "no bug". Every candidate
//! falls into one of three dispositions:
//!
//!   * **FIXED** — a verified-correct behavior. ONE passing test guards it (a regression here
//!     would be a genuine loss). Cited: what it guards.
//!   * **CARRIED** — a documented/accepted limitation (owner-decided, with a named void
//!     condition). ONE passing test pins the CURRENT accepted output, so any drift is caught;
//!     the doc-comment states the IDEAL and the tracking issue (#219 / #248 where applicable).
//!   * **OPEN** — a real, un-accepted parser bug. BOTH a passing test pinning the current
//!     (buggy) output AND an `#[ignore]`d test asserting the IDEAL, ignore-reason citing
//!     **issue #249** ("KNOWN BUG #249 (Bx): …; un-ignore when fixed"). Running the ideals with
//!     `--ignored` FAILS — that failure is the proof the fixture captures the bug.
//!
//! **Every test here is SCAFFOLDING.** It depends on internal scanner entry points
//! (`param_scan::parse_decl`, `arg_scan::parse`, `opaque_scan::opaque_at`, `paren_balance::scan`,
//! `delim_balance::balanced_strict`, `parts::native_parts`, `ref_scan::scan`, `decl_walk`,
//! `body_walk`, `section_scan`, `state_head_scan`, `item_end_at`, `segmenter::item_starts`,
//! the `driver::params_split` root splitter) and on the internal `segment()` tree. It NEVER
//! promotes to the cross-language corpus.
//!
//! Group map (each with a `// ===` header): params & splitters / opacity: strings-chars-lifetimes
//! / comments / segmenter & header / walks & extents / native islands.

use frame_compiler::text::emit::driver;
use frame_compiler::text::scan::arg_scan::Refusal;
use frame_compiler::text::scan::literals::Target;
use frame_compiler::text::scan::opaque_scan::OpaqueAt;
use frame_compiler::text::scan::parts::native_parts;
use frame_compiler::text::scan::{
    arg_scan, body_walk, decl_walk, delim_balance, item_end_at, opaque_scan, param_scan,
    paren_balance, ref_scan, section_scan, segment, segmenter, state_head_scan, string_scan,
    SegmentError,
};
use frame_compiler::tree::body::{LiteralPart, NativePart, ParamGroup, RefKind};
use frame_compiler::tree::{Item, SystemParams};
use frame_compiler::Source;

// ============================================================================
// SHARED HELPERS
// ============================================================================

/// Build a minimal `@@system S(<interior>) { ... }`, run the real `segment()` pipeline, and
/// return its SystemParams (the production path split_system_params -> parse_one_param).
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
    let ast = segment(&src, Target::Rust).unwrap();
    ast.items
        .iter()
        .find_map(|it| match it {
            Item::System(s) => Some(s.params.clone()),
            _ => None,
        })
        .expect("expected exactly one @@system")
}

/// Segment `text` for `target` and return each item's (kind, start, end).
fn item_kinds(text: &str, target: Target) -> Vec<(&'static str, usize, usize)> {
    let src = Source::new("t.frm", text.as_bytes().to_vec()).unwrap();
    let ast = segment(&src, target).unwrap();
    ast.items
        .iter()
        .map(|it| {
            let k = match it {
                Item::Bom(_) => "Bom",
                Item::Native(_) => "Native",
                Item::Pragma(_) => "Pragma",
                Item::System(_) => "System",
                Item::Efsm(_) => "Efsm",
            };
            let s = it.span();
            (k, s.start, s.end)
        })
        .collect()
}

fn count_systems(kinds: &[(&'static str, usize, usize)]) -> usize {
    kinds.iter().filter(|(k, _, _)| *k == "System").count()
}

/// Total number of `NativePart::Ref` nodes across a native-parts partition.
fn count_refs(parts: &[NativePart]) -> usize {
    parts.iter().filter(|p| matches!(p, NativePart::Ref(_))).count()
}

/// Total number of interpolation `Hole`s across every `Literal` in a native-parts partition.
fn count_holes(parts: &[NativePart]) -> usize {
    parts
        .iter()
        .map(|p| match p {
            NativePart::Literal(l) => l
                .parts
                .iter()
                .filter(|pp| matches!(pp, LiteralPart::Hole(_)))
                .count(),
            _ => 0,
        })
        .sum()
}

// ============================================================================
// === PARAMS & SPLITTERS ===
// ============================================================================

// ---------------------------------------------------------------------------
// OPEN B1 — the emit-side `params_split` / `param_names` root splitter is a bare
// `.split(',')` with NO bracket/angle nesting guard. It drives BOTH the state-param names in
// `StateNode.params` (scan/machine.rs:81 calls `params_split`) AND every handler/method param
// list at emit. Any param whose type or default carries a top-level `,` (a generic
// `Map<K, V>`, an `fn(int, str)`, a default `f(1, 2)`) is mis-split into a phantom extra param
// with a truncated type — javac-rejected malformed output downstream. KNOWN BUG #249 (B1).
// ---------------------------------------------------------------------------

#[test]
fn open_b1_params_split_comma_blind_current() {
    // The exact root splitter, direct. `Map<K, V>` is torn at the interior comma.
    assert_eq!(
        driver::params_split("m: Map<K, V>"),
        vec![
            ("m".to_string(), Some("Map<K".to_string())),
            ("V>".to_string(), None),
        ],
        "params_split is comma-blind: the generic's interior `,` fabricates a phantom `V>` param"
    );
    // param_names shares the mechanism: the phantom `V>` leaks into the call-arg name list.
    assert_eq!(driver::param_names("m: Map<K, V>"), "m, V>");

    // A closure/fn type and a comma-bearing default fail the same way.
    assert_eq!(
        driver::params_split("cb: fn(int, str)"),
        vec![
            ("cb".to_string(), Some("fn(int".to_string())),
            ("str)".to_string(), None),
        ]
    );
    assert_eq!(
        driver::params_split("m: int = f(1, 2)"),
        vec![
            ("m".to_string(), Some("int = f(1".to_string())),
            ("2)".to_string(), None),
        ]
    );
}

#[test]
#[ignore = "KNOWN BUG #249 (B1): params_split/param_names split state & handler params on a \
            bracket/angle-blind `.split(',')`, so any param type or default carrying a top-level \
            `,` is mis-split into a phantom param with a truncated type (javac-rejected). This \
            asserts the IDEAL nesting-aware single param — un-ignore when #249 (B1) is fixed."]
fn open_b1_params_split_comma_blind_ideal() {
    assert_eq!(
        driver::params_split("m: Map<K, V>"),
        vec![("m".to_string(), Some("Map<K, V>".to_string()))],
        "#249 (B1): the generic's interior `,` must be protected -> one param `m: Map<K, V>`"
    );
    assert_eq!(driver::param_names("m: Map<K, V>"), "m");
}

// ---------------------------------------------------------------------------
// OPEN B2 — the per-param body split `parse_one_param` (scan/mod.rs) uses two string-blind
// `split_once('=')` then `split_once(':')` calls with NO nesting guard, even though the
// surrounding ParamScan is angle-aware and correctly keeps the whole body as ONE param. A `=`
// nested inside a generic type (a Rust associated-type binding `Iterator<Item = u8>`) truncates
// the type and fabricates a bogus default. KNOWN BUG #249 (B2). (Dedup of two audit entries.)
// ---------------------------------------------------------------------------

#[test]
fn open_b2_parse_one_param_eq_in_angle_current() {
    // ParamScan itself is CORRECT — it keeps the whole body as one balanced param. This proves
    // the defect is purely the DOWNSTREAM native `parse_one_param` fold.
    assert_eq!(
        param_scan::parse_decl(b"x: impl Iterator<Item = u8>"),
        vec![(ParamGroup::Domain, "x: impl Iterator<Item = u8>".to_string())],
        "ParamScan keeps the associated-type-binding body whole (the split bug is downstream)"
    );

    // segment() -> parse_one_param mis-splits at the angle-interior `=`.
    let p = sysparams("x: impl Iterator<Item = u8>");
    assert_eq!(p.domain.len(), 1);
    assert_eq!(p.domain[0].name, "x");
    assert_eq!(
        p.domain[0].ty.as_deref(),
        Some("impl Iterator<Item"),
        "type is truncated at the nested `=`"
    );
    assert_eq!(
        p.domain[0].default.as_deref(),
        Some("u8>"),
        "a spurious default is invented from the type tail"
    );

    // The `Box<dyn Iterator<Item = u8>>` form fails identically.
    let q = sysparams("x: Box<dyn Iterator<Item = u8>>");
    assert_eq!(q.domain[0].ty.as_deref(), Some("Box<dyn Iterator<Item"));
    assert_eq!(q.domain[0].default.as_deref(), Some("u8>>"));
}

#[test]
#[ignore = "KNOWN BUG #249 (B2): parse_one_param splits a param body on the FIRST `=` with no \
            nesting guard, so a `=` inside a generic type (associated-type binding) truncates \
            the type and invents a bogus default. This asserts the IDEAL whole-type parse — \
            un-ignore when #249 (B2) is fixed."]
fn open_b2_parse_one_param_eq_in_angle_ideal() {
    let p = sysparams("x: impl Iterator<Item = u8>");
    assert_eq!(p.domain.len(), 1);
    assert_eq!(p.domain[0].name, "x");
    assert_eq!(
        p.domain[0].ty.as_deref(),
        Some("impl Iterator<Item = u8>"),
        "#249 (B2): the whole associated-type binding is the type"
    );
    assert_eq!(p.domain[0].default, None, "#249 (B2): there is no default");
}

// ---------------------------------------------------------------------------
// OPEN B4 — an empty start-state group `$()` records ONE phantom empty-named param instead of
// ZERO. The group path calls record_part at group-close regardless of `has_val`, so an empty
// group produces a record (an empty bare/domain segment IS correctly dropped — asymmetric).
// `$()` should mean "start state, no state args". KNOWN BUG #249 (B4).
// ---------------------------------------------------------------------------

#[test]
fn open_b4_empty_group_phantom_param_current() {
    assert_eq!(
        param_scan::parse_decl(b"$()"),
        vec![(ParamGroup::State, "".to_string())],
        "an empty `$()` group records one phantom empty-named State param"
    );
    // End-to-end: SystemParams.state carries the phantom.
    let p = sysparams("$()");
    assert_eq!(p.state.len(), 1);
    assert_eq!(p.state[0].name, "");
    assert!(p.enter.is_empty());
    assert!(p.domain.is_empty());
}

#[test]
#[ignore = "KNOWN BUG #249 (B4): an empty `$()` (or `$>()`) group records one phantom \
            empty-named param instead of zero (the group close-path records unconditionally, \
            unlike the dropped empty bare segment). This asserts the IDEAL zero params — \
            un-ignore when #249 (B4) is fixed."]
fn open_b4_empty_group_phantom_param_ideal() {
    assert_eq!(
        param_scan::parse_decl(b"$()"),
        Vec::<(ParamGroup, String)>::new(),
        "#249 (B4): an empty `$()` group is zero params"
    );
    let p = sysparams("$()");
    assert!(p.state.is_empty(), "#249 (B4): `$()` seeds no state args");
}

// ---------------------------------------------------------------------------
// CARRIED #248 — a balanced comparison-operator straddle across adjacent header defaults
// favors the TEMPLATE reading (no downstream arity at a declaration to adjudicate), so the two
// params MERGE into one. Owner-accepted (favor the template; #248). IDEAL: two params.
// ---------------------------------------------------------------------------

#[test]
fn carried_248_operator_straddle_favors_template() {
    // IDEAL would be two Domain params (`x: bool = a < b` and `y: bool = c > d`); the balanced
    // `<`...`>` straddle merges them into one. Accepted — see
    // https://github.com/frame-lang/framec/issues/248 (favor the template; a real fix needs
    // type-hint extraction). Pinned so any policy change is noticed.
    assert_eq!(
        param_scan::parse_decl(b"x: bool = a < b, y: bool = c > d"),
        vec![(
            ParamGroup::Domain,
            "x: bool = a < b, y: bool = c > d".to_string()
        )],
        "#248 favor-the-template: the operator straddle merges to one param (y swallowed)"
    );
}

// ---------------------------------------------------------------------------
// FIXED F5 #3 — a trailing domain param after a `$>(` enter group is no longer dropped: the
// enter sigil's `>` is consumed as part of the 3-byte sigil, never bracket-counted (the retired
// ParamSplit drove depth negative and silenced the trailing separator). Guards the sigil walk.
// (Also covered in param_scan.rs; duplicated here as a durable corpus guard.)
// ---------------------------------------------------------------------------

#[test]
fn fixed_f5_3_trailing_param_after_enter_group_kept() {
    assert_eq!(
        param_scan::parse_decl(b"$(slot: int), $>(timeout: int), name: String"),
        vec![
            (ParamGroup::State, "slot: int".to_string()),
            (ParamGroup::Enter, "timeout: int".to_string()),
            (ParamGroup::Domain, "name: String".to_string()),
        ],
        "F5 #3: all three params survive — the `$>(` `>` is not bracket-counted"
    );
}

// ---------------------------------------------------------------------------
// FIXED F5 #4 — a group default's own balanced `)` is kept (the retired hand
// `trim_end_matches(')')` ate the user's closer, truncating `f(1)` to `f(1`). Guards the
// depth-walk group closer.
// ---------------------------------------------------------------------------

#[test]
fn fixed_f5_4_group_default_balanced_paren_kept() {
    assert_eq!(
        param_scan::parse_decl(b"$(g: int = f(1)), k: int"),
        vec![
            (ParamGroup::State, "g: int = f(1)".to_string()),
            (ParamGroup::Domain, "k: int".to_string()),
        ],
        "F5 #4: the group's balanced `)` is found — `f(1)` intact, following param still splits"
    );
}

// ============================================================================
// === OPACITY: STRINGS-CHARS-LIFETIMES ===
// ============================================================================

// ---------------------------------------------------------------------------
// CARRIED #219 (site 1/5) — ParamScan's opacity is `"`-only (deliberately: agrees with
// ParenBalance's interior boundary and dodges the Rust `'a`-lifetime hazard). A `,` inside a
// single-quoted char default at depth 0 is counted as a separator. IDEAL: one param. Void
// condition: a target-aware char-vs-lifetime leaf (tracking #219).
// ---------------------------------------------------------------------------

#[test]
fn carried_219_param_scan_char_default_comma_single() {
    // IDEAL: one Domain param `sep: char = ','`. The `"`-only scan splits at the char's comma.
    assert_eq!(
        param_scan::parse_decl(b"sep: char = ','"),
        vec![
            (ParamGroup::Domain, "sep: char = '".to_string()),
            (ParamGroup::Domain, "'".to_string()),
        ],
        "#219: the `,` inside `','` is miscounted as a separator (`\"`-only opacity)"
    );
}

// ---------------------------------------------------------------------------
// CARRIED #219 (site 2/5) — the SAME `"`-only limit at the declaration site DIVERGES from the
// target-aware call site: `arg_scan` (OpaqueScan) parses the identical bytes correctly. IDEAL:
// declaration and call site agree (2 params). Tracking #219.
// ---------------------------------------------------------------------------

#[test]
fn carried_219_param_scan_char_default_comma_multi_vs_argscan() {
    // Declaration site (`"`-only): 3 parts — the char's `,` splits, `'` becomes a bogus param.
    assert_eq!(
        param_scan::parse_decl(b"a: char = ',', b: int = 2"),
        vec![
            (ParamGroup::Domain, "a: char = '".to_string()),
            (ParamGroup::Domain, "'".to_string()),
            (ParamGroup::Domain, "b: int = 2".to_string()),
        ],
        "#219: declaration site (`\"`-only) mis-splits the char default into 3 parts"
    );
    // Call site (target-aware): the SAME text parses to 2 args on C — a recorded DIVERGENCE.
    let f = b"(a: char = ',', b: int = 2)";
    let out = arg_scan::parse(f, 1, f.len() - 1, Target::C);
    assert_eq!(
        out.primary.args.len(),
        2,
        "#219: call site (target-aware arg_scan) correctly parses the same bytes as 2 args"
    );
}

// ---------------------------------------------------------------------------
// CARRIED #219 (site 3/5) — ParenBalance's skip_string is `"`-only, so the first `)` inside a
// single-quoted char default is counted as the group closer. read_name_params_brace uses it to
// bound `@@system Name(...)`, so the interior is truncated at the wrong `)`. IDEAL: Some(15).
// Tracking #219.
// ---------------------------------------------------------------------------

#[test]
fn carried_219_paren_balance_char_default_closer() {
    // IDEAL: the group's matching `)` is the last byte -> Some(15). `"`-only stops at the `)`
    // inside `')'` -> Some(13), truncating the param default.
    assert_eq!(
        paren_balance::scan(b"(x: char = ')')", 0),
        Some(13),
        "#219: paren_balance stops at the `)` inside `')'` (`\"`-only), not the real closer at 15"
    );
}

// ---------------------------------------------------------------------------
// CARRIED #219 (site 4/5) — the target-aware arg_scan (OpaqueScan) treats a Rust lifetime `'a`
// as the START of an (unterminated) char literal, so `Cow<'a, str>, n` fires refusal
// UnterminatedOpaque and swallows the trailing `, n` into one verbatim arg (F3). IDEAL: two
// args. Void condition: a Rust lifetime-vs-char leaf. Tracking #219.
// ---------------------------------------------------------------------------

#[test]
fn carried_219_arg_scan_rust_lifetime_swallow() {
    let s = b"Cow<'a, str>, n";
    let out = arg_scan::parse(s, 0, s.len(), Target::Rust);
    let vals: Vec<String> = out.primary.args.iter().map(|a| a.value.clone()).collect();
    assert_eq!(
        vals,
        vec!["Cow<'a, str>, n".to_string()],
        "#219 (F3): the lifetime `'a` opens an unterminated char literal, swallowing `, n`"
    );
    assert_eq!(
        out.refusal,
        Refusal::UnterminatedOpaque,
        "#219 (F3): the refusal channel names the unterminated-opaque cause"
    );
}

// ---------------------------------------------------------------------------
// CARRIED #219 (site 5/5) — the target-aware delim_balance/close_brace path does NOT dodge the
// Rust lifetime either: a `'a` opens a spurious char literal that pairs with a LATER real char
// quote, exposing the enclosing braces to a miscount. IDEAL: the braces balance -> Some(27).
// Tracking #219.
// ---------------------------------------------------------------------------

#[test]
fn carried_219_delim_balance_rust_lifetime_brace_miscount() {
    let body = b"{ let c = |v: &'a T| '}'; }";
    // IDEAL: Some(27) (len) — the braces balance. The lifetime `'a` opens a char literal that
    // runs to the `'` of `'}'`, so the matching `}` is found 4 bytes early -> Some(23).
    assert_eq!(
        delim_balance::balanced_strict(body, 0, body.len(), b'{', b'}', Target::Rust),
        Some(23),
        "#219: the lifetime `'a` opens a spurious char literal -> the `}}` matches early at 23"
    );
    // And the underlying misclassification: the lifetime `'` is read as a literal opener.
    assert!(
        matches!(
            opaque_scan::opaque_at(b"|v: &'a str| '}'", 5, Target::Rust),
            OpaqueAt::Literal(_)
        ),
        "#219: opaque_at reads the lifetime `'` at offset 5 as a char-literal opener"
    );
}

// ---------------------------------------------------------------------------
// FIXED — escapes, unterminated (newline / EOF), and per-target multiline are all handled
// correctly in opaque_scan/string_scan. Guards the escape + unterminated + multiline policy for
// the 4 CLI-supported targets.
// ---------------------------------------------------------------------------

#[test]
fn fixed_escapes_unterminated_multiline_per_target() {
    // A bare newline is unterminated for C/Java/Python `"`, but NOT for Rust (multiline honored).
    assert_eq!(opaque_scan::opaque_at(b"\"abc\ndef", 0, Target::C), OpaqueAt::Unterminated);
    assert_eq!(
        opaque_scan::opaque_at(b"\"abc\ndef\"", 0, Target::Rust),
        OpaqueAt::Literal(9)
    );
    // An escaped quote does not terminate; a trailing backslash at EOF is unterminated.
    assert_eq!(opaque_scan::opaque_at(b"\"a\\\"b\"", 0, Target::C), OpaqueAt::Literal(6));
    assert_eq!(opaque_scan::opaque_at(b"\"a\\", 0, Target::C), OpaqueAt::Unterminated);
    // string_scan declines an unterminated string.
    assert_eq!(string_scan::scan(b"\"abc", 0), None);
}

// ---------------------------------------------------------------------------
// FIXED — the intended Python string-aware f-string hole case (the Δ1 goal): a `}` hidden inside
// a nested char literal within a hole does NOT prematurely close the hole. Guards the
// opaque-aware hole balancer.
// ---------------------------------------------------------------------------

#[test]
fn fixed_fstring_hole_nested_char_brace() {
    assert_eq!(
        opaque_scan::opaque_at(b"\"{ d['}'] }\"", 0, Target::Python3),
        OpaqueAt::Literal(12),
        "the hole closes at the REAL `}}`, not the one inside `'}}'`"
    );
}

// ============================================================================
// === COMMENTS ===
// ============================================================================

// ---------------------------------------------------------------------------
// OPEN B8 — a C/C++ `//` line comment ending in `\` splices with the next physical line, so the
// following line stays commented. opaque_scan's $LineBody stops at the first `\n` with no
// backslash-newline splice, so a start-of-line `@@system` on the continuation is parsed as a
// real item. This is the ONLY comment divergence for a CLI-supported target. KNOWN BUG #249 (B8).
// ---------------------------------------------------------------------------

#[test]
fn open_b8_c_line_comment_backslash_splice_current() {
    // `// comment\<newline>@@system Foo {` — the `\` should splice, but the comment ends at the
    // first `\n`, so the `@@system` is (wrongly) parsed as a real System.
    let kinds = item_kinds("// comment\\\n@@system Foo {\n}\nint x;\n", Target::C);
    assert_eq!(
        kinds,
        vec![("Native", 0, 12), ("System", 12, 28), ("Native", 28, 36)],
        "B8: the `//`+`\\` splice is ignored, so the `@@system Foo` line becomes a spurious System"
    );
    assert_eq!(count_systems(&kinds), 1, "one spurious System from the un-spliced comment");
}

#[test]
#[ignore = "KNOWN BUG #249 (B8): a C/C++ `//` line comment ending in `\\` splices with the next \
            physical line, but opaque_scan's $LineBody stops at the first newline with no \
            backslash-newline splice, so a following `@@system` is parsed as a real item. This \
            asserts the IDEAL (no System — the line is spliced into the comment) — un-ignore \
            when #249 (B8) is fixed."]
fn open_b8_c_line_comment_backslash_splice_ideal() {
    let kinds = item_kinds("// comment\\\n@@system Foo {\n}\nint x;\n", Target::C);
    assert_eq!(
        count_systems(&kinds),
        0,
        "#249 (B8): the backslash splices the `@@system` line into the `//` comment"
    );
}

// ---------------------------------------------------------------------------
// CARRIED — an unterminated `/* ... EOF` block comment maps to OpaqueScan::Unterminated ->
// opaque_extent None -> the segmenter falls back to byte-by-byte and re-enters $Sol on the
// newlines INSIDE the never-closed comment, so a `@@system` at start-of-line becomes a live
// item. SAME policy as the carried unterminated-LITERAL case (T-N1/T-N2). IDEAL (policy call):
// treat the rest of the file as one unterminated comment (no item). (Dedup of two audit entries.)
// ---------------------------------------------------------------------------

#[test]
fn carried_unterminated_block_comment_reveals_system() {
    // Top-level segment: the `@@system` inside the never-closed `/*` is parsed as a live System.
    let kinds = item_kinds("/* unterminated\n@@system Foo {\n}\n", Target::C);
    assert_eq!(
        kinds,
        vec![("Native", 0, 16), ("System", 16, 32), ("Native", 32, 33)],
        "unterminated-comment policy: the `@@system` inside the open `/*` is a live System"
    );
    // The segmenter surface confirms the item-start inside the unterminated comment.
    let src = b"code\n/* open never closes\n@@system Real {\n    machine:\n        $A { }\n}\n";
    assert_eq!(
        segmenter::item_starts(src, 0, Target::Java),
        vec![26],
        "unterminated-comment policy: item_starts finds the `@@system` inside the open `/*`"
    );
}

// ---------------------------------------------------------------------------
// CARRIED — Lua `--[[ ... ]]` long comment is mis-scanned as a `--` LINE comment (opaque_scan
// has no LuaLongBracket leaf), so it ends at the first newline; multi-line content after it
// (including a `@@system`) leaks out. Lua is CLI-refused (main.rs), so this is gated/carried.
// IDEAL: the whole `--[[ ... ]]` is one comment.
// ---------------------------------------------------------------------------

#[test]
fn carried_lua_long_comment_misscanned_as_line() {
    // IDEAL: the long comment `--[[x\n]]` is one 8-byte comment (Comment(8)). Mis-scanned as a
    // `--` line comment, it stops at the first newline, NOT at the `]]` close.
    match opaque_scan::opaque_at(b"--[[x\n]]y", 0, Target::Lua) {
        OpaqueAt::Comment(end) => assert_eq!(
            end, 5,
            "Lua `--[[` is mis-scanned as a `--` line comment ending at the newline (offset 5), \
             not the `]]` long-comment close (a correct long comment would end at 8)"
        ),
        other => panic!("expected a (mis-scanned line) Comment, got {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// CARRIED — Ruby `=begin`/`=end` block comment is recognized at ANY column (block_open_len uses
// starts_with with no column-0 anchor), whereas real Ruby only treats it as a comment at column
// 0. A mid-line `=begin` opens a spurious block comment. Ruby is CLI-refused, so gated/carried.
// IDEAL: `=begin` mid-line is not a comment.
// ---------------------------------------------------------------------------

#[test]
fn carried_ruby_begin_any_column() {
    // A mid-line `=begin` (offset 2 in `x =begin y`) OPENS a block comment (it never closes here
    // -> Unterminated). IDEAL: mid-line `=begin` opens nothing.
    assert_eq!(
        opaque_scan::opaque_at(b"x =begin y", 2, Target::Ruby),
        OpaqueAt::Unterminated,
        "Ruby `=begin` is recognized mid-line (no column-0 anchor), opening a spurious block comment"
    );
}

// ---------------------------------------------------------------------------
// FIXED — comment recognition for the 4 CLI-supported targets (python/java/rust/c) is correct
// across the adversarial battery: a `}` inside a comment does not close a system body; a comment
// opener inside a string is a Literal; `@@` inside a comment is never a pragma. Guards #113/#219
// comment/string interaction.
// ---------------------------------------------------------------------------

#[test]
fn fixed_cli_target_comment_recognition_correct() {
    // A `}` inside a `//` comment does NOT close the system body — the body closes at the real `}`.
    let k = item_kinds("@@system Foo {\n  // }\n}\nafter\n", Target::C);
    assert_eq!(count_systems(&k), 1, "the closer inside the line comment is skipped; one well-formed System");
    // `@@system` inside a `#` line comment is native water, never a pragma.
    assert_eq!(count_systems(&item_kinds("# @@system Foo {\nx=1\n", Target::Python3)), 0);
    // `@@system` inside a `/* */` block comment is native water.
    assert_eq!(count_systems(&item_kinds("/* @@system Foo {} */\nint x;\n", Target::C)), 0);
    // A comment opener inside a string is a Literal, not a Comment.
    assert_eq!(opaque_scan::opaque_at(b"\"// x\"Y", 0, Target::C), OpaqueAt::Literal(6));
}

// ============================================================================
// === SEGMENTER & HEADER ===
// ============================================================================

// ---------------------------------------------------------------------------
// OPEN B5 — the `@@system`/`@@fsm` header "skip to `{`" (read_name_params_brace) is
// opaque-blind: it scans raw bytes to the first `{` after the name/params/bases with NO
// string/comment awareness (unlike the params/body sub-scans it feeds). A `{` inside a header
// comment or string is mistaken for the body opener — either a SILENT wrong extent or a LOUD
// false UnclosedBody. KNOWN BUG #249 (B5).
// ---------------------------------------------------------------------------

#[test]
fn open_b5_header_skip_to_brace_opaque_blind_current() {
    // SILENT wrong extent: the `{` inside the header comment `/* {} */` is taken as the body
    // opener, so the item ends past the comment's `}` (offset 18) and the real body is orphaned.
    let a = b"@@system Foo /* {} */ {\n    machine:\n        $A { }\n}\n";
    assert_eq!(
        item_end_at(a, 0, Target::Java),
        18,
        "B5 silent: the header comment's brace is taken as the body opener (ends at 18, not 53)"
    );
    // LOUD false-error variant: an unbalanced `{` inside the header comment makes close_brace
    // fail -> read_pragma Err -> item_end_at swallows to EOF (surfaces as UnclosedBody).
    let b = b"@@system Foo /* { */ {\n    machine:\n        $A { }\n}\n";
    assert_eq!(
        item_end_at(b, 0, Target::Rust),
        b.len(),
        "B5 loud: an unbalanced header-comment brace makes the item swallow to EOF (false UnclosedBody)"
    );
}

#[test]
#[ignore = "KNOWN BUG #249 (B5): the header skip-to-`{` (read_name_params_brace) is \
            string/comment-blind, so a `{` inside a header comment or string is mistaken for the \
            body opener (silent wrong extent or loud false UnclosedBody). This asserts the IDEAL \
            extent (the whole construct, past the REAL closing brace) — un-ignore when #249 (B5) \
            is fixed."]
fn open_b5_header_skip_to_brace_opaque_blind_ideal() {
    let a = b"@@system Foo /* {} */ {\n    machine:\n        $A { }\n}\n";
    assert_eq!(
        item_end_at(a, 0, Target::Java),
        53,
        "#249 (B5): the header comment's brace must be skipped; the item spans past the real closer"
    );
}

// ---------------------------------------------------------------------------
// OPEN B6 — a Python string carrying an unmatched `{` mis-scans past its own closing quote: the
// opaque-aware hole balancer treats the enclosing string's own closing `"` as the opener of a
// new (unterminated) nested string and matches the string's `{` against a LATER `}`. The hole
// overruns the terminator -> Unterminated -> FAIL-policy close_brace REJECTS the system with
// UnclosedBody. A Δ1 regression not covered by the landing. KNOWN BUG #249 (B6).
// ---------------------------------------------------------------------------

#[test]
fn open_b6_python_unmatched_brace_false_reject_current() {
    // The 3-byte string literal `"{"` is reported Unterminated (the hole overruns its quote).
    assert_eq!(
        opaque_scan::opaque_at(b"{\"{\": 1}", 1, Target::Python3),
        OpaqueAt::Unterminated,
        "B6: a string with more opens than closes mis-scans past its own closing quote"
    );
    // End-to-end: a valid Python @@system with `x = \"{\"` is falsely rejected.
    let src = Source::new("p.frm", "@@system Q {\n    x = \"{\"\n}\n".as_bytes().to_vec()).unwrap();
    match segment(&src, Target::Python3) {
        Err(SegmentError::UnclosedBody { name, .. }) => assert_eq!(name, "Q"),
        other => panic!("B6: expected a false UnclosedBody for Q, got {other:?}"),
    }
}

#[test]
#[ignore = "KNOWN BUG #249 (B6): a Python string with an unmatched `{` mis-scans past its own \
            closing quote (the opaque-aware hole balancer treats the string's own closing quote \
            as a new nested-string opener), and the FAIL-policy close_brace turns the overrun \
            into a false UnclosedBody. This asserts the IDEAL (the `\"{\"` literal is valid; the \
            system parses) — un-ignore when #249 (B6) is fixed."]
fn open_b6_python_unmatched_brace_false_reject_ideal() {
    assert_eq!(
        opaque_scan::opaque_at(b"{\"{\": 1}", 1, Target::Python3),
        OpaqueAt::Literal(4),
        "#249 (B6): the quoted-brace literal is a valid 3-byte string (ends at 4)"
    );
    let src = Source::new("p.frm", "@@system Q {\n    x = \"{\"\n}\n".as_bytes().to_vec()).unwrap();
    assert!(
        segment(&src, Target::Python3).is_ok(),
        "#249 (B6): the system Q must parse (its quoted-brace string is valid)"
    );
}

// ============================================================================
// === WALKS & EXTENTS ===
// ============================================================================

// ---------------------------------------------------------------------------
// OPEN B3 — a multi-line initializer in a decl-section member is truncated at end-of-line and
// the continuation lines become bogus members. decl_extent's Line branch is `to_end_of_line`
// with NO unbalanced-continuation logic (unlike legacy #185), and decl_read slices its window
// to the eol so its DelimBalance cannot see past the newline. KNOWN BUG #249 (B3).
// ---------------------------------------------------------------------------

#[test]
fn open_b3_decl_multiline_init_shatters_current() {
    // `x: int = compute(\n a,\n b\n)` — the balanced initializer is shattered into 4 decls: the
    // truncated `x: int = compute(` plus three bogus members from the continuation lines.
    let body = b"    x: int = compute(\n        a,\n        b\n    )\n";
    let (starts, unterm) = decl_walk::decl_starts(body, 0, body.len(), false, Target::Rust);
    assert_eq!(
        starts,
        vec![4, 30, 41, 47],
        "B3: the multi-line initializer shatters into 4 decl starts (one true + three bogus)"
    );
    assert!(!unterm, "B3: the eol-bounded walk does not report an unterminated body");
}

#[test]
#[ignore = "KNOWN BUG #249 (B3): a multi-line initializer in a decl-section member is truncated \
            at end-of-line (decl_extent's Line branch has no unbalanced-continuation logic, \
            unlike legacy #185), so the continuation lines become bogus members. This asserts \
            the IDEAL single decl covering the whole balanced initializer — un-ignore when #249 \
            (B3) is fixed."]
fn open_b3_decl_multiline_init_shatters_ideal() {
    let body = b"    x: int = compute(\n        a,\n        b\n    )\n";
    let (starts, _unterm) = decl_walk::decl_starts(body, 0, body.len(), false, Target::Rust);
    assert_eq!(
        starts,
        vec![4],
        "#249 (B3): the balanced multi-line initializer is ONE decl (`x`), not four"
    );
}

// ---------------------------------------------------------------------------
// OPEN B7 — body_walk's brace-depth undercounts when a frame-assignment line ends in an
// unbalanced block-opening `{`. A frame assignment's extent runs to end-of-line, so body_walk
// jumps PAST the trailing `{` without feeding it to the `{`/`}` depth counter; every statement
// inside that block is then sampled one depth too low. block_depth feeds Python/GDScript
// reindentation and Java unreachable suppression. KNOWN BUG #249 (B7).
// ---------------------------------------------------------------------------

#[test]
fn open_b7_body_walk_brace_undercount_current() {
    // Frame-assignment RHS opens a block: the inner `@@:self.foo()` is recorded at depth 0.
    let fa = b"$.config = {\n@@:self.foo()\n}\n";
    assert_eq!(
        body_walk::stmt_starts(fa, 0, fa.len(), Target::Python3).0,
        vec![(0, 0), (13, 0)],
        "B7: the frame-assign RHS's brace is not counted; the inner stmt is recorded at depth 0"
    );
    // The byte-identical NATIVE opener records the same inner statement at depth 1 (correct).
    let nv = b"x = {\n@@:self.foo()\n}\n";
    assert_eq!(
        body_walk::stmt_starts(nv, 0, nv.len(), Target::Python3).0,
        vec![(6, 1)],
        "B7: the native RHS's brace IS counted; the inner stmt is depth 1 (the correct contrast)"
    );
}

#[test]
#[ignore = "KNOWN BUG #249 (B7): body_walk's brace-depth undercounts when a frame-assignment \
            line ends in an unbalanced block-opening `{` (the `{` is absorbed into the \
            eol-bounded RHS extent and never counted), so statements inside the block are \
            sampled one depth too low. This asserts the IDEAL depth 1 for the inner statement — \
            un-ignore when #249 (B7) is fixed."]
fn open_b7_body_walk_brace_undercount_ideal() {
    let fa = b"$.config = {\n@@:self.foo()\n}\n";
    let starts = body_walk::stmt_starts(fa, 0, fa.len(), Target::Python3).0;
    let inner = starts
        .iter()
        .find(|(off, _)| *off == 13)
        .expect("#249 (B7): the inner @@:self.foo() statement is recorded at offset 13");
    assert_eq!(
        inner.1, 1,
        "#249 (B7): the inner statement is inside the `{{`-block and must be at depth 1"
    );
}

// ---------------------------------------------------------------------------
// CARRIED — a decl-section member whose NAME collides with a section keyword
// (interface/machine/domain/actions/operations) manufactures a spurious section and the member
// is lost. SectionScan records a section start at any depth-0 word-start matching a keyword
// followed by `:`. Matches the hand oracle `section_keyword_starts` (differential-tested) — a
// section-detection design choice carried from legacy. IDEAL: the member stays in its section.
// ---------------------------------------------------------------------------

#[test]
fn carried_section_keyword_member_splits_section() {
    // A `domain:` member named `machine: int = 5` is at brace depth 0, so record_kw fires and
    // manufactures a spurious Machine section over the domain variable (the member is lost).
    let src = b"@@system Foo {\n  domain:\n    machine: int = 5\n}\n";
    let body_start = src.iter().position(|&b| b == b'{').unwrap() + 1;
    let close_start = src.len() - 2;
    assert_eq!(
        section_scan::keyword_starts(src, body_start, close_start, Target::Rust),
        vec![(17, 24, 2), (29, 37, 1)],
        "section-keyword member: the `machine:` domain VARIABLE is read as a spurious Machine section"
    );
}

// ---------------------------------------------------------------------------
// CARRIED — a braceless state head yields a header_node span `[at, open+1)` that overruns the
// buffer by one byte (StateHeadScan sets open==end==limit when no body `{` is found — the T-S2
// register). segment()'s top-level coverage check does not validate this nested FrameSpan, so it
// is latent until something slices the header span. Documented T-S2 driver artifact. IDEAL:
// header_node.end <= bytes.len().
// ---------------------------------------------------------------------------

#[test]
fn carried_braceless_state_head_overrun() {
    let src = b"$S(x: int)\n"; // braceless head, limit == len == 11
    let p = state_head_scan::scan(src, 0, src.len(), Target::Rust);
    assert!(!p.open_found, "braceless head: no body brace before limit (T-S2)");
    assert_eq!(p.open, src.len(), "braceless head: open == limit");
    assert_eq!(p.end, src.len(), "braceless head: end == limit");
    assert!(
        p.open + 1 > src.len(),
        "braceless head: header_node span end = open+1 ({}) overruns the buffer (len {})",
        p.open + 1,
        src.len()
    );
}

// ============================================================================
// === NATIVE ISLANDS ===
// ============================================================================

// ---------------------------------------------------------------------------
// FIXED #224 — a Frame sigil (`$.x` / `@@:self.y`) inside a TERMINATED string literal is CONTENT,
// never a FrameRef (a FrameRef can only exist as a NativePart or inside a Hole — the wrong answer
// is structurally unrepresentable). Guards the #224 two-answers bug.
// ---------------------------------------------------------------------------

#[test]
fn fixed_224_sigil_in_string_is_content_not_ref() {
    let s = b"x = \"$.field and @@:self.y\"";
    let parts = native_parts(s, 0, s.len(), Target::Java);
    assert_eq!(count_refs(&parts), 0, "#224: no FrameRef is spliced from string content");
    assert_eq!(count_holes(&parts), 0, "#224: the terminated string has no holes");
    assert_eq!(parts.len(), 2, "#224: a Text run then one Literal");
    assert!(matches!(parts[0], NativePart::Text(_)), "#224: leading `x = ` is Text");
    assert!(matches!(parts[1], NativePart::Literal(_)), "#224: the string is one Literal");
}

// ---------------------------------------------------------------------------
// FIXED T-N1/T-N2 — an unterminated literal/comment in water flushes as ONE plain Text run to
// the limit (no island scan inside), so a sigil in unterminated string content stays content.
// Guards the Δ3 unterminated-body rescue.
// ---------------------------------------------------------------------------

#[test]
fn fixed_tn1_unterminated_literal_flushes_one_text_run() {
    let s = b"x = \"unterminated $.field";
    let parts = native_parts(s, 0, s.len(), Target::Java);
    assert_eq!(count_refs(&parts), 0, "T-N1: no ref is scanned from unterminated string content");
    assert_eq!(parts.len(), 1, "T-N1: a single Text run over the whole tail");
    match &parts[0] {
        NativePart::Text(t) => {
            assert_eq!(t.span.start, 0);
            assert_eq!(t.span.end, 25, "T-N1: the run covers the whole 25-byte tail");
        }
        other => panic!("T-N1: expected one Text run, got {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// FIXED T-N8 — a `{{` escape in a Python string no longer phantom-opens a hole; only a real
// `{expr}` becomes a Hole. Guards the Δ2 double_brace_skip.
// ---------------------------------------------------------------------------

#[test]
fn fixed_tn8_double_brace_escape_no_phantom_hole() {
    let s = b"y = f\"{{a}} {b}\"";
    let parts = native_parts(s, 0, s.len(), Target::Python3);
    assert_eq!(
        count_holes(&parts),
        1,
        "T-N8: `{{{{a}}}}` is content; exactly one Hole opens (for `{{b}}`)"
    );
    assert_eq!(count_refs(&parts), 0, "T-N8: no top-level ref (the ref, if any, is inside the hole)");
}

// ---------------------------------------------------------------------------
// CARRIED (#224-class at the `to`-boundary) — native_parts scans the INTERIOR of a literal that
// overruns `to`: the opaque arm rejects a literal whose close lies beyond `limit`, then the walk
// re-runs the island recognizers over the literal's interior, splicing a false FrameRef from
// string CONTENT. UNREACHABLE from segment() (the segmenter places item/hole boundaries with the
// SAME opaque machine, so a closed literal never straddles `to`) — reported/carried for
// completeness. IDEAL: a single Text run `q="$.c` (the documented "water" outcome), no Ref.
// ---------------------------------------------------------------------------

#[test]
fn carried_224class_native_parts_to_boundary_scans_interior() {
    // The `"` at index 2 opens a literal whose close `"` is at index 10 (beyond to=6); the walk
    // advances one byte and re-scans the interior, so `$.c` from string content becomes a Ref.
    let b = b"q=\"$.count\"end";
    let parts = native_parts(b, 0, 6, Target::C);
    assert_eq!(parts.len(), 2, "#224-class to-boundary: a Text run then a (false) Ref");
    match &parts[0] {
        NativePart::Text(t) => {
            assert_eq!((t.span.start, t.span.end), (0, 3), "leading `q=\"` is Text[0..3]");
        }
        other => panic!("expected leading Text, got {other:?}"),
    }
    match &parts[1] {
        NativePart::Ref(r) => {
            assert_eq!(r.kind, RefKind::StateVar);
            assert_eq!(r.name, "c", "the `c` from inside string content is spliced as a StateVar ref");
            assert_eq!((r.span.start, r.span.end), (3, 6));
        }
        other => panic!("#224-class to-boundary: expected a false Ref, got {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// CARRIED — ref_scan recognizes `@@:self.` (self, a dot, nothing) as a ContextSelf ref with an
// EMPTY name; `@@:self.f.` yields a trailing-dot name `f.`. This is intentional SHAPE
// recognition (it matches the hand oracle frame_ref_at_hand, differential-tested);
// membership/well-formedness is deferred to validate.rs (E408). IDEAL: reject / no empty-named
// ref. Reported for completeness.
// ---------------------------------------------------------------------------

#[test]
fn carried_ref_scan_self_dot_empty_name() {
    assert_eq!(
        ref_scan::scan(b"@@:self.", 0),
        Some((RefKind::ContextSelf, String::new(), 8)),
        "ref_scan recognizes `@@:self.` as a ContextSelf ref with an EMPTY name (shape only)"
    );
    assert_eq!(
        ref_scan::scan(b"@@:self.f.", 0),
        Some((RefKind::ContextSelf, "f.".to_string(), 10)),
        "ref_scan yields a trailing-dot name `f.` for `@@:self.f.`"
    );
}
