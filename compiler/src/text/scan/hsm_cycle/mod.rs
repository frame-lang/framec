//! HSM parent-chain cycle detector, **dogfooded as a plain `@@system` GRAPH WALKER** — the
//! first back-half machine, and the demonstration that dogfooding is not only for byte
//! scanners. There is no `@@[scan]` here and no byte input; the graph is the parent array,
//! and the drive loop lives in this wrapper (bounded, so a broken machine cannot hang).
//!
//! A cycle in the parent chain would infinite-loop the HSM handler dispatch — this catches it.
//!
//! `.gen.rs` regen: `framec-ng -l rust --emit hsm_cycle.frs | grep -v '^#!\[allow' >
//! hsm_cycle.gen.rs`.

/// The parent of node `cur` in the graph, or `-1` if it is a root / out of range. The graph
/// query LEAF; the machine owns the walk.
fn parent_of(parents: &[i32], cur: i32) -> i32 {
    if cur >= 0 && (cur as usize) < parents.len() {
        parents[cur as usize]
    } else {
        -1
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
    use super::parent_of;
    include!("hsm_cycle.gen.rs");
}

/// Does the parent chain contain a cycle? `parents[i]` is the parent index of state `i`, or a
/// negative value for a root. Driven by the HsmCycle graph-walker system.
pub fn has_cycle(parents: &[i32]) -> bool {
    let n = parents.len();
    let mut m = fsm::HsmCycle::new(parents.to_vec(), n);
    // Each of n start nodes follows at most count+1 hops before a root or the cycle guard, so
    // the walk terminates well within this bound; the extra steps at $Done are no-ops.
    let bound = (n + 2) * (n + 2) + 16;
    for _ in 0..bound {
        m.step();
    }
    m.cyclic
}
