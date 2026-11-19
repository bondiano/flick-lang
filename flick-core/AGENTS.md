# flick-core

The compiler and editor core: parsing to emission, projects and IDE
queries. `flick`, `flick-wasm`, `flick-model` and `flick-backend` link it.

## Layout

- `crate::source` and `crate::diagnostics` re-export `flick-diagnostics`
  (the `codes!` registry and `explain/<CODE>.md`), `crate::syntax`
  re-exports `flick-syntax` and `crate::ffi` re-exports `flick-ffi`. Their
  tests that run the checker, the formatter or the lowering are in
  `src/frontend_tests/`.
- `src/codegen/` — re-exports the emitters of `backends/js` and
  `backends/go`; owns the TypeScript projection (`ts.rs`, `ts/`).
- `src/ide/` — queries shared by `flick lsp` and the playground.
- `src/plugin/` — plugin block expansion; the plugins are in `plugins/`.

## Rules

- Give a new top-level module a layer in `src/layer_tests.rs` and import
  only from its layer or lower ones. Outside `ts`, `src/codegen/` reads
  only the IR and build input: no `types`, `iface`, `opt`, `project`,
  `build`, `syntax` or `std::fs`.
- `build.rs` compiles `builtin_source.rs`, `implicit_packages.rs`,
  `source.rs` and `diagnostics/{codes,model}.rs` of `flick-diagnostics`
  and `syntax/{kind.rs,lexer/}` of `flick-syntax` on their own: keep
  their `crate::` imports within that set.
- A new code needs a `codes!` entry, `explain/<CODE>.md`, its Russian
  `docs/src/error-translations/<CODE>.md`, then
  `node docs/scripts/errors.mjs --write`. Never reuse a number; mark a
  retired code in its explanation.
- Gate disk, Go and source-map code by `native-project` and editor code
  by `ide` ([features](../CONTRIBUTING.md#cargo-features)); without
  `native-project`, read files through `project::Host`.
- Add an integration suite as a file in `tests/compiler/` plus a
  `pub mod` line in its `main.rs`.

## Checks

Pick rows from the [validation matrix](../docs/agents/validation.md),
with `FLICK_STD_PATH="$PWD/packages/std"` exported.

- `cargo test -p flick-core --lib <module>::`; `layer_tests` for imports.
- After a code change: `pnpm -C docs test` and
  `cargo test -p flick --test cli every_code_has_an_explanation`.
- `just feature-check` and `just feature-test` after a `cfg(feature)`;
  `just wasm` when code the browser build compiles grows.
- Print the syntax tree of a file, paths absolute:
  `DUMP=/tmp/t.txt DUMP_FILE=/abs/x.fl cargo test -p flick-core --test corpus -- --ignored --exact dump`
