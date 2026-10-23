---
title: "14. Sources"
---

The sources below describe existing mechanisms. The status of decisions about Flick's syntax, source types and semantics is given in the project sections.

- [Content Mappers PR](https://github.com/microsoft/typescript-go/pull/4712) — the starting point for TS integration.

- [ets-go](https://github.com/mikearnaldi/ets-go) — an example of a separate syntax on top of TS and Effect.

- [The Effect type](https://effect.website/docs/v4/getting-started/the-effect-type) — deferred computations, results, errors and dependencies.

- [Effect.gen](https://effect.website/docs/v4/getting-started/using-generators) — the generator representation of sequential composition.

- [Effect errors](https://effect.website/docs/v4/error-management/two-error-types) — expected errors, defects and interruption.

- [Services](https://effect.website/docs/v4/requirements-management/services) and [Layers](https://effect.website/docs/v4/requirements-management/layers) — providing and building the environment.

- [Scope](https://effect.website/docs/v4/resource-management/scope) and [Fibers](https://effect.website/docs/v4/concurrency/fibers) — resources and concurrent execution.

- [Koka](https://koka-lang.github.io/koka/doc/book.html) — call effects and effect-polymorphic functions.

- [PureScript type classes](https://book.purescript.org/chapter6.html) — classes, instances and dictionaries.

- [TypeScript for functional programmers](https://www.typescriptlang.org/docs/handbook/typescript-in-5-minutes-func.html) — the expressiveness and limits of TS.

- [OCaml polymorphism](https://ocaml.org/manual/5.4/polymorphism.html) — value restriction, mutation and variance.

- [GHC higher rank](https://ghc.gitlab.haskell.org/ghc/doc/users_guide/exts/rank_polymorphism.html) and [functional dependencies](https://ghc.gitlab.haskell.org/ghc/doc/users_guide/exts/functional_dependencies.html) — variants of expressiveness for the type core.

- [PureScript type classes](https://github.com/purescript/documentation/blob/master/language/Type-Classes.md) — coherence and orphan instances.

- [Elm ports](https://guide.elm-lang.org/interop/ports.html), [PureScript FFI](https://book.purescript.org/chapter10.html), [ReScript bindings](https://rescript-lang.org/docs/manual/bind-to-js-function/) — different JavaScript interop contracts.

- [Schema v4](https://effect.website/docs/v4/schema/getting-started) and [Schema for Effect data types](https://effect.website/docs/v4/schema/effect-data-types) — codecs, services and external representations.

- [Ref](https://effect.website/docs/v4/state-management/ref), [Hash](https://effect.website/docs/v4/trait/hash) and [Equal](https://effect.website/docs/v4/trait/equal) — state and collection semantics.

- [Option](https://effect.website/docs/v4/data-types/option), [Redacted](https://effect.website/docs/v4/data-types/redacted), [BigDecimal](https://effect.website/docs/v4/data-types/bigdecimal) and [Duration](https://effect.website/docs/v4/data-types/duration) — standard library types.

- [Layer memoization](https://effect.website/docs/v4/requirements-management/layer-memoization), [Fiber API](https://effect.website/docs/v4/api/effect/Fiber) and [Effect.ts](https://github.com/Effect-TS/effect/blob/main/packages/effect/src/Effect.ts) — sharing, cancellation and finalizer contracts.

- [Higher-rank polymorphism GHC](https://ghc.gitlab.haskell.org/ghc/doc/users_guide/exts/rank_polymorphism.html) — annotated polymorphic arguments and the limits of inference.

- [ECMAScript numeric types](https://tc39.es/ecma262/multipage/ecmascript-data-types-and-values.html) — the exact semantics of Number and BigInt.

- [Effect Cause](https://effect.website/docs/v4/data-types/cause) and [the verified source snapshot](https://github.com/Effect-TS/effect/tree/81fc8d9c7855d0f31bf493f379ec7bc2a6150a6f) — the reference for runtime contracts; changing the dependency requires re-verification.

- [Effect Running Effects](https://effect.website/docs/v4/getting-started/running-effects), [Chunk](https://effect.website/docs/v4/data-types/chunk) and [Scheduler](https://effect.website/docs/v4/api/effect/Scheduler) — callback boundaries, collections and cooperative yielding.
