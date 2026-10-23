# flick-lang

Rust implementation of **Flick** — an ML-family language with HKT, type
classes and a single Effect backend (JavaScript/TypeScript output).

The language specification (Flick 0.7) is part of the documentation site:
[`docs/src/content/docs/spec/`](./docs/src/content/docs/spec/) in English and
[`docs/src/content/docs/ru/spec/`](./docs/src/content/docs/ru/spec/) in Russian,
the original text.

## Crates

| Crate | Purpose |
| - | - |
| `flick-core` | Shared Rust core for the compiler and the LSP |
| `flick` | The `flick` CLI |
| `flick-wasm` | wasm-bindgen interface for the in-browser demo (compile preview, REPL) |

## Development

Requires Rust 1.94 (pinned in `rust-toolchain.toml`) and `just`; the e2e suite
also needs Node.js >= 24.18.0 and pnpm (pinned via `packageManager`).

```sh
just lint   # fmt --check + clippy -D warnings
just test
just check  # lint + test
just e2e    # runtime tests + compiled programs on Node
```

`examples/first-slice` is a complete project (services, layers, tests); run `flick run` or `flick test` inside it.

## Documentation

The Starlight site in `docs/` serves English at `/` and Russian at `/ru/`.
English pages live in `docs/src/content/docs/`; Russian translations mirror
their paths under `docs/src/content/docs/ru/`. Keep matching filenames so
the language selector opens the same page, and keep code examples unchanged.
Sidebar group labels are translated in `docs/astro.sidebar.mjs`; additional
Russian search UI strings live in `docs/src/content/i18n/ru.json`.

Build with `pnpm --filter @flick-lang/docs build`, then run
`pnpm --filter @flick-lang/docs preview` to view the static site and search index.
