// Adapters of Effect's `Effect` for the part `Effect.Observe`: logs, spans
// and metrics around a task, with Flick's span options and string
// attributes.
import { Effect, type LogLevel, type Metric, type Scope, type Tracer } from "effect";

import type { FlickExit } from "../cause.ts";
import { describeDefect } from "../cause.ts";
import { noSuchElement } from "../effect/Cause.ts";
import { type FlickSpanOptions, flickSpanLinks, nativeSpanOptions } from "../effect/Tracer.ts";
import { flickExit } from "./outcome.ts";

type Attributes = ReadonlyArray<readonly [string, string]>;

export const flickLogWithLevel = (level: LogLevel.Severity, message: string): Effect.Effect<void> =>
  Effect.logWithLevel(level)(message);

export const flickTracer = (): Effect.Effect<Tracer.Tracer> => Effect.tracer;

export const flickCurrentSpan = () => Effect.mapError(Effect.currentSpan, noSuchElement);
export const flickCurrentParentSpan = () => Effect.mapError(Effect.currentParentSpan, noSuchElement);
export const flickSpanLinksNow = () => Effect.map(Effect.spanLinks, flickSpanLinks);

export const flickLinkSpans =
  (spans: ReadonlyArray<Tracer.AnySpan>, attributes: Attributes) =>
  <A, E, R>(self: Effect.Effect<A, E, R>) =>
    Effect.linkSpans(self, spans, Object.fromEntries(attributes));

export const flickMakeSpan = (name: string, options: FlickSpanOptions): Effect.Effect<Tracer.Span> =>
  Effect.makeSpan(name, nativeSpanOptions(options));

export const flickMakeSpanScoped = (name: string, options: FlickSpanOptions): Effect.Effect<Tracer.Span, never, Scope.Scope> =>
  Effect.makeSpanScoped(name, nativeSpanOptions(options));

export const flickUseSpan = <A, E, R>(
  name: string,
  options: FlickSpanOptions,
  f: (span: Tracer.Span) => Effect.Effect<A, E, R>,
): Effect.Effect<A, E, R> => Effect.useSpan(name, nativeSpanOptions(options), f);

export const flickWithSpanScoped =
  (name: string, options: FlickSpanOptions) =>
  <A, E, R>(self: Effect.Effect<A, E, R>) =>
    Effect.withSpanScoped(self, name, nativeSpanOptions(options));

export const flickTrack =
  <I, S, A>(metric: Metric.Metric<I, S>, f: (exit: FlickExit<A>) => I) =>
  <E, R>(self: Effect.Effect<A, E, R>) =>
    Effect.track(self, metric, (exit) => f(flickExit(exit)));

export const flickTrackDefects =
  <I, S>(metric: Metric.Metric<I, S>, f: (defect: string) => I) =>
  <A, E, R>(self: Effect.Effect<A, E, R>) =>
    Effect.trackDefects(self, metric, (defect) => f(describeDefect(defect)));
