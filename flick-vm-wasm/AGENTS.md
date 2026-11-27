# flick-vm-wasm

The `wasm-bindgen` runtime wrapper around `flick-vm` (`default-features = false`)
for browsers and Node.js. Exposes a single-threaded cooperative `VmSession`
that verifies and loads/saves `.fvm` images from `Uint8Array` buffers, enforces
`#992` capabilities and resource limits (with wall-time deadlines backed by JS
`performance.now()` on `wasm32`), bridges console/clock/random/env/fetch host
callbacks to JS, and steps evaluations in bounded fuel slices (`start_eval`,
`step`, `abandon`).

## Rules

- Depend on `flick-vm` with `default-features = false` so zero OS-only crates
  (`tokio`, `hyper`, `wasmtime`, `libffi`, `rustix`, `mio`) enter the
  `wasm32-unknown-unknown` dependency graph (`flick-vm/tests/wasm_deps.rs`
  enforces this).
- Keep VM execution and sandboxing semantics in `flick-vm`; this crate only
  bridges JS configuration, host callbacks, `performance.now()`, and JSON
  serialization across the `wasm-bindgen` boundary.
- Stay within the gzipped size budget enforced by `just wasm` (separate from
  `flick_wasm.wasm`'s raw-byte budget); `just ratchet` requires a `# reason:`
  for any budget increase.

## Checks

- `cargo clippy -p flick-vm-wasm --target wasm32-unknown-unknown -- -D warnings`
- `cargo test -p flick-vm-wasm`
- `cargo test -p flick-vm --test wasm_deps`
- `just wasm`
- `node tools/vm-wasm/index.mjs`
