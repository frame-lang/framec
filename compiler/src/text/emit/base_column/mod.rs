//! The handler/action body **base-column min-fold**, dogfooded as a plain `@@system`
//! ([`base_column.frs`]) — the emit-side twin of [`super::stmt_walk`], riding the same read-only
//! borrowed domain (`&'a [Stmt]`). It reifies the `base` computation `emit_body` feeds to the
//! statement walk: the SHALLOWEST logical column across a body's statements, the reindent baseline
//! everything else is measured relative to. framec owns the cursor + the `min`/`seen` registers +
//! the halt; the per-`Stmt` column extraction is the [`col_at`] leaf.
//!
//! The byte-for-byte ORACLE it replaced is preserved as
//! [`super::driver::base_column_hand`] and gated in `tests/base_column.rs` (GATE-A); because
//! `base` feeds StmtWalk's reindent, `tests/stmt_walk.rs` byte-parity is a second, transitive
//! gate on this value.
//!
//! `.gen.rs` regen: `framec-ng -l rust --emit base_column.frs | grep -v '^#!\[allow' >
//! base_column.gen.rs`.

use crate::tree::body::Stmt;

/// The logical column of the statement at `i`, or `-1` when there is none to record — a
/// `Stmt::Trivia` (whitespace/comment between statements has no column of its own) or an
/// out-of-bounds index. The 8-way match is exactly the arms of the original `emit_body`
/// `.filter_map(...)`: `Native -> logical_indent`, `Transition`/`StackPush -> t.col`,
/// `StackPop`/`StackPopBare`/`Forward -> x.col`, `Assign -> a.col`, `ReturnCall -> r.col`,
/// `SelfCall -> c.col`, `Trivia -> None`. Columns are `u32` and always fit an `i64`, so `-1` is a
/// clean out-of-band sentinel the machine keys its skip on. (Frame cannot match a Rust enum; this
/// is the un-Frame-able field access surfaced as one leaf, as [`super::stmt_walk::kind_at`] is for
/// the walk.)
fn col_at(stmts: &[Stmt], i: usize) -> i64 {
    match stmts.get(i) {
        None => -1,
        Some(Stmt::Trivia(_)) => -1,
        Some(Stmt::Native(n)) => n.logical_indent as i64,
        Some(Stmt::Transition(t)) | Some(Stmt::StackPush(t)) => t.col as i64,
        Some(Stmt::StackPop(x)) | Some(Stmt::StackPopBare(x)) | Some(Stmt::Forward(x)) => x.col as i64,
        Some(Stmt::Assign(a)) => a.col as i64,
        Some(Stmt::ReturnCall(r)) => r.col as i64,
        Some(Stmt::SelfCall(c)) => c.col as i64,
    }
}

mod fsm {
    #![allow(
        dead_code,
        unused_parens,
        non_snake_case,
        unused_variables,
        unused_mut,
        unused_imports
    )]
    use super::{col_at, Stmt};
    include!("base_column.gen.rs");
}

/// The body's BASE column: the shallowest logical column across `stmts`, or `0` when no statement
/// records a column (an all-`Trivia` or empty body — the original `.unwrap_or(0)`). Driven by the
/// `BaseColumn` min-fold system. The bounded drive loop lives here, as in
/// [`super::stmt_walk::walk`] — a broken machine cannot hang.
pub(super) fn compute(stmts: &[Stmt]) -> u32 {
    let mut m = fsm::BaseColumn::new(stmts, stmts.len());
    // Each $Scan step advances the cursor by one or halts, so the machine reaches $Done in at
    // most stmts.len()+1 steps; the slack covers the terminal step and any $Done no-ops.
    let bound = stmts.len() + 8;
    for _ in 0..bound {
        m.step();
    }
    if m.seen {
        m.min
    } else {
        0
    }
}
