// Runtime gate fixture for the C target (#77/#78) — float state-vars through
// the void* slot, executed (not just compiled).
//
// The C state-var path stored floats via `(void*)(intptr_t)` (truncating)
// and read them back as `(int)(intptr_t)` — values silently corrupted at
// RUNTIME while compiling cleanly. Now packed/unpacked via the c_marshal
// bit-pun pair, symmetric with interface returns (#72). This fixture
// asserts the round-tripped values, covering the state-var initializer
// (whole-number trap `0.0`), read-modify-write, and a float literal return.

#include <stdio.h>
#include <stdlib.h>

@@[main]
@@system Roundtrip {
    interface:
        bump()
        peek(): float
        radius(): float
        early(): float
    machine:
        $S {
            $.cool: float = 0.0

            bump() { $.cool = $.cool + 1.5 }
            peek(): float { @@:($.cool) }
            radius(): float { @@:(32.0) }
            early(): float { @@:return(18.0) }
        }
    domain:
        r: float = 1.0
}

int main() {
    Roundtrip* p = @@Roundtrip();
    Roundtrip_bump(p);
    Roundtrip_bump(p);
    double got = Roundtrip_peek(p);
    if (got != 3.0) { printf("FAIL peek: %f\n", got); return 1; }
    double rad = Roundtrip_radius(p);
    if (rad != 32.0) { printf("FAIL radius: %f\n", rad); return 1; }
    double er = Roundtrip_early(p);
    if (er != 18.0) { printf("FAIL early: %f\n", er); return 1; }
    printf("PASS: 17_float_roundtrip\n");
    Roundtrip_destroy(p);
    return 0;
}
