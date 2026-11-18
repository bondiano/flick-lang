# flick-backend

The `flick:backend` component contract: its WIT world, the records
generated from it, request/result validation and the handshake. The
`host` feature adds the wasmtime host. The official emitters in
[`backends/`](../backends/AGENTS.md) and the fixture
`tests/backends/literal` implement the world; `flick/src/backends/`
selects and drives components.

## Rules

- `wit/backend.wit` is the source of truth: `src/lib.rs` generates the
  native records from it. Keep `VERSION` equal to its package version.
- The ABI and record schemas are snapshots in `tests/snapshots/`. Review
  their diff as a contract change and update the
  [contract](../docs/backends/contract.md).
- Keep `inputs.read` the world's only import: a backend gets no
  filesystem, clock, network or process access.
- Both official components link this crate. After a change, run
  `just check-component js` and `just check-component go`; when the bytes
  differ, `just update-component <name>` refreshes the shipped image.

## Checks

- `cargo test -p flick-backend`
- `just wasm-backends`: the host tests, both official components rebuilt
  and compared, and the conformance run over the language corpus.
- The "Plugins or their host" row of the
  [validation matrix](../docs/agents/validation.md) for host changes.
