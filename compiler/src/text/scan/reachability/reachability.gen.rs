use std::collections::HashMap;
use std::any::Any;


// STATE-REACHABILITY analysis, dogfooded as a plain `@@system` GRAPH WALKER (the second
// back-half machine, after HsmCycle). A state that no transition/stack-push/parent path can
// reach from the start state is dead — worth a warning. The graph is an EDGE LIST: `from[e] ->
// to[e]` for each of `edge_count` edges over `node_count` nodes. `seed` is the initial visited
// set (just the start node). framec owns the WALK; the leaf `relax` queries+grows the frontier.
//
// The walk is iterative relaxation (no explicit stack): each $Pass sweeps every edge once and
// marks a `to` node visited when its `from` node already is; it repeats until a $Pass changes
// nothing (or `node_count` passes — the longest simple path — elapse). `visited` then holds
// exactly the nodes reachable from the start. The wrapper drives `step()` a bounded number of
// times and reads `visited`.

#[derive(Clone)]
enum ReachabilityVars {
    Pass {  },
    Scan {  },
    EndPass {  },
    Done {  },
}
#[derive(Clone)]
enum ReachabilityArgs {
    Pass {  },
    Scan {  },
    EndPass {  },
    Done {  },
}
#[derive(Clone)]
struct ReachabilityComp {
    state: String,
    vars: ReachabilityVars,
    args: ReachabilityArgs,
}

pub struct Reachability {
    compartment: ReachabilityComp,
    stack: Vec<ReachabilityComp>,
    pub from: Vec<i32>,
    pub to: Vec<i32>,
    pub edge_count: usize,
    pub node_count: usize,
    pub visited: Vec<bool>,
    pub changed: bool,
    pub p: usize,
    pub e: usize,
}

impl Reachability {
    pub fn new(from: Vec<i32>, to: Vec<i32>, edge_count: usize, node_count: usize, seed: Vec<bool>) -> Reachability {
        let compartment = ReachabilityComp { state: "Pass".to_string(), vars: ReachabilityVars::Pass {  }, args: ReachabilityArgs::Pass {  } };
        Reachability { compartment, stack: Vec::new(), from: from, to: to, edge_count: edge_count, node_count: node_count, visited: seed, changed: false, p: 0, e: 0 }
    }

    pub fn step(&mut self) {
        match self.compartment.state.as_str() {
            "Pass" => self.Pass_step(),
            "Scan" => self.Scan_step(),
            "EndPass" => self.EndPass_step(),
            _ => {}
        }
    }

    fn Pass_step(&mut self) {
        if self.p >= self.node_count {
            let mut __next = ReachabilityComp { state: "Done".to_string(), vars: ReachabilityVars::Done {  }, args: ReachabilityArgs::Done { } };
            self.compartment = __next;
            return Default::default();
        }
        self.changed = false;
        self.e = 0;
        let mut __next = ReachabilityComp { state: "Scan".to_string(), vars: ReachabilityVars::Scan {  }, args: ReachabilityArgs::Scan { } };
        self.compartment = __next;
        return Default::default();
    }

    fn Scan_step(&mut self) {
        if self.e >= self.edge_count {
            let mut __next = ReachabilityComp { state: "EndPass".to_string(), vars: ReachabilityVars::EndPass {  }, args: ReachabilityArgs::EndPass { } };
            self.compartment = __next;
            return Default::default();
        }
        let grew = relax(&mut self.visited, &self.from, &self.to, self.e);
        if grew {
            self.changed = true;
        }
        self.e = self.e + 1;
        let mut __next = ReachabilityComp { state: "Scan".to_string(), vars: ReachabilityVars::Scan {  }, args: ReachabilityArgs::Scan { } };
        self.compartment = __next;
        return Default::default();
    }

    fn EndPass_step(&mut self) {
        if self.changed {
            self.p = self.p + 1;
            let mut __next = ReachabilityComp { state: "Pass".to_string(), vars: ReachabilityVars::Pass {  }, args: ReachabilityArgs::Pass { } };
            self.compartment = __next;
            return Default::default();
        }
        let mut __next = ReachabilityComp { state: "Done".to_string(), vars: ReachabilityVars::Done {  }, args: ReachabilityArgs::Done { } };
        self.compartment = __next;
        return Default::default();
    }

}

