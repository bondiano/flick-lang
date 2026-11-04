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
  `flick!` builds declarations, and a type is `flick!(b, type T)`, told from
  `type N = …` by its `=`.
- **Plugins, interface `0.4.0`:** the names of a plugin's code are its own:
  its locals and the user's do not capture each other, and `Pairs.make`
  names the module `Pairs` whatever the file calls it. A plugin declares
  companions such as `Users.table`, exported with their type, and documents
  what it declares with the `///` lines above its block (`block.docs`,
  `b.doc`), which hovers, completions and `flick doc` show. Editors go to a
  generated name at the span its plugin gave it, outline it, and refuse to
  rename it outside its block.
- **Plugins, interface `0.5.0`:** a plugin lays out its blocks for the
  formatter (`format`, pieces of its words, lines, levels and the user's code
  in holes), and `cli` does again, a clause a line; a block without a layout
  moves with its head. A block inside the user's code of another expands in
  a round of its own. Editors highlight the heads of plugins as macros, and
  `cli` is no keyword of the grammars any more.
- **Breaking:** `deriving Schema`, `Url` and `Form` are derives of `std`'s
  plugin, written in Flick with new combinators (`Schema.unsafe_record`,
  `Schema.unsafe_adt`, `Schema.lazy`, `Schema.recursive`, `Route.codec`,
  `Form.make`, …); the JSON, JSON Schema, routes and forms are the same.
  What they cannot derive is `std/D001`, `std/D009`, `std/D010` (were
  `FLICK-D001`, `FLICK-D009`, `FLICK-D010`), on the class in `deriving`. A
  phantom parameter takes a witness too, and a field of an alias of another
  module names that alias's own witness, so the alias derives `Schema`.
- **Breaking:** `deriving HttpApi` is a derive of `std`'s plugin, which
  writes `T.endpoints` and `T.api` in Flick with `HttpApi.Derive.at`,
  `wire_codec`, `with_header_auth` and `with_optional_header_auth`. What it
  cannot read as endpoints is `std/D011`, on the endpoint; a part with no
  codec stays `FLICK-D011`, on its type, and a value named after a type is
  its codec only when it is a `PureCodec Json` of it. An error without a
  `Problem` instance, and a path whose `:name` parameters are not the fields
  of its `path` part, are `FLICK-C001`. A group's check gets the request's
  headers alone.
- **Plugins, interface `0.8.0`:** a row of types, `{NotFound, Conflict}`, in
  a plugin's types, and a type may stand in several places of its code.
- **Plugins, interface `0.7.0`:** `let rec` and `forall` types in a plugin's
  declarations.
- **Plugins, interface `0.6.0`:** a class marked `@derive` (or `@derive
  std`) is derived by a plugin: `deriving C { args }` and `deriving instance
  C (M.T a)` call its `derive`, whose declarations follow the type. A class
  without a derive is `FLICK-C013`, "has no derive".
- **Breaking:** `deriving Cli.Value` is the plugin of `std`'s now, through
  `Cli.choice` and `Cli.refined`; a type it cannot derive is `std/L010`
  (was `FLICK-C013`), and on the go target a refined type's predicate is
  checked. The runtimes derive no `Value` any more.
- **Plugins:** `flick fmt` lays out blocks with the plugins of the project's
  packages; a component built for another version of `flick:plugin` is
  `FLICK-P035` on its import, naming both versions.
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
