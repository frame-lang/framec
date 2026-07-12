//! Descent battery — task: `skip_string` (a positioned probe).
//!
//! Drives every candidate at EVERY cursor position over one buffer, which is how
//! the real SyntaxSkippers are used, and measures:
//!
//!   * correctness — all candidates must agree with the incumbent at every position
//!   * asymptotics — time at n, 2n, 4n, 8n; ~2x/doubling = linear, ~4x = quadratic
//!   * copying     — bytes copied per probe (a positioned probe should copy 0)
//!
//! Emits one JSON line per (candidate, size) on stdout for the runner to collect.

#[path = "efsm.rs"]
mod efsm_gen;
#[path = "system.rs"]
mod system_gen;

use std::time::Instant;

// ---------------------------------------------------------------------------
// The task specification. Construct-agnostic — every candidate is judged
// against THIS, not against each other.
//
//   skip_string(bytes, i) -> Option<usize>
//     Some(j) if a double-quoted string starts at i and closes at j-1
//     None    otherwise
// ---------------------------------------------------------------------------

/// CANDIDATE 0 — the incumbent: a hand-rolled native byte loop.
/// This is what `unified.rs::skip_simple_string` does today. Borrows; no copy.
fn incumbent(bytes: &[u8], i: usize) -> Option<usize> {
    if i >= bytes.len() || bytes[i] != b'"' {
        return None;
    }
    let mut j = i + 1;
    while j < bytes.len() {
        let b = bytes[j];
        j += 1;
        if b == b'\\' {
            j += 1;
        } else if b == b'"' {
            return Some(j);
        }
    }
    None
}

/// CANDIDATE 1 — `@@fsm`, borrowed + positioned (RFC-0042.1).
/// The machine is built ONCE over a borrowed slice; each probe re-seeds the
/// cursor in O(1). Zero copies for the entire sweep.
fn efsm_sweep(bytes: &[u8]) -> (Vec<Option<usize>>, u64) {
    let mut m = efsm_gen::SkipString::over(bytes);
    let mut out = Vec::with_capacity(bytes.len());
    for i in 0..bytes.len() {
        out.push(if m.scan_at(i) && m.accepted {
            Some(m.cursor)
        } else {
            None
        });
    }
    (out, 0) // bytes copied: zero
}

/// CANDIDATE 2 — `@@system`, a real machine (mode in Frame states) that must
/// OWN its input. This is #209: there is no borrowed domain type, so the buffer
/// is copied on EVERY probe — exactly as the 71 sites in the real skippers do.
fn system_sweep(bytes: &[u8]) -> (Vec<Option<usize>>, u64) {
    let mut out = Vec::with_capacity(bytes.len());
    let mut copied: u64 = 0;
    for i in 0..bytes.len() {
        let mut m = system_gen::SysSkipString::new();
        m.bytes = bytes.to_vec(); // <-- #209. The copy. O(n), once per probe.
        copied += bytes.len() as u64;
        m.end = bytes.len();
        m.skip_at(i);
        out.push(if m.ok { Some(m.result) } else { None });
    }
    (out, copied)
}

// ---------------------------------------------------------------------------
// Corpus: deterministic, and deliberately hostile — escaped quotes, empty
// strings, unterminated strings, quotes inside comments.
// ---------------------------------------------------------------------------
fn corpus(n: usize) -> Vec<u8> {
    let unit = br#"let s = "a\"b"; // a " in a comment
let t = ""; let u = "unterminated
x = y + 1;
"#;
    let mut v = Vec::with_capacity(n + unit.len());
    while v.len() < n {
        v.extend_from_slice(unit);
    }
    v.truncate(n);
    v
}

fn median(mut xs: Vec<f64>) -> f64 {
    xs.sort_by(|a, b| a.partial_cmp(b).unwrap());
    xs[xs.len() / 2]
}

fn bench<F: FnMut(&[u8]) -> (Vec<Option<usize>>, u64)>(
    mut f: F,
    buf: &[u8],
    reps: usize,
) -> (f64, u64, Vec<Option<usize>>) {
    let (_, _) = f(buf); // warm-up, discarded
    let mut times = Vec::new();
    let mut last = Vec::new();
    let mut copied = 0;
    for _ in 0..reps {
        let t = Instant::now();
        let (r, c) = f(buf);
        times.push(t.elapsed().as_secs_f64());
        last = r;
        copied = c;
    }
    (median(times), copied, last)
}

fn main() {
    // n, 2n, 4n, 8n. The @@system candidate is quadratic, so keep the base small
    // enough that 8n still terminates in reasonable time.
    let sizes = [2_000usize, 4_000, 8_000, 16_000];
    let reps = 5;

    for &n in &sizes {
        let buf = corpus(n);

        let (t_inc, c_inc, r_inc) = bench(|b| (b.iter().enumerate().map(|(i, _)| incumbent(b, i)).collect(), 0), &buf, reps);
        let (t_efsm, c_efsm, r_efsm) = bench(efsm_sweep, &buf, reps);
        let (t_sys, c_sys, r_sys) = bench(system_sweep, &buf, reps.min(3));

        // CORRECTNESS — the only gating axis (RFC-0056.1 D4).
        let efsm_ok = r_efsm == r_inc;
        let sys_ok = r_sys == r_inc;

        let row = |name: &str, t: f64, copied: u64, agrees: bool| {
            println!(
                r#"{{"task":"skip_string","candidate":"{}","n":{},"secs":{:.9},"ns_per_el":{:.2},"bytes_copied":{},"agrees":{}}}"#,
                name,
                n,
                t,
                t * 1e9 / n as f64,
                copied,
                agrees
            );
        };
        row("incumbent(native)", t_inc, c_inc, true);
        row("@@fsm(borrowed+positioned)", t_efsm, c_efsm, efsm_ok);
        row("@@system(owns input)", t_sys, c_sys, sys_ok);
    }
}
