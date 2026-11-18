# packages

Flick source packages: `core`, compiled into the compiler, and the implicit
packages every program sees (`flick-core/src/implicit_packages.rs`). This
file covers the `.fl` packages; [effect-ts](effect-ts/AGENTS.md) is the
TypeScript adapter. Ownership and `FLICK_STD_PATH` are in
[CONTRIBUTING](../CONTRIBUTING.md#standard-library-source-packages).

## Rules

- Core modules are listed in `CORE` of
  `flick-core/src/types/builtin_sources.rs` and embedded at build time.
- List a new module in its package's `src/modules.txt` after the modules
  it imports; `import` every non-core module a module uses.
- A `foreign … from "flick:M"` needs its JS export under `effect-ts/src/`
  and a line in `src/go/*.txt` naming a `runtime-go/flickrt` function, or
  a reasoned entry in `flick-core/src/codegen/go/js_only.txt`.
- Mark a built-in the compiler or a backend must recognize with
  `@lang <role>` (`flick-ir/src/lang.rs`); target forms belong in backends.
- Keep signatures the spec states word for word as in
  `docs/src/content/docs/ru/spec`.
- A std export that wraps an Effect export is named in
  `effect-ts/coverage/`; rename it there too.
- Doc comments become the API pages and their examples run; describe each
  module as Flick's own.
- The coverage table of the
  [testing guide](../docs/src/content/docs/contributing/testing.md) says
  where a behavior's tests go.

## Checks

With `FLICK_STD_PATH` set as in the
[validation matrix](../docs/agents/validation.md):

- `cargo test -p flick-core --lib types::builtins::`: imports and no
  warnings; `write_fixes_of_built_in_signatures -- --ignored` writes fixes.
- `cargo test -p flick-core --lib go_bindings`
- `cargo test -p flick-core --test stdlib_signatures`
- `cargo test -p flick-core --test corpus fl_sources_are_formatted`
- `just language`; `cargo build -p flick && node docs/scripts/api.mjs`.
- The matrix rows the change affects.
