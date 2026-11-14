# JavaScript backend implementation

This crate owns the Rust JavaScript/TypeScript emitter. It consumes portable
`flick_ir` modules and plain `codegen::Options`, including frontend-prepared
TypeScript projections. It does not depend on `flick-core`, `Compilation`,
`Settings`, a checker, or project/filesystem lookups.

`flick-backend-support` supplies the helpers actually shared with Go and the
frontend. `codegen::RuntimeLayout` places physical lexical files, while
`codegen::sourcemap` encodes version 3 maps from supplied source paths and
UTF-16 positions. Enable `source-maps` for mappings and map encoding.

The native development adapter is `flick-core::codegen`; frontend type rules,
`.iface`, `.d.ts` and Promise facades remain shared. The public backend ABI is
the separate [`flick:backend` world](../../flick-backend/wit/backend.wit), not
this crate's Rust interface. The `component` feature implements all three operations over those records;
`guest` exports them. Production default selection and the browser loader
still need the integration described in the
[backend contract](../../docs/backends/contract.md).

Standalone checks:

```sh
cargo test -p flick-backend-js --features source-maps
cargo check -p flick-backend-js --target wasm32-wasip2
```

Build the actual component with `just js-component` (`target/backends/js.wasm`).
It uses `wasm32-unknown-unknown` and the workspace's pinned component encoder;
no WASI adapter is attached. The WASI standard library's HashMap seed would
otherwise request ambient randomness, which this world deliberately lacks.

The component requires `flick.javascript@0.1.0` for export visibility,
entry classification and physical output/source-map paths. TS additionally
requires `flick.typescript@0.2.0`; foreign bindings use
`flick.javascript-ffi@0.1.0`. Release optimization and worker/direct-method
exports are computed from reference IR and these facts, with explicit
`Program.lang`; the frontend retains its typing and diagnostic rules.

`just wasm-backends` builds this component and exercises artifact parity and
explicit pinned `build/run/test`. This does not establish default release
selection, browser loading or complete official-backend conformance.
