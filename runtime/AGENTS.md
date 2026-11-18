# runtime

`@flick-lang/runtime`: Flick's own JavaScript runtime, which JS output
imports by default (`effect = "rt"`). The Effect adapter is
`packages/effect-ts/`; the Go runtime is `runtime-go/`.

## Layout

- `src/core/` — data and pure helpers; `stubs.ts` throws for bindings
  only the adapter implements.
- `src/rt/` — the kernel (fibers, owners, causes, layers) and runners;
  `src/rt/effect/` binds std modules such as `Effect` and `Queue` on it.
- `src/ui/` — DOM, markup and React helpers.
- `rt_exports.txt`, `rt_stubs.txt`, `rt_unavailable.txt` — written from
  `dist/` by `tools/rt-exports.mjs`; the compiler binds an export to this
  runtime only when it is listed, otherwise it uses the adapter.
- `adapter_only/` — std bindings only the adapter has; the list only
  shrinks (`flick-core/src/build/capability_tests.rs`).

## Rules

- Keep the package free of dependencies and Effect imports
  (`test/package-boundary.test.ts`).
- After changing exports, run
  `pnpm -C runtime build && node tools/rt-exports.mjs` and keep all four
  files it writes, including `packages/effect-ts/rt_bridge_stubs.txt`.
- The manifests are compiled into the official JS backend component, which
  CLI builds use: refresh it with `just update-component js`; CI runs
  `just check-component js` through `just wasm-plugins`.
- Compiled programs, the site and the e2e suites import `dist/`: rebuild
  it after editing `src/`.
- Behavior follows the runtime contract in
  [docs/formal/runtime.md](../docs/formal/runtime.md); trace events match
  `runtime-go/flickrt/trace.go`.
- Keep `src/rt/vendor/` (vendored find-my-way) close to its source.

## Checks

The "Runtime" row of the [validation matrix](../docs/agents/validation.md).
Manifest drift alone: `node tools/rt-exports.mjs --check` after a build.
