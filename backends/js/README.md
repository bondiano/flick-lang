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
this crate's Rust interface. Production component export/selection and the
browser loader still need the integration described in the
[backend contract](../../docs/backends/contract.md).

Standalone checks:

```sh
cargo test -p flick-backend-js --features source-maps
cargo check -p flick-backend-js --target wasm32-wasip2
```
