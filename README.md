<h1 align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="./docs/public/flick-wordmark-dark.svg">
    <source media="(prefers-color-scheme: light)" srcset="./docs/public/flick-wordmark.svg">
    <img src="./docs/public/flick-wordmark.svg" width="600" height="240" alt="Flick — a sharp violet woodpecker shaped like a lowercase f.">
  </picture>
</h1>

<p align="center">
  <strong>Functional programming. Explicit effects. JavaScript.</strong>
</p>

<p align="center">
  <a href="https://flick-lang.dev">Documentation</a> ·
  <a href="./docs/src/content/docs/learn/index.md">Learn</a> ·
  <a href="./docs/src/content/docs/start/getting-started.md">Get started</a> ·
  <a href="./examples/first-slice/">Examples</a> ·
  <a href="./editors/README.md">Editor setup</a>
</p>

Flick is an ML-family language that compiles to JavaScript and TypeScript,
with [Effect](https://effect.website/) at its runtime foundation, and to Go
for a standalone executable.
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
Node.js **24.18.0 or newer**, and **pnpm 12.16.1**. Go 1.23 or newer is
needed only for the Go target (`--emit go`, `just go-test`), and Lean 4
(the version in [`formal/lean-toolchain`](./formal/lean-toolchain)) only for
the proofs in [`formal/`](./formal/README.md):

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
- [Specification](./docs/src/content/docs/spec/index.md) — the normative
  description of the language; the formal semantics and its
  Lean proofs are in [`docs/formal/`](./docs/formal/) and
  [`formal/`](./formal/README.md).

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

`flick-core` enables all compiler capabilities by default; the native `flick`
crate keeps that complete configuration. Compiler-only consumers can select
capabilities with `--no-default-features --features <names>`.

| Cargo feature | Capability |
| - | - |
| `ide` | Editor queries, projections and retained editor metadata; enables `typescript` and `diagnostic-fixes`. |
| `build-artifacts` | Published `.iface`, `package.json` and publication fingerprints. |
| `source-maps` | Source markers, mapping data, `.map` files and `sourceMappingURL`. |
| `typescript` | TypeScript output, `.d.ts` and Promise façades; JavaScript exports and Sync wrappers remain available without it. |
| `diagnostic-fixes` | Lazy construction and JSON serialization of fixes/edits; primary diagnostics and validation remain available without it. |
| `native-project` | Project discovery, TOML/FFI-policy parsing, installed packages, disk-cache contracts and pnpm adapters; activates `toml`. |
| `terminal-diagnostics` | Human-readable diagnostic rendering; activates `annotate-snippets`. |
| `builtin-docs` | Documentation comments in embedded builtin sources. Without it, build-time lexing removes those comments, not user-program documentation or string contents. |
| `server-stdlib` | The optional server/runner/CLI/exporter builtin group. |

Shared in-memory project/configuration types, `Host`-backed source loading,
type checking, JSON primary diagnostics and both debug/release profiles remain
available without defaults. Unsupported TypeScript output requests report an
error instead of silently producing JavaScript. Without `diagnostic-fixes`,
JSON diagnostic events omit `fixes`.

`flick-wasm` disables the core's default features and forwards only its default
`server-stdlib` feature, preserving existing server-code Transpile support.
Use `just wasm` or `just docs-wasm` for this smaller browser compiler.
Build the package separately: Cargo unifies dependency features in workspace
builds, where the native CLI enables the complete core.

For a reduced builtin surface, build `flick-wasm` with `--no-default-features`.
This removes `File`, `Runner.Node`, `Ui.Server`, `Runner.Cloudflare`,
`Http.ServerError`, `Http.ServerRequest`, `Http.ServerResponse`, `Http.Multipart`,
`Http.Platform`, `Http.Middleware`, `Http.Router`, `Http.Server`, `Http.StaticServer`,
`HttpApi`, `Rpc`, `Server`, `Config`, `Otlp`, `Prometheus`, `DevTools`, `Cli.Prompt`
and `Cli`. Whole-module removal also removes their mixed client APIs, including
`Rpc.call`, `HttpApi.call` and nonsecret `Config` operations. `Platform`, SQL,
Effect/UI and HTTP-client modules remain; unavailable `cli` and `server fn`
syntax reports a diagnostic rather than reaching lowering.

`just feature-check` checks isolated feature combinations with strict Clippy;
`just feature-test` exercises the compiler's capability boundaries.
Both are included in the existing lint/test recipes.

Development also uses [`just`](https://just.systems/) and `cargo-machete`:

```sh
cargo install cargo-machete --version 0.9.2 --locked
just lint   # fmt --check + clippy -D warnings + cargo machete
just test
just check  # lint + test
just e2e    # runtime tests + compiled programs on Node
just bundle-size  # browser bundles against their gzip budgets
```

`examples/first-slice` is a complete project (services, layers, tests); run `flick run` or `flick test` inside it.

[`examples/mtl`](./examples/mtl/README.md) walks through writing a task-time planner using custom
[`MaybeT`](./examples/mtl/src/Mtl/MaybeT.fl) (short-circuit on `None`),
[`StateT`](./examples/mtl/src/Mtl/StateT.fl) (remaining time and completed tasks),
[`ReaderT`](./examples/mtl/src/Mtl/ReaderT.fl) (shared configuration), and
[`Identity`](./examples/mtl/src/Mtl/Identity.fl) (pure base).
The [domain plan](./examples/mtl/src/App/Plan.fl) stacks
`MaybeT (StateT PlanState (ReaderT Config Identity))`: stopping preserves the
completed prefix and remaining time, rather than rolling state back.
It has no `Eff` base; the [Platform/Cli shell](./examples/mtl/src/App/Main.fl)
handles I/O, and the Cli runtime is still Effect-backed, not dependency-free.
The [example guide](./examples/mtl/README.md) explains the wrappers, instances,
explicit lifts and run order, with a runnable CLI walkthrough. Compare
[`mtl`](https://github.com/haskell/mtl) and the
[Haskell transformer tutorial](https://mmhaskell.com/monads/transformers).

```sh
flick run --project examples/mtl -- catalog
flick run --project examples/mtl -- plan --budget 60 docs tests lint
flick run --project examples/mtl -- plan --budget 36 --overhead 3 docs lint
flick run --project examples/mtl -- plan --budget 25 docs lint
flick test --project examples/mtl
```

The 60-minute plan completes all three tasks with zero minutes left. A
36-minute plan also fits `docs lint` when each task adds three minutes of overhead.
The 25-minute plan retains `docs` and five minutes, then exits nonzero because
`lint` does not fit.

[`examples/cli-tool`](./examples/cli-tool/README.md) is a notes CLI from one
`cli` block: subcommands, the notes file from `NOTES_FILE`, a wizard that asks
for the first note, and `--completions`.

```sh
flick run --project examples/cli-tool -- add buy milk
flick run --project examples/cli-tool -- list
```

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
The visual contract of the site is recorded in `PRODUCT.md`, `DESIGN.md`
and `.impeccable/`.

</details>

## License

[MIT](./LICENSE).
