---
title: "Implementation decisions"
---

## Decisions the specification leaves to the implementation

| Question | Decision |
| - | - |
| Parser | Hand-written recursive descent with Pratt parsing for operators; a lossless CST (trivia kept) for `fmt` and the LSP |
| Incrementality | No query framework. A build is cached whole in `.flick/cache/` by a key over all its inputs, including the sources and the interfaces of installed dependencies (see [Outputs and the cache](../../reference/cli/#outputs-and-the-cache)); an `.iface` records the fingerprints of the interfaces it was built against |
| Spans | UTF-8 byte ranges (see [Common options and diagnostics](../../spec/12-cli/#common-options-and-diagnostics)); the LSP translates them to UTF-16 |
| Snapshot tests | `insta` for diagnostics, IR and JS output (see [Testing](../testing/)) |
| Node host | Node.js 24.18.0 is the minimum supported version, checked by `flick doctor` |
| Browser host | Portable runtime and Effect adapters load without Node.js. Node-specific operations load their built-ins and platform packages only when executed; a browser reports an explicit Node-only error |
| pnpm | A pinned version through `packageManager` in the project template (`pnpm@12.14.0`); `flick doctor` compares the installed pnpm with it. Dependency commands disable manager bootstrapping and automatic pinning before checking or running pnpm, so a refused frozen install does not rewrite the lockfile's manager metadata |
| Effect | Exactly 4.0.5, commit `81fc8d9c7855d0f31bf493f379ec7bc2a6150a6f` (see [A complex Cause](../../spec/07-errors-and-concurrency/#a-complex-cause-what-the-error-row-promises), [The compiler](../../spec/11-compiler/) and [Effect release](../../reference/runtime/#effect-release)) |
| Effect API coverage | Every export of `effect`, `effect/cli`, `effect/http` and `effect/http-api` is classified in a manifest; a linter in `cargo test` fails on an unclassified, stale or changed-signature export (see [Effect coverage](../../reference/effect-surface/)) |
| Effect Node platform | The HTTP server and `Terminal`/`FileSystem`/`Path` for the CLI come from `@effect/platform-node`, pinned to 4.0.5 alongside Effect as a dependency of `@flick-lang/runtime` |
| PostgreSQL driver | `pg` (node-postgres), an optional peer dependency of `@flick-lang/runtime`, bound through `foreign` in the standard library module `Database` (`Database.pg`) |
| React | React 19 and `react-dom/client`, optional peer dependencies of `@flick-lang/runtime` |
| UI state | Atoms of `effect/reactivity` (unstable, pinned with Effect) instead of a registry of Flick's own; a root or a server request provides its registry. This replaces the first version's "no SSR and no full hooks model": a root-local `Ref` cannot give a server render state of its own per request nor hand that state to the client, and Effect's registry, serialization and `Hydration` already do. The cost is about 25 KB gzip, which the bundle budgets carry (see [User interfaces](../../reference/ui/)). Only `runtime/src/atoms.ts` and `runtime/src/effect/reactivity/` import `effect/reactivity` (an oxlint rule refuses it elsewhere), so a registry of Flick's own, if the bundle budget or a breaking Effect release calls for one, replaces that layer alone |
| UI handlers | A handler's task has the row `{Atom.Write}` and no errors, so `Ui`, `Action` and `View` carry no service or error rows: a task provides any other service itself and handles its own errors, and a root's supervisor gets `Failure none`, its defects. With the rows, every view, slot and prop type repeated `r e` though handlers became top-level actions; an action that needs a service or fails names that in its own task, where it is handled |
| UI tracing | Every run of an action is a span named by its label, and every `Atom.Write` write is logged at the debug level with its cause and its `Module:line`, which the compiler passes to `Atom.set`/`update`/`modify`. Both are always in the runtime rather than behind an opt-in layer: they cost about 4.4 KB gzip (spans 2.3 KB, Effect's logger 1.7 KB), which the UI bundle budgets carry (counter and atoms 104 KB, playground 112 KB) |
| Entry points | An app names its programs in `entries`, each built into a runner `flick-<name>.js` beside modules written once; `entry` is the entry `main`, whose runner keeps the name `flick-main.js`. A module that only re-exports `main` is an entry too: a re-export is the same entity, so the runner imports `main` from the module that defines it, as every importer does, since a module's JavaScript has only its own definitions. A re-exported `tests` is not a test module, which would run another module's tests twice. Web pages are not entries: a bundler takes JavaScript modules importing `.fl` |
| Development of pages | `flick dev` restarts the server entries on a change, as it does any app, and runs Vite for the client entries. A page's entry script accepts its own replacement: `hotDispose` of `Runner.Browser` reads the values of the labelled states (each top-level `let` of an atom, and a form draft's texts, `Type.field`) of the mounted roots, then interrupts the running program, whose finalizers unmount its roots, so no watcher or subscription is left twice; `hotCarry` makes the next program's first root start from those values. An atom found by its label in the new module takes the old value; a renamed or removed one starts afresh. Task atoms run again |
| Libraries without Effect | A library built on Effect also writes `_core/`, the modules that need no Effect built against `@flick-lang/runtime/core`; one build cannot serve both backends. `core` `Option`/`Result` are plain objects that Effect's `Equal.equals` and `Hash.hash` do not equate with its own, and the Effect build of a module imports Effect, which an app with `effect = "none"` must not load. Measured on 2026-10-28: `_core/` of `@flick-lang/dts-importer` is 13% of its modules |
| Documentation bundles | ESM execution workers and size-split vendor chunks preserve Effect's module execution order. A standalone Starlight 404 page provides English and Russian recovery links without becoming a documentation or search entry |
| Run generation locks | Atomically reserved `<pid>-<n>` directories protect construction before the child lock exists. Unix checks owner/child liveness without sending a signal; malformed locks, permission errors and unknown states are kept. Non-Unix hosts remain conservative |
| `.d.ts` resolution | The npm package `typescript` in the Node helper `@flick-lang/dts-importer`; a JSON request on stdin, the surface as JSON on stdout (see [Importing a package's `.d.ts`](../../reference/ffi/#importing-a-packages-dts)) |
| npm package scope | `@flick-lang/*` (`@flick-lang/runtime`, `@flick-lang/dts-importer`); the standard library ships inside the compiler |
| Generated names | An instance dictionary is `$i_<Class>_<Type>` when its head is names separated by spaces (`Show Float`), which maps back to one head; any other head (`Show (List a)`) keeps an 8-hex suffix of its FNV hash. Constructor tags keep their 48-bit hash (`Circle#d930eed2c811`) in every profile, private types included: the tag is a value's identity across packages and modules, `derive`, `show` and `equals` read it at run time, and a debug and a release build must agree on it. Measured on 2026-10-25: without the hash the playground would be 138 bytes smaller after gzip (17 tags), the other tracked programs at most 24, so it stays |
| Release checks | Release calls the reusable CI for the tagged commit; native builds and publication both depend on its success |

## Resolutions of gaps in the specification

The specification fixes the semantics but leaves a number of surface details
open. The decisions below are taken and described in the reference.

| Gap | Decision |
| - | - |
| Comments | `//` line, `/* */` block with nesting, `///` doc comment for `flick doc`. `//` works inside `sql { }` too (see [Whitespace and comments](../../reference/grammar/#whitespace-and-comments)) |
| Strings | Escapes `\n \t \r \\ \" \0 \u{…}` (braced form only, 1 to 6 hex digits, no surrogates; `\0` cannot be followed by a digit); raw `r"…"`, `r#"…"#`, `r##"…"##`…; multi-line `"""…"""` with the common indentation removed; no interpolation (see [String literals](../../reference/grammar/#string-literals)) |
| Operators | A fixed table, no user-defined operators. From weakest to strongest: `\|>` (left), `\|\|` (right), `&&` (right), `== != < <= > >=` (non-associative), `++` (right), `+ -` (left), `* / %` (left, `%` is the JS remainder), `**` (right), prefix `-`, `not`, `await`, `ignore`, application, `.field` / `M.x`. Bitwise operations are the functions `Float.bit_*` (see [Operator table](../../reference/grammar/#operator-table)) |
| Bool and `if` | `Bool` is the JS boolean; literals `true`/`false` (allowed in patterns); `if c then a else b` is an expression, `else` may be left out when the branch is `Unit`, `else if` chains, a multi-line form by layout (see [Layout](../../reference/grammar/#layout) and [Expressions](../../reference/grammar/#expressions)) |
| Float literals | The grammar of JS `NumericLiteral`: decimal, exponent `1e-3`, `0x`/`0o`/`0b`, the `_` separator by JS rules (not at the start or end of a digit run, not doubled, not next to `.`/`e`/a prefix), BigInt suffix `n` (no fraction or exponent); legacy octal `010` and a leading zero like `09` are errors. Unlike JS, `.5` and `5.` are errors, since `.` is field access (see [Float literals](../../reference/grammar/#number-literals)) |
| Tests | No new syntax: a module of the test root exports `tests : List Test.Test`; `Test.test`, `Test.effect_test`, `Test.property`, `Test.group` and the generators `Gen.*` are library functions (see [Tests](../../reference/stdlib/#tests)) |
| Platform API | `Platform.args`, `Platform.env`, `stdout_line`/`stderr_line`/`stdout_write` are a standard library module (see [Modules](../../reference/stdlib/#modules)) |
| `AppError` | A Prelude type: `type error AppError = AppError { message : String; details : Option String }`; `App.fail : String ->{; AppError} a`; the runner prints `error: <message>` safely and exits with code `1` (see [Entry runner](../../reference/runtime/#entry-runner)) |
| SQL table metadata | A block `sql table Name "table" { col : sql_type [not null \| null] [primary key] [unique] [default] [generated always \| generated by default] as T [via codec] }`; it declares the row type `Name` and the table `Name$table : Sql.Table "table" { … }` of `Sql.Column`s; tables generated from migrations take the same form (see [Tables](../../reference/sql/#tables)) |
| FFI overrides | The importer writes only `<Pkg>/Generated.fl`; a module of your own, usually `<Pkg>.fl`, holds ordinary `foreign` declarations with `reviewed "<pkg>@<ver> sha256:…"` and re-exports the rest; `flick ffi check` compares the fingerprints (see [Reviewed bindings and `ffi check`](../../reference/ffi/#reviewed-bindings-and-ffi-check)) |
| Shadowing and ambiguity | Local bindings shadow everything without a diagnostic. A top-level definition named like an import `exposing (name)` is `FLICK-R009`; names from `exposing (..)` are shadowed by top-level definitions silently. Two imports exposing one name by name as different entities are `FLICK-R010` at once; a conflict between `(..)` imports is `FLICK-R010` only where the name is used; two imports under one qualifier are `FLICK-R010`. Re-exports of the same entity do not conflict. The order of imports changes nothing (see [Shadowing and ambiguity](../../reference/modules/#shadowing-and-ambiguity)) |
| HTTP | No syntax (see [The boundary of the language core](../../spec/07-errors-and-concurrency/#the-boundary-of-the-language-core)): `Http.*` and `HttpApi.*` are library combinators; endpoint parts are described by codecs built from `Schema a` (see [Typed HTTP APIs](../../reference/http-api/)) |
| CLI eDSL | See [CLI eDSL](../../spec/10-type-programming-and-edsl/#cli-edsl-flags-arguments-and-subcommands). A block `cli "name" { … }` with the clauses `description`, `flag`, `arg`, `subcommand` and `handle`; it elaborates into the library functions `Cli.*` over `effect/cli`, which are usable directly (see [CLI eDSL](../../reference/cli-dsl/) and the [grammar](../../reference/grammar/#expressions)) |
| Class methods | A method is declared by a plain signature `name : T` (there is no `val` keyword; the old form is `FLICK-S010`). A default method is its signature with a `let name … =` right below it in the class body; an instance may leave it out. A signature without a `let` is an abstract method, which an instance must define (`FLICK-C011` otherwise; see [Instances](../../reference/classes/#instances)) |
| Packages with Flick modules | A package declares `"flick": { "iface-dir": "<dir>" }` in its `package.json`; module `A.B` is described by `<dir>/A/B.iface`. `flick build` writes the files and `flick pack` the field, not the user. Installation is pnpm only (`flick install`/`add`); Flick reads the installed tree of the project's direct dependencies (see [Packages](../../reference/project/#packages) and [Location in a package](../../reference/iface/#location-in-a-package)) |

## Instances of user types

The [MTL example](https://github.com/bondiano/flick-lang/tree/main/examples/mtl)
defines `Functor`, `Applicative` and `Monad` instances in Flick for
[Identity](https://github.com/bondiano/flick-lang/blob/main/examples/mtl/src/Mtl/Identity.fl),
[MaybeT](https://github.com/bondiano/flick-lang/blob/main/examples/mtl/src/Mtl/MaybeT.fl),
[StateT](https://github.com/bondiano/flick-lang/blob/main/examples/mtl/src/Mtl/StateT.fl),
[ReaderT](https://github.com/bondiano/flick-lang/blob/main/examples/mtl/src/Mtl/ReaderT.fl),
[WriterT](https://github.com/bondiano/flick-lang/blob/main/examples/mtl/src/Mtl/WriterT.fl)
and [ExceptT](https://github.com/bondiano/flick-lang/blob/main/examples/mtl/src/Mtl/ExceptT.fl).
They use ordinary constructors, pattern matching and `do`; no transformer
primitive or foreign implementation is needed.

Three capture rules make these instances expressible:

- A generic bound `a^c` bounds the complete value, including its existing
  qualifications; it does not add `c` to an otherwise unknown capture set.
  A function or nominal wrapper may still need a hidden-capture qualifier.
- Curried stages retain generic values as well as named regions.
  Constructors retain their fields' captures, and patterns project the
  enclosing bound back to fields, including delayed computations.
- Prelude callbacks are capture-polymorphic per invocation: for example,
  `flat_map` takes `(forall v. a^v -> (m b)^(c + v))^c`.
  `v` is independent of the computation's bound `d`: a `StateT` state or
  `ReaderT` environment can arrive only when the computation is run.

These are bounds, not erasure: concrete scoped regions and explicit stored
qualifications survive substitution, and mutable storage stays invariant.
See [Regions and captures](../../reference/regions/).

To use the layers, follow [App.Plan](https://github.com/bondiano/flick-lang/blob/main/examples/mtl/src/App/Plan.fl):

```flick
type alias Planner a = MaybeT (StateT PlanState (ReaderT Config Identity)) a
```

Its `do` program imports `ask`, `get` and `put` from `Mtl.Classes`; inherited
capability instances cross the intervening layers automatically. The shared
`MonadTrans.lift` deliberately reaches an explicitly typed inner action.
Run the stack from outside in: `MaybeT.run`, `StateT.run` with initial state,
`ReaderT.run` with configuration, then `Identity.run`. Failure preserves the
state of completed steps; the default planner CLI remains unchanged.

`MonadState`, `MonadReader`, `MonadWriter` and `MonadError` put the monad first,
with an FD from it to the capability's payload. Coverage closes determining
head variables over the context's dependencies and instantiated superclasses.
Coherent selection still uses complete heads, never context search; structural
context restrictions and solver budgets remain mandatory. Each transformer
owns its instances, and the outermost repeated native capability wins.

`pass` stores a rank-1 `(w -> w)^c` modifier independently of the action bound;
putting a polymorphic `forall` inside its tuple would require impredicative
instantiation. Explicit local reshape annotations retain this stored qualifier.
Actual later-supplied state/environment captures still reach the result.
`catch_error` is handler-first and catches domain errors, not typed Eff errors,
defects or interruption. StateT outside ExceptT restarts the handler at entry
state; WriterT outside ExceptT cannot return a failed output pair. Reversing
those layers preserves the state or log before the domain error.

The D15 example includes all five MonadTrans instances, the 20 capability
paths, laws and an `App.Capabilities` workflow over Identity and a cold Eff base.
This is the enumerated D15 set, not all Haskell transformers. No transformer
primitive, quantified MonadTrans superclass or Eff runtime adapter is needed.
