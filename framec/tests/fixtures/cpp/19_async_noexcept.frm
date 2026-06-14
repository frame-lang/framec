// Gate fixture for the C++ target (#88 / RFC-0049) — an `@@[async]` system
// whose generated casing + coroutine machinery must compile under BOTH default
// and -fno-exceptions. The async casing used to wrap the busy-gate cleanup in
// exception-handling keywords plus an unconditional E703 throw; both are
// rejected with exceptions disabled. They are now an RAII busy-guard (cleanup
// on co_return AND unwind) and an #if-guarded E703 throw with an abort fallback
// (RFC-0049 R2+R3). The FrameTask's std::rethrow_exception is a function call
// (legal with exceptions off, dead because handlers never throw), so the
// wrapper was the only blocker.
//
// Self-contained (no embedded systems / driver), so the only thing that can
// make -fno-exceptions reject it is a residual unguarded exception keyword in
// the async casing.

@@[async]
@@system Async19 {
    interface:
        async work(n: int): int
    machine:
        $S {
            work(n: int): int { @@:(n) }
        }
}
