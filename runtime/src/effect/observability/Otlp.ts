// Effect's `effect/observability` OTLP exporters for the standard library:
// each layer brings the fetch client and the JSON serialization it sends
// with, so a Flick program provides it as it is.
import { Layer } from "effect";
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
