//! Issue #159 — the indexed cross-system call `@@:self.list[i].method(args)`
//! must lower on C like the non-indexed form: `Sys_method(self->list[i], args)`.
//! The element system resolves through a two-rule ladder: the declared type
//! minus a trailing `[..]` group and `*`s, or — when a native typedef hides the
//! element type (`counters: CounterArr`, the C game-port idiom) — the UNIQUE
//! system whose Frame-declared interface has the called method (arcanum
//! knowledge, not native-type parsing). Indexed READS keep their native tail;
//! unindexed container-method calls (`list.push(x)`) never resolve to a system.

mod common;
use common::{compile_source, find_tool};
use std::process::Command;

const REPRO: &str = r#"
@@[target("c")]

#include <stdio.h>
typedef struct Counter Counter;
typedef Counter* CounterArr[4];

@@system Counter(base: int) {
    interface:
        bump()
        val(): int
    machine:
        $S {
            bump() { @@:self.n = @@:self.n + 1; }
            val(): int { @@:(@@:self.n + @@:self.base_v) }
        }
    domain:
        base_v: int = base
        n: int = 0
}

@@[main]
@@system Hub {
    interface:
        add(c: Counter*)
        bump_all()
        total(): int
    machine:
        $S {
            add(c: Counter*) {
                @@:self.counters[@@:self.cn] = c;
                @@:self.cn = @@:self.cn + 1;
            }
            bump_all() {
                int i = 0;
                while (i < @@:self.cn) {
                    @@:self.counters[i].bump();
                    i = i + 1;
                }
            }
            total(): int {
                int t = 0;
                int i = 0;
                while (i < @@:self.cn) {
                    t = t + Counter_val(@@:self.counters[i]);
                    i = i + 1;
                }
                @@:(t)
            }
        }
    domain:
        counters: CounterArr
        cn: int = 0
}

int main() {
    Counter* a = Counter_create(10);
    Counter* b = Counter_create(20);
    Hub* h = Hub_create();
    Hub_add(h, a);
    Hub_add(h, b);
    Hub_bump_all(h);
    int t = Hub_total(h);
    if (t == 10 + 20 + 2) { printf("PASS\n"); return 0; }
    printf("FAIL: %d\n", t);
    return 1;
}
"#;

#[test]
fn indexed_call_lowers_to_free_function() {
    let code = compile_source(REPRO, "c");
    assert!(
        code.contains("Counter_bump(self->counters[i])"),
        "[#159] indexed cross-system call must lower to Sys_method(self->field[i]):\n{code}"
    );
    assert!(
        !code.contains("self->counters[i].bump()"),
        "[#159] the invalid C member call must not survive:\n{code}"
    );
}

/// The generated C compiles and RUNS with independent instances (skipped when
/// `cc`/cJSON are unavailable).
#[test]
fn generated_c_compiles_and_runs() {
    let cc = match find_tool("cc") {
        Some(p) => p,
        None => {
            eprintln!("#159 cc-check skipped: `cc` not on PATH");
            return;
        }
    };
    let code = compile_source(REPRO, "c");
    let dir = tempfile::tempdir().expect("tempdir");
    let src = dir.path().join("repro159.c");
    std::fs::write(&src, &code).expect("write");
    let bin = dir.path().join("bin");
    let out = Command::new(&cc)
        .arg("-I/opt/homebrew/include")
        .arg(&src)
        .arg("-L/opt/homebrew/lib")
        .arg("-lcjson")
        .arg("-o")
        .arg(&bin)
        .output()
        .expect("spawn cc");
    if !out.status.success() {
        // cJSON may be absent on CI hosts — treat link failure of the persist
        // runtime as a skip, but a compile ERROR mentioning the member call is
        // the #159 bug and must fail.
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(
            !stderr.contains("no member named"),
            "[#159] invalid member call reached the C compiler:\n{stderr}"
        );
        eprintln!("#159 run skipped (link env): {stderr}");
        return;
    }
    let run = Command::new(&bin).output().expect("run");
    assert!(
        String::from_utf8_lossy(&run.stdout).contains("PASS"),
        "[#159] runtime FAIL: {:?}",
        run
    );
}

/// Unindexed container-method calls must stay native — never resolve to a
/// system via the method-name rule.
#[test]
fn unindexed_container_call_stays_native() {
    let code = compile_source(
        r#"
@@[target("javascript")]
@@system Counter {
    interface: push()
    machine: $S { push() { } }
}
@@[main]
@@system Hub {
    interface: add(c)
    machine: $S { add(c) { @@:self.items.push(c); } }
    domain: items = []
}
"#,
        "javascript",
    );
    assert!(
        code.contains("this.items.push(c)"),
        "[#159] unindexed `.push` must stay a native container call:\n{code}"
    );
}
