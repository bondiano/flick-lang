---
title: "Покрытие Effect"
---

Стандартная библиотека оборачивает закреплённую версию Effect (см. [Граница языкового ядра](../../spec/07-errors-and-concurrency/#граница-языкового-ядра)): у каждого публичного экспорта точек входа `effect`, `effect/cli`,
`effect/http`, `effect/http-api` и `effect/reactivity` есть типизированная обёртка или явное
решение, а линтер в `cargo test` проверяет, что при обновлении Effect
ничего не потеряно.

## Снимок

`stdlib/Effect/snapshot.json` — поверхность установленной версии Effect, записанная
командой `node tools/dts-importer/surface.mjs > stdlib/Effect/snapshot.json`:
все точки входа из её `package.json`, а для пяти оборачиваемых точек — каждый
экспорт каждого модуля в виде `Module.name` с его категорией (`value`, `type` или
`value+type`) и отпечатком: 16 шестнадцатеричными цифрами SHA-256 от его
объявлений, напечатанных без комментариев. Изменение документирующего комментария
сохраняет отпечаток; изменение сигнатуры — нет.

## Манифест

`stdlib/Effect/surface/entrypoints.toml` задаёт решение для каждой точки входа:
её модули классифицируются в каталоге либо она исключается целиком, поэтому
для новой точки входа Effect тоже требуется решение.

```toml
"effect" = { modules = "core" }
"effect/ai" = { excluded = "AI provider clients are outside the 0.1.0 standard library" }
```

`stdlib/Effect/surface/<dir>/<Module>.toml` (или `<Module>.<part>.toml` для
большого модуля) присваивает каждому экспорту модуля ровно один статус:

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

| Статус | Значение | Смысл |
| - | - | - |
| `wrapped` | `"<Flick.Name> <fingerprint>"` | типизированный символ стандартной библиотеки, проверенный по этому отпечатку |
| `language` | конструкция | покрывается языком: `Effect.gen` — прямыми вызовами, `Context.Service` — `service`, `Data.TaggedError` — `type error`, `Layer.provide` — `provide`, `Match` — `match` |
| `excluded` | причина | только для TypeScript (`HKT`, `Types`, проверки типов, извлечение типов) либо противоречит инварианту (`unsafe*`, `Mutable*`, порождение задачи без владельца, выполнение эффекта вне средства запуска точки входа, управление потоком через канал ошибок) |
| `pending` | ответственный этап, `M<n>` | временно без обёртки |

`"*"` обозначает все экспорты модуля, у которых нет собственной записи.

## Линтер

`flick-core/tests/effect_surface.rs` завершается с ошибкой, указывая `Module.name`
и причину, в следующих случаях:

- экспорт или точка входа без решения либо решение для того,
  чего не существует;
- экспорт `wrapped`, чей отпечаток отличается от проверенного;
- символ `wrapped`, который стандартная библиотека не экспортирует или на который
  не ссылается ни один тест в `tests/run/stdlib/` (ссылки разрешает проверяющий модуль);
- внешняя привязка символа `wrapped` без `reviewed
  "effect@<pinned> …"` либо проверенная по другой версии;
- `pending` без ответственного этапа, снимок версии, отличной от
  закреплённой в `runtime/package.json`.

`just effect-coverage` выводит количества по статусам и модулям; CI добавляет их
в сводку запуска.

## Переход на другую версию

`just effect-bump <version>` загружает версию, записывает её снимок,
выводит добавленные, удалённые и изменённые элементы и недостающие возможности
в матрице возможностей среды выполнения, а также обновляет закреплённые версии
(`runtime/package.json`, `runtime/src/capabilities.ts`, тест версии среды выполнения,
`flick/src/toolchain.rs`). Линтер продолжает завершаться с ошибкой, пока манифест
и проверенные привязки не будут согласованы с новым снимком.

## Нестабильные модули

Effect помечает `effect/reactivity` как `@stability unstable` в 4.0.5, и так
же помечены его привязки: `Reactivity.Atom` (атомы, сервис `Registry`,
который хранит их значения, и его `layer`) и `Reactivity.AsyncResult`
(значение атома, вычисленное задачей). Их API может измениться с любым
выпуском Effect; стабильная поверхность — построенные на них модули UI.

## Привязки

Обёртка привязывает экспорт через среду выполнения, которая реэкспортирует каждый
обёрнутый модуль Effect как `@flick-lang/runtime/effect/<Module>`
(`runtime/src/effect/<Module>.ts`), поэтому скомпилированный код по-прежнему
импортирует только пакет среды выполнения. Адаптер для того, что нельзя передать
как есть в виде значения Flick (запись параметров с полями `Option`, ошибка Effect,
которая становится `type error` во Flick), находится в том же файле
и называется `flick<Name>`.

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

Имена записываются в snake_case; основной аргумент идёт последним, поэтому `task |> Effect.retry
schedule` читается по порядку: `arity 1` вызывает форму Effect с данными в последнем
аргументе, которая возвращает функцию от основного аргумента. Записи, передаваемые
в Effect, сохраняют его имена полей (`elapsedSincePrevious`).
