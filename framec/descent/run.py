#!/usr/bin/env python3
"""The Descent battery (RFC-0056.1) — standalone local harness.

For each task, compile every candidate with framec, build its driver, run it over
a fixed corpus at n / 2n / 4n / 8n, and emit the report.

    python3 framec/descent/run.py            # run + write report/latest.{json,md}
    python3 framec/descent/run.py --task skip_string

Design notes (see RFC-0056.1 §4.4):

  * CORRECTNESS gates. COST NEVER DOES. This is a laboratory instrument, not a
    ratchet. Production picks the fastest correct candidate by default; the
    battery exists so that choice is made from data instead of folklore.

  * We measure WORK, not just wall time. Time can hide super-linear work behind a
    fast memcpy — `bytes_copied` cannot. The `@@system` candidate's wall-clock
    growth reads ~2.8x/doubling (ambiguous), while its bytes-copied growth is a
    clean 4x — i.e. provably quadratic. Always report both.
"""
import argparse, json, os, subprocess, sys, time

HERE = os.path.dirname(os.path.abspath(__file__))
TASKS = os.path.join(HERE, "tasks")
REPORT = os.path.join(HERE, "report")
FRAMEC = os.path.expanduser("~/.frame/local/bin/framec")


def sh(cmd, cwd=None):
    r = subprocess.run(cmd, cwd=cwd, shell=True, capture_output=True, text=True)
    return r.returncode, r.stdout, r.stderr


def build_task(task_dir):
    """framec-compile every .frs candidate, then rustc the driver."""
    for frs in sorted(f for f in os.listdir(task_dir) if f.endswith(".frs")):
        rc, out, err = sh(f"{FRAMEC} compile -l rust -o . {frs}", cwd=task_dir)
        if rc != 0:
            return f"framec failed on {frs}: {(out + err).strip().splitlines()[-1]}"
    rc, out, err = sh("rustc --edition 2021 -O -o driver driver.rs", cwd=task_dir)
    if rc != 0:
        return f"rustc failed: {err.strip().splitlines()[0] if err.strip() else '?'}"
    return None


def run_task(name):
    d = os.path.join(TASKS, name)
    err = build_task(d)
    if err:
        return {"task": name, "error": err, "rows": []}
    rc, out, _ = sh("./driver", cwd=d)
    rows = [json.loads(l) for l in out.splitlines() if l.startswith("{")]
    return {"task": name, "error": None, "rows": rows}


def growth(series):
    """Ratio per doubling. ~2 = linear, ~4 = quadratic."""
    return [None] + [
        (b / a if a else None) for a, b in zip(series, series[1:])
    ]


def render(results):
    lines = []
    lines.append("# The Descent — battery report\n")
    lines.append(f"_Generated {time.strftime('%Y-%m-%d %H:%M')} · "
                 f"`{os.uname().sysname} {os.uname().machine}` · "
                 f"framec `{sh(FRAMEC + ' --version')[1].strip()}`_\n")
    lines.append("**Correctness gates. Cost is data, never a veto** (RFC-0056.1 D4) — "
                 "production uses the most efficient correct candidate by default.\n")
    lines.append("> Read `copy-growth` before `time-growth`. Wall time can hide "
                 "super-linear *work* behind a fast `memcpy`; bytes-copied cannot.\n")

    for res in results:
        lines.append(f"\n## TASK: `{res['task']}`\n")
        if res["error"]:
            lines.append(f"**BUILD FAILED** — {res['error']}\n")
            continue
        cands = []
        for r in res["rows"]:
            if r["candidate"] not in cands:
                cands.append(r["candidate"])

        lines.append("| candidate | n | ns/el | time-growth | bytes copied | copy-growth | agrees |")
        lines.append("|---|---:|---:|---:|---:|---:|:--:|")
        for c in cands:
            rs = [r for r in res["rows"] if r["candidate"] == c]
            tg = growth([r["secs"] for r in rs])
            cg = growth([r["bytes_copied"] for r in rs])
            for r, t, g in zip(rs, tg, cg):
                lines.append(
                    f"| `{c}` | {r['n']} | {r['ns_per_el']:.1f} | "
                    f"{'—' if t is None else f'{t:.2f}x'} | {r['bytes_copied']:,} | "
                    f"{'—' if not g else f'{g:.2f}x'} | "
                    f"{'✅' if r['agrees'] else '❌'} |"
                )
        lines.append("")
    return "\n".join(lines)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--task", default=None)
    a = ap.parse_args()

    names = [a.task] if a.task else sorted(
        d for d in os.listdir(TASKS) if os.path.isdir(os.path.join(TASKS, d))
    )
    results = [run_task(n) for n in names]

    os.makedirs(REPORT, exist_ok=True)
    with open(os.path.join(REPORT, "latest.json"), "w") as f:
        json.dump(results, f, indent=1)
    md = render(results)
    with open(os.path.join(REPORT, "latest.md"), "w") as f:
        f.write(md)
    print(md)

    # Correctness is the ONLY gate.
    bad = [r for res in results for r in res["rows"] if not r["agrees"]]
    if bad:
        print(f"\nFAIL: {len(bad)} candidate/size pairs disagree with the task spec.")
        return 1
    if any(res["error"] for res in results):
        return 1
    print("\nAll candidates agree with the task specification.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
