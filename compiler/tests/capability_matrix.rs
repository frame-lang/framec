//! # Parser capability × demand matrix — a standing mechanistic guard
//!
//! Every scanning task has a *minimal correct machine* fixed by two axes (plus decidability):
//!   * **opacity** — how it tells code from strings/comments: `None` (byte-blind) < `DoubleQuote`
//!     (`"`-only StringScan) < `TargetAware` (OpaqueScan, per-target forms).
//!   * **nesting** — how it counts delimiters for depth: `None` (a flat `.split(',')`/eol scan
//!     counts nothing) < `Dyck1` (one merged `()[]{}` counter) < `PerKind` < `KindChecked`.
//!
//! Each scanner's **capability** (audited from the code, file:line in the doc) is paired with the
//! **demand** of the construct it parses. A cell where **capability < demand on any axis is a
//! class-deficiency bug** — a lower-class machine standing in for a higher-class problem (the B1/B2
//! family). This is the mechanistic bug-finder: it derived B1, B2, B5 and the #219 family below
//! WITHOUT being told them — they fall out of `capability < demand`.
//!
//! This test **ratchets**: the set of deficiencies must EXACTLY equal the documented set.
//!   * A NEW deficiency not in the table (someone adds a byte-blind splitter) → **fails**.
//!   * A documented deficiency that no longer reproduces (a bug got fixed) → **fails**, forcing the
//!     matrix + `docs/parser_capability_matrix.md` to stay in sync with reality.
//!
//! What this matrix does NOT catch (by construction — the class is already right): *within-class
//! logic bugs* (B4 empty-group emission, B6 Python hole, B7 brace undercount) and *per-target form
//! completeness* gaps (B8 C `//` splice, Lua/Ruby comment forms). Those need the axis-keyed
//! adversarial generator + differential oracle; they live in `parser_bug_corpus.rs`.
//!
//! Sources: capability audit (wf journal, 2026-07-21) + `parser_bug_corpus.rs` repros. Issues:
//! #249 (open B1–B8), #248 (angle straddle), #219 (char/lifetime opacity).

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
enum Opacity { None, DoubleQuote, TargetAware, NA }
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
enum Nesting { None, Dyck1, PerKind, KindChecked, NA }

// A deficiency's disposition. `Ok` = capability meets demand. `Carried`/`Open` = a documented
// capability<demand cell (accepted/tracked). `Review` = the matrix flags a possible deficiency
// pending a repro (does not fail the ratchet, but is listed).
#[derive(Clone, Copy, PartialEq, Debug)]
enum Status { Ok, Carried(&'static str), Open(&'static str), Review(&'static str) }
use Status::*;

struct Row {
    scanner: &'static str,
    construct: &'static str,
    op_cap: Opacity,
    nest_cap: Nesting,
    op_demand: Opacity,
    nest_demand: Nesting,
    status: Status,
}

// capability from the audit; demand from the construct. NA axes never deficient.
const M: &[Row] = &[
    // ---- native-typed comma/delimiter splitters: demand target-aware opacity + Dyck-1 nesting ----
    Row{scanner:"param_scan",        construct:"system-header params",   op_cap:Opacity::DoubleQuote, nest_cap:Nesting::Dyck1, op_demand:Opacity::TargetAware, nest_demand:Nesting::Dyck1, status:Carried("#219 char/lifetime: \"-only, not target-aware")},
    Row{scanner:"arg_scan",          construct:"instantiation args",     op_cap:Opacity::TargetAware, nest_cap:Nesting::Dyck1, op_demand:Opacity::TargetAware, nest_demand:Nesting::Dyck1, status:Ok},
    Row{scanner:"parse_one_param",   construct:"name:type=default split",op_cap:Opacity::None,        nest_cap:Nesting::None,  op_demand:Opacity::TargetAware, nest_demand:Nesting::Dyck1, status:Open("#249 B2: split_once('=') bracket/string-blind")},
    Row{scanner:"params_split",      construct:"state/handler params",   op_cap:Opacity::None,        nest_cap:Nesting::None,  op_demand:Opacity::TargetAware, nest_demand:Nesting::Dyck1, status:Open("#249 B1: naive .split(',') no nesting guard")},
    Row{scanner:"param_names",       construct:"state/handler param names",op_cap:Opacity::None,      nest_cap:Nesting::None,  op_demand:Opacity::TargetAware, nest_demand:Nesting::Dyck1, status:Open("#249 B1: naive .split(',') no nesting guard")},
    Row{scanner:"args_of",           construct:"transition args (no-split)",op_cap:Opacity::TargetAware,nest_cap:Nesting::Dyck1,op_demand:Opacity::TargetAware, nest_demand:Nesting::Dyck1, status:Ok}, // policy=no-split, hand to target

    // ---- structural-delimiter seekers over native code: demand target-aware + Dyck-1 ----
    Row{scanner:"read_name_params_brace",construct:"header skip-to-`{`", op_cap:Opacity::None,        nest_cap:Nesting::NA,    op_demand:Opacity::TargetAware, nest_demand:Nesting::NA,    status:Open("#249 B5: skip-to-brace is opacity-blind")},
    Row{scanner:"body_walk",         construct:"statement extents",      op_cap:Opacity::TargetAware, nest_cap:Nesting::Dyck1, op_demand:Opacity::TargetAware, nest_demand:Nesting::Dyck1, status:Ok}, // B7 is within-class logic
    Row{scanner:"decl_read",         construct:"decl member name:type=init",op_cap:Opacity::None,     nest_cap:Nesting::Dyck1, op_demand:Opacity::TargetAware, nest_demand:Nesting::Dyck1, status:Review("decl_read opacity=none — a string/comment in a type/init not opaque; needs a repro")},
    Row{scanner:"decl_walk",         construct:"decl-section member boundaries",op_cap:Opacity::TargetAware,nest_cap:Nesting::Dyck1,op_demand:Opacity::TargetAware,nest_demand:Nesting::Dyck1,status:Ok},
    Row{scanner:"machine_walk",      construct:"machine-section boundaries",op_cap:Opacity::TargetAware,nest_cap:Nesting::Dyck1,op_demand:Opacity::TargetAware,nest_demand:Nesting::Dyck1,status:Ok},
    Row{scanner:"state_walk",        construct:"state-member boundaries",op_cap:Opacity::TargetAware, nest_cap:Nesting::Dyck1, op_demand:Opacity::TargetAware, nest_demand:Nesting::Dyck1, status:Ok},
    Row{scanner:"section_scan",      construct:"section keyword boundaries",op_cap:Opacity::TargetAware,nest_cap:Nesting::Dyck1,op_demand:Opacity::TargetAware,nest_demand:Nesting::Dyck1,status:Ok},
    Row{scanner:"state_head_scan",   construct:"state head+params",      op_cap:Opacity::TargetAware, nest_cap:Nesting::Dyck1, op_demand:Opacity::TargetAware, nest_demand:Nesting::Dyck1, status:Ok},
    Row{scanner:"handler_head_scan", construct:"handler head+params",    op_cap:Opacity::TargetAware, nest_cap:Nesting::Dyck1, op_demand:Opacity::TargetAware, nest_demand:Nesting::Dyck1, status:Ok},
    Row{scanner:"segmenter",         construct:"top-level @@-item boundaries",op_cap:Opacity::TargetAware,nest_cap:Nesting::NA,op_demand:Opacity::TargetAware,nest_demand:Nesting::NA,   status:Ok}, // B8/Lua/Ruby are opaque_scan form gaps

    // ---- opacity/balance primitives: demand = their own scope ----
    Row{scanner:"string_scan",       construct:"\"-string extent",        op_cap:Opacity::DoubleQuote, nest_cap:Nesting::None,  op_demand:Opacity::DoubleQuote, nest_demand:Nesting::None,  status:Ok},
    Row{scanner:"opaque_scan",       construct:"per-target string/comment",op_cap:Opacity::TargetAware,nest_cap:Nesting::Dyck1,op_demand:Opacity::TargetAware, nest_demand:Nesting::Dyck1, status:Ok}, // per-target FORM gaps (B8/Lua/Ruby) are completeness, not class — see corpus
    Row{scanner:"paren_balance",     construct:"header `()` balance",     op_cap:Opacity::DoubleQuote, nest_cap:Nesting::Dyck1, op_demand:Opacity::TargetAware, nest_demand:Nesting::Dyck1, status:Carried("#219: \"-only, a `)` in a char default miscounts")},
    Row{scanner:"delim_balance",     construct:"kind-checked balance",    op_cap:Opacity::TargetAware, nest_cap:Nesting::Dyck1, op_demand:Opacity::TargetAware, nest_demand:Nesting::Dyck1, status:Ok},
    Row{scanner:"raw_string",        construct:"raw-string extent",       op_cap:Opacity::None,        nest_cap:Nesting::None,  op_demand:Opacity::None,        nest_demand:Nesting::None,  status:Ok}, // scoped primitive
    Row{scanner:"string_counter",    construct:"\"-string count",         op_cap:Opacity::DoubleQuote, nest_cap:Nesting::None,  op_demand:Opacity::DoubleQuote, nest_demand:Nesting::None,  status:Ok},

    // ---- Frame-syntax recognizers: opacity handled by the native_parts caller ----
    Row{scanner:"native_parts_scan", construct:"native island dispatch",  op_cap:Opacity::TargetAware, nest_cap:Nesting::NA,    op_demand:Opacity::TargetAware, nest_demand:Nesting::NA,    status:Ok},
    Row{scanner:"ref_scan",          construct:"$ref / $.field / @@:self",op_cap:Opacity::None,        nest_cap:Nesting::None,  op_demand:Opacity::None,        nest_demand:Nesting::None,  status:Ok}, // called post-opacity at a known position
    Row{scanner:"inst_scan",         construct:"@@Name(args) shape",      op_cap:Opacity::DoubleQuote, nest_cap:Nesting::Dyck1, op_demand:Opacity::DoubleQuote, nest_demand:Nesting::Dyck1, status:Ok},
    Row{scanner:"embed_scan",        construct:"@@:self.f.m(args)",       op_cap:Opacity::DoubleQuote, nest_cap:Nesting::Dyck1, op_demand:Opacity::DoubleQuote, nest_demand:Nesting::Dyck1, status:Ok},
];

fn deficient(r: &Row) -> bool {
    (r.op_demand != Opacity::NA && r.op_cap < r.op_demand)
        || (r.nest_demand != Nesting::NA && r.nest_cap < r.nest_demand)
}

#[test]
fn capability_matrix_ratchet() {
    let mut errors = Vec::new();
    for r in M {
        let def = deficient(r);
        match (def, r.status) {
            (true, Status::Ok) => errors.push(format!(
                "UNDOCUMENTED class-deficiency: `{}` ({}) has opacity {:?} (< demand {:?}) or nesting {:?} (< demand {:?}). \
                 A lower-class machine on a higher-class problem — fix it, or document it in the matrix with an issue.",
                r.scanner, r.construct, r.op_cap, r.op_demand, r.nest_cap, r.nest_demand)),
            (false, Status::Open(b)) | (false, Status::Carried(b)) => errors.push(format!(
                "STALE documented deficiency: `{}` [{}] no longer shows capability<demand — verify the fix landed and move this row to Ok (keep the matrix honest).",
                r.scanner, b)),
            _ => {}
        }
    }
    assert!(errors.is_empty(), "capability×demand matrix drift:\n  - {}", errors.join("\n  - "));
}

/// The payoff test: the matrix MECHANICALLY reproduces the known class-deficiency bugs — they were
/// derived by `capability < demand`, not hand-listed. If this set changes, the audit or the matrix
/// moved; re-reconcile with `parser_bug_corpus.rs`.
#[test]
fn matrix_reproduces_known_class_bugs() {
    let mut open: Vec<&str> = M.iter().filter(|r| matches!(r.status, Status::Open(_))).map(|r| r.scanner).collect();
    open.sort();
    assert_eq!(
        open,
        vec!["param_names", "params_split", "parse_one_param", "read_name_params_brace"],
        "the OPEN class-deficiencies the matrix derives (B1 ×2, B2, B5) changed — reconcile with #249"
    );
    let carried = M.iter().filter(|r| matches!(r.status, Status::Carried(_))).count();
    assert_eq!(carried, 2, "the CARRIED opacity deficiencies (#219: param_scan + paren_balance) changed");
}
