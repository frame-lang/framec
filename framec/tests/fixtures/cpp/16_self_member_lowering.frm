// Regression fixture for issue #69 — C++ `@@:self` lowering in ALL sections.
//
// RFC-0046: `@@:self.<field>` lowers to `this->field`, and an embedded-system
// call `@@:self.<embed>.method()` derefs the `std::shared_ptr` as
// `this->embed->method()`. This fixture exercises every section where the
// lowering must fire: a handler body, an `operations:` body, an `actions:`
// body, a native `return @@:self.x`, a `@@:(@@:self.x)` return-expr, and a
// cross-system call. Everything here is valid C++ *except* the `@@:self`
// references, so the only thing that can make g++ reject the emitted .cpp is
// an unlowered `@@:self` — exactly #69. (A bare native `self.` is passthrough
// under RFC-0046 and would be the author's error; this fixture tests the
// portable construct.)
@@system Inner(seed: int) {
    interface:
        ping(): int
    machine:
        $A {
            ping(): int { @@:(@@:self.seed) }
    }
    domain:
        seed: int = seed
}

@@[main]
@@system SelfLowering(inner_seed: int) {
    operations:
        op_read(): int { @@:(@@:self.n) }              // operations: body

    interface:
        bump()
        via_action(): int
        cross(): int

    machine:
        $A {
            bump() { @@:self.n = @@:self.n + 1; }       // handler body
            via_action(): int { @@:(@@:self.helper()) } // action call
            cross(): int { @@:(@@:self.inner.ping()) }  // cross-system call
        }

    actions:
        helper(): int { return @@:self.n; }            // actions: body, native return

    domain:
        n: int = 0
        inner: Inner = @@Inner(inner_seed)
}
