# Changelog

All notable changes to Flick are recorded here. The project follows
[Semantic Versioning](https://semver.org/); before 1.0 a minor release may
break programs.

## Unreleased

- **A closure over a generic callback in a payload:** with `k : Int -> c`,
  `Som (fun x -> k (x + 1))` checks against `Opt (Int -> c)` as `Som k`
  does, where it was `FLICK-T003` "region `c` differs". Such a closure that
  returns another type variable than the payload's is now `FLICK-T008`.
- **Children in `ui`:** inside `ui { … }` the children of an element stand
  one by one, each an argument on the line of the call or a line of its own
  under it, with no list around them; an element with none is written
  without them, and one argument that is a list is the list of the
  children. A list literal there is `FLICK-T073`, which
  `flick check --fix --only T073` rewrites. `Ui.each items node` gives a
  node for each item. `ui` is a block of the plugin of `std` and no longer
  a reserved word.
- **Comparisons by `Ord`:** `< <= > >=` order any type with an `Ord`
  instance by its `compare`, as they already did a type variable: `Bool`, a
  derived type, a record, a list. A type without one is `FLICK-C001`, where
  it was `FLICK-T003`.
- **`show` of `Option` and `Result`:** an argument with fields is shown in
  parentheses, as a derived `Show` writes one: `Some (Card Spades 3)` and
  `Err (ArithmeticError "division by zero")`, where they were
  `Some Card Spades 3` and `Err ArithmeticError "division by zero"`. A number,
  a string, a list, a tuple or a record stays as it was (`Some -1`).
- **Cells:** `@cell` before a type declaration marks a type whose values
  hold one value each, as a state made by a pure call does. A value of such a
  type that is no function is not generalized over what the cell holds, and
  one whose type has variables there is `FLICK-T072`. The atoms of the
  standard library are cells; for an atom the refusal stays `FLICK-U005`,
  now worded by the UI plugin. A package's `watch` may name the tag `@cell`.
- **Stream queries on Flick's runtime:** `Atom.query_stream` runs on the
  default runtime: its atom shows the latest element of the adapter's
  `Stream`, starts it anew on a mutation of its keys and stops it when no view
  reads it. A program that uses it keeps its UI on the runtime.
- **Routes on Flick's runtime:** `Atom.route` and `Ui.route_link` run on the
  default runtime: the route reads the served URL on a server and the page's
  in a browser, again on back and forward, and a write or a plain click on a
  link pushes its URL without a reload. A program that uses them keeps its UI
  on the runtime.
- **Playground on Flick's own runtime:** the browser compiler checks and builds
  a program as `flick build --rt` does, and the playground runs it on Flick's
  own runtime. Bindings that runtime does not have yet still reach the Effect
  adapter, which the playground loads as before.

- **Serializable atoms on Flick's runtime:** `Atom.serializable`,
  `serializable_family`, `serializable_task` and `serializable_result` run on
  the default runtime. `Ui.Server.state` hands their values to the client, and
  `Ui.Dom.mount` starts from them, without running a hydrated task again. A
  program that uses them no longer needs `@flick-lang/effect-ts`.
- **Streamed server markup on Flick's runtime:** `Ui.Server.stream` renders on
  the default runtime and hands its chunks to the adapter's `Stream`, so a
  program that streams pages keeps its atoms and views on the runtime instead
  of moving its whole UI to the adapter.
- **Local mutation and loops:** `let mut name = value` declares a local
  monomorphic variable and `name = value` assigns it. Pure functions may
  change their variables; nested functions, tasks and lazy sequences cannot
  capture them (`FLICK-G005`). `loop { … }` returns its `break value`;
  `while condition do body` and `for … do` return `Unit`. `continue` starts
  the next iteration. Exits target the nearest loop and close scopes normally.
  Completed assignments survive an error in `try`; an error in the right-hand
  side leaves its target unchanged. Assigning an immutable name is
  `FLICK-T069`, mutation in a strict comprehension is `FLICK-T070`, invalid
  loop control is `FLICK-T071`, and unused `mut` is `FLICK-R020`.
  `loop` and `while` are contextual words, preserving `Loop.loop` and
  `Schedule.while`. IR 0.7.0 adds variables and explicit sequential loops.

- **Saved modules:** `flick check`, `build`, `run` and `test` save each module
  of the project that checks without a diagnostic in the user's cache
  directory (`flick/modules`), under a key of its text, the interfaces it
  imports, the checked configuration, the compiler and std. The next process
  takes it instead of checking it again. A module with plugin blocks, a
  module application or an inline module, or in a project whose packages
  watch calls, is checked each time.

- **Watching:** `flick dev`, `flick test --watch` and `--watch` start a run
  once the watched files settle, so a save that truncates a file and then
  writes it is built once and `dev` restarts the app once for it.

- **HTTP request ownership:** each request owns its resources through response
  streaming. Closing a server stops admission before outer resources are
  released, interrupts requests left after the grace period and waits for
  their finalizers on native JS, Effect and Go. Cleanup and uninterruptible
  work may outlast that period. Go now applies the documented 20-second
  default and distinguishes an immediate timeout from an infinite one.

- **Regions by use:** the checker asks where a value uses a region, not what
  it holds, everywhere: a value may keep a handle past its `scope` while
  nothing uses it there, and a use there is `FLICK-G002`. The `captures`
  discipline, the `[build] regions` key and the `flick live` code-block tag
  are gone; `.iface` version 12. `Setup s` keeps an instance's region live,
  a handler of a region-indexed service needs its region live where it is
  installed, and lifecycle work outlives its instance (`Lifecycle s`). The
  reference model and the Lean development have one typing judgment, by use,
  with T4′ as their region safety theorem.

- **Opt-in implicit conversions:** `@implicit` marks a ground, context-free
  instance of a class with one pure `a -> b` method. A unique visible
  conversion is inserted at a ground nominal type mismatch, without chains
  or inference. Built-in numeric conversions remain explicit.
- **Explicit conversions:** the Prelude class `Into a b` with `into : a -> b`
  names a conversion that loses nothing, to the type its place expects:
  `(into n : Float)`, `into n * 10n`. The core has `Into Int Float` and
  `Into Int BigInt`; a program declares it for its own types.
- **SQL composition:** `exists $(template)` and `is_in value $(template)`
  accept reusable `Sql.Template` values with no named parameters. Captures,
  codecs and placeholders compose without running a query. `is_in` requires
  one result column of the compared type. Conduit's article list and count
  now share one filter, including the slug filter.
- **Effect rows:** `->{Async, Database}` means `->{Async, Database; none}`.
  The trailing `;` before omitted errors is optional in every runtime arrow,
  not just rows with sets; `flick fmt` removes it. Errors still follow `;`.
- **Holes in strings:** `"n = \{n + 1}"` is `"n = " ++ display (n + 1)`, through
  the new Prelude class `Display`: a `String` is itself, without the quotes of
  `show`, and `Int`, `Float` and `Bool` are spelled as `show` spells them. A
  hole ends on the line of its string and may nest strings with holes. Raw
  strings have no holes, `\{` in a multi-line string is `FLICK-S005`, and the
  release optimizer folds the constant parts into one string. `\{` was an
  invalid escape before, so no program changes its meaning.
- **Plugins check calls:** a library whose `[package]` says
  `watch = ["Ui.Event.queue", "*.parse"]` beside its `plugin` gets, through the
  new `check-calls`, every place of a checked module that applies one of those
  names, with each argument's span, type and constant value after constant
  folding. For a refined type's `parse`, it also receives the refinement
  metadata needed to validate that constant. An entry
  that is no `Module.name`, `Module.Sub.name` or `*.name`, and a `watch` without a `plugin`, are
  `FLICK-P032`. Two checks of the compiler's moved into the plugin of `std` and
  kept their codes, spans, messages and explanations: `FLICK-U001`, the capacity
  or limit of an event policy, and `FLICK-D006`, a constant that fails the check
  of its refined type. Rebuild a plugin's component for the new interface.
- **A plugin's pass over the IR:** a library whose `[package]` says `hooks = ["ir"]`
  beside its `plugin` gets the IR of the project's modules as CBOR after the
  checks of effects and gives what `flick build`, `run` and `test` compile
  instead. The compiler checks it for form (`FLICK-P047` when it does not hold
  together, from a new validator in `flick-ir`) and notes the rewrite with
  `FLICK-P048`: what the program does is then outside Flick's checks.
- **A plugin's `emit`:** a library whose `[package]` says `hooks = ["emit"]` beside
  its `plugin` rewrites the files `flick build` writes, its scripts, their maps
  and interfaces, before they are written; a script changed without its map is
  `FLICK-P046`. `flick run` and `flick test` build without it.
- **Files of other languages:** a library whose `[package]` says `load = ["proto"]`
  beside its `plugin` turns each `.proto` file into a module: `import App.User`
  finds `App/User.proto`, the plugin's `load` gives its declarations at the
  file's spans, the project checks them, the editor goes to the file, and a
  changed file rebuilds the project. Two libraries loading one extension are
  `FLICK-P045`. Rebuild a plugin's component for the new interface.
- **`component` is a block of `std`:** the plugin of `std` gives the code of
  `component name (props : P) = body`, as the compiler did, so programs, their
  diagnostics, their hover and their bundles stay as they were. The word starts a
  component only before a name at the start of a declaration. A malformed one is
  `std/U010`. The editor finds and renames a local of a block's code, and a
  `CallSite` there names the definition the block gives. A long component no
  longer gets the length and complexity lints of a function.
- **Blocks of `std` need no import:** every module has the heads of the plugin of
  `std`, `cli`, `codec`, `server` and `sql`, as in 0.1. An import of one,
  `import Cli exposing (plugin cli)`, is `FLICK-R012`, which `flick check --fix`
  removes; only a block without `{`, such as `cli Main`, still needs it. Another
  plugin may not give these heads (`FLICK-P044`), and `FLICK-P037` is retired. A
  block inside the code of another expands also where a file is checked on its
  own, as a doc example is.
- **A plugin's code:** a plugin may give `match` with guards, `if` and blocks of
  statements as the body of a `fun` or a `let`, and use a pattern of the user's in
  several places, as the props of a component its setup and its render bind. A
  diagnostic under the plugin's code may be a warning or a note and carry fixes,
  which `flick check --fix` applies when safe. Renaming a local the user's code of
  a block binds, as a parameter of a `server fn`, renames its uses too. Rebuild a
  plugin's component for the new interface.
- **Documentation:** the Learn, Guides, API and Ecosystem pages are gone. Their
  addresses lead to the start pages, Getting started and the reference; building
  from source is a section of Getting started.
- **A compiler built from source:** takes the `std` of its own checkout instead of
  the release tag its version names, so `flick new` and `flick run` work without
  `FLICK_STD_PATH`. A `std` that does not check with the compiler is the new
  `FLICK-P043`, which names the module, instead of a crash. A release whose
  `std-v<version>` tag is missing fails before it is built.
- **Workspace plugins:** `[[workspace.plugins]]` in a workspace's root gives every
  member a plugin, as a dependency and its grants. The editor reports the root
  manifest's errors in a member, and a changed root manifest rebuilds the members.
  `flick.lock` pins the build of a local plugin's component, `install --locked`
  refuses another, and `flick plugins` lists the plugins of a project.
- **Plugins:** a plugin that fails is no longer quiet. A block of a plugin that does
  not load is `FLICK-P036`; a failure in completion, hover, highlighting or layout
  goes to the server's log, and `flick fmt` reports it and exits 1; a first-party
  plugin that panics fails its block, not the compiler. `[plugins.<package>]
  disable = ["sql", "Cli.Value"]` refuses part of a plugin with `FLICK-P042`. The
  editor shows why a `flick.toml` does not read, a missing `.wasm` says how to
  build it, and a rebuilt plugin rebuilds what used it.
- **Sets of services and errors:** a `type alias` of a row names a set, as in
  `type alias Orders = {...Reads, Billing; OrderNotFound}`. `...` merges a set
  into a row, also in an arrow (`->{...Orders, Async | r; Boom}`), and `-` takes
  an element or a set out (`{...Orders, -Log; -InvalidEmail}`). A set expands
  where it is named, so it fits wherever its row does; `Eff {...Orders} none a`
  takes the errors of a set into its row of errors. Like other arrow rows, a row
  with a set needs no trailing `;`: `->{...Env}`.
- **Aliases in declarations:** a declaration may use an alias written below it.
  `run : Unit ->{Time; Failure} Float` above `type alias Failure = Oops` was
  `FLICK-T002`.
- **GADTs:** a `match` follows a type variable across the parts of the matched
  value. On `(x, y)` of type `(Tagged a, Tagged a)` it needs no arm for a pair of
  different constructors, and an arm for one is `FLICK-T010`.
- **Regions:** `f a` for a type variable `f` holds what `f` stands for. A method of
  `instance Keep (Holder h)` may now keep its parameter in a later stage, which was
  refused with or without a signature, and a region held through `f` stays visible
  in the stages of a class method.
- **Signatures:** a foreign function with `retention owner` and one region gets
  `Fiber.Outlives`, `Fiber.Provided` and `Detached` without writing them. New
  warning `FLICK-T059` names what a signature would get anyway, such as a qualifier
  of a later stage or the signature of an instance method that repeats its class, and `flick check --fix --only T059` removes it. `flick fmt`
  drops an outer `forall` on a `foreign` too.
- **Types as shown:** hover, inlay hints, completion, `flick doc`, inserted
  signatures and error messages leave out an outer `forall` and the qualifier of a
  later stage when a signature written that way gets them back, as `lift : Monad m
  => (m a)^c -> (MaybeT m a)^c`. A `forall` of rank 2 stays, and so does what a
  message is about: G001–G004 and a mismatch with a polymorphic side show all of it.
- **Signatures:** a local function needs none. Its type is inferred, and without
  a signature it has one type. A top-level or recursive local function still
  does, and an instance method still takes the one of its class. G001, G003 and
  G004 get edits that write what is missing, and G001 says when the qualifier it
  names is one no one wrote.
- **`[js.dependencies]`:** a library of `[dependencies]` names the npm packages
  it needs on the js target, and `flick install` adds those of the project's
  whole dependency closure to its `package.json`. A js build without them is
  `FLICK-P034`, and two packages that disagree on a version are `FLICK-P040`.
- **Foreign modules of a library:** `foreign f from "./x.mjs" "f"` in a library
  of `[dependencies]` ships with a js build beside the compiled module, so it
  works from a fetched library, in a packed app, and finds the project's npm
  packages; the import in the module is `./x.mjs` instead of a path to the
  library's sources. On go, its `x.go` is named by the package instead of by its
  absolute path. A file that cannot ship is `FLICK-P041`.
- **Implicit packages:** the packages every program sees without depending on
  them are a list, of one member `std` for now: a second is an entry and a
  directory `packages/<name>` beside it, with its own `modules.txt`, and
  `FLICK_STD_PATH` finds it next to `std`. Their go binding tables are now part
  of the build's key.
- **`std`:** its modules import each other explicitly, and a test keeps it so;
  programs may still qualify built-in modules without an import. The line of
  `Form.report` in its emitted `place` label moves with the imports above it.
- **`Scope.current`:** it is not exposed. Its region was any, so a child could
  stay under an owner longer than the region its type names and use a resource
  after its release. Take the owner from `scope s { … }`.
- **`Detached`:** work a region keeps needs it, as a child does: a finalizer,
  the release of a resource, a listener, the handlers of a server. A `control`
  operation in a finalizer stopped it without an error once a handler let the
  body go; now it is `FLICK-E017`.
- **`Scope.run`:** `scope s { body }` stands for `Scope.run (fun s -> body)`, and
  `Scope.run` takes a function polymorphic in the region, also as a value. The
  IR has no `Scope` node; its version is 0.4.0.
- **`Layer.run`:** `provide layer { body }` stands for `Layer.run layer (fun
  () -> body)`; the type of `provide` is still the checker's. A child reaches
  the handlers around where it started. The IR has no `Provide` node, and
  `Operation` is `Perform`; its version is 0.3.0.
- **`Fail`:** `Fail.fail` fails with an error; `Fail e` in the row of an arrow
  is the error row `e`; `| Fail.fail p ->` in a `try` is `| p ->`. The IR has
  no `Try` node: a `try` is a `Handle`, and the IR version is 0.2.0.
- **Effects:** what may suspend says so with the built-in effect `Async`,
  which the scheduler handles at the root; a row without it runs at once.
  In `std` the functions that wait have it: `Effect.sleep`, `Fiber.join`, queues, `Database`,
  `FileSystem`, `Http.Client`, the servers. `main`, the tasks of actions,
  events, atoms and tests may have it.
- **FFI:** a `foreign` names its mode only for `pure` and `effect`; without
  a word its signature tells, a call of the host, a promise when its row
  has `Async`. `foreign sync` and `foreign async` are `FLICK-F001`, whose
  fix removes the word and gives a promise's row its `Async`.
- **Types:** a layer that needs less fits where one that needs more is
  expected, as an `Eff` does.
- **Fixes:** `FLICK-E002` adds what a row lacks where it is written: the
  signature, an `Eff`, or the row given to a type, as `Handlers none`;
  adding `Async` alone is safe, and so is the `FLICK-T003` of a written type
  that lacks only `Async`. `flick check --fix --only E002,F001,T003`
  applies only the fixes of those codes.

Earlier changes of 0.2: [CHANGELOG-0.2-1.md](CHANGELOG-0.2-1.md).

## 0.1.0 — 2026-11-03

The first public release: [CHANGELOG-0.1.md](CHANGELOG-0.1.md).
