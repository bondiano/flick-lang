# flick

The `flick` command-line tool. It drives `flick-core` for commands,
caches, package fetching, the language server and the wasmtime hosts of
backend and plugin components. Language rules, `flick.toml` parsing and
editor queries live in `flick-core`.

## Layout

- `src/args.rs`, `src/commands/` — clap arguments; a module per command.
- `src/lsp/` — protocol, scheduling and publishing over
  `flick-core/src/ide/`.
- `src/backends/`, `src/component.rs`, `src/plugins/` — component hosts.
- `src/stdlib.rs` — which implicit packages a run uses.
- `tests/cli/` — one test binary; `tests/e2e.rs` runs `tests/run/`.

## Rules

- Add a CLI suite as a file in `tests/cli/` plus a `pub mod` line in its
  `main.rs`.
- Mark a test that needs Node, Go or Docker `#[ignore = "needs …"]`.
  `just e2e-tests` runs every ignored test of `flick` and `flick-core`
  unless `tests/ignored_tests.rs` maps it to another recipe; extend that
  mapping when a test needs another runner.
- Build code under `cfg(feature = "wasm-plugins")` with
  `just wasm-plugins`; `just lint` does not compile it, releases do.
- In `src/lsp/` and `src/mapper.rs`, stdout carries only the protocol:
  log through `tracing` or stderr.
- LSP message handlers run in `Server::guarded`, which catches a panic,
  logs `language server panicked` and drops caches. The analysis thread
  in `src/lsp/worker.rs` has no guard: after a panic there, `Server::poll`
  starts a new worker, held requests are analyzed in place, and
  diagnostics update again on the next edit.
- Update `docs/src/content/docs/reference/cli.md` and its `ru/` pair
  with a user-visible command or flag.

## Checks

Pick rows from the [validation matrix](../docs/agents/validation.md).

- `cargo test -p flick --bin flick <module>::` — unit tests, e.g. `lsp::`.
- `cargo test -p flick --test cli <suite>` — CLI integration tests.
- `cargo test -p flick --test ignored_tests` after adding an `#[ignore]`.
- One case of `tests/run`, after `just e2e-prepare`:
  `FLICK_E2E_ONLY=<path part> cargo test --profile e2e -p flick --test e2e -- --ignored run_cases`
- `just lsp-bench` and `just lsp-soak` for LSP latency and memory.
