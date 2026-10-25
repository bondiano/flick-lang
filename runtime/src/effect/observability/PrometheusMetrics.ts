// Effect's Prometheus exposition of the metrics a program records.
import { Option } from "effect";
import * as PrometheusMetrics from "effect/observability/PrometheusMetrics";

export * from "effect/observability/PrometheusMetrics";

/** `Prometheus.format ()`: the metrics in the Prometheus text format. */
export const prometheusFormat = () => PrometheusMetrics.format();

/** `Prometheus.format_with options ()`: with a prefix for every name, or names renamed. */
export const prometheusFormatWith = (
  options: {
    readonly prefix: Option.Option<string>;
    readonly rename: Option.Option<(name: string) => string>;
  },
  _unit: unknown,
) =>
  PrometheusMetrics.format({
    prefix: Option.getOrUndefined(options.prefix),
    metricNameMapper: Option.getOrUndefined(options.rename),
  });
