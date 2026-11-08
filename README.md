<h1 align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="./docs/public/flick-readme-dark.svg">
    <source media="(prefers-color-scheme: light)" srcset="./docs/public/flick-readme.svg">
    <img src="./docs/public/flick-readme.svg" width="800" height="240" alt="Flick: functions that do what they say.">
  </picture>
</h1>

<p align="center">
  <a href="https://bondiano.github.io/flick-lang/">Documentation</a> ·
  <a href="./docs/src/content/docs/start/why.md">Why Flick</a> ·
  <a href="./docs/src/content/docs/start/getting-started.md">Get started</a> ·
  <a href="./docs/src/content/docs/reference/types.md">Reference</a> ·
  <a href="./examples/">Examples</a> ·
  <a href="./editors/README.md">Editor setup</a>
</p>

Flick is a language for whole apps: the browser, the server and the command
line. A function's signature says what it needs and how it can fail, the
compiler holds the body to it, and the code still reads like plain code. It
compiles to JavaScript and TypeScript, where effects run on
[Effect](https://effect.website/) for now, and to Go for a standalone
executable. It is alpha: host calls such as `Console.log`, the clock and
randomness are not named in signatures yet.
[Why Flick](./docs/src/content/docs/start/why.md) says what it is for, and
who it is not for yet.

## A little Flick

```flick
module Main exposing (main)

type Greeting = Hello String | Goodbye

message : Greeting -> String
let message greeting =
  match greeting with
  | Hello name -> "Hello, " ++ name ++ "!"
  | Goodbye -> "See you soon!"

main : Unit ->{} Unit
let main () =
  Platform.stdout_line (message (Hello "Flick"))
```

`message` is pure: its arrow is a plain `->`. `main` runs in the runtime, so its
arrow has braces, which would name the services it needs and how it can fail.
Generated programs are ordinary ES modules.

## Install

On macOS or Linux:

```sh
curl -fsSL https://bondiano.github.io/flick-lang/install.sh | sh
# or
brew install bondiano/tap/flick
```

On Windows, download the `flick` archive from the
[releases](https://github.com/bondiano/flick-lang/releases) and put
`flick.exe` on your `PATH`. A project also needs Node.js 24.18 or newer and
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

- [Reference](./docs/src/content/docs/reference/types.md) — the language, the
  standard library, the runtime and the tools.
- [Specification](./docs/src/content/docs/spec/index.md) — the normative
  description of the language; the formal semantics and its
  Lean proofs are in [`docs/formal/`](./docs/formal/) and
  [`formal/`](./formal/README.md).

The documentation site includes a **playground**, which runs Flick in an
isolated browser worker and shows the JavaScript the compiler writes.

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
