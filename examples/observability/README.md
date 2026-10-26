# Observability

A shop that reports to an OpenTelemetry collector. The spans, log records
and metrics of two checkouts go out through `Otlp.layer`; an audit traces
through a tracer of its own, `Otlp.tracer`, given to one task with
`Tracer.with_tracer`.

The collector is `OTEL_EXPORTER_OTLP_ENDPOINT`, `http://localhost:4318` by
default. To see what arrives, run a collector that prints it:

```sh
docker run --rm -p 4318:4318 otel/opentelemetry-collector:latest
```

or Jaeger, whose UI then shows the traces at `http://localhost:16686`:

```sh
docker run --rm -p 4318:4318 -p 16686:16686 jaegertracing/all-in-one:latest
```

Then, in this directory:

```sh
pnpm install
flick run
```

`flick/tests/examples.rs` runs the example against a local collector in
Node and checks that every signal reaches it.
