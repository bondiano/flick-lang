# runtime-go

The Go target's runtime. `flickrt/` is written beside every Go program the
compiler emits; each other directory (`flickpg/`, `flickcompress/`) is an
optional package, written only for a program that reaches it.

## Layout

- `flickrt/` — the runtime package. The Go backend embeds its non-test
  `.go` files (`backends/go/build.rs`) and `go.mod`/`go.sum`
  (`backends/go/src/codegen/runtime.rs`).
- `flickrt/testdata/` — oracles; several are written by `tools/go-*`.
- `<package>/modules.txt` — the third-party modules an optional package
  needs; their versions come from `go.mod`.

## Rules

- Keep `flickrt` on the standard library and Go 1.23: a program with no
  optional package gets a `go 1.23` module with no requirements and builds
  offline. A third-party dependency belongs in an optional package and
  its `modules.txt`.
- Regenerate the files whose header names a `tools/go-*` generator, and
  their test data, with `just go-oracles` under the Node of
  `.tool-versions`; do not edit them by hand. `just go-oracles check` fails on any uncommitted
  change under `flickrt/`, so while editing review
  `git diff -- runtime-go/flickrt` instead.
- A std binding reaches Go through `packages/<package>/src/go/*.txt`
  (JS export to `flickrt` function).
- CLI builds use the distributed Go backend component, which embeds these
  files: after a change, run `just update-component go` before `--emit go`
  builds, e2e or `just language` see it. CI runs
  `just check-component go` through `just wasm-plugins`.
- Behavior follows [docs/formal/runtime.md](../docs/formal/runtime.md).
  `flickrt/trace.go` lists the trace events that
  `runtime/src/rt/trace.ts`, `packages/effect-ts/src/trace.ts` and
  `flick-model/src/rt/` share; change them together.

## Checks

`just go-test` for the package; the "Runtime" row of the
[validation matrix](../docs/agents/validation.md) for behavior.
