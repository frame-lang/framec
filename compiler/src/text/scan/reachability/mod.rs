//! State-reachability analysis, **dogfooded as a plain `@@system` GRAPH WALKER** — the second
//! back-half machine, after [`super::hsm_cycle`]. No `@@[scan]`, no byte input: the graph is an
//! edge list, and the bounded drive loop lives in this wrapper (so a broken machine cannot
//! hang). A state unreachable from the start is dead code — the caller turns that into a
//! warning.
//!
//! `.gen.rs` regen: `framec-ng -l rust --emit reachability.frs | grep -v '^#!\[allow' >
//! reachability.gen.rs`.

/// Relax one edge: if `from[e]` is already visited and `to[e]` is not, mark `to[e]` and report
/// growth. The graph query+mutation LEAF; the machine owns the sweep. Bounds-checked, so a
/// malformed edge list can never panic — it simply does not grow the frontier.
fn relax(visited: &mut [bool], from: &[i32], to: &[i32], e: usize) -> bool {
    if e >= from.len() || e >= to.len() {
        return false;
    }
    let (u, v) = (from[e], to[e]);
    if u < 0 || v < 0 {
        return false;
    }
    let (u, v) = (u as usize, v as usize);
    if u >= visited.len() || v >= visited.len() {
        return false;
    }
    if visited[u] && !visited[v] {
        visited[v] = true;
        true
    } else {
        false
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
    use super::relax;
    include!("reachability.gen.rs");
}

/// Which nodes are reachable from ANY seeded node? `from[e] -> to[e]` is edge `e`; `seed[i]` is
/// the initial visited mask (**multi-source** — several start nodes at once). Returns a
/// `node_count`-long mask: `mask[i]` is true iff node `i` is reachable from the seed. This is the
/// single engine-drive path every reachability query in the compiler flows through, so there is
/// exactly one implementation of the transitive-closure walk to trust (#219).
pub fn reachable_from_seed(
    from: &[i32],
    to: &[i32],
    node_count: usize,
    seed: Vec<bool>,
) -> Vec<bool> {
    // The visited mask the engine returns is exactly `seed` grown in place; normalize its length
    // to `node_count` so a mis-sized seed can never make a caller's `mask[i]` index panic.
    let mut seed = seed;
    seed.resize(node_count, false);
    let edge_count = from.len().min(to.len());
    let mut m = fsm::Reachability::new(from.to_vec(), to.to_vec(), edge_count, node_count, seed);
    // At most node_count passes (the longest simple path), each sweeping edge_count edges plus
    // a handful of control transitions; the extra steps at $Done are no-ops.
    let bound = (node_count + 2) * (edge_count + 3) + 16;
    for _ in 0..bound {
        m.step();
    }
    m.visited
}

/// Which nodes are reachable from a single `start`? Single-source convenience over
/// [`reachable_from_seed`]. `from[e] -> to[e]` is edge `e`. Returns a `node_count`-long mask.
pub fn reachable(from: &[i32], to: &[i32], node_count: usize, start: usize) -> Vec<bool> {
    let mut seed = vec![false; node_count];
    if start < node_count {
        seed[start] = true;
    }
    reachable_from_seed(from, to, node_count, seed)
}
