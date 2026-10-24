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

The Astro/Starlight site in `docs/` serves English at `/` and Russian at
`/ru/`. The custom homepages live in `docs/src/pages/`; documentation lives
in `docs/src/content/docs/`, with matching Russian paths under `ru/`.
Keep translated code examples identical and preserve reference URLs.

The main entry points are Learn, Guides, API and Ecosystem. `/play/` runs a
single Flick module in an isolated browser worker; `/transpile/` shows its
real debug or release JavaScript without executing it. Both use the
WebAssembly build of the same compiler core.

Interactive widgets are Flick programs: `docs/src/widgets/` owns search,
theme controls, the live homepage example and code copying;
`docs/src/playground/` shares Play/Transpile through a small entrypoint
(`Playground.fl`), model (`State.fl`), actions (`Actions.fl`), layout
(`View.fl`), and components (`Toolbar.fl`, `Editor.fl`, `Results.fl`).
`Dom.fl` contains browser ports. TypeScript is limited to mounting and
browser/compiler/worker interop. `Ui.Dom` mounts browser roots; widget
event policies remain in Flick. The sandbox supplies the bundled runtime,
not arbitrary npm packages, Node filesystem access or database connections.

The header and editor render their initial Flick views on the server and
hydrate them in place; native text entered before startup is preserved.
The UI mounts before background loading starts. WASM initialization,
checking and builds run in `compiler.worker.ts`; user programs run in a
separate disposable worker. Canceled queued checks are never dispatched;
an already-running pure WASM call finishes without applying stale output.
Syntax highlighting loads independently through Shiki's JavaScript engine
and the existing Flick grammar. Flick renders safe token spans over a
native textarea; selection, caret, scrolling and resize remain native.
The ambient wave runs without a header toggle and honors reduced motion.

`docs/astro.sidebar.mjs` owns section labels and translations;
`docs/src/styles/custom.css` owns the shared visual tokens. Geist is
self-hosted with its license under `docs/public/fonts/`.

After installing workspace dependencies, build and preview the complete
site (including its static Pagefind search index):

```sh
pnpm -C runtime build
just docs
pnpm -C docs preview
```

For live development, run `pnpm -C docs dev` with a built native compiler
(`cargo build -p flick`). Flick compilation is asynchronous, and Vite's
server/client passes reuse builds for the same source state. Restart the
dev server after rebuilding the native compiler.

Use the production preview to check search; its index is generated during
the build. Search uses Pagefind's supported main-thread mode: its 1.5
blob-worker initializer loses the absolute index URL. Revisit this when
upgrading Pagefind; it does not affect program-worker isolation.
`pnpm -C docs test` exercises linking and executing a real compiled program.
The visual contract and approved composition are recorded in `PRODUCT.md`,
`DESIGN.md` and `.impeccable/`.
