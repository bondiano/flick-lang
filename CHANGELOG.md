# Changelog

All notable changes to Flick are recorded here. The project follows
[Semantic Versioning](https://semver.org/); before 1.0 a minor release may
break programs.

## Unreleased

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
