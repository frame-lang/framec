//! Issue #159 — the indexed cross-system call `@@:self.list[i].method(args)`
//! must lower on C like the non-indexed form: `Sys_method(self->list[i], args)`.
//! The element system resolves through a two-rule ladder: the declared type
//! minus a trailing `[..]` group and `*`s, or — when a native typedef hides the
//! element type (`counters: CounterArr`, the C game-port idiom) — the UNIQUE
//! system whose Frame-declared interface has the called method (arcanum
//! knowledge, not native-type parsing). Indexed READS keep their native tail;
//! unindexed container-method calls (`list.push(x)`) never resolve to a system.

mod common;
use common::{compile_expect_error, compile_source, find_tool};
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

/// #159 reopen — the called method's name colliding with the CALLING system's
/// own interface method must not block resolution: the caller is excluded from
/// the uniqueness scan (parent and child routinely share `tick`/`update`).
#[test]
fn caller_name_collision_still_lowers() {
    let code = compile_source(
        r#"
@@[target("c")]
typedef struct Counter Counter;
typedef Counter* CounterArr[4];
@@system Counter {
    interface: tick(dt: float)
    machine: $S { tick(dt: float) { @@:self.n = @@:self.n + 1; } }
    domain: n: int = 0
}
@@[main]
@@system Hub {
    interface: tick(dt: float)
    machine: $S { tick(dt: float) { @@:self._tick_all(dt); } }
    actions:
        _tick_all(dt: float) {
            @@:self.counters[0].tick(dt);
        }
    domain:
        counters: CounterArr
}
"#,
        "c",
    );
    assert!(
        code.contains("Counter_tick(self->counters[0], dt)"),
        "[#159 reopen] collision-named indexed call must still lower:\n{code}"
    );
}

/// Go twin: leading `[]*Sys` slice spelling resolves via rule 1, and the
/// collision-named call case-maps (`tick` → `Tick`).
#[test]
fn go_slice_spelling_and_collision_case_map() {
    let code = compile_source(
        r#"
@@[target("go")]
package main
@@system Counter {
    interface: tick(dt: float64)
    machine: $S { tick(dt: float64) { @@:self.n = @@:self.n + 1; } }
    domain: n: int = 0
}
@@[main]
@@system Hub {
    interface: tick(dt: float64)
    machine: $S { tick(dt: float64) { @@:self._tick_all(dt); } }
    actions:
        _tick_all(dt: float64) {
            @@:self.counters[0].tick(dt);
        }
    domain:
        counters: []*Counter
}
"#,
        "go",
    );
    assert!(
        code.contains("s.counters[0].Tick(dt)"),
        "[#159 reopen] go collision-named indexed call must case-map:\n{code}"
    );
}

/// #159 second reopen — TWO child system types define the same method
/// (`Ghost.tick` / `GhostPen.tick`: every game system has `tick`), so the
/// method-uniqueness fallback is unavailable BY DESIGN. The robust path is the
/// visible array spelling `counters: Counter*[4]` — framec emits the proper C
/// declarator (`Counter* counters[4];`, brackets after the name) and the
/// element system resolves structurally from the declared type, no inference.
#[test]
fn two_tick_bearing_children_with_visible_array_type() {
    let code = compile_source(
        r#"
@@[target("c")]
typedef struct Counter Counter;
@@system Counter {
    interface: tick(dt: float)
    machine: $S { tick(dt: float) { @@:self.n = @@:self.n + 1; } }
    domain: n: int = 0
}
@@system Pen {
    interface: tick(dt: float)
    machine: $S { tick(dt: float) { @@:self.p = @@:self.p + 1; } }
    domain: p: int = 0
}
@@[main]
@@system Hub {
    interface: run(dt: float)
    machine: $S { run(dt: float) { @@:self._tick_all(dt); } }
    actions:
        _tick_all(dt: float) {
            @@:self.counters[0].tick(dt);
            @@:self.pen.tick(dt);
        }
    domain:
        counters: Counter*[4]
        pen: Pen* = @@Pen()
}
"#,
        "c",
    );
    assert!(
        code.contains("Counter* counters[4];"),
        "[#159] array domain field must emit the C declarator form:\n{code}"
    );
    assert!(
        code.contains("Counter_tick(self->counters[0], dt)"),
        "[#159] indexed call must resolve structurally from the visible type:\n{code}"
    );
    assert!(
        code.contains("Pen_tick(self->pen, dt)"),
        "[#159] sibling plain call unaffected:\n{code}"
    );
}

/// #159 round 3 — the TOKEN RULE: the element system is the unique
/// defined-system name appearing as an identifier token in the declared type,
/// covering every visible spelling in one structural rule. C++'s canonical
/// pointer-vector resolves the COLLIDED method (tick on caller + two children)
/// and derefs `->`; a VALUE vector derefs `.`.
#[test]
fn cpp_vector_spellings_resolve_and_deref() {
    let src = |vecty: &str| {
        format!(
            r#"
@@[target("cpp_17")]
#include <vector>
@@system Counter {{
    interface: tick(dt: double)
    machine: $S {{ tick(dt: double) {{ this->n = this->n + 1; }} }}
    domain: n: int = 0
}}
@@system Pen {{
    interface: tick(dt: double)
    machine: $S {{ tick(dt: double) {{ this->p = this->p + 1; }} }}
    domain: p: int = 0
}}
@@[main]
@@system Hub {{
    interface: tick(dt: double)
    machine: $S {{ tick(dt: double) {{ @@:self._all(dt); }} }}
    actions:
        _all(dt: double) {{
            @@:self.counters[0].tick(dt);
        }}
    domain:
        counters: {vecty} = {vecty}()
}}
"#
        )
    };
    let ptr = compile_source(&src("std::vector<Counter*>"), "cpp_17");
    assert!(
        ptr.contains("this->counters[0]->tick(dt)"),
        "[#159r3] pointer-vector collided call must deref ->:\n{ptr}"
    );
    let val = compile_source(&src("std::vector<Counter>"), "cpp_17");
    assert!(
        val.contains("this->counters[0].tick(dt)"),
        "[#159r3] value-vector call must use . :\n{val}"
    );
}

/// #159 round 3 — Lua: collided indexed call resolves via the informational
/// `Counter[]` annotation (colon-lowered in BOTH handler and action bodies),
/// and the unresolvable untyped+collided case is a hard **E617** instead of a
/// silent dot call (legal Lua that passes the first arg as `self`).
#[test]
fn lua_collided_colon_and_e617() {
    let src = |field: &str| {
        format!(
            r#"
@@[target("lua")]
@@system Counter {{
    interface: tick(dt)
    machine: $S {{ tick(dt) {{ self.n = self.n + 1 }} }}
    domain: n: int = 0
}}
@@system Pen {{
    interface: tick(dt)
    machine: $S {{ tick(dt) {{ self.p = self.p + 1 }} }}
    domain: p: int = 0
}}
@@[main]
@@system Hub {{
    interface: tick(dt)
    machine: $S {{ tick(dt) {{ @@:self._all(dt) }} }}
    actions:
        _all(dt) {{
            @@:self.counters[1].tick(dt)
        }}
    domain:
        {field}
}}
"#
        )
    };
    let typed = compile_source(&src("counters: Counter[] = {}"), "lua");
    assert!(
        typed.contains("self.counters[1]:tick(dt)"),
        "[#159r3] annotated lua indexed call must colon-lower (action body too):\n{typed}"
    );
    let err = compile_expect_error(&src("counters = {}"), "lua");
    assert!(
        err.contains("E617"),
        "[#159r3] unresolvable lua indexed call must be E617, not a silent dot:\n{err}"
    );
}

/// #164 — the C flavor of E617: a typedef-hidden element type with a
/// collided method must be a hard error (the verbatim member call is invalid
/// C — structs have no methods), never silent invalid output. The
/// unique-method typedef case still resolves via rule 2, and the
/// direct-named spelling stays the documented shape.
#[test]
fn c_typedef_collided_is_e617() {
    let src = |domain_ty: &str, with_pen: bool| {
        let pen = if with_pen {
            r#"
@@system Pen {
    interface: tick(dt: float)
    machine: $S { tick(dt: float) { @@:self.p = @@:self.p + 1; } }
    domain: p: int = 0
}
"#
        } else {
            ""
        };
        format!(
            r#"
@@[target("c")]
typedef struct Counter Counter;
typedef Counter* CounterPtr;
@@system Counter {{
    interface: tick(dt: float)
    machine: $S {{ tick(dt: float) {{ @@:self.n = @@:self.n + 1; }} }}
    domain: n: int = 0
}}
{pen}
@@[main]
@@system Hub {{
    interface: run(dt: float)
    machine: $S {{ run(dt: float) {{ @@:self._all(dt); }} }}
    actions:
        _all(dt: float) {{
            @@:self.counters[0].tick(dt);
        }}
    domain:
        counters: {domain_ty}
}}
"#
        )
    };
    // collided (Counter.tick + Pen.tick) + typedef → E617 with C guidance
    let err = compile_expect_error(&src("CounterPtr[4]", true), "c");
    assert!(
        err.contains("E617") && err.contains("invalid C"),
        "[#164] typedef+collided must be E617 with the C consequence:\n{err}"
    );
    // unique method + typedef → rule 2 resolves, no error
    let ok = compile_source(&src("CounterPtr[4]", false), "c");
    assert!(
        ok.contains("Counter_tick(self->counters[0], dt)"),
        "[#164] unique-method typedef must still lower via rule 2:\n{ok}"
    );
    // direct-named + collided → rule 1 resolves, no error
    let ok2 = compile_source(&src("Counter*[4]", true), "c");
    assert!(
        ok2.contains("Counter_tick(self->counters[0], dt)"),
        "[#164] direct-named spelling must lower regardless of collisions:\n{ok2}"
    );
}
