// Effect's `Tracer` for the bindings of the standard library, and the
// adapters that read spans and run tasks in them.
import { Context, Effect, type LogLevel, Option, Tracer } from "effect";

import { inspect } from "../host.ts";
import { tag } from "../tags.ts";

export * from "effect/Tracer";

export interface FlickLink {
  readonly span: Tracer.AnySpan;
  readonly attributes: ReadonlyArray<readonly [string, string]>;
}

const text = (value: unknown): string => (typeof value === "string" ? value : inspect(value));

/** `Tracer.SpanOptions` as Flick holds it. */
export interface FlickSpanOptions {
  readonly attributes: ReadonlyArray<readonly [string, string]>;
  readonly links: ReadonlyArray<FlickLink>;
  readonly parent: Option.Option<Tracer.AnySpan>;
  readonly root: boolean;
  readonly kind_: Tracer.SpanKind;
  readonly sampled: Option.Option<boolean>;
  readonly level: Option.Option<LogLevel.LogLevel>;
  readonly captureStackTrace: boolean;
  readonly disablePropagation: boolean;
}

/** Effect's span options of a Flick `Tracer.SpanOptions`. */
export const nativeSpanOptions = (options: FlickSpanOptions): Tracer.SpanOptions => ({
  attributes: Object.fromEntries(options.attributes),
  links: options.links.map((link) => ({ span: link.span, attributes: Object.fromEntries(link.attributes) })),
  parent: Option.getOrUndefined(options.parent),
  root: options.root,
  kind: options.kind_,
  sampled: Option.getOrUndefined(options.sampled),
  level: Option.getOrUndefined(options.level),
  captureStackTrace: options.captureStackTrace,
  annotations: Context.make(Tracer.DisablePropagation, options.disablePropagation),
});

/** `Tracer.with_span`. */
export const flickWithSpan =
  (name: string, options: FlickSpanOptions) =>
  <A, E, R>(self: Effect.Effect<A, E, R>) =>
    Effect.withSpan(self, name, nativeSpanOptions(options));

/** The links of spans as Flick holds them. */
export const flickSpanLinks = (links: ReadonlyArray<Tracer.SpanLink>): Array<FlickLink> =>
  links.map((link) => ({ span: link.span, attributes: Object.entries(link.attributes).map(([k, v]) => [k, text(v)] as const) }));

/** `Tracer.parent_span`. */
export const flickParentSpan = (): Effect.Effect<Option.Option<Tracer.AnySpan>> => Effect.serviceOption(Tracer.ParentSpan);

/** `Tracer.external_span`. */
export const flickExternalSpan = (options: {
  readonly spanId: string;
  readonly traceId: string;
  readonly sampled: boolean;
}): Tracer.AnySpan => Tracer.externalSpan(options);

export const flickSpanId = (span: Tracer.AnySpan): string => span.spanId;
export const flickTraceId = (span: Tracer.AnySpan): string => span.traceId;
export const flickSampled = (span: Tracer.AnySpan): boolean => span.sampled;

const native = (span: Tracer.AnySpan): Option.Option<Tracer.Span> =>
  span._tag === "Span" ? Option.some(span) : Option.none();

export const flickSpanName = (span: Tracer.AnySpan) => Option.map(native(span), (s) => s.name);
export const flickSpanKind = (span: Tracer.AnySpan) => Option.map(native(span), (s) => s.kind);
export const flickParent = (span: Tracer.AnySpan) => Option.flatMap(native(span), (s) => s.parent);

export const flickLinks = (span: Tracer.AnySpan): ReadonlyArray<FlickLink> =>
  Option.match(native(span), {
    onNone: () => [],
    onSome: (s) =>
      s.links.map((link) => ({
        span: link.span,
        attributes: Object.entries(link.attributes).map(([k, v]) => [k, text(v)] as const),
      })),
  });

const STARTED = tag("Tracer.SpanStatus.Started");
const ENDED = tag("Tracer.SpanStatus.Ended");

/** `Tracer.status`: an external span counts as running since 0. */
export const flickStatus = (span: Tracer.AnySpan) =>
  Option.match(native(span), {
    onNone: () => ({ _tag: STARTED, _0: 0n }),
    onSome: ({ status }) =>
      status._tag === "Started"
        ? { _tag: STARTED, _0: status.startTime }
        : { _tag: ENDED, _0: status.startTime, _1: status.endTime },
  });

export const flickAttributes = (span: Tracer.AnySpan): ReadonlyArray<readonly [string, string]> =>
  Option.match(native(span), {
    onNone: () => [],
    onSome: (s) => [...s.attributes].map(([k, v]) => [k, text(v)] as const),
  });

const provide =
  <I, S>(reference: Context.Key<I, S>, value: S) =>
  <A, E, R>(self: Effect.Effect<A, E, R>): Effect.Effect<A, E, R> =>
    Effect.provideService(self, reference, value);

/** `Tracer.with_tracer`. */
export const flickWithTracer = (tracer: Tracer.Tracer) => provide(Tracer.Tracer, tracer);

/** `Tracer.with_trace_level`. */
export const flickWithTraceLevel = (level: LogLevel.LogLevel) => provide(Tracer.CurrentTraceLevel, level);

/** `Tracer.with_minimum_trace_level`. */
export const flickWithMinimumTraceLevel = (level: LogLevel.LogLevel) => provide(Tracer.MinimumTraceLevel, level);
