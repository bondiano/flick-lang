# Changelog

All notable changes to Flick are recorded here. The project follows
[Semantic Versioning](https://semver.org/); before 1.0 a minor release may
break programs.

## Unreleased

- **A function applied where it is written takes its argument's type.**
  In `(fun h -> h ()) say` and `say |> fun h -> h ()` the parameter `h`
  has the type of `say`, as in `let h = say`: an effectful callback fits,
  a polymorphic field of a record argument is read at an instance, and a
  field the argument lacks is reported where the body reads it. A function
  bound to a name first still takes its parameter's type from its body.

## 0.2.0 — 2026-11-24

The second public release. Before 1.0 a minor release may break programs:
the **Breaking** section lists what to change, and
`flick check --fix --only <codes>` rewrites most of it.

### Language

- **Function contracts:** `where require … ensure …` on a function is
  type-checked by every build and verified by `flick prove` in a decidable
  fragment of pure integer and data code (`FLICK-T087`…`T089`). `flick prove`
  also checks the proof modules of a project's proof root.
- **Refined types** are a block of `std`; `T.prove v` builds one without a
  runtime check where the compiler proves the check from the facts of the
  path, and exported invariants cross modules.
- **Effect handlers:** `effect E =` with `tail`, `abort` and `control`
  operations, handled by `try … with`; `Async` marks what may suspend.
- **GADTs**, **enums** (`untagged type`), default signatures,
  `deriving Generic` and derives written as plugins.
- **Local mutation and loops:** `let mut`, `loop`, `while`, `for`, labeled
  `break`/`continue`.
- **New forms:** `lazy { … }`, `let lazy` and suspended `~` parameters;
  `defer`/`errdefer`; `let … else`; `result { … }`; `orelse`; `<|`, `>>`,
  `<<`; string holes `"\{x}"`; sets of services and errors in rows
  (`...Orders`, `-Log`).
- **Fixed-width numbers:** `Int8`…`Int64`, `UInt8`…`UInt64`, `Float32`.
- **Errors of a row:** `match` takes apart a value of `Errors {…}`, and one
  error fits where several are expected.
- **Type inference:** local functions need no signature and are generic;
  class instances are resolved with inferred variance.
- **Regions** are checked by use, with soundness proved in Lean for the
  reference core; several holes the formal rules close are now errors.

### Tooling

- **Backends are WebAssembly components.** The official `flick/js` and
  `flick/go` backends ship as pinned components; `--target js|go` selects
  one, and a backend declares its own options, templates, package commands
  and runners. Third-party backends use the same interface.
- **Plugins:** `std` ships `sql`, `cli`, `server`, `codec`, `ui`,
  `component`, `refined`, `suite`/`test` and the derives as plugin blocks.
  Plugins can check calls, rewrite the IR, rewrite emitted files and load
  files of other languages (`load = ["proto"]`). Plugins may be workspace
  plugins, with granted file, network and environment access.
- **FFI:** inline foreign code and preambles, `@native` overrides over Flick
  fallback bodies, backend-declared adapter metadata, and foreign modules
  that ship with a library.
- **Testing:** `suite`/`test` blocks with `.skip`/`.only`, hooks, timeouts,
  fixtures, `test.each`, typed matchers (`expect x |> to.equal y`) and test
  companions that see a module's private declarations, alike on Flick's
  runtime, the Effect adapter and Go.
- **Flick's own runtime** is the default for JS: Console, HTTP values and
  servers, RPC clients, serializable atoms, routes and streamed markup run
  without the Effect adapter. The playground runs on it too.
- **Performance:** saved checked modules between runs, a faster Go target
  (direct instance calls, unboxed fields, allocation-free `Option`),
  release-time evaluation of closed pure code and lazy-cell rewrites proved
  equivalent in Lean.
- **Editor:** incremental LSP with background analysis, rename of class
  methods across a workspace, completion ranked by expected type, and fixes
  for captured `let mut` and missing services.

### Breaking

- **Host services:** the standard library names what it uses
  (`Host.Console`, `Host.Clock`, `Host.Random`, `Host.Environment`,
  `Host.Arguments`, `Host.Process`); UI roots need `Host.BrowserLive`.
  `flick check --fix --only E002` adds them to rows.
- **Plugin blocks need no import**, but `import … exposing (plugin …)`
  written for 0.1 is `FLICK-R012`, which `--fix` removes.
- **Tests:** `Test.expect` takes the value it checks; the old form is
  `Test.check`. `suite` and `test` are reserved at the start of a top-level
  declaration.
- **Syntax:** `lazy` is a keyword (`Schema.lazy` is `Schema.suspend`);
  braces after `then`/`else` hold statements (`FLICK-T074`); `ui` children
  stand one by one without a list (`FLICK-T073`); `from_result (Err e)` is
  `Fail.fail e` (`FLICK-T075`).
- **CLI:** backend-specific build flags became backend options with
  `--target`; `[build] effect = "none"` is gone.
- **Server functions** outside a browser take their server from
  `Server.with_origin`, not `FLICK_SERVER_URL`.
- **Layers and regions:** a layer's build and lazy layer refs require
  `Fiber.Provided`; `Scope.current` is no longer exposed; `LogLevel` is an
  enum.
- Interfaces (`.iface`), the IR and the plugin and backend interfaces have
  new versions: rebuild dependencies and plugin components with 0.2.
## 0.1.0 — 2026-11-03

The first public release: [CHANGELOG-0.1.md](CHANGELOG-0.1.md).
