# Changelog

All notable changes to Flick are recorded here. The project follows
[Semantic Versioning](https://semver.org/); before 1.0 a minor release may
break programs.

## Unreleased

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
