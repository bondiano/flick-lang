---
title: "Наблюдаемость"
---

Трасса отвечает на вопрос «где запрос потратил время?», запись журнала —
«что произошло?», метрика — «как часто и сколько?». Во Flick это разные
механизмы: `Tracer` создаёт спаны, логгеры Effect получают записи журнала,
а `Metric` хранит числовые ряды в реестре метрик. Логи и спаны в этом
реестре не накапливаются.

Слой OTLP или DevTools предоставляется вокруг наблюдаемого кода.
У Prometheus другая модель: маршрут читает реестр метрик по запросу
сборщика. Без экспортёра данные не уходят в коллектор OTLP, но логгер
Effect по умолчанию всё равно пишет в консоль.

## Спаны

Вызов `Tracer.with_span name options task` выполняет `task` в новом
спане, который заканчивается вместе с задачей; `Tracer.parent_span ()` —
спан, в котором выполняется текущая задача. Спан несёт атрибуты, ссылки
на другие спаны и роль, названную как в OpenTelemetry
(`Tracer.internal`, `server`, `client`, `producer`, `consumer`). По
умолчанию спан — потомок текущего; `root = true` начинает новую трассу,
а `external_span { spanId; traceId; sampled }` вводит спан другого
процесса, чтобы использовать его как родителя или ссылку.

```flick
span_options : Tracer.SpanOptions
let span_options =
  {
    attributes = [("user", "ada")];
    links = [];
    parent = None;
    root = false;
    kind_ = Tracer.internal;
    sampled = None;
    level = None;
    captureStackTrace = false;
    disablePropagation = false;
  }
```

Вложенные вызовы `with_span` наследуют текущий спан как родителя.
Используйте стабильное имя операции, например `checkout`, а детали
конкретного запроса помещайте в атрибуты: так трассы можно сравнивать,
не теряя контекст отдельных вызовов.

## Метрики

`Metric` хранит счётчики, датчики, частоты, гистограммы и сводки квантилей
в реестре метрик программы. Долгоживущую метрику создавайте один раз —
например, на верхнем уровне, — а обновляйте из эффектного кода.

| Вид | Что хранит |
| - | - |
| `counter name options` | накопленную сумму входов; при `incremental = true` отрицательные обновления игнорируются, поэтому сумма не убывает |
| `gauge name options` | последний вход; `modify` прибавляет к нему |
| `frequency name options` | сколько раз встретилось каждое слово |
| `histogram name options` | количество значений до каждой границы и их сумму |
| `timer name options` | гистограмма длительностей в миллисекундах |
| `summary name options` | каждый квантиль со значением; `maxAge` и `maxSize` ограничивают сохраняемые входы |

```flick
checkouts : Metric.Counter
let checkouts =
  Metric.counter "checkouts" { description = None; attributes = []; incremental = true }
```

Вызов `Metric.update input metric` записывает вход; равнозначная запись —
`metric |> Metric.update input`. При `incremental = true` обновления
`2`, `3`, `-1` оставляют значение `5`, а не `4`: этот флаг задаёт
монотонность, а не выбор между приращениями и накопленным значением
при экспорте.

`Metric.value metric` читает состояние метрики, `Metric.snapshot ()` —
реестр, а `Metric.dump ()` выводит его как диагностический текст.
`with_attributes` создаёт отдельный ряд для каждого набора атрибутов.
Используйте ограниченные множества значений (`status`, а не уникальный
ID запроса), иначе число рядов будет расти вместе с трафиком.
`linear_boundaries`, `exponential_boundaries` и
`boundaries_from_iterable` строят границы гистограммы;
`enable_runtime_metrics` учитывает файберы, которые задача начинает
и завершает.

## Экспорт OTLP

`Otlp` экспортирует данные по HTTP: накапливает партии спанов и логов
и периодически читает метрики. Закрытие предоставленного слоя отправляет
оставшиеся данные с учётом тайм-аута завершения. Держите слой вокруг
всего приложения, а не создавайте экспортёр заново на каждом запросе.

Общий слой и слои отдельных сигналов по-разному понимают `url`:

| Слой | Какой URL передать |
| - | - |
| `layer`, `layer_with` | базовый URL, например `http://localhost:4318`; пути `/v1/traces`, `/v1/logs`, `/v1/metrics` добавляются автоматически |
| `traces`, `traces_with` | полный адрес приёма трасс, например `http://localhost:4318/v1/traces` |
| `logs`, `logs_with` | полный адрес приёма логов с окончанием `/v1/logs` |
| `metrics`, `metrics_with` | полный адрес приёма метрик с окончанием `/v1/metrics` |

Функция использует настройки спана и счётчик выше. Она экспортирует спан;
чтобы отправить ещё и счётчик, нужен `Otlp.layer` или слой метрик.

```flick
observe_checkout : Unit ->{} Unit
let observe_checkout () =
  provide (Otlp.traces { url = "http://localhost:4318/v1/traces"; service_name = "shop" }) {
    Tracer.with_span "checkout" span_options (task { Metric.update 1 checkouts })
  }
```

`Otlp.settings url service_name` задаёт JSON без gzip, дополнительных
заголовков и атрибутов ресурса, с настройками пакетной отправки Effect
по умолчанию. Передайте обновлённую запись в `layer_with`, `traces_with`,
`logs_with` или `metrics_with`, чтобы изменить версию сервиса, атрибуты
ресурса, заголовки, формат записи, интервал экспорта или тайм-аут
завершения. `max_batch_size` относится к трассам и логам, но не к метрикам.
При `gzip = true` сжимаются тела от 1 КБ. `Otlp.protobuf` выбирает
protobuf вместо `Otlp.json`; этот сериализатор попадает в бандл, только
если программа его использует.

### Конфигурация развёртывания

`Otlp.from_config None` читает текущий `ConfigProvider` (по умолчанию —
окружение процесса) и экспортирует JSON. Сигнал включается **только**
когда его `OTEL_TRACES_EXPORTER`, `OTEL_LOGS_EXPORTER` или
`OTEL_METRICS_EXPORTER` называет `otlp`: одного адреса коллектора
недостаточно.

Также читаются `OTEL_EXPORTER_OTLP_ENDPOINT`, адреса отдельных сигналов
`OTEL_EXPORTER_OTLP_<SIGNAL>_ENDPOINT`, заголовки, сжатие и тайм-ауты;
настройки партий `OTEL_BSP_*`, `OTEL_BLRP_*` и `OTEL_METRIC_EXPORT_*`;
параметры ресурса `OTEL_SERVICE_NAME`, `OTEL_SERVICE_VERSION`
и `OTEL_RESOURCE_ATTRIBUTES`. `Otlp.from_config (Some "shop")`
переопределяет имя сервиса из конфигурации. В тестах предоставляйте
отдельный `ConfigProvider`, а не меняйте окружение всего процесса.

## Prometheus

Вызов `Prometheus.format ()` возвращает записанные программой метрики
в текстовом формате Prometheus; `format_with { prefix; rename }`
добавляет префикс к каждому имени или переименовывает метрики. Значение
`Prometheus.route "/metrics"` — маршрут `Http.Router`, отвечающий на GET
метриками и их типом содержимого, готовый для опроса:

```flick
make_metrics_app :
  Unit ->{}
    (Http.ServerRequest.ServerRequest ->{; Http.ServerError.HttpServerError}
      Http.ServerResponse.ServerResponse)
let make_metrics_app () =
  Http.Router.router Http.Router.config [Prometheus.route "/metrics"]
```

Постройте роутер из эффектного кода и передайте его обработчик
в `Http.Server.serve` внутри области. Маршрут читает текущие значения
при каждом опросе; сам он ничего не отправляет серверу Prometheus.

## DevTools

Вызов `DevTools.layer url` отправляет спаны программы, как только они
начинаются и заканчиваются, по WebSocket в Effect DevTools, слушающий
на `url` (значение Effect по умолчанию — `ws://localhost:34437`);
DevTools должны слушать заранее. Вызов `DevTools.layer_with { url;
headers }` добавляет заголовки к рукопожатию WebSocket — например,
токен, который проверяет прокси перед DevTools; их отправляет только
WebSocket в Node.

`Otlp`, `Prometheus`, `DevTools` и [RPC](../rpc/) оборачивают модули
Effect со статусом unstable; используйте согласованные версии
компилятора и среды выполнения.
