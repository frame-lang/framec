# @frame-lang/framec-wasm

WebAssembly build of [**framec**](https://github.com/frame-lang/framec) — the
transpiler for the [Frame](https://github.com/frame-lang/framec) state-machine
language. String in, generated source out, entirely in-process: no native
binary, no subprocess, no network. Ideal for web playgrounds, editor
integrations, and Node tooling.

Frame is a DSL for specifying state machines that transpiles to production code
in 17 target languages. This package exposes a single function that runs the
full framepiler compiled to WebAssembly.

## Install

```bash
npm install @frame-lang/framec-wasm
```

## Usage (Node)

```js
const { run } = require("@frame-lang/framec-wasm");

const source = `
@@system Hello {
    interface:
        greet()
    machine:
        $Start {
            greet() {
                print("Hello, World!")
            }
        }
}
`;

const python = run(source, "python_3");
console.log(python); // generated Python source
```

## API

```ts
function run(source: string, target: string): string;
```

- **`source`** — Frame source text.
- **`target`** — one of framec's target names: `"python_3"`, `"typescript"`,
  `"javascript"`, `"c"`, `"cpp"`, `"csharp"`, `"java"`, `"rust"`, `"go"`,
  `"kotlin"`, `"swift"`, `"php"`, `"ruby"`, `"lua"`, `"erlang"`, `"dart"`,
  `"gdscript"`, or `"graphviz"`. The caller selects the target, so the source
  need not carry an `@@[target(...)]` directive.
- **Returns** the generated code, or the compiler's error text on failure.

## License

[Apache-2.0](https://github.com/frame-lang/framec/blob/main/LICENSE) — same as framec.
