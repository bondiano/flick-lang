# flick-ir

The typed IR a checked build hands to backends, `ir` plugin hooks, the
reference model and tools, with its JSON/CBOR forms and a structural
check. The lowering that builds it lives in `flick-core/src/ir/`.

## Layout

- `src/program.rs` — `Program`, `VERSION` and the reader compatibility rule.
- `src/expr.rs`, `data.rs`, `handler.rs`, `foreign.rs` — the module tree.
- `src/visit.rs`, `src/validate.rs` — traversal, and the form check applied
  to what a plugin `transform` hook returns.
- `src/memory/` — the ANF ownership IR: produced by
  `flick-core/src/opt/perceus/`, run by `flick-model/src/memory/`; its
  contract is [docs/formal/memory.md](../docs/formal/memory.md).
- `src/bridge_plan/` — standalone foreign-bridge metadata, outside
  `Program` and `.iface`.

## Rules

- `Program` crosses process boundaries as CBOR (backend components, plugin
  `transform`) and `VERSION` keys saved precompiled modules. Bump it as the
  doc comment of `src/program.rs` says when an older reader could misread
  the change.
- Keep serde_json and CBOR code behind the `json`/`cbor` features; the
  browser compiler builds without them.
- The `flick` binary embeds the official JS and Go backends as checked-in
  components (`backends/*/package/`) built from this crate. After changing
  code they include, rebuild them with `just update-component js` and
  `just update-component go`, then review the new `backend.json` pins.

## Checks

- `cargo test -p flick-ir --all-features`: the JSON/CBOR tests are
  feature-gated, so a plain `-p flick-ir` run skips them.
- `just check-component js` and `just check-component go` after a change the
  components include.
- Then the "IR, lowering or optimization" row of the
  [validation matrix](../docs/agents/validation.md).
