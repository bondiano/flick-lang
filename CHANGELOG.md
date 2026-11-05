# Changelog

All notable changes to Flick are recorded here. The project follows
[Semantic Versioning](https://semver.org/); before 1.0 a minor release may
break programs.

## Unreleased

- **Enums:** `untagged type Method = Get = "GET" | Post = "POST"` is an
  enum: each value is its literal at run time, all strings or all `Int`s.
  `match` and `==` compare the value, derived `Ord` follows the order of
  declaration, and the compiler gives each enum its instance of the new
  core class `Enum.Enum t b`, whose methods are the companions `T.value`,
  `T.parse` (checked at compile time for a constant, `FLICK-D006`) and
  `T.all`. The interface format is version 4.
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
- **Plugins, interface `0.3.0`:** a block without a body (`Shape.schema`,
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
- **Breaking:** `server fn` and `server name` are the plugin `server` of
  `std`: a file that has them imports `Server exposing (plugin server)`,
  which `flick check --fix` adds (`FLICK-P037`). The plugin writes
  `name$impl`, `name$procedure`, the call `name` and `name$server` with
  `Server.Derive`; the shape of a `server fn` is `std/E010`, a payload, a
  success or an error without a Schema witness `FLICK-D001` (a type alias
  of a record derives `Schema` for it), and `server name` of a value that
  is no `server fn` stays `FLICK-E010`. `FLICK-E012` and the `server-stdlib`
  feature are gone: every build has the plugin.
- **Breaking:** `schema T` is `T.schema`, the witness a type's companion
  names; `flick check --fix` rewrites it. `codec s { … }` is the plugin
  `codec` of `std`: a file that has one imports `Codec exposing (plugin
  codec)`, which the fix of `FLICK-P037` adds. Its diagnostics are
  `std/D002` to `std/D005` and `std/D007` with the texts they had, and a
  field whose codec decodes another type than the record's is `std/D012`.
  A block is a `Codec` of services when a field calls `field_effect` or
  `on_missing_effect`. `schema` and `codec` are no keywords.
- **Classes:** a default signature: the class body repeats a method's
  signature with constraints only its default needs
  (`describe : Show a => a -> String`), and each instance that takes the
  default satisfies them. `deriving Generic` (module `Generic`) gives a
  type its representation, `Sum`s of `Con`s of `Field`s in pairs, with
  `to_rep` and `from_rep`; and `deriving C` of a class of one parameter
  with a default for each method takes the defaults, so a class written
  once over `Generic`, as `Default`, derives for every type. In release
  the optimizer makes such an instance the one written by hand: a default
  taken becomes the instance's own method, a method of a known instance
  its code, and a `match` of a known constructor its arm.
- **Plugins, interface `0.12.0`:** a plugin reaches files, variables of the
  environment and hosts over HTTP that the project grants in
  `[plugins.<package>]` (`fs-read`, `net`, `env`), through
  `compiler.read_file`, `env` and `fetch`; anything else is `FLICK-P038`,
  naming the line to add. Answers over HTTP are kept as snapshots under
  `.flick/plugin-snapshots/`, which a project commits; `--offline`, now a
  global option, and `CI` read only those, and a missing one is
  `FLICK-P039`. A kept result holds while what its call read is the same.
  `Plugin::collect` takes the compiler.
- **Plugins, interface `0.11.0`:** `hover` gets the facts of the block's
  code, as `check` does, once it checked.
- **Plugins, interface `0.10.0`:** `check` after checking: a plugin gets the
  type the checker found for each `_` of its code, at the span it gave it,
  with the fields of a closed record, and may add errors under its codes.
- **Plugins, interface `0.9.0`:** a declaration block sees the signature
  right above it, and code with a type of it, `signed(span)`, takes it from
  the module; a block knows its `module`; `parse-body` parses a body as
  after a `let`'s `=`; tuple types and patterns; and a function in a
  plugin's code needs no signature.
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
- **Builds:** WebAssembly plugins run on wasmtime 48, a long-term support
  release; building the compiler takes Rust 1.95.
- **Editors:** on the header line of a module `flick ffi import` wrote,
  the code action "Regenerate from `<pkg>@<version>`" replaces the file with
  what the import writes now; a file of the header alone is filled in. When
  the `.d.ts` reader cannot run, the action is disabled with the reason.

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
