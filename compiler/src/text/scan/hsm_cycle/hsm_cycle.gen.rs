use std::collections::HashMap;
use std::any::Any;

struct Compartment {
    state: String,
    state_vars: HashMap<String, Box<dyn Any>>,
    state_args: HashMap<String, Box<dyn Any>>,
}
impl Compartment {
    fn new(state: &str) -> Compartment {
        Compartment { state: state.to_string(), state_vars: HashMap::new(), state_args: HashMap::new() }
    }
}


// HSM parent-chain CYCLE detector, dogfooded as a plain `@@system` GRAPH WALKER (not a byte
// scanner) — the first back-half machine, cracking the non-byte drive pattern. A cycle in the
// parent chain (`$A => $B => $A`) would infinite-loop the HSM handler dispatch, so it must be
// caught. The graph is the `parents` array (parent[i] = parent index, or -1 for a root),
// passed via `new(parents, count)`. framec owns the WALK ($Next picks a start node, $Follow
// chases parents); the leaf `parent_of` queries the graph. A node whose chain exceeds `count`
// hops is in a cycle (pigeonhole). The wrapper drives `step()` a bounded number of times.
//
// cyclic ends true iff any parent chain cycles.

pub struct HsmCycle {
    compartment: Compartment,
    stack: Vec<Compartment>,
    pub parents: Vec<i32>,
    pub count: usize,
    pub k: usize,
    pub cur: i32,
    pub steps: usize,
    pub cyclic: bool,
}

impl HsmCycle {
    pub fn new(parents: Vec<i32>, count: usize) -> HsmCycle {
        let mut compartment = Compartment::new("Next");
        HsmCycle { compartment, stack: Vec::new(), parents: parents, count: count, k: 0, cur: 0, steps: 0, cyclic: false }
    }

    pub fn step(&mut self) {
        match self.compartment.state.as_str() {
            "Next" => self.Next_step(),
            "Follow" => self.Follow_step(),
            _ => {}
        }
    }

    fn Next_step(&mut self) {
        if self.k >= self.count {
            let mut __next = Compartment::new("Done");
            self.compartment = __next;
            return Default::default();
        }
                self.cur = self.k as i32;
                self.steps = 0;
        let mut __next = Compartment::new("Follow");
        self.compartment = __next;
        return Default::default();
    }

    fn Follow_step(&mut self) {
        let p = parent_of(&self.parents, self.cur);
                if p < 0 {
                    self.k = self.k + 1;
            let mut __next = Compartment::new("Next");
            self.compartment = __next;
            return Default::default();
        }
                if self.steps > self.count {
                    self.cyclic = true;
            let mut __next = Compartment::new("Done");
            self.compartment = __next;
            return Default::default();
        }
                self.cur = p;
                self.steps = self.steps + 1;
        let mut __next = Compartment::new("Follow");
        self.compartment = __next;
        return Default::default();
    }

}

