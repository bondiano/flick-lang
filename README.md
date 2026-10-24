<p align="center">
  <img src="./docs/public/flick-logo.svg" width="240" height="240" alt="Flick logo: a lowercase f shaped like a toggle switch.">
</p>

<h1 align="center">Flick</h1>

<p align="center">
  <strong>Functional programming. Explicit effects. JavaScript.</strong>
</p>

<p align="center">
  <a href="./docs/src/content/docs/learn/index.md">Learn</a> ·
  <a href="./docs/src/content/docs/start/getting-started.md">Get started</a> ·
  <a href="./examples/first-slice/">Examples</a> ·
  <a href="./editors/README.md">Editor setup</a>
</p>

Flick is an ML-family language that compiles to JavaScript and TypeScript,
with [Effect](https://effect.website/) at its runtime foundation.
Pattern matching, higher-kinded types and type classes meet typed effects,
errors and services, with regions to keep scoped resources from escaping.

## A little Flick

```flick
module Main exposing (main)

type Greeting = Hello String | Goodbye

message : Greeting -> String
let message greeting =
  match greeting with
  | Hello name -> "Hello, " ++ name ++ "!"
  | Goodbye -> "See you soon!"

main : Unit ->{; AppError} Unit
let main () =
  Platform.stdout_line (message (Hello "Flick"))
```

`message` is pure. `main` performs an effect, and its signature makes the
possible `AppError` explicit. Generated programs are ordinary ES modules.

## Try it from source

From a checkout, install the [pinned Rust toolchain](./rust-toolchain.toml),
Node.js **24.18.0 or newer**, and **pnpm 12.14.0**:

```sh
pnpm install --frozen-lockfile
cargo build -p flick
./target/debug/flick --project examples/first-slice run
```

The example combines services, layers and typed errors. To create your own
project once `flick` is on your `PATH`, follow
[Getting started](./docs/src/content/docs/start/getting-started.md).
Use matching compiler and runtime versions.

## Explore

- [Learn Flick](./docs/src/content/docs/learn/index.md) — start with types,
  functions and effects, including a path for TypeScript and Effect users.
- [Guides](./docs/src/content/docs/guides/index.md) — work through real tasks.
- [API & reference](./docs/src/content/docs/api/index.md) — syntax, standard
  library and runtime contracts.
- [Ecosystem](./docs/src/content/docs/ecosystem/index.md) — compiler, browser
  playground, JavaScript interop and editor tooling.
- **Flick 0.7 specification:** [English](./docs/src/content/docs/spec/) ·
  [Русский — оригинал](./docs/src/content/docs/ru/spec/).

The documentation site includes **Play**, which runs Flick in an isolated
browser worker, and **Transpile**, which shows the generated JavaScript.
See the documentation-site development notes below to run them locally.

## Contributing

Bug reports, small examples and documentation improvements are welcome.
Use [issues](https://github.com/bondiano/flick-lang/issues) to report a
problem or discuss a change.

| Crate | Purpose |
| - | - |
| [`flick-core`](./flick-core/) | Shared compiler and language-server core |
| [`flick`](./flick/) | The `flick` command-line tool |
| [`flick-wasm`](./flick-wasm/) | The same compiler core, built for the browser |

Development also uses [`just`](https://just.systems/):

```sh
just lint   # fmt --check + clippy -D warnings
just test
just check  # lint + test
just e2e    # runtime tests + compiled programs on Node
```

`examples/first-slice` is a complete project (services, layers, tests); run `flick run` or `flick test` inside it.

<details>
<summary>Documentation-site development</summary>

### Documentation site

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

</details>

## License

[MIT](./LICENSE).
