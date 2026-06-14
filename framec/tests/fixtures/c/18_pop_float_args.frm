// Runtime gate fixture for the C target (#83 / RFC-0048) — float args on a
// `pop$` transition, executed (not just compiled).
//
// At a `pop$` the popped target state is runtime-determined, so the declared
// `$>` / `<$` param type is statically unknown. The old C codegen pushed
// pop-args via `(void*)(intptr_t)(value)`, which truncates a float and leaves
// the typed reader dereferencing a non-box (crash). `{sys}_ARG_PUSH` now
// dispatches on the VALUE's static type via `_Generic`, so float pop-args
// heap-box and round-trip. Covers BOTH type-blind sites:
//   - enter-args:  `-> (3.25) pop$`  → restored `$Idle.$>(x: float)`
//   - exit-args:   `(2.5) -> pop$`   → leaving  `$Work.<$(y: float)`
// Dyadic fractions (exact in float and double) so `==` is sound while any
// integer truncation still fails.

#include <stdio.h>
#include <stdlib.h>

@@[main]
@@system PopArgs {
    interface:
        go()
        finish()
        peek_enter(): float
        peek_exit(): float
    machine:
        $Idle {
            $>(x: float) { @@:self.enter_seen = x; }
            go() { push$ -> $Work }
            peek_enter(): float { @@:(@@:self.enter_seen) }
            peek_exit(): float { @@:(@@:self.exit_seen) }
        }
        $Work {
            <$(y: float) { @@:self.exit_seen = y; }
            finish() { (2.5) -> (3.25) pop$ }
        }
    domain:
        enter_seen: float = 0.0
        exit_seen: float = 0.0
}

int main() {
    PopArgs* p = @@PopArgs();
    PopArgs_go(p);
    PopArgs_finish(p);
    double e = PopArgs_peek_enter(p);
    double x = PopArgs_peek_exit(p);
    if (e != 3.25) { printf("FAIL pop enter-arg: %f\n", e); return 1; }
    if (x != 2.5)  { printf("FAIL pop exit-arg: %f\n", x); return 1; }
    printf("PASS: 18_pop_float_args enter=%f exit=%f\n", e, x);
    PopArgs_destroy(p);
    return 0;
}
