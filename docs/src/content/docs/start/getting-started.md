---
title: Getting started
description: Install the toolchain, create an application and run it.
---

A local application needs the Flick compiler, Node.js 24.18 or later and
pnpm. Download a `flick` archive for your platform from the
[releases](https://github.com/bondiano/flick-lang/releases), or build it from
a source checkout as described in [Ecosystem](../../ecosystem/).
Each archive holds the single `flick` binary; put it on your `PATH`:

```sh
tar -xzf flick-x86_64-unknown-linux-gnu.tar.gz
install -m 755 flick ~/.local/bin/
flick --version
```

On Windows, extract the `.zip` and add the folder with `flick.exe` to `PATH`.

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
plugin builds each imported `.fl` with `flick build`.

```js
// astro.config.mjs
import flick from "@flick-lang/astro";

export default defineConfig({ integrations: [flick()] });
```

```astro
<div id="app"></div>
<script>
  import { runMain } from "@flick-lang/runtime";
  import { main } from "../app/Main.fl";

  runMain(main);
</script>
```

The [Play](../../play/) surface of this site is such a program: a Flick
module with a `ui` view, compiling your code with Flick's compiler built to
WebAssembly.

[Transpile](../../transpile/) shows the generated JavaScript beside the
editable source, with separate debug and release output.
