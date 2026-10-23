// Effect's `HttpTraceContext` for the bindings of the standard library.
import { Option, type Tracer } from "effect";
import * as Headers from "effect/http/Headers";

export * from "effect/http/HttpTraceContext";

/**
 * `Http.TraceContext.to_headers`: Effect's `toHeaders`, also for an external
 * span, which has no parent.
 */
export const flickToHeaders = (span: Tracer.AnySpan): Headers.Headers => {
  const parent = span._tag === "Span" ? Option.map(span.parent, (p) => `-${p.spanId}`) : Option.none();
  return Headers.fromInput({
    b3: `${span.traceId}-${span.spanId}-${span.sampled ? "1" : "0"}${Option.getOrElse(parent, () => "")}`,
    traceparent: `00-${span.traceId.padStart(32, "0")}-${span.spanId}-${span.sampled ? "01" : "00"}`,
  });
};
