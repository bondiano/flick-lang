# flick-wasm

The wasm-bindgen interface of the compiler core for the site playground:
a `Session` per edited module, plus `format`, `explain` and
`compiler_version`. `just docs-wasm` writes its bindings to the ignored
`docs/src/playground/wasm/`, imported by `compiler.worker.ts` and
`ide.worker.ts` beside it.

## Rules

- Keep compiler behavior in `flick-core`; this crate holds the in-memory
  project, converts positions and serializes results.
- Pass every position across the boundary as a UTF-16 offset
  (`utf16_offset`, `byte_offset`); the core counts UTF-8 bytes.
- Results are JSON strings: update the types in the playground workers in
  the same change as their shape.
- The core is built with `embedded-std` and `ide` only: no file system,
  Go backend or source maps. Workspace builds unify the full feature set,
  so build this package on its own
  ([Cargo features](../CONTRIBUTING.md#cargo-features)).
- The playground compiles for Flick's own runtime as `flick build --rt`
  does; `flick/tests/cli/wasm_demo.rs` pins that parity.
- Stay within the raw-byte budget, the `-le` line of the justfile's
  `wasm` recipe; `just ratchet` fails a raise without a new `# reason:`.

## Checks

Use the playground row of the
[validation matrix](../docs/agents/validation.md).

- `cargo test -p flick-wasm` — sessions with the browser feature set.
- `cargo test -p flick --test cli wasm_demo` — parity with the CLI, on
  the full-feature core.
- `cargo clippy -p flick-wasm --target wasm32-unknown-unknown -- -D warnings`
- `just wasm` — the size gate; it needs Binaryen 133
  ([setup](../CONTRIBUTING.md#setup)).
- `just e2e-js` and `just bundle-size` for the playground itself.
