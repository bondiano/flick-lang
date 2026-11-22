# flick-plugin

The Rust SDK for Flick compiler plugins: the `Plugin` trait, the arena
`Builder`, the `flick!` macro (`macros/`) and the `flick:plugin` WIT world.
The bundled plugins in [`plugins/`](../plugins/AGENTS.md) link it natively;
WASM components build it with the `guest` feature.

## Layout

- `wit/` — the component interface; the types in `src/` mirror it.
- `src/wit*.rs` — conversions to and from the bindings; `wit_conversions!`
  serves the guest and the host alike.
- `src/guest.rs` — component glue (`export!`), compiled only with `guest`.
- `macros/` — `flick!`: Flick written in Rust tokens, built through
  `Builder` calls.
- The host is elsewhere: `flick-core/src/plugin/` (expansion, rendering,
  the linked std plugin) and `flick/src/plugins/wasm.rs` (wasmtime).

## Rules

- Change a WIT type together with its Rust mirror and the `src/wit*.rs`
  conversions. A new node kind also needs `src/builder.rs` and the
  compiler's `flick-core/src/plugin/render.rs`.
- Keep `VERSION` in `src/lib.rs` equal to the `package flick:plugin@…`
  line of every `.wit` file (a unit test checks); expansion caches key
  on it.
- Update `backends/js/src/component/templates/plugin/lib.rs` with an API change:
  `flick new --template plugin` (both JS and Go) copies it and no test compiles it.
- Document a breaking change in the
  [plugins reference](../docs/src/content/docs/reference/plugins.md), the
  [upgrade guide](../docs/src/content/docs/handbook/upgrading.md) (en and
  ru) and `CHANGELOG.md`.

## Checks

- `cargo test -p flick-plugin -p flick-plugin-macros`
- `just wasm-plugins`: the only recipe that builds `guest` for
  `wasm32-wasip2` and runs WASM plugins end to end.
- The "Plugins or their host" row of the
  [validation matrix](../docs/agents/validation.md).
