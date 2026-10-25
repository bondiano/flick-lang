---
title: Ecosystem
description: The compiler, Effect-based runtime, browser compiler, web integration and editor tools available in the Flick repository.
---

Flick is an ML-family language that compiles to JavaScript on Effect. It is not a replacement JavaScript runtime or a replacement for Effect: the compiler checks Flick programs, generated ES modules use the Flick runtime adapter, and that adapter builds on Effect.

You can explore the language in [Play](/play/) and inspect generated JavaScript in [Transpile](/transpile/). For exact signatures and contracts, use the [API & reference index](../api/).

## What is available, and where?

The [source repository](https://github.com/bondiano/flick-lang) contains the tools below. A package name in a manifest does **not** establish that a public npm release exists. In particular, the Astro integration and declaration importer are private workspace packages; the examples on this page do not ask you to install them from the public registry.

| Component | Location and distribution boundary |
| --- | --- |
| Native compiler and CLI | Rust crate `flick`, sharing `flick-core` with the browser compiler. Build it from the checkout; the repository also has a tagged-release workflow for native archives. |
| Browser compiler | Rust crate `flick-wasm`, built to WebAssembly with wasm-bindgen. Used by this site's browser tools, not presented here as a public npm compiler package. |
| Runtime adapter | Workspace package `@flick-lang/runtime` in `runtime/`, with JavaScript and TypeScript declarations. Its manifest pins Effect `4.0.5`; use the matching compiler/runtime versions. |
| Astro/Vite integration | Private workspace package `@flick-lang/astro` in `tools/astro-flick/`. This site depends on it with `workspace:*`. |
| TypeScript declaration importer | Private workspace package `@flick-lang/dts-importer` in `tools/dts-importer/`. Requires its local build and an installed package whose declarations can be read. |
| Editor support | Sources under `editors/` for VS Code, Zed and tree-sitter; the language server is part of the native `flick` binary. |

Use the [GitHub releases page](https://github.com/bondiano/flick-lang/releases) to check which native archives are actually available. The release workflow targets macOS, Linux and Windows on x86-64 and ARM64; a configured target is not a guarantee that an archive exists for every version.

## Native compiler and CLI

The CLI provides project creation, type checking, compilation, execution, testing, formatting and dependency commands. Builds emit ES modules; TypeScript output and `.d.ts` emission are also supported. Dependency management delegates to pnpm rather than introducing a separate package registry.

From a source checkout with the repository's Rust toolchain installed, build the native executable:

```sh
cargo build -p flick
./target/debug/flick --version
```

Once the desired binary is on your `PATH`, these commands inspect your environment and work on a Flick project:

```sh
flick doctor
flick check
flick build
```

`check` and `build` do not install dependencies or execute your application. `doctor` reports the compiler, Node.js, pnpm, runtime and Effect versions, which is useful before diagnosing an integration problem. See [Getting started](../start/getting-started/), [Compiler CLI](../reference/cli/) and [Project configuration](../reference/project/) for the project workflow and requirements.

## WebAssembly browser compiler

`flick-wasm` exposes checking, compilation preview and a build result containing JavaScript modules and diagnostics. It uses the shared compiler core, with built-in library sources available to the compiler. Its browser host represents one source module, not a local project's file system or installed npm dependency tree.

[Play](/play/) is the place to try supported browser examples; [Transpile](/transpile/) is the place to inspect generated JavaScript. Compilation and execution are different steps: compiling to JavaScript does not make Node.js filesystem APIs, a PostgreSQL connection or arbitrary foreign packages available in a browser. The repository's `just docs-wasm` task builds the compiler and its browser bindings for the documentation site.

## Runtime and the Effect foundation

`@flick-lang/runtime` is the adapter used by generated modules. It fixes Flick's contracts for scope owners, child fibers, finalizers, failure causes and entry execution on top of the pinned Effect version. Optional React and PostgreSQL adapters have optional peer dependencies; importing the core runtime is not a claim that every host has those facilities.

Effect already provides a substantial TypeScript foundation: typed errors, dependency injection, structured concurrency, resource safety, scheduling and observability. Read the [official Effect documentation](https://effect.website/docs/) for those concepts and the underlying library. Flick adds a different source language and compile-time rules; it does not replace Effect's scheduler or claim that TypeScript applications need to migrate.

The [runtime reference](../reference/runtime/) describes Flick-specific execution guarantees. The [standard library](../reference/stdlib/) and [Effect coverage](../reference/effect-surface/) explain the exposed surface and its explicit exclusions. Do not treat all upstream Effect APIs, or APIs from a different Effect version, as automatically available Flick built-ins.

## Astro and Vite integration

The local `@flick-lang/astro` package allows an Astro page or client script to import an exported value from a `.fl` file. Its Astro integration registers a Vite plugin that invokes the native `flick build` command and re-exports the compiled module. It is a build-time integration, not the WebAssembly browser compiler.

In a workspace that already provides this local package, configure Astro as follows:

```js
import flick from "@flick-lang/astro";
import { defineConfig } from "astro/config";

export default defineConfig({ integrations: [flick()] });
```

The same package exports `flickVite` from `@flick-lang/astro/vite` for a Vite configuration. The plugin resolves its compiler from the `binary` option, then `FLICK_BIN`, then `flick` on `PATH`; its other options are `profile` and `outDir`. It watches Flick sources under the imported file's directory and compiles with the selected native binary. Vite resolves the generated runtime and foreign JavaScript imports, so those dependencies must be available to the host project.

This documentation site uses the workspace integration itself. See the [integration source](https://github.com/bondiano/flick-lang/tree/main/tools/astro-flick) before adopting it outside the checkout: it is private workspace tooling, not a documented public npm install.

## Editors and syntax tools

The native language server runs with `flick lsp --stdio`. It provides diagnostics with quick fixes, source-type hover, definition, type definition and references, completion, signature help, refactors, document and workspace symbols, semantic highlighting, folding, inlay type hints, formatting and rename for supported project symbols. Built-in library definitions open as real read-only `.fl` files; browsing them does not make them editable project sources.

- **VS Code:** `editors/vscode/` contains the TextMate grammar and language-server client. The repository's setup uses a local extension; `flick.path` selects the executable.
- **Zed:** `editors/zed/` contains the extension and tree-sitter integration. Its launcher tries an explicit `lsp.flick.binary.path`, then the worktree's `PATH`, then a release matching the extension version, with an offline cache. A local path avoids depending on a matching downloadable release.
- **tree-sitter:** `editors/tree-sitter-flick/` is a token-level grammar with nested brackets, not the compiler's complete semantic analysis. Precise semantic information comes from the language server.

Follow the [editor reference](../reference/editor/) for capabilities and local Zed configuration, and the [editor setup README](https://github.com/bondiano/flick-lang/blob/main/editors/README.md) for checkout-based installation. These source integrations are not a claim of marketplace availability or feature parity with other language ecosystems.

## JavaScript, TypeScript and declaration importing

There are two directions to the interop boundary:

1. **Call JavaScript from Flick.** A `foreign` declaration supplies the Flick signature and execution mode: `pure`, `sync`, `async` or `effect`. Metadata describes argument arity, nullable values, typed error conversion, cancellation and callback retention. Those are contracts to review, not guarantees inferred from JavaScript.
2. **Consume Flick from JavaScript or TypeScript.** The compiler emits ES modules, can emit TypeScript, and can write `.d.ts` files for representable public types. Runtime functions expose Effect values; they do not become Promises just because they are exported. Optional Promise facades have their own eligibility and failure contracts.

For an installed JavaScript package, `flick ffi import <package> --out <dir>` reads its `.d.ts` through the Node helper `@flick-lang/dts-importer`. The helper is local workspace tooling: the compiler looks for it in the project's `node_modules`, or uses the script named by `FLICK_DTS_IMPORTER`. The importer supports a defined TypeScript subset and reports exports as supported, needing policy or unsupported. Generated blocked bindings cannot silently be called.

Declarations alone cannot establish purity, error conversion or how long a library retains a callback. Supply and review the relevant policy, then use `flick ffi check` to compare reviewed bindings with the installed declarations. Importing declarations does not translate an arbitrary TypeScript implementation into Flick or prove that every npm API is compatible.

Continue with [Foreign functions](../reference/ffi/) for modes, policies and limitations, and [Code generation](../reference/codegen/) for the emitted module shape, declarations and Promise facades.
