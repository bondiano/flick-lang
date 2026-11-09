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

main : Unit ->{} Unit
let main () =
  Platform.stdout_line "Hello from Flick"
```

`->{}` says that `main` runs in the runtime and asks for no services. Host calls
such as `Platform.stdout_line` are not named in signatures yet;
[Why Flick](../why/#where-the-promise-stops) has the details.

## A project

Flick compiles to ES modules that run with the Flick runtime and
Effect 4.1.2. Packages are installed with pnpm. Create a project and run it:

```sh
flick new my-app
cd my-app
flick install
flick run
```

`flick new` writes `flick.toml`, `package.json` with the pinned runtime and
Effect, `src/App/Main.fl`, the module `src/App/Greeting.fl` it imports and
its test `test/App/GreetingTest.fl`:

```flick
module App.Main exposing (main)

import App.Greeting exposing (greet)

main : Unit ->{; AppError} Unit
let main () =
  let name = Platform.args () |> List.head |> Option.with_default "world"
  Platform.stdout_line (greet name)
```

`flick run -- Ada` prints `Hello, Ada!`.

`flick check` type-checks without running anything, `flick build` writes the
JavaScript to `dist/`, `flick test` runs the tests of the test root and
`flick dev` rebuilds and restarts on every change. The [CLI reference](../../reference/cli/)
has every command.

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
