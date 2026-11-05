---
title: "Покрытие Effect"
---

Стандартная библиотека оборачивает закреплённую версию Effect
(см. [Граница языкового ядра](../../spec/05-errors-and-concurrency/#граница-языкового-ядра)).
Для каждого публичного экспорта из точек входа `effect`, `effect/cli`, `effect/devtools`,
`effect/http`, `effect/http-api`, `effect/observability`, `effect/process`, `effect/reactivity` и `effect/rpc` есть типизированная обёртка или явно
записанное решение. Линтер, который выполняется в `cargo test`,
следит за тем, чтобы при обновлении Effect ни один экспорт не остался без решения.

## Снимок

`stdlib/Effect/snapshot.json` описывает API установленной версии Effect.
Снимок записывается командой `node tools/dts-importer/index.mjs surface > stdlib/Effect/snapshot.json`
и содержит все точки входа из её `package.json`. Для девяти оборачиваемых
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
cron = "cron expressions need a parser of their own"
```

| Статус | Значение | Смысл |
| - | - | - |
| `wrapped` | `"<Flick.Name> <fingerprint>"` | типизированный символ стандартной библиотеки, проверенный по этому отпечатку |
| `language` | конструкция | возможность уже есть в языке: вместо `Effect.gen` используются прямые вызовы, вместо `Context.Service` — `service`, вместо `Data.TaggedError` — `type error`, вместо `Layer.provide` — `provide`, вместо `Match` — `match` |
| `excluded` | причина | возможность нужна только в TypeScript (`HKT`, `Types`, проверки типов, извлечение типов) либо противоречит инварианту языка (`unsafe*`, `Mutable*`, порождение задачи без владельца, выполнение эффекта вне средства запуска точки входа, управление потоком через канал ошибок) |
| `pending` | чего не хватает | обёртки временно нет |

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
- у решения `language`, `excluded` или `pending` не указана причина;
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

Effect помечает `effect/reactivity` как `@stability unstable` в 4.1.2, и так
же помечены его привязки: `Reactivity.Atom` (атомы, сервис `Registry`,
который хранит их значения, и его `layer`) и `Reactivity.AsyncResult`
(значение атома, вычисленное задачей). Их API может измениться с любым
выпуском Effect; стабильная поверхность — построенные на них модули UI.

`effect/rpc` тоже нестабилен, как и `Rpc`: процедуры описываются кодеками,
тип группы — запись её обработчиков, `listen` обслуживает группу по HTTP,
пока не закроется её область, а `call` и `call_stream` вызывают процедуру
того же значения. Сообщения пишутся в NDJSON, если `ServeOptions` и
`call_with` / `call_stream_with` не выбирают `json`, `json_rpc`, `ndjson_rpc`
или `binary` (бинарные кадры Effect, которые попадают в бандл, только если
программа их называет). `listen_websocket` обслуживает ту же группу
по WebSocket, а клиент обращается к ней с `transport = Rpc.websocket`;
`listen_tcp` — по TCP в Node, с `Rpc.tcp` и сериализацией, которая делит
поток на кадры; `serve_stdio` отвечает на вызовы через stdin и stdout, а
`spawn` запускает такую программу и подключается к ней; `serve_worker`/`worker`
делают то же для Web Worker, `serve_node_worker`/`node_worker` — для потока
worker_threads Node; `with_middleware`
выполняет хук над payload и заголовками каждого вызова до обработчика, а
`with_headers` добавляет заголовки к вызовам задачи; `uninterruptible`
заставляет сервер довести обработчик до конца, даже если клиент ушёл;
`call_local` и `call_stream_local` вызывают группу в памяти — для тестов;
`connect` держит одно соединение, которое делят `call_on` и
`call_stream_on`.

`effect/observability` тоже нестабилен, как и `Otlp` и `Prometheus`.
`Otlp.layer`, `traces`, `logs` и `metrics` экспортируют span'ы, записи журнала
и метрики по протоколу OpenTelemetry через HTTP с JSON, каждый со своим
fetch-клиентом. `Otlp.settings url service` и `layer_with`, `traces_with`,
`logs_with`, `metrics_with` добавляют версию и атрибуты сервиса, заголовки
запросов, gzip, protobuf (`Otlp.protobuf` попадает в бандл, только если его
назвать), интервалы отправки, размер пачки и таймаут завершения.
`Otlp.from_config` экспортирует то, что говорит конфигурация `OTEL_`
текущего `ConfigProvider`; сигнал отправляется, только если его
`OTEL_<SIGNAL>_EXPORTER` называет `otlp`.
`Prometheus.format` выдаёт записанные метрики в текстовом формате Prometheus,
`format_with` — с префиксом или переименованными, а `Prometheus.route path` —
маршрут `Http.Router`, который их отдаёт.

`effect/devtools` нестабилен, как и `DevTools`: `DevTools.layer url` отправляет
span'ы программы по WebSocket в Effect DevTools, которые слушают `url`, а
`DevTools.layer_with { url; headers }` добавляет заголовки к рукопожатию
(в Node).

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
# import Schedule exposing (Schedule)
#
/// Recurs forever, each step the duration after the previous one ended.
spaced : Duration.Duration -> Schedule none none i Float
foreign pure spaced from "@flick-lang/runtime/effect/Schedule" "spaced"
  reviewed "effect@4.1.2 sha256:498f22b480d2397e"

retry :
  forall c r e a r2 e2 o.
  Schedule.Schedule r2 e2 (Errors e) o -> (Eff r e a)^c ->{r + r2; e + e2} a
foreign effect retry from "@flick-lang/runtime/effect/Effect" "retry"
  arity 1
  retention call
  reviewed "effect@4.1.2 sha256:508d9e1864f6c9d3"
```

Имена записываются в snake_case, а основной аргумент передаётся последним.
Поэтому `task |> Effect.retry
schedule` можно читать слева направо. Указание `arity 1` вызывает вариант API
Effect с основным аргументом в конце: сначала он принимает расписание
и возвращает функцию от основного аргумента. В записях, передаваемых
в Effect, сохраняются его имена полей (`elapsedSincePrevious`).
