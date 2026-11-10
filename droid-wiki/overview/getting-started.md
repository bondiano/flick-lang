# Getting started

This page describes the repository's pinned development setup. Commands below are documented workflows, not evidence that this wiki generation ran the compiler or test suites.

## Prerequisites

`rust-toolchain.toml` pins Rust 1.95.0 with rustfmt, Clippy, and the `wasm32-unknown-unknown` and `wasm32-wasip2` targets. `package.json` pins pnpm 12.16.1 and requires Node.js 24.18.0 or newer. Go 1.23 or newer is needed for Go-target work; Lean is pinned by `formal/lean-toolchain`.

`CONTRIBUTING.md` also calls for `just`, cargo-machete 0.9.2, and cargo-nextest for the relevant checks. `flake.nix` defines a development shell.

## Build and run

From the repository root:

```sh
pnpm install --frozen-lockfile
cargo build -p flick
./target/debug/flick --project examples/first-slice run
```

For direct Cargo tests that load the standard library, set its path in the same shell:

```sh
export FLICK_STD_PATH="$PWD/packages/std"
cargo test -p flick-core --test check
```

`justfile` sets this variable automatically. `flick/src/stdlib.rs` handles the installed compiler's standard-library discovery and fetching; using the local package avoids the release-tag fetch.

## Validation

```sh
just lint
just test
just check
just e2e
just model
```

These are separate gates. In particular, `just check` does not include the reference model, Lean, or all runtime integration checks. Select the applicable commands from `docs/agents/validation.md`, summarized in [testing](../how-to-contribute/testing.md).

## Documentation site

The production build includes a WASM compiler and a Pagefind search index:

```sh
pnpm -C runtime build
pnpm -C packages/effect-ts build
just docs
pnpm -C docs preview
```

`just docs-dev` starts the development workflow. See [browser tools](../systems/browser-tools.md) and `CONTRIBUTING.md` before changing playground or widget code.
