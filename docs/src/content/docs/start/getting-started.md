---
title: Getting started
description: Install the toolchain, create an application and run it.
---

A local application needs the Flick compiler, Node.js 24.18 or later and
pnpm. Download a `flick` archive for your platform from the
[releases](https://github.com/bondiano/flick-lang/releases), or build it from
a source checkout as described in [Ecosystem](../../ecosystem/).

Flick compiles to ES modules that run with the Flick runtime and
Effect 4.1.2. Packages are installed with pnpm. Create a project and run it:

```sh
flick new my-app
cd my-app
flick install
flick run
```

`flick new` writes `flick.toml`, `package.json` with the pinned runtime and
Effect, and `src/App/Main.fl`:

```flick
module App.Main exposing (main)

main : Unit ->{; AppError} Unit
let main () =
  Platform.stdout_line "Hello from Flick"
```

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
