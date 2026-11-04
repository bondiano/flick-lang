# Changelog

All notable changes to Flick are recorded here. The project follows
[Semantic Versioning](https://semver.org/); before 1.0 a minor release may
break programs.

## Unreleased

- **Plugins:** `import Sql exposing (plugin sql)` makes `sql { … }` a block
  the plugin of `Sql`'s package expands: the user's code inside keeps its
  place for errors and editors, the plugin's code is checked in place of
  the block, and what plugins give is cached per block. Plugins are
  WebAssembly components of `flick:plugin` (`flick-plugin` crate, wasmtime
  with fuel and memory limits in release builds) or Rust values linked in.
  See [plugins](https://bondiano.github.io/flick-lang/reference/plugins/).
  A head takes arguments, strings and `( … )`, before its body
  (`h "x" (e) { … }`); a plugin reports under codes of its own, shown under
  its package (`std/L001`, explained by `flick explain std/L001`), and
  answers editors' completions, hovers and highlights in its blocks. The
  interface is `flick:plugin@0.2.0`. Writing one: `flick!` builds a
  plugin's code written in Flick (`flick!(b, Cli.default (#d : #ty))`), a
  `Cursor` reads a block, `flick_core::plugin::testing::expand` shows what a
  plugin makes of blocks for its tests, `flick expand` prints the code of a
  program's blocks, and `flick new --template plugin` makes a library with a
  plugin.
- **Plugins, interface `0.3.0`:** a block without a body (`schema Shape`,
  raw text as `head r#"…"#`), heads with names and numbers among their
  arguments (`sql p { … }`), and blocks that are declarations: `head table
  Name "t" { … }`, or a head before the rest of a declaration (`server fn
  ping x = …`). Their code is declarations of the module (`let`,
  signatures, `type alias`, types and instances), exported and imported like
  the others; the user's patterns and declarations are holes too.
  `flick!` builds declarations, and its type form is now `flick!(b, ty T)`.
- **Plugins, interface `0.4.0`:** the names of a plugin's code are its own:
  its locals and the user's do not capture each other, and `Pairs.make`
  names the module `Pairs` whatever the file calls it. A plugin declares
  companions such as `Users.table`, exported with their type, and documents
  what it declares with the `///` lines above its block (`block.docs`,
  `b.doc`), which hovers, completions and `flick doc` show. Editors go to a
  generated name at the span its plugin gave it, outline it, and refuse to
  rename it outside its block.
- **Breaking:** `cli "name" { … }` is the plugin of `std`: a file that
  writes one imports it, `import Cli exposing (plugin cli)`. Without the
  import the block is `FLICK-P037`, which `flick check --fix` fixes. Its
  rules are `std/L001`…`std/L007` (formerly `FLICK-L001`…`FLICK-L007`) and
  `std/L009` for a malformed clause, which was `FLICK-S007`; the formatter
  prints the block as written. It now checks in every build of the compiler,
  the browser's too (`FLICK-L008` is retired).
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
