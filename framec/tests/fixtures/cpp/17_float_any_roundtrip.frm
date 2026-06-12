// Runtime gate fixture for the C++ target (#77/#78) — float values through
// the std::any erasure layer, executed (not just compiled).
//
// #77's class: a bare literal deduces to double, the declared-type
// `std::any_cast<float>` read then throws std::bad_any_cast at RUNTIME while
// compiling cleanly — which is why the -fsyntax-only gate (#60) missed it.
// This fixture exercises both filed manifestations: a float state-var
// initializer (whole-number trap `0.0`) with read-modify-write, and a
// float-returning handler with a literal return (`32.0`).

#include <iostream>
#include <cstdlib>

@@[main]
@@system Roundtrip {
    interface:
        bump()
        peek(): float
        radius(): float
    machine:
        $S {
            $.cool: float = 0.0

            bump() { $.cool = $.cool + 1.5 }
            peek(): float { @@:($.cool) }
            radius(): float { @@:(32.0) }
        }
    domain:
        r: float = 1.0
}

int main() {
    auto p = @@Roundtrip();
    p.bump();
    p.bump();
    float got = p.peek();
    if (got != 3.0f) { std::cout << "FAIL peek: " << got << "\n"; return 1; }
    float rad = p.radius();
    if (rad != 32.0f) { std::cout << "FAIL radius: " << rad << "\n"; return 1; }
    std::cout << "PASS: 17_float_any_roundtrip\n";
    return 0;
}
