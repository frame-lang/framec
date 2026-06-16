// Runtime gate fixture for the C target (#77/#78/#81) — floats through
// every void* slot family, executed (not just compiled).
//
// The C float path has regressed twice while compiling cleanly: first
// stored via `(void*)(intptr_t)` (truncating, #77/#78), then via a
// pointer-width bit-pun that corrupted on 32-bit targets (#81). Doubles
// now travel as heap/stack boxes with container ownership. This fixture
// asserts round-tripped values across: state-var init + read-modify-write,
// an interface float PARAM (wrapper stack-box), float returns (`@@:(...)`
// and early `@@:return(...)`), and float STATE / ENTER / EXIT args
// (typed owned pushes).
//
// All literals are dyadic fractions (exact in both float and double), so
// `==` asserts are bit-sound while any integer truncation still fails:
// a truncating slot turns 3.25 into 3, 0.75 into 0.

#include <stdio.h>
#include <stdlib.h>

@@[main]
@@system Roundtrip {
    interface:
        bump()
        set(v: float)
        peek(): float
        radius(): float
        early(): float
        go()
        result(): float
        exits(): float
    machine:
        $S {
            $.cool: float = 0.25

            <$(x: float) { @@:self.exit_seen = x; }

            bump() { $.cool = $.cool + 1.5 }
            set(v: float) { $.cool = v }
            peek(): float { @@:($.cool) }
            radius(): float { @@:(32.5) }
            early(): float { @@:return(18.25) }
            go() { (1.25) -> (0.75) $T(2.5) }
        }

        $T(sa: float) {
            $.sum: float = 0.0

            $>(ea: float) { $.sum = sa + ea }

            result(): float { @@:($.sum) }
            exits(): float { @@:(@@:self.exit_seen) }
        }
    domain:
        exit_seen: float = 0.0
}

int main() {
    Roundtrip* p = @@Roundtrip();
    Roundtrip_bump(p);
    Roundtrip_bump(p);
    double got = Roundtrip_peek(p);
    if (got != 3.25) { printf("FAIL peek: %f\n", got); return 1; }
    Roundtrip_set(p, 6.25);
    double set_back = Roundtrip_peek(p);
    if (set_back != 6.25) { printf("FAIL set/peek: %f\n", set_back); return 1; }
    double rad = Roundtrip_radius(p);
    if (rad != 32.5) { printf("FAIL radius: %f\n", rad); return 1; }
    double er = Roundtrip_early(p);
    if (er != 18.25) { printf("FAIL early: %f\n", er); return 1; }
    Roundtrip_go(p);
    double res = Roundtrip_result(p);
    if (res != 3.25) { printf("FAIL state+enter args: %f\n", res); return 1; }
    double ex = Roundtrip_exits(p);
    if (ex != 1.25) { printf("FAIL exit arg: %f\n", ex); return 1; }
    printf("PASS: 17_float_roundtrip\n");
    Roundtrip_destroy(p);
    return 0;
}
