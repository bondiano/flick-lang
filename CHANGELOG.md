# Changelog

All notable changes to Flick are recorded here. The project follows
[Semantic Versioning](https://semver.org/); before 1.0 a minor release may
break programs.

## Unreleased

- **Braces after `then` and `else`:** they hold statements, as after `do` and
  `loop`, so `if a > b then { low = b; high = a }` assigns both variables. A
  record right after `then` or `else` goes in parentheses; written the old way
  it is `FLICK-T074`, which `flick check --fix --only T074` rewrites.
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
- **Plugins:** a declaration a plugin gives may be an `unboxed type`, and
  `flick!` reads one. With a constructor only generated code can name, a
  plugin declares a type of its own over another at no cost when the program
  runs. Such a constructor is named by the code of the declaring plugin
  alone (`FLICK-P036` otherwise), and its type leaves the module opaque.
  A type the code of a block declares names classes in `deriving` as a
  declaration of the module does, and a derive is told what such a type
  wraps (`DeriveTarget.wraps`, which was `refined`).
- **Plugins:** a plugin is told where each value it watches is named, with
  the declaration around it (`CallSite.binding`), as it was for the
  serializable atoms alone.
- **Refined types:** `refined type` is a declaration of the plugin of `std`,
  which writes the type and its `parse`, `value` and check as code of the
  language; `refined` is no reserved word. The check is a companion too,
  `T.check : U -> Bool`, a refined type needs no Effect of its own, and
  another module sees it as an opaque type. A class in its `deriving` takes
  no arguments.
- **Plugins:** a block that opens a module may name a function its values
  are applied through (`Expr::Open.through`): inside `ui` a foreign component
  takes its props through `Ui.component` by this, not by a rule of the
  checker.
- **Labels:** `@label kind` before a type declaration gives the top-level
  values of that type their names where the program runs, as the atoms,
  views and actions of the standard library have them; a library marks its
  own types. `labels = "components"` keeps the kind `component`.
- **Owner services:** `@owner` before a service declaration says that what
  is made under the service belongs to its handler. A function that needs
  one is no maker to those that name it, as one that needs `Setup` was by a
  rule of the compiler; `Setup` is marked so.
- **Sync services:** a service whose operations run at once is marked
  `@sync` in the module that declares it, and one that only reads, which a
  stage lends to the clauses of a `for`, `@sync reader`. `Setup` and
  `Atom.Read` are marked so; the compiler no longer names them. The marks
  are those of the built-in modules.
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

Earlier changes of 0.2: [CHANGELOG-0.2-2.md](CHANGELOG-0.2-2.md), then
[CHANGELOG-0.2-1.md](CHANGELOG-0.2-1.md).

## 0.1.0 — 2026-11-03

The first public release: [CHANGELOG-0.1.md](CHANGELOG-0.1.md).
