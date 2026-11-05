---
title: "Observability"
---

A trace answers “where did this request spend its time?”; a log records
what happened; a metric answers “how often, and how much?”. Flick exposes
all three through ordinary runtime functions, with separate destinations:
`Tracer` creates spans, Effect's loggers consume log records, and `Metric`
stores numeric series in a metric registry. Logs and spans are not kept in
that registry.

Provide an OTLP or DevTools layer around the work you want to observe.
Prometheus uses a different model: mount a route that reads the metric
registry when a scraper requests it. Without an exporter, telemetry is not
sent to an OTLP collector; Effect's default logger still writes to the
console.

## Spans

`Tracer.with_span name options task` runs `task` in a new span, which
ends with the task; `Tracer.parent_span ()` is the span the current task
runs in. A span carries attributes, links to other spans and a kind as
OpenTelemetry names it (`Tracer.internal`, `server`, `client`,
`producer`, `consumer`). By default a span is a child of the current
span; `root = true` starts a trace, and `external_span { spanId;
traceId; sampled }` brings in a span of another process to use as a
parent or a link.

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

Nested `with_span` calls inherit the current span as their parent. Use a
stable operation name such as `checkout`, with request-specific details in
attributes: traces remain comparable without losing the context of a
particular call.

## Metrics

`Metric` keeps counters, gauges, frequencies, histograms and summaries
in the program's metric registry. Create a long-lived metric once — for
example, at the top level — then update it from runtime code.

| Kind | What it keeps |
| - | - |
| `counter name options` | a running total of inputs; `incremental = true` ignores negative updates, so the total cannot decrease |
| `gauge name options` | keeps the last input; `modify` adds to it instead |
| `frequency name options` | counts how often each word was seen |
| `histogram name options` | the count of values up to each boundary, and their total |
| `timer name options` | a histogram of durations, in milliseconds |
| `summary name options` | each quantile with its value, `maxAge` and `maxSize` bounding the inputs kept |

```flick
checkouts : Metric.Counter
let checkouts =
  Metric.counter "checkouts" { description = None; attributes = []; incremental = true }
```

`Metric.update input metric` records an input; equivalently,
`metric |> Metric.update input`. With `incremental = true`, updates
`2`, `3`, `-1` leave a count of `5`, not `4`: this flag is a monotonicity
rule, not a choice of delta versus cumulative export.

`Metric.value metric` reads its state; `Metric.snapshot ()` reads the
registry, and `Metric.dump ()` renders it as diagnostic text.
`with_attributes` adds a separate series for each distinct attribute set.
Keep metric attributes bounded (`status`, not a unique request ID), or the
number of series grows with traffic.
`linear_boundaries`, `exponential_boundaries` and
`boundaries_from_iterable` build histogram boundaries;
`enable_runtime_metrics` counts the fibers a task starts and ends.

## OTLP export

`Otlp` exports over HTTP, batching traces and logs and periodically reading
metrics. Closing the provided layer flushes pending data, subject to its
shutdown timeout. Keep the layer around the whole application rather than
rebuilding an exporter for each request.

The combined and single-signal layers interpret `url` differently:

| Layer | URL to supply |
| - | - |
| `layer`, `layer_with` | a base URL, such as `http://localhost:4318`; signal paths `/v1/traces`, `/v1/logs`, `/v1/metrics` are appended |
| `traces`, `traces_with` | the full trace endpoint, such as `http://localhost:4318/v1/traces` |
| `logs`, `logs_with` | the full log endpoint, ending in `/v1/logs` |
| `metrics`, `metrics_with` | the full metric endpoint, ending in `/v1/metrics` |

This function uses the span options and counter above. It exports the span;
exporting the counter as well requires `Otlp.layer` or a metrics layer.

```flick
# import App.Telemetry exposing (span_options, checkouts)
#
observe_checkout : Unit ->{} Unit
let observe_checkout () =
  provide (Otlp.traces { url = "http://localhost:4318/v1/traces"; service_name = "shop" }) {
    Tracer.with_span "checkout" span_options (task { Metric.update 1 checkouts })
  }
```

`Otlp.settings url service_name` starts with JSON, no gzip, no extra
headers or resource attributes, and Effect's batching defaults. Pass a
record update to `layer_with`, `traces_with`, `logs_with` or
`metrics_with` to set the service version, resource attributes, request
headers, serialization, export interval or shutdown timeout. `max_batch_size`
applies to traces and logs, not metrics. With `gzip = true`, bodies of at
least 1 KB are compressed. `Otlp.protobuf` selects protobuf instead of
`Otlp.json`; only programs that name it bundle that serializer.

### A tracer of a scope

`Otlp.tracer options` makes a tracer in the current scope instead of a
layer: `Tracer.with_tracer` gives it to one task, so only that task's
spans go to its service, and the spans left are sent when the scope
closes. It holds the scope's region and cannot outlive it. The URL is the
full trace endpoint, as for `traces`. `examples/observability` exports a
checkout with `Otlp.layer` and an audit with `Otlp.tracer` to a local
collector.

```flick
# import App.Telemetry exposing (span_options)
#
audit : Unit ->{} Unit
let audit () =
  scope {
    let tracer = Otlp.tracer { url = "http://localhost:4318/v1/traces"; service_name = "audit" }
    Tracer.with_tracer tracer (task { Tracer.with_span "audit" span_options Effect.void })
  }
```

### Deployment configuration

`Otlp.from_config None` reads the current `ConfigProvider` (the process
environment by default) and exports in JSON. A signal is enabled **only**
when its `OTEL_TRACES_EXPORTER`, `OTEL_LOGS_EXPORTER` or
`OTEL_METRICS_EXPORTER` names `otlp`; setting an endpoint alone does not
enable export.

The configuration also reads `OTEL_EXPORTER_OTLP_ENDPOINT`, per-signal
`OTEL_EXPORTER_OTLP_<SIGNAL>_ENDPOINT`, headers, compression and timeouts;
batch settings `OTEL_BSP_*`, `OTEL_BLRP_*` and `OTEL_METRIC_EXPORT_*`; and
resource settings `OTEL_SERVICE_NAME`, `OTEL_SERVICE_VERSION` and
`OTEL_RESOURCE_ATTRIBUTES`. `Otlp.from_config (Some "shop")` overrides the
configured service name. Use a supplied `ConfigProvider` for isolated
configuration in tests rather than mutating the process environment.

## Prometheus

`Prometheus.format ()` is the metrics a program has recorded, in
Prometheus's text format; `format_with { prefix; rename }` prefixes every
name or renames each. `Prometheus.route "/metrics"` is a route of
`Http.Router` answering GET with them and their content type, ready for
a scraper:

```flick
make_metrics_app :
  Unit ->{}
    (Http.ServerRequest.ServerRequest ->{; Http.ServerError.HttpServerError}
      Http.ServerResponse.ServerResponse)
let make_metrics_app () =
  Http.Router.router Http.Router.config [Prometheus.route "/metrics"]
```

Build the router from runtime code, then pass its handler to
`Http.Server.serve` inside a scope. The route reports current registry
values on each scrape; it does not push to a Prometheus server.

## UI actions and atom writes

Each task an action or an event handler of a mounted root runs is a span,
named by the action's label (`App.Counter.increment`; `Ui.event` for a
handler with none). Its attributes are `flick.action.policy`,
`flick.action.capacity` or `flick.action.limit` where the policy has one,
`flick.event` (`click`, `input`, ...) and `flick.fiber`. An event the policy
turns away, while a task of the action runs, is a `rejected` event of that
task's span; an interrupted task gets an `interrupted` event.

A root mounted under `References.with_minimum_log_level LogLevel.Debug`
logs every atom write at the debug level, with its cause:

```text
atom App.count ← action App.increment ← event click ← span 5f2c… (fiber #12) at App:14
```

The compiler gives each `Atom.set`, `Atom.update` and `Atom.modify` its
place, `Module:line`. The names are labels, which release builds keep with
`labels = "all"`. A write outside any action shows `task` as its cause.

## Server renders and server functions

A request's trace goes on in what serves it. Effect's HTTP server opens a
span per request, a child of the `traceparent` the request carries;
`Ui.Server.render` and `Ui.Server.stream` run as a `flick.render` span
under it. A [server function](../server-fn/) call carries the caller's
trace in `traceparent`, and the server runs it as a span named after it,
`App.Shop.place`, in that trace: a page's render and the server functions
its handler calls are one trace with the browser's request.

## DevTools

`DevTools.layer url` sends a program's spans, as they start and end, over
a WebSocket to the Effect DevTools listening at `url` (Effect's default
is `ws://localhost:34437`), which must already listen.
`DevTools.layer_with { url; headers }` adds headers to the WebSocket's
handshake — a token a proxy in front of the DevTools checks, say; only
Node's WebSocket sends them.

`flick run --devtools` and `flick dev --devtools` need no code: they
provide `DevTools.layer` to the whole of `main`, at
`ws://localhost:34437` or the URL given (`--devtools=ws://host:port`),
with the minimum log level at Debug. The DevTools then show the spans of
actions and, as events of those spans, the log of each atom write. If no
DevTools listen, the program runs as without the flag.

`Otlp`, `Prometheus`, `DevTools` and [RPC](../rpc/) wrap Effect modules
marked unstable; use matching compiler and runtime versions.
