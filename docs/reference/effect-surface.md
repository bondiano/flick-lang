# Effect coverage

The standard library wraps the pinned Effect release (§7 "Граница языкового
ядра"): every public export of the entrypoints `effect`, `effect/cli`,
`effect/http` and `effect/http-api` has a typed wrapper or an explicit
decision, and a linter in `cargo test` checks that nothing is lost when
Effect moves.

## Snapshot

`stdlib/Effect/snapshot.json` is the surface of the installed Effect, written
by `node tools/dts-importer/surface.mjs > stdlib/Effect/snapshot.json`:
every entrypoint of its `package.json`, and for the four wrapped ones each
export of each module as `Module.name` with its kind (`value`, `type` or
`value+type`) and a fingerprint, 16 hex digits of the SHA-256 of its
declarations printed without comments. A changed doc comment keeps the
fingerprint; a changed signature does not.

## Manifest

`stdlib/Effect/surface/entrypoints.toml` decides each entrypoint: its
modules are classified under a directory, or it is excluded as a whole, so a
new entrypoint of Effect needs a decision too.

```toml
"effect" = { modules = "core" }
"effect/ai" = { excluded = "AI provider clients are outside the 0.1.0 standard library" }
```

`stdlib/Effect/surface/<dir>/<Module>.toml` (or `<Module>.<part>.toml` for a
large module) gives each export of the module exactly one status:

```toml
[wrapped]
spaced = "Schedule.spaced 498f22b480d2397e"

[language]
gen = "a runtime function body: direct calls"

[excluded]
isSchedule = "TS-only: a type guard; Flick values are typed statically"

[pending]
cron = "M20"
```

| Status | Value | Meaning |
| - | - | - |
| `wrapped` | `"<Flick.Name> <fingerprint>"` | a typed symbol of the standard library, reviewed against that fingerprint |
| `language` | the construct | covered by the language: `Effect.gen` by direct calls, `Context.Service` by `service`, `Data.TaggedError` by `type error`, `Layer.provide` by `provide`, `Match` by `match` |
| `excluded` | the reason | TypeScript-only (`HKT`, `Types`, type guards, type extractors) or against an invariant (`unsafe*`, `Mutable*`, a fork without an owner, running an effect outside the entry runner, control flow through the error channel) |
| `pending` | the owning stage, `M<n>` | temporarily unwrapped |

`"*"` stands for every export of the module without an entry of its own.

## Linter

`flick-core/tests/effect_surface.rs` fails, naming `Module.name` and why, on:

- an export or entrypoint without a decision, or a decision for one that
  does not exist;
- a `wrapped` export whose fingerprint differs from the reviewed one;
- a `wrapped` symbol the standard library does not export, or that no test
  under `tests/run/stdlib/` refers to (resolved by the checker);
- a foreign binding of a `wrapped` symbol without `reviewed
  "effect@<pinned> …"`, or reviewed against another release;
- `pending` without an owning stage, a snapshot of another release than
  `runtime/package.json` pins.

`just effect-coverage` prints the counts by status and module; CI adds them
to the run's summary.

## Moving to another release

`just effect-bump <version>` downloads the release, writes its snapshot,
prints what was added, removed and changed and what the runtime's
capability matrix lacks, and moves the pins (`runtime/package.json`,
`runtime/src/capabilities.ts`, the runtime's version test,
`flick/src/toolchain.rs`). The linter stays red until the manifest and the
reviewed bindings agree with the new snapshot.

## Bindings

A wrapper binds the export through the runtime, which re-exports each
wrapped module of Effect as `@flick-lang/runtime/effect/<Module>`
(`runtime/src/effect/<Module>.ts`), so compiled code still imports only
the runtime package. An adapter for what a Flick value cannot pass as is
(an options record with `Option` fields, an Effect error that becomes a
Flick `type error`) sits in the same file, named `flick<Name>`.

```flick
/// Recurs forever, each step the duration after the previous one ended.
spaced : Duration.Duration -> Schedule none none i Number
foreign pure spaced from "@flick-lang/runtime/effect/Schedule" "spaced"
  reviewed "effect@4.0.5 sha256:498f22b480d2397e"

retry :
  forall c r e a r2 e2 o.
  Schedule.Schedule r2 e2 (Errors e) o -> (Eff r e a)^c ->{r + r2; e + e2} a
foreign effect retry from "@flick-lang/runtime/effect/Effect" "retry"
  arity 1
  retention call
  reviewed "effect@4.0.5 sha256:508d9e1864f6c9d3"
```

Names are snake case; the subject comes last, so `task |> Effect.retry
schedule` reads in order: `arity 1` calls Effect's data-last form, which
returns the function of the subject. Records that cross to Effect keep its
field names (`elapsedSincePrevious`).
