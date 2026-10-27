// The spans the local collector got, for the SSR tracing run test.
import { received } from "../observability/receiver.mjs";

export { start, stop } from "../observability/receiver.mjs";

/** Every span the collector got, by name. */
const spans = () =>
  received()
    .split("\n")
    .filter((line) => line.startsWith("/v1/traces "))
    .flatMap((line) => JSON.parse(line.slice("/v1/traces ".length)).resourceSpans)
    .flatMap((r) => r.scopeSpans.flatMap((s) => s.spans));

/** Whether a span named `a` and one named `b` are in one trace. */
export const shares = (a, b) => {
  const all = spans();
  const traces = new Set(all.filter((s) => s.name === a).map((s) => s.traceId));
  return all.some((s) => s.name === b && traces.has(s.traceId));
};

/** The names of all the spans, sorted. */
export const names = () => [...new Set(spans().map((s) => s.name))].sort().join(", ");
