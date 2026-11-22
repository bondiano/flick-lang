---
title: Getting started
description: Install the toolchain, create an application and run it.
---

## Install

A local application needs the Flick compiler, Node.js 24.18 or later and
pnpm. On macOS or Linux, install the latest release into `~/.local/bin`:

```sh
curl -fsSL https://bondiano.github.io/flick-lang/install.sh | sh
flick --version
```

or with Homebrew: `brew install bondiano/tap/flick`. You can also download a
`flick` archive for your platform from the
[releases](https://github.com/bondiano/flick-lang/releases), or
[build it from source](#from-source).

On Windows, extract the `.zip` and add the folder with `flick.exe` to `PATH`.

`flick` also needs `git` on `PATH` and, the first time a project installs or
checks, network access to github.com: it fetches the implicit `std` package
from the `std-v<version>` tag of its release, and any `git` dependency, with
`git`. A checkout of the repository, or `FLICK_STD_PATH` naming a local
`packages/std`, removes that fetch; path dependencies need no network.

A project pins pnpm in `package.json`, and `flick install` changes nothing
when `pnpm --version` is another version. Activate the pinned one with
`corepack prepare pnpm@12.16.1 --activate` (`flick new` writes that version;
use the one in your `package.json` when it differs), or install it with
`npm install --global pnpm@12.16.1`. `flick doctor` prints the versions it
finds beside the ones the project needs.

### Continuous integration

The repository is also a GitHub Action: it downloads a release archive, checks
its `.sha256` and puts `flick` on `PATH` for the rest of the job.

```yaml
- uses: actions/checkout@v7
- uses: bondiano/flick-lang@main # pin the tag of a release that has the action
  with:
    version: latest # or v0.1.0
- run: flick --version
```

### From source

The checkout's `rust-toolchain.toml` picks the Rust version:

```sh
git clone https://github.com/bondiano/flick-lang
cd flick-lang
cargo build --release -p flick
./target/release/flick --version
```

A compiler built this way takes the `std` of its checkout, so keep the
checkout where you built it, or set `FLICK_STD_PATH` to its `packages/std`.

## A first program

The smallest program prints a line:

```flick
module Main exposing (main)

main : Unit ->{Host.Console} Unit
let main () =
  Platform.stdout_line "Hello from Flick"
```

`->{Host.Console}` says that `main` runs in the runtime and needs the console
that `Platform.stdout_line` writes to; `flick run` provides it. A `foreign`
binding is still taken on trust:
[Why Flick](../why/#where-the-promise-stops) has the details.

## A project

Flick compiles to ES modules that run on Flick's own runtime,
`@flick-lang/runtime`. Packages are installed with pnpm. Create a project and
run it:

```sh
flick new my-app
cd my-app
flick install
flick run
```

`flick new` writes `flick.toml`, `package.json` with the pinned runtime,
`src/App/Main.fl`, the module `src/App/Greeting.fl` it imports and its test
`test/App/GreetingTest.fl`:

```flick
module App.Main exposing (main)

import App.Greeting exposing (greet)

run : Unit ->{Host.Arguments, Host.Console} Unit
let run () =
  let name = Host.Arguments.args () |> List.head |> Option.with_default "world"
  Host.Console.stdout_line (greet name)

main : Unit ->{; AppError} Unit
let main () = provide (Layer.merge Host.arguments Host.console) { run () }
```

`run` names the `Host` services it uses, and `main` provides their live
layers, so a test can call `run` with its own. `flick run -- Ada` prints
`Hello, Ada!`.

The first `flick new` or build downloads the selected official backend, a
release asset the compiler does not hold, and pins it in `flick.lock`; later
builds work offline.

Templates come from the selected target backend or a third-party package:

```sh
# Pick a template from the default JavaScript backend (app, cli, fullstack, lib, plugin)
flick new my-cli --template cli

# Create a Go project (app, cli, lib, plugin; no package.json, requires Go on PATH)
flick new my-go-app --target go --template app

# Scaffold from a third-party template package (path, tarball URL + --sha256, or git + --tag)
flick new my-starter --from ./my-template-pkg --template starter
flick new my-starter --from https://example.com/starter.tgz --sha256 <64-hex> --template starter

# Add a project to an existing non-empty directory while keeping existing files
flick init --target go --template lib
```

Passing an unsupported `--template` reports `FLICK-P060` with the list of
templates that contributor provides. `--from` uses the same source verification
as `flick add` and records the pin in `flick.lock`.

`flick check` type-checks without running anything, `flick build` writes the
JavaScript to `dist/`, `flick test` runs the tests of the test root and
`flick dev` rebuilds and restarts on every change. The [CLI reference](../../reference/cli/)
has every command.

The project's `AGENTS.md` is for coding agents: it shows where to look up
Flick's syntax and APIs and asks for `flick fmt`,
`flick check --warnings-as-errors` and `flick test` before finishing.

Next, [design a program with effects](../../handbook/designing-with-effects/):
separate user lookup logic from external calls and provide different services
in the application and its tests.

## In a web page

Astro and Vite import Flick modules directly with `@flick-lang/astro`: the
plugin builds each imported `.fl` with `flick build`. The package is published
on npm, versioned with the compiler, and its sources are in the Flick
repository ([source](https://github.com/bondiano/flick-lang/tree/main/tools/astro-flick)).
Place the `.fl` file inside a Flick project; the new project's default RT
configuration matches the runtime import below.

```js
// astro.config.mjs
import flick from "@flick-lang/astro";

export default defineConfig({ integrations: [flick()] });
```

```astro
<div id="app"></div>
<script>
  import { runMain } from "@flick-lang/runtime/rt";
  import { main } from "../app/Main.fl";

  runMain(main);
</script>
```

The [playground](../../play/) of this site is such a program: a Flick module
with a `ui` view, compiling your code with Flick's compiler built to
WebAssembly. Its JavaScript tab shows what the compiler writes, for the debug
or the release profile.
