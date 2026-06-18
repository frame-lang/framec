# Framepiler

![CI](https://github.com/frame-lang/framec/actions/workflows/ci.yml/badge.svg)
![License](https://img.shields.io/badge/license-Apache--2.0-blue)
![Version](https://img.shields.io/badge/version-4.5.0-green)

framec (aka the **framepiler**) — is the transpiler for the Frame language. Currently framec supports output to 17 target languges + Graphviz. Frame is a domain-specific language for specifying state machines that transpiles to production code in multiple target languages. You write `@@system` blocks inside your native source files, and the framepiler expands them into full state machine implementations. All native code passes through unchanged — your native compiler handles everything outside the `@@system` blocks and other `@@` tagged pragmas and statements.

## Quick Start

```bash
cargo install framec
```

Create a file `hello.fpy`:

```
@@[target("python_3")]

@@system Hello {
    interface:
        greet()

    machine:
        $Start {
            greet() {
                print(f"Hello, {self.name}!")
            }
        }

    domain:
        name = "World"
}

if __name__ == "__main__":
    h = @@Hello()
    h.greet()
```

Transpile and run:

```bash
framec hello.fpy         # emits hello.py
python3 hello.py         # prints: Hello, World!
```

## Use from JavaScript / Node

framec is also published to npm as a WebAssembly build — the full transpiler,
string in / generated source out, with no native binary or subprocess:

```bash
npm install @frame-lang/framec-wasm
```

```js
const { run } = require("@frame-lang/framec-wasm");
const python = run(frameSource, "python_3"); // generated code, or error text
```

See [`framec-wasm/`](framec-wasm/) for the crate and build details.

## Supported Languages

### Core

| Language | Target Name | Extension |
|---|---|---|
| Python | `python_3` | `.fpy` |
| TypeScript | `typescript` | `.fts` |
| JavaScript | `javascript` | `.fjs` |
| C | `c` | `.fc` |
| C++ | `cpp` | `.fcpp` |
| C# | `csharp` | `.fcs` |
| Java | `java` | `.fjava` |
| Rust | `rust` | `.frs` |
| Go | `go` | `.fgo` |

### Experimental

Kotlin, Swift, PHP, Ruby, Lua, Erlang, Dart, GDScript

### Visualization

| Output | Target Name |
|---|---|
| GraphViz DOT | `graphviz` |

## Usage

```bash
# Transpile to Python (auto-detected from @@target in file)
framec myfile.fpy

# Override target language
framec -l typescript myfile.frm

# Transpile all files in a directory
framec compile-project -l python_3 -o ./output ./src

# Generate state chart
framec -l graphviz myfile.frm | dot -Tpng -o chart.png

# See all options
framec --help
```

## Documentation

📖 **Full documentation site: [docs.frame-lang.org](https://docs.frame-lang.org)** — rendered and searchable.

The same documentation is also available as source in this repository:

- [Getting Started](docs/frame_getting_started.md) — learn Frame from scratch
- [Language Reference](docs/frame_language.md) — complete Frame language reference
- [Cookbook](docs/frame_cookbook.md) — 111 recipes from traffic lights through EIP patterns, protocol/systems stress tests, deferred event processing, and a scanner/parser pair
- [Runtime Architecture](docs/frame_runtime.md) — how generated code works
- [Per-Language Guides](docs/per_language_guides/) — target-specific idioms and gotchas (Python, TypeScript, Rust, Java, …)
- [Agents Guide](docs/AGENTS_README.md) — orientation for LLM-assisted editing of Frame code
- [Framepiler Design](docs/framepiler_design.md) — transpiler internals
- [Contributing](CONTRIBUTING.md) — build from source, run tests, submit PRs
- [Changelog](CHANGELOG.md) — release history

## Versioning

Frame has two version numbers that move on different schedules:

- **framec semver** (e.g. `4.5.0`) tracks the compiler release line. Patch and minor releases are bug-fix and additive — existing `.fpy` / `.frs` / `.fts` sources continue to compile. Major bumps may require source changes; migration notes ship in [`docs/releases/`](docs/releases/).
- **Grammar version** (e.g. `v0.30`) tracks the Frame language specification itself, and moves much more slowly than the compiler.

Generated code is de facto byte-stable across patch and minor releases of `framec` for sources that don't use changed features. Each release's `CHANGELOG.md` entry calls out specifically where output differs from the previous version. See [Versioning & Stability](docs/frame_language.md#versioning--stability) in the language reference for the full contract.

## License

[Apache License 2.0](LICENSE)
