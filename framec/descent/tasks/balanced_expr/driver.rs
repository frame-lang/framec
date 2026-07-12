//! Descent battery — task: `balanced_expr`.
//!
//! The point of THIS task is to prove the harness catches the *other* failure
//! mode. `skip_string` showed it can catch "correct but slow" (`@@system`).
//! Here, `fsm_bounded` is FAST, ZERO-COPY, and CERTIFIABLY REGULAR — and WRONG.
//!
//! A battery that ranked candidates on cost would crown it. Correctness gates.

#[path = "efsm.rs"]
mod efsm_gen;
#[path = "fsm_bounded.rs"]
mod bounded_gen;

use std::time::Instant;

/// THE TASK SPECIFICATION (the incumbent): `ExprScannerFsm`'s actual core.
/// Counts ANY opener and ANY closer — kinds are never matched. A counter, not a stack.
fn incumbent(bytes: &[u8], i: usize) -> Option<usize> {
    let mut depth: i64 = 0;
    let mut j = i;
    while j < bytes.len() {
        match bytes[j] {
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(j + 1);
                }
            }
            _ => {}
        }
        j += 1;
    }
    None
}

fn efsm_sweep(bytes: &[u8], probes: &[usize]) -> (Vec<Option<usize>>, u64) {
    let mut m = efsm_gen::BalancedExpr::over(bytes);
    let mut out = Vec::with_capacity(probes.len());
    for &i in probes {
        out.push(if m.scan_at(i) && m.accepted { Some(m.cursor) } else { None });
    }
    (out, 0)
}

fn bounded_sweep(bytes: &[u8], probes: &[usize]) -> (Vec<Option<usize>>, u64) {
    let mut m = bounded_gen::BalancedExprK3::over(bytes);
    let mut out = Vec::with_capacity(probes.len());
    for &i in probes {
        out.push(if m.scan_at(i) && m.accepted { Some(m.cursor) } else { None });
    }
    (out, 0)
}

/// Corpus. Nesting depth 5 — deeper than the bounded candidate's K=3. Nothing
/// exotic: `f(g(h(i(j(x)))))` is an ordinary expression.
fn corpus(n: usize) -> (Vec<u8>, Vec<usize>) {
    let unit = b"f(g(h(i(j(x))))) + a[b[c[d]]] - {p:{q:{r:1}}};\n";
    let mut v = Vec::with_capacity(n + unit.len());
    while v.len() < n {
        v.extend_from_slice(unit);
    }
    v.truncate(n);
    // probe at every opener — the positions a real scanner is driven from
    let probes: Vec<usize> = (0..v.len())
        .filter(|&i| matches!(v[i], b'(' | b'[' | b'{'))
        .collect();
    (v, probes)
}

fn median(mut xs: Vec<f64>) -> f64 {
    xs.sort_by(|a, b| a.partial_cmp(b).unwrap());
    xs[xs.len() / 2]
}

fn main() {
    let sizes = [4_000usize, 8_000, 16_000, 32_000];
    let reps = 5;

    for &n in &sizes {
        let (buf, probes) = corpus(n);

        let mut run = |name: &str, f: &mut dyn FnMut(&[u8], &[usize]) -> (Vec<Option<usize>>, u64)| {
            let _ = f(&buf, &probes); // warm-up
            let mut ts = Vec::new();
            let mut last = Vec::new();
            let mut copied = 0;
            for _ in 0..reps {
                let t = Instant::now();
                let (r, c) = f(&buf, &probes);
                ts.push(t.elapsed().as_secs_f64());
                last = r;
                copied = c;
            }
            (name.to_string(), median(ts), copied, last)
        };

        let inc = run("incumbent(native)", &mut |b, p| {
            (p.iter().map(|&i| incumbent(b, i)).collect(), 0)
        });
        let ef = run("@@fsm(unbounded counter)", &mut efsm_sweep);
        let bd = run("@@fsm(bounded int(0..3))", &mut bounded_sweep);

        for (name, t, copied, res) in [&inc, &ef, &bd] {
            let agrees = *res == inc.3;
            println!(
                r#"{{"task":"balanced_expr","candidate":"{}","n":{},"secs":{:.9},"ns_per_el":{:.2},"bytes_copied":{},"agrees":{}}}"#,
                name,
                n,
                t,
                t * 1e9 / probes.len().max(1) as f64,
                copied,
                agrees
            );
        }
    }

    // Show the mis-scan concretely, once, on stderr — so the failure is legible
    // and not just a boolean.
    let src = b"f(g(h(i(j(x)))))REST";
    eprintln!("\n--- why the bounded candidate fails (D5: it changed the language) ---");
    eprintln!("input        : {}", String::from_utf8_lossy(src));
    let mut m = efsm_gen::BalancedExpr::over(&src[..]);
    m.scan_at(1);
    eprintln!("unbounded    : end={:>3}  region={:?}", m.cursor, String::from_utf8_lossy(&src[1..m.cursor]));
    let mut k = bounded_gen::BalancedExprK3::over(&src[..]);
    k.scan_at(1);
    eprintln!("bounded K=3  : end={:>3}  region={:?}   <-- MIS-SCAN, accepted={}, no diagnostic",
              k.cursor, String::from_utf8_lossy(&src[1..k.cursor]), k.accepted);
}
