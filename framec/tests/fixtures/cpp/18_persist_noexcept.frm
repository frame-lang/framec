// Gate fixture for the C++ target (#87 / RFC-0049) — persisted system whose
// generated save/load must compile under BOTH default and `-fno-exceptions`
// (the Godot-web requirement). It exercises every save/restore path the #87
// change touches: a state-VAR (int), a typed state-ARG (float), and an
// enter-ARG (int). The save side now probes `std::any` with the non-throwing
// pointer `any_cast<T>(&v)` (R1, never a catch); the E700 quiescence guard and
// the tolerant typed restore keep `throw`/`try` only behind
// `#if defined(__cpp_exceptions)`, with an `abort`/null-guard fallback (R3).
//
// Everything here is valid C++ apart from its Frame constructs, so the only
// thing that can make `-fno-exceptions` reject the emitted .cpp is a residual
// unguarded `try`/`catch`/`throw` in the persist codegen.
//
// No `#include <nlohmann/json.hpp>` here on purpose: framec emits it for
// persisted C++ systems (#94), so this fixture also proves the output compiles
// STANDALONE.

@@[persist(std::string)]
@@[save(save_state)]
@@[load(load_state)]
@@system Persist18 {
    interface:
        setup(n: int, f: float)
        read(): int
    machine:
        $Idle {
            setup(n: int, f: float) { -> (n) $Active(f) }
            read(): int { @@:(0) }
        }
        $Active(sa: float) {
            $.local: int = 0

            $>(ea: int) { $.local = ea }
            read(): int { @@:($.local) }
        }
}
