---
title: "14. Источники"
---

Источники ниже описывают существующие механизмы. Статус решений о синтаксисе, исходных типах и семантике Flick указан в разделах проекта.

- [Content Mappers PR](https://github.com/microsoft/typescript-go/pull/4712) — исходная точка интеграции с TS.

- [ets-go](https://github.com/mikearnaldi/ets-go) — пример отдельного синтаксиса поверх TS и Effect.

- [Тип Effect](https://effect.website/docs/v4/getting-started/the-effect-type) — отложенные вычисления, результат, ошибки и зависимости.

- [Effect.gen](https://effect.website/docs/v4/getting-started/using-generators) — генераторное представление последовательной композиции.

- [Ошибки Effect](https://effect.website/docs/v4/error-management/two-error-types) — ожидаемые ошибки, defects и interruption.

- [Сервисы](https://effect.website/docs/v4/requirements-management/services) и [Layers](https://effect.website/docs/v4/requirements-management/layers) — предоставление и построение окружения.

- [Scope](https://effect.website/docs/v4/resource-management/scope) и [Fibers](https://effect.website/docs/v4/concurrency/fibers) — ресурсы и конкурентное выполнение.

- [Koka](https://koka-lang.github.io/koka/doc/book.html) — эффекты вызовов и эффектно-полиморфные функции.

- [PureScript type classes](https://book.purescript.org/chapter6.html) — классы, instances и dictionaries.

- [TypeScript для функциональных программистов](https://www.typescriptlang.org/docs/handbook/typescript-in-5-minutes-func.html) — выразительность и ограничения TS.

- [OCaml polymorphism](https://ocaml.org/manual/5.4/polymorphism.html) — value restriction, mutation и variance.

- [GHC higher rank](https://ghc.gitlab.haskell.org/ghc/doc/users_guide/exts/rank_polymorphism.html) и [functional dependencies](https://ghc.gitlab.haskell.org/ghc/doc/users_guide/exts/functional_dependencies.html) — варианты выразительности типового ядра.

- [PureScript type classes](https://github.com/purescript/documentation/blob/master/language/Type-Classes.md) — coherence и orphan instances.

- [Elm ports](https://guide.elm-lang.org/interop/ports.html), [PureScript FFI](https://book.purescript.org/chapter10.html), [ReScript bindings](https://rescript-lang.org/docs/manual/bind-to-js-function/) — разные контракты JavaScript interop.

- [Schema v4](https://effect.website/docs/v4/schema/getting-started) и [Schema для Effect data types](https://effect.website/docs/v4/schema/effect-data-types) — codecs, services и внешние представления.

- [Ref](https://effect.website/docs/v4/state-management/ref), [Hash](https://effect.website/docs/v4/trait/hash) и [Equal](https://effect.website/docs/v4/trait/equal) — состояние и семантика коллекций.

- [Option](https://effect.website/docs/v4/data-types/option), [Redacted](https://effect.website/docs/v4/data-types/redacted), [BigDecimal](https://effect.website/docs/v4/data-types/bigdecimal) и [Duration](https://effect.website/docs/v4/data-types/duration) — типы стандартной библиотеки.

- [Layer memoization](https://effect.website/docs/v4/requirements-management/layer-memoization), [Fiber API](https://effect.website/docs/v4/api/effect/Fiber) и [Effect.ts](https://github.com/Effect-TS/effect/blob/main/packages/effect/src/Effect.ts) — sharing, отмена и контракты finalizers.

- [Higher-rank polymorphism GHC](https://ghc.gitlab.haskell.org/ghc/doc/users_guide/exts/rank_polymorphism.html) — аннотированные полиморфные аргументы и ограничения inference.

- [ECMAScript numeric types](https://tc39.es/ecma262/multipage/ecmascript-data-types-and-values.html) — точная семантика Number и BigInt.

- [Effect Cause](https://effect.website/docs/v4/data-types/cause) и [проверенный исходный snapshot](https://github.com/Effect-TS/effect/tree/81fc8d9c7855d0f31bf493f379ec7bc2a6150a6f) — ориентир runtime-контрактов; изменения зависимости требуют повторной проверки.

- [Effect Running Effects](https://effect.website/docs/v4/getting-started/running-effects), [Chunk](https://effect.website/docs/v4/data-types/chunk) и [Scheduler](https://effect.website/docs/v4/api/effect/Scheduler) — callback-границы, коллекции и cooperative yielding.
