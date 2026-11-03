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
  <a href="https://bondiano.github.io/flick-lang/">Documentation</a> ·
  <a href="./docs/src/content/docs/learn/index.md">Learn</a> ·
  <a href="./docs/src/content/docs/start/getting-started.md">Get started</a> ·
  <a href="./examples/">Examples</a> ·
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

## Install

Download the `flick` archive for your platform from the
[releases](https://github.com/bondiano/flick-lang/releases) and put the
binary on your `PATH`. A project also needs Node.js 24.18 or newer and
pnpm:

```sh
flick new my-app
cd my-app
flick install
flick run -- Ada
```

[Getting started](./docs/src/content/docs/start/getting-started.md) walks
through the generated project. Editor support is in
[`editors/`](./editors/README.md).

## Build from source

From a checkout, with the [pinned Rust toolchain](./rust-toolchain.toml),
Node.js **24.18.0 or newer** and **pnpm 12.16.1**:

```sh
pnpm install --frozen-lockfile
cargo build -p flick
./target/debug/flick --project examples/first-slice run
```

Go 1.23 or newer is needed only for the Go target (`--emit go`).
[CONTRIBUTING.md](./CONTRIBUTING.md) lists the rest of the development setup.

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

## Examples

- [`examples/first-slice`](./examples/first-slice/) — services, layers,
  typed errors and tests; `flick run` or `flick test` inside it.
- [`examples/mtl`](./examples/mtl/README.md) — a task-time planner over
  hand-written monad transformers (`MaybeT`, `StateT`, `ReaderT`).
- [`examples/cli-tool`](./examples/cli-tool/README.md) — a notes CLI from
  one `cli` block: subcommands, a wizard and shell completions.
- [`examples/observability`](./examples/observability/README.md) — spans,
  logs and metrics sent to an OpenTelemetry collector.
- [`examples/conduit`](./examples/conduit/README.md) — RealWorld's Conduit
  end to end: a Go server over PostgreSQL and a browser client sharing one
  typed API contract.

## Contributing

Bug reports, small examples and documentation improvements are welcome; see
[CONTRIBUTING.md](./CONTRIBUTING.md). Changes are listed in
[CHANGELOG.md](./CHANGELOG.md).

## License

[MIT](./LICENSE).
