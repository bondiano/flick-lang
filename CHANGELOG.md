# Changelog

All notable changes to Flick are recorded here. The project follows
[Semantic Versioning](https://semver.org/); before 1.0 a minor release may
break programs.

## Unreleased

- **Plugins:** `import plugin "pkg" exposing (head)` makes `head { … }` a
  block a plugin of a dependency expands: the user's code inside keeps its
  place for errors and editors, the plugin's code is checked in place of
  the block, and what plugins give is cached per block. Plugins are
  WebAssembly components of `flick:plugin` (`flick-plugin` crate, wasmtime
  with fuel and memory limits in release builds) or Rust values linked in.
  See [plugins](https://bondiano.github.io/flick-lang/reference/plugins/).
- **Builds:** the built-in modules are lowered in one pass (a small build
  takes about a quarter of the time); the checked modules of libraries are
  saved between runs.

## 0.1.0 — 2026-11-03

The first public release.

- **Language:** ML-family syntax with indentation-based bodies, ADTs and
  pattern matching, higher-kinded types and type classes with deriving,
  typed effect rows of services and errors, regions that keep scoped
  resources from escaping, `do`/`for` sugar, `cli`, `sql`, `schema` and
  `ui { … }` embedded languages, and `server fn` calls shared by a server and
  a browser page.
- **Targets:** ES modules over [Effect](https://effect.website/) 4.1.2 with
  source maps, TypeScript declarations and Promise façades, and a
  standalone Go executable (`emit = "go"`).
- **Toolchain:** the `flick` CLI (`new`, `check`, `build`, `run`, `dev`,
  `test`, `fmt`, `install`, `pack`, `diff`, `ffi`, `doc`, `doctor`, …),
  workspaces with path dependencies and `[scripts]`, `.iface` interfaces
  (`iface-version` 3) for packages, and a language server.
- **Runtime:** `@flick-lang/runtime` on npm for the JavaScript target and
  `@flick-lang/astro` for Astro and Vite.
- **Editors:** VS Code and Zed extensions and a tree-sitter grammar in
  [`editors/`](./editors/README.md).
- **Documentation:** the guide, reference and specification at
  [bondiano.github.io/flick-lang](https://bondiano.github.io/flick-lang/) with an in-browser playground,
  and the formal semantics with Lean proofs in [`formal/`](./formal/README.md).
