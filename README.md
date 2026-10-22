# flick-lang

Rust implementation of **Flick** — an ML-family language with HKT, type
classes and a single Effect backend (JavaScript/TypeScript output).

The language is fully specified but not yet implemented:

- Spec: [`Flick спецификация ML языка с HKT и Effect.md`](./Flick%20спецификация%20ML%20языка%20с%20HKT%20и%20Effect.md) (Flick 0.7)

## Crates

| Crate | Purpose |
| - | - |
| `flick-core` | Shared Rust core for the compiler and the LSP |
| `flick` | The `flick` CLI |
| `flick-wasm` | wasm-bindgen interface for the in-browser demo (compile preview, REPL) |

## Development

Requires Rust 1.94 (pinned in `rust-toolchain.toml`) and `just`; the e2e suite
also needs Node.js >= 24.18.0 and pnpm (pinned via `packageManager`).

```sh
just lint   # fmt --check + clippy -D warnings
just test
just check  # lint + test
just e2e    # runtime tests + compiled programs on Node
```
