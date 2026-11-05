# Contributing to Flick

Bug reports, small examples and documentation improvements are welcome.
Use [issues](https://github.com/bondiano/flick-lang/issues) to report a
problem or discuss a change. The [testing guide](./docs/src/content/docs/contributing/testing.md)
describes the test suites and their fixture formats, and
[implementation decisions](./docs/src/content/docs/contributing/implementation-decisions.md)
the architecture of the compiler.

## Setup

Install the [pinned Rust toolchain](./rust-toolchain.toml), Node.js
**24.18.0 or newer** and **pnpm 12.16.1**. Go 1.23 or newer is needed only
for the Go target (`--emit go`, `just go-test`), and Lean 4 (the version in
[`formal/lean-toolchain`](./formal/lean-toolchain)) only for the proofs in
[`formal/`](./formal/README.md). Development also uses
[`just`](https://just.systems/), `cargo-machete` and, for `just e2e`,
[`cargo-nextest`](https://nexte.st/) (the Nix dev shell has them all):

```sh
cargo install cargo-machete --version 0.9.2 --locked
cargo install cargo-nextest --locked
pnpm install --frozen-lockfile
just lint   # fmt --check + clippy -D warnings + cargo machete
just test
just check  # lint + test
just e2e    # runtime tests + compiled programs on Node
just bundle-size  # browser bundles against their gzip budgets
```

## Crates

| Crate | Purpose |
| - | - |
| [`flick-core`](./flick-core/) | Shared compiler and language-server core |
| [`flick`](./flick/) | The `flick` command-line tool |
| [`flick-wasm`](./flick-wasm/) | The same compiler core, built for the browser |
| [`flick-model`](./flick-model/) | Reference checker and interpreter of the core calculus in [`docs/formal/core.md`](./docs/formal/core.md) |
| [`flick-pretty`](./flick-pretty/) | The pretty printer of the formatter and the code generators |

## Cargo features

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

Shared in-memory project/configuration types, `Host`-backed source loading,
type checking, JSON primary diagnostics and both debug/release profiles remain
available without defaults. Unsupported TypeScript output requests report an
error instead of silently producing JavaScript. Without `diagnostic-fixes`,
JSON diagnostic events omit `fixes`.

`flick-wasm` disables the core's default features.
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

## Documentation site

The Astro/Starlight site in `docs/` serves English at `/` and Russian at
`/ru/`. The custom homepages live in `docs/src/pages/`; documentation lives
in `docs/src/content/docs/`, with matching Russian paths under `ru/`.
Keep translated code examples identical and preserve reference URLs.

Every ```` ```flick ```` block of those pages must check on its own
(`cargo test -p flick-core --test corpus docs_examples_check`; imports are
taken on trust, not loaded). Declare the names a fragment uses on hidden
lines: a line that is `#` alone or starts with `# ` is compiled without that
prefix and left off the page, as rustdoc does. `flick ignore` only parses a
block, for syntax that does not compile yet; `flick fail` must report an
error, for an example of one. Give both locales the same hidden lines and
markers.

English is the original of each page, except under `spec/`, where the
Russian text is. A translation records in its frontmatter `source_hash`, the
hash of the original it was made from; `node docs/scripts/parity.mjs`
(part of `pnpm -C docs test`) reports a page without its pair and a
translation whose original has changed since. After bringing a translation
up to date, record it with
`node docs/scripts/parity.mjs --stamp docs/src/content/docs/ru/<page>.md`
(a translated chapter of the specification is `docs/src/content/docs/spec/<page>.md`).

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

For live development, run `just docs-dev`: it builds the playground's
compiler and the native one, then starts `pnpm -C docs dev`. Flick compilation is asynchronous, and Vite's
server/client passes reuse builds for the same source state. Restart the
dev server after rebuilding the native compiler.

Use the production preview to check search; its index is generated during
the build. Search uses Pagefind's supported main-thread mode: its 1.5
blob-worker initializer loses the absolute index URL. Revisit this when
upgrading Pagefind; it does not affect program-worker isolation.
`pnpm -C docs test` exercises linking and executing a real compiled program.

## Releases

A tag `vX.Y.Z` runs `.github/workflows/release.yml`: it checks that every
package carries that version, runs CI, builds the `flick` archives for
macOS, Linux and Windows, publishes `@flick-lang/runtime` and
`@flick-lang/astro` to npm and creates the GitHub release. Record the
changes in [`CHANGELOG.md`](./CHANGELOG.md) first.
