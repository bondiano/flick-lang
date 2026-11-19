# plugins

The plugins of the standard library: `cli`, `derive` (derives, `server
fn`, `codec`), `refined`, `sql`, `test` and `ui`. They are not WASM
components: `flick-core/src/plugin/first_party.rs` links them into every
compiler build, the browser's too, as one plugin owned by `std`.

## Rules

- Reach the compiler only through the SDK; SDK changes follow
  [flick-plugin/AGENTS.md](../flick-plugin/AGENTS.md).
- A new block head goes in `flick-syntax/src/syntax/heads.rs` and in each
  hook of `Std` in `first_party.rs` that handles it. It is new syntax.
- Codes show as `std/<code>` and share one namespace across these crates:
  search `plugins/` before taking one. Each needs
  `packages/std/explain/<code>.md` for `flick explain`; `cli`, `derive`,
  `sql` and `test` check theirs in `std_explains_every_code_*` tests,
  `refined` (`D006`) and `ui` by review.
- Codes in `OWN_CODES` of `first_party.rs` stay compiler codes
  (`FLICK-U001`), explained in `flick-diagnostics/src/diagnostics/explain/`.
- Keep dependencies buildable for `wasm32-unknown-unknown`; their code
  counts against the `just wasm` budget.
- Snapshot expansions in `<crate>/tests/` with
  `flick_core::plugin::testing::expand`; checker fixtures and programs
  go where the
  [testing guide](../docs/src/content/docs/contributing/testing.md) says.

## Checks

- `cargo test -p flick-plugin-<name>` for each changed crate, with
  `FLICK_STD_PATH` as the [validation matrix](../docs/agents/validation.md)
  says.
- `cargo test -p flick-core --test check` for `tests/check/` fixtures.
- The "Plugins or their host" row; `just wasm` when code size changes.
