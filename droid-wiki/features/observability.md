# Observability

Active contributors: bondiano, Vassiliy Kuzenkov. Non-bot authors on `origin/main` for standard-library application APIs and examples.

Flick applications can instrument tasks with logs, spans, and metrics, then explicitly install exporters through layers. These are application libraries, not a repository monitoring service or a compiler plugin.

## Instrumentation vocabulary

| Surface | Capability |
| - | - |
| `Tracer` | Scoped spans, parent/link relationships, attributes, tracer selection |
| `Metric` | Counters, gauges, frequencies, histograms, summaries, snapshots |
| `Logger`, `Log`, `Effect` | Logger values/layers, service-based logs, runtime log calls and formatting |
| `Http.Middleware.tracer` | Server spans that continue incoming trace context |
| `Otlp` | Batched trace/log/metric export over HTTP |
| `Prometheus` | Metric registry as scrapeable text or a router route |
| `DevTools` | Live spans sent to an already-running DevTools WebSocket |

`Tracer.with_span` wraps an `Eff r e a` while retaining its rows/result; the span ends with the task. `Tracer.with_tracer` scopes tracer selection to supplied work. A `Metric input state` separates recorded inputs from snapshot state; allocating the metric description is different from recording a measurement.

`Logger.layer` changes which loggers process messages within a provide. Formats include structured fields rather than forcing consumers to reverse-parse console strings. Logging configuration and a `Redacted` value still require an explicit disclosure policy; instrumentation is not automatically safe to export.

## Exporter lifecycle and configuration

`Otlp.layer { url; service_name }` installs traces, logs, and metrics under `/v1/traces`, `/v1/logs`, and `/v1/metrics`. Individual layers select one signal. `Otlp.Settings` controls resource attributes, service version, headers, serialization, gzip, batching, intervals, and shutdown timeout. JSON is the default; protobuf is explicitly selected.

Exporters are scope-owned: the public API documents flushing remaining batches as the layer closes. `Otlp.tracer` creates an independent tracer holding the acquisition region; it cannot be treated as a global unscoped value.

`Otlp.from_config` reads the provided `ConfigProvider` (process environment by default). Signal exporter selectors, endpoint/header/compression/timeout variables, batching variables, and service/resource names determine the configured export. Merely defining an endpoint does not select every signal.

`Prometheus.route path` returns a normal GET route; the application decides where to mount and expose it. `DevTools.layer` expects a server already listening, with `ws://localhost:34437` documented as Effect's default; `DevTools.layer_with` adds Node-only handshake headers.

`Otlp`, `Prometheus`, and `DevTools` are marked unstable in their module documentation. Their foreign binding identities also do not establish full target/backend parity; consult [runtimes](../packages/runtimes/index.md) for implementation support.

## Complete example and change entry points

`examples/observability/src/App/Main.fl` reads `OTEL_EXPORTER_OTLP_ENDPOINT` with a local default, records nested checkout/payment spans and a counter, provides an OTLP layer, then acquires an independent audit tracer inside a scope. It demonstrates instrumentation and exporter ownership; it is not evidence that a collector was contacted during wiki generation.

To change instrumentation types, start with `packages/std/src/Tracer.fl`, `packages/std/src/Metric.fl`, and `packages/std/src/Logger.fl`. For export behavior inspect `packages/std/src/Otlp.fl` plus its selected backend binding, including shutdown/error handling; for inbound HTTP context inspect `packages/std/src/Http/TraceContext.fl` and `packages/std/src/Http/Middleware.fl`.

Execution fixtures in `tests/run/stdlib/observability/` cover tracer, OTLP, configuration, exporter settings, and Prometheus behavior. Runtime changes need the relevant TypeScript/Go/conformance/e2e checks from `docs/agents/validation.md`; real collector integration should be reported separately from local fixtures. No exporter or monitoring service was started for these pages.

## Key source files

| Path | Purpose |
| - | - |
| `packages/std/src/Tracer.fl` | Span options and scoped tracing |
| `packages/std/src/Metric.fl` | Typed metric input/state APIs |
| `packages/std/src/Logger.fl` | Logger configuration, formatting, and layers |
| `packages/std/src/Otlp.fl` | Exporter settings, configuration, and lifecycle |
| `packages/std/src/Prometheus.fl` | Metric text formatting and HTTP route |
| `packages/std/src/DevTools.fl` | WebSocket exporter API |
| `examples/observability/src/App/Main.fl` | End-to-end application instrumentation |

Continue with [application features](application-features.md) or the shared [effect and lifetime rules](effects-and-regions.md).
