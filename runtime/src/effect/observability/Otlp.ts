// Effect's `effect/observability` OTLP exporters for the standard library:
// each layer brings the fetch client and the JSON serialization it sends
// with, so a Flick program provides it as it is.
import { type Duration, Layer, Option } from "effect";
import { FetchHttpClient } from "effect/http";
import * as Otlp from "effect/observability/Otlp";
import * as OtlpLogger from "effect/observability/OtlpLogger";
import * as OtlpMetrics from "effect/observability/OtlpMetrics";
import * as OtlpSerialization from "effect/observability/OtlpSerialization";
import * as OtlpTracer from "effect/observability/OtlpTracer";

export * from "effect/observability/Otlp";

/** `Otlp.Options`. */
export interface Options {
  readonly url: string;
  readonly service_name: string;
}

const transport = /*#__PURE__*/ Layer.mergeAll(FetchHttpClient.layer, OtlpSerialization.layerJson);

const resource = (options: Options) => ({ serviceName: options.service_name });

/** `Otlp.layer`: traces, logs and metrics, sent under `url` (`/v1/traces`, …). */
export const otlpLayer = (options: Options) =>
  Layer.provide(Otlp.layer({ baseUrl: options.url, resource: resource(options) }), transport);

/** `Otlp.traces`: spans, sent to `url`. */
export const otlpTraces = (options: Options) =>
  Layer.provide(OtlpTracer.layer({ url: options.url, resource: resource(options) }), transport);

/** `Otlp.logs`: log records, sent to `url`. */
export const otlpLogs = (options: Options) =>
  Layer.provide(OtlpLogger.layer({ url: options.url, resource: resource(options) }), transport);

/** `Otlp.metrics`: metrics, sent to `url`. */
export const otlpMetrics = (options: Options) =>
  Layer.provide(OtlpMetrics.layer({ url: options.url, resource: resource(options) }), transport);

/** `Otlp.Serialization`: how the exporters write, as the layer of Effect's. */
export type Serialization = Layer.Layer<OtlpSerialization.OtlpSerialization>;

/** `Otlp.json`. */
export const otlpJson: Serialization = OtlpSerialization.layerJson;

/** `Otlp.protobuf`: smaller, and only a program that names it bundles it. */
export const otlpProtobuf: Serialization = OtlpSerialization.layerProtobuf;

/** `Otlp.Settings`. */
export interface Settings {
  readonly url: string;
  readonly service_name: string;
  readonly service_version: Option.Option<string>;
  readonly attributes: ReadonlyArray<readonly [string, string]>;
  readonly headers: ReadonlyArray<readonly [string, string]>;
  readonly gzip: boolean;
  readonly serialization: Serialization;
  readonly export_interval: Option.Option<Duration.Duration>;
  readonly max_batch_size: Option.Option<number>;
  readonly shutdown_timeout: Option.Option<Duration.Duration>;
}

/** The options every exporter of `settings` takes. */
const common = (settings: Settings) => ({
  resource: {
    serviceName: settings.service_name,
    serviceVersion: Option.getOrUndefined(settings.service_version),
    attributes: Object.fromEntries(settings.attributes),
  },
  headers: Object.fromEntries(settings.headers),
  compression: settings.gzip ? ("gzip" as const) : ("none" as const),
  shutdownTimeout: Option.getOrUndefined(settings.shutdown_timeout),
});

const sending = (settings: Settings) => Layer.mergeAll(FetchHttpClient.layer, settings.serialization);

/** `Otlp.layer_with`: traces, logs and metrics as `settings` say, under its `url`. */
export const otlpLayerWith = (settings: Settings) => {
  const interval = Option.getOrUndefined(settings.export_interval);
  const layer = Otlp.layer({
    baseUrl: settings.url,
    ...common(settings),
    maxBatchSize: Option.getOrUndefined(settings.max_batch_size),
    tracerExportInterval: interval,
    loggerExportInterval: interval,
    metricsExportInterval: interval,
  });
  return Layer.provide(layer, sending(settings));
};

/** `Otlp.traces_with`. */
export const otlpTracesWith = (settings: Settings) =>
  Layer.provide(
    OtlpTracer.layer({
      url: settings.url,
      ...common(settings),
      maxBatchSize: Option.getOrUndefined(settings.max_batch_size),
      exportInterval: Option.getOrUndefined(settings.export_interval),
    }),
    sending(settings),
  );

/** `Otlp.logs_with`. */
export const otlpLogsWith = (settings: Settings) =>
  Layer.provide(
    OtlpLogger.layer({
      url: settings.url,
      ...common(settings),
      maxBatchSize: Option.getOrUndefined(settings.max_batch_size),
      exportInterval: Option.getOrUndefined(settings.export_interval),
    }),
    sending(settings),
  );

/** `Otlp.metrics_with`. */
export const otlpMetricsWith = (settings: Settings) =>
  Layer.provide(
    OtlpMetrics.layer({
      url: settings.url,
      ...common(settings),
      exportInterval: Option.getOrUndefined(settings.export_interval),
    }),
    sending(settings),
  );
