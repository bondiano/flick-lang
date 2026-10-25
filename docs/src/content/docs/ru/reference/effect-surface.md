---
title: "Покрытие Effect"
---

Стандартная библиотека оборачивает закреплённую версию Effect
(см. [Граница языкового ядра](../../spec/07-errors-and-concurrency/#граница-языкового-ядра)).
Для каждого публичного экспорта из точек входа `effect`, `effect/cli`,
`effect/http`, `effect/http-api`, `effect/reactivity` и `effect/rpc` есть типизированная обёртка или явно
записанное решение. Линтер, который выполняется в `cargo test`,
следит за тем, чтобы при обновлении Effect ни один экспорт не остался без решения.

## Снимок

`stdlib/Effect/snapshot.json` описывает API установленной версии Effect.
Снимок записывается командой `node tools/dts-importer/surface.mjs > stdlib/Effect/snapshot.json`
и содержит все точки входа из её `package.json`. Для шести оборачиваемых
точек входа в нём перечислены все экспорты каждого модуля в виде `Module.name`.
У каждого экспорта указаны категория (`value`, `type` или `value+type`)
и отпечаток — 16 шестнадцатеричных цифр SHA-256 от объявлений, напечатанных
без комментариев. Изменение документирующего комментария не меняет отпечаток,
а изменение сигнатуры меняет.

## Манифест

`stdlib/Effect/surface/entrypoints.toml` определяет, как обрабатывается
каждая точка входа: либо её модули описываются в указанном каталоге,
либо она исключается целиком. Для каждой новой точки входа Effect
тоже нужно выбрать один из этих вариантов.

```toml
"effect" = { modules = "core" }
"effect/ai" = { excluded = "AI provider clients are outside the 0.1.0 standard library" }
```

Файл `stdlib/Effect/surface/<dir>/<Module>.toml` (или `<Module>.<part>.toml`
для большого модуля) задаёт ровно один статус для каждого экспорта:

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
| `language` | конструкция | возможность уже есть в языке: вместо `Effect.gen` используются прямые вызовы, вместо `Context.Service` — `service`, вместо `Data.TaggedError` — `type error`, вместо `Layer.provide` — `provide`, вместо `Match` — `match` |
| `excluded` | причина | возможность нужна только в TypeScript (`HKT`, `Types`, проверки типов, извлечение типов) либо противоречит инварианту языка (`unsafe*`, `Mutable*`, порождение задачи без владельца, выполнение эффекта вне средства запуска точки входа, управление потоком через канал ошибок) |
| `pending` | ответственный этап, `M<n>` | обёртки временно нет |

Запись `"*"` распространяется на все экспорты модуля, у которых нет собственной записи.

## Линтер

`flick-core/tests/effect_surface.rs` завершается с ошибкой и указывает
`Module.name` и причину, если:

- у экспорта или точки входа нет решения;
- решение относится к несуществующему экспорту или точке входа;
- отпечаток экспорта `wrapped` отличается от проверенного;
- стандартная библиотека не экспортирует символ `wrapped` или ни один тест
  в `tests/run/stdlib/` на него не ссылается (ссылки разрешает модуль проверки типов);
- у внешней привязки символа `wrapped` нет отметки `reviewed
  "effect@<pinned> …"` либо привязка проверена по другой версии;
- у `pending` не указан ответственный этап;
- версия снимка отличается от закреплённой в `runtime/package.json`.

`just effect-coverage` выводит число экспортов по статусам и модулям.
CI добавляет эти данные в сводку запуска.

## Переход на другую версию

`just effect-bump <version>` загружает указанную версию и записывает её снимок.
Команда выводит добавленные, удалённые и изменённые элементы, а также возможности,
которых недостаёт в матрице возможностей среды выполнения. Она также обновляет
закреплённые версии в `runtime/package.json`, `runtime/src/capabilities.ts`,
тесте версии среды выполнения и `flick/src/toolchain.rs`.
Линтер продолжает завершаться с ошибкой, пока манифест и проверенные привязки
не будут согласованы с новым снимком.

## Нестабильные модули

Effect помечает `effect/reactivity` как `@stability unstable` в 4.0.5, и так
же помечены его привязки: `Reactivity.Atom` (атомы, сервис `Registry`,
который хранит их значения, и его `layer`) и `Reactivity.AsyncResult`
(значение атома, вычисленное задачей). Их API может измениться с любым
выпуском Effect; стабильная поверхность — построенные на них модули UI.

`effect/rpc` тоже нестабилен, как и `Rpc`: процедуры описываются кодеками,
тип группы — запись её обработчиков, `listen` обслуживает группу по HTTP
с NDJSON, пока не закроется её область, а `call` и `call_stream` вызывают
процедуру того же значения.

## Привязки

Обёртка обращается к экспорту через среду выполнения. Среда выполнения
реэкспортирует каждый обёрнутый модуль Effect по пути
`@flick-lang/runtime/effect/<Module>` (`runtime/src/effect/<Module>.ts`).
Поэтому скомпилированный код по-прежнему импортирует только пакет среды выполнения.

Если значение нельзя передать как есть в виде значения Flick, в том же файле
размещается адаптер с именем `flick<Name>`. Например, он нужен для записи
параметров с полями `Option` или для ошибки Effect, которая во Flick
становится `type error`.

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

Имена записываются в snake_case, а основной аргумент передаётся последним.
Поэтому `task |> Effect.retry
schedule` можно читать слева направо. Указание `arity 1` вызывает вариант API
Effect с основным аргументом в конце: сначала он принимает расписание
и возвращает функцию от основного аргумента. В записях, передаваемых
в Effect, сохраняются его имена полей (`elapsedSincePrevious`).
