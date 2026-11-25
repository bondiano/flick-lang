# backends

The official code generators, each a Rust crate and a shipped component of
the [`flick:backend`](../flick-backend/AGENTS.md) world: `js` (JavaScript
and TypeScript, see its [README](js/README.md)) and `go` (one Go module per
entry, with `runtime-go` embedded). They read portable `flick_ir` and the
host's projections, never `flick-core`.
`lua` is a preview component of the same world, not official: it pins the
host profiles of `lua/profiles/` and checks programs against them, and its
`generate` refuses. Its emitter, `lua/src/emit/`, lowers the pure core of the
IR to structured Lua and checks the dialect's VM limits; `generate` does not
call it yet. See [Lua host profiles](../docs/backends/lua-profiles.md).

## Layout

- `<name>/src/codegen/` — the emitter.
- `<name>/src/component/` — the world's exports: request data, optimize,
  emit.
- `<name>/package/` — `component.wasm` and its pin `backend.json` with the
  ranges its descriptor accepts. A compiler built from this checkout reads
  them (`flick/src/backends/official.rs`); a release resolves them from the
  release a tag `backend-<name>-v<version>` publishes (`backends.yml`).
- `flick/src/backends/request/` — the projections the host sends.
- `flick-core/src/codegen/` — emitter tests, linking these crates natively.

## Rules

- `flick build/run/test` run the committed `package/component.wasm`.
  After changing anything a component links, run `just update-component
  <name>`, or the CLI and the e2e suites keep the old code. The browser
  compiler (JS) and `flick-core` tests see a change at once.
- Both images embed the binding tables `flick-backend-support/build.rs`
  reads; the Go image also embeds `runtime-go/flickrt`, its optional
  packages and `go.mod`/`go.sum` (`go/build.rs`).
- Find built-ins through `Program.lang` by role (`flick-ir/src/lang.rs`),
  not by module or name; their target forms belong here.
- Go tables of `packages/*/src/go/` bind foreign keys to `flickrt`; their
  format is on `Bindings::parse` in `go/src/codegen/foreign.rs`.

## Checks

- `cargo test -p flick-core --lib codegen::` and the JS README's
  standalone checks.
- `just check-component js` or `go`, then `just wasm-backends`.
- Lua: `cargo test -p flick-backend-lua`, `just lua-probes` (in `nix
  develop`), `just check-component lua`, and the `lua_profiles` tests of
  `just wasm-backends`.
- The "IR, lowering or optimization" row of the
  [validation matrix](../docs/agents/validation.md).
