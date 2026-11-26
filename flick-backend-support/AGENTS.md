# flick-backend-support

Portable helpers shared by the JS/Go emitters and the frontend projections.
[README.md](README.md) maps each module to its consumers and states the
dependency boundary.

## Layout

- `src/decision.rs` — the `match` decision graph that `decide.rs` in both
  emitters walks.
- `src/layout/` — deterministic data layout computation (`LayoutEngine`,
  `PrimitiveTable`, niches, field packing, and finite two-phase recursion
  indirection) for layout-aware backends.
- `src/specialize/` — deterministic whole-program polymorphic specialization
  with uniform boxed-cell fallback (`specialize`, `SpecializeConfig`,
  `SpecializedProgram`, `SpecializeStats`) for layout-aware backends, gated
  behind the opt-in `specialize` feature so existing JS/Go/Lua components stay
  unchanged until they opt in.
- `src/opt/` — the portable release passes. `flick-core/src/opt/` is their
  compiler adapter and holds most of their tests and snapshots.
- `build.rs` — generates the binding tables from
  `packages/effect-ts/compiler-{bindings,families}.json`,
  `runtime/rt_{exports,stubs,unavailable}.txt` and
  `packages/effect-ts/rt_bridge_stubs.txt`.

## Rules

- Add a helper here only for a second concrete consumer; target
  representations stay in `backends/js/` or `backends/go/`.
- Change a binding table in its package-owned source file; the build
  script regenerates the Rust constants.
- Release passes keep every observable contract of the debug lowering, as
  [the codegen reference](../docs/src/content/docs/reference/codegen.md)
  states.
- Code behind `native-project` is absent from minimal and browser builds.
- The official JS/Go components in `backends/*/package/` are built from
  this crate and embedded in `flick`; rebuild them with
  `just update-component js` and `just update-component go`.

## Checks

- `cargo test -p flick-backend-support`
- `cargo test -p flick-core --lib opt::` for passes and
  `cargo test -p flick-core --lib codegen::` for emitted code.
- `just feature-check` when `native-project`-gated code changes.
- `just check-component js` and `just check-component go`.
- Then the "IR, lowering or optimization" row of the
  [validation matrix](../docs/agents/validation.md).
