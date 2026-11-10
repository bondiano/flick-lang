# Packages

Active contributors: bondiano, Vassiliy

The repository contains Rust crates, Flick libraries, TypeScript packages, and Go modules. Their roles differ: compiler support crates define shared representations, libraries define the language surface, and runtimes execute emitted code.

## Package map

| Area | Purpose | Key source files |
| - | - | - |
| [Runtimes](runtimes/index.md) | Flick's own runtime, optional Effect adapter, and Go runtime | `runtime/package.json`, `packages/effect-ts/package.json`, `runtime-go/go.mod` |
| [Libraries](libraries.md) | Core and standard-library module families | `packages/core/flick.toml`, `packages/std/flick.toml` |
| [Plugins](plugins.md) | SDK, macros, component interface and first-party extensions | `flick-plugin/src/lib.rs`, `flick-plugin/wit/plugin.wit` |
| [Support crates](support-crates.md) | Shared IR, pretty printing, and WASM compiler API | `flick-ir/src/lib.rs`, `flick-pretty/src/lib.rs`, `flick-wasm/src/lib.rs` |

`Cargo.toml` enumerates Rust workspace members; `pnpm-workspace.yaml` enumerates JavaScript tooling, runtime, site, editor, and example packages. A Cargo member is not necessarily a separately released product.

## Changing a package boundary

Check downstream code generation, runtime imports, serialized IR, and publication metadata before changing a shared type or export. `CONTRIBUTING.md` describes capability-isolated compiler builds and release coordination. See [dependencies](../reference/dependencies.md) for external dependency boundaries and [systems](../systems/index.md) for the compiler callers.
