// Compile-gate fixture for the C target (#60) — covers the two C codegen
// bug classes that text snapshots were blind to:
//
//   #72 — void*-slot marshalling: float return (symmetric pack/unpack),
//         float param, struct-by-value param (wrapper stack-box), and
//         struct-by-value return (heap box).
//   #73 — embedded-system call `@@:self.sub.method()` with the C-idiomatic
//         pointer-typed field (`sub: Sub*`) must lower to the free-function
//         family `Sub_method(self->sub, …)`, not struct-member access.
//
// Everything here is valid C apart from the Frame constructs, so the only
// thing that can make `gcc -fsyntax-only` reject the emitted .c is a
// marshalling or embed-lowering regression.

#include <stdio.h>
#include <stdlib.h>
#include <string.h>

typedef struct { float x; float y; } Vector2;

@@system Inner {
    interface:
        ping(): int
    machine:
        $A { ping(): int { @@:(7) } }
}

@@[main]
@@system Marshal {
    interface:
        set_court(size: Vector2)
        set_margin(m: float)
        court(): Vector2
        radius(): float
        relay(): int
    machine:
        $A {
            set_court(size: Vector2) {
                @@:self.w = size.x;
                @@:self.h = size.y;
            }
            set_margin(m: float) {
                @@:self.margin = m;
            }
            court(): Vector2 {
                Vector2 v;
                v.x = @@:self.w;
                v.y = @@:self.h;
                @@:(v)
            }
            radius(): float { @@:(@@:self.w / 2.0f + @@:self.margin) }
            relay(): int { @@:(@@:self.inner.ping()) }
        }
    domain:
        w: float = 0.0f
        h: float = 0.0f
        margin: float = 0.0f
        inner: Inner* = @@Inner()
}
