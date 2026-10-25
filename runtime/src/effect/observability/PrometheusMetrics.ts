// Effect's Prometheus exposition of the metrics a program records.
import * as PrometheusMetrics from "effect/observability/PrometheusMetrics";

export * from "effect/observability/PrometheusMetrics";

/** `Prometheus.format ()`: the metrics in the Prometheus text format. */
export const prometheusFormat = () => PrometheusMetrics.format();
