// Effect's `Layer` for the bindings of the standard library, and the
// adapters where a Flick value differs from what Effect takes.
import { type Cause, Context, Effect, Exit, Layer, type LogLevel, Option, Scope, Tracer } from "effect";

import { Failure, RawCause } from "../cause.ts";

export * from "effect/Layer";

/** `Layer.merge_all`: the first layer and the rest. */
export const flickMergeAll = <A, E, R>(
  first: Layer.Layer<A, E, R>,
  rest: ReadonlyArray<Layer.Layer<A, E, R>>,
): Layer.Layer<A, E, R> => Layer.mergeAll(first, ...rest);

/** `Layer.tap_cause`: `f` sees a `Failure`. */
export const flickTapCause =
  <E, X, E2, R2>(f: (failure: Failure) => Effect.Effect<X, E2, R2>) =>
  <A, R>(self: Layer.Layer<A, E, R>): Layer.Layer<A, E | E2, R | Exclude<R2, Scope.Scope>> =>
    Layer.tapCause(self, (cause: Cause.Cause<E>) => f(new Failure(cause)));

/** `Layer.catch_cause`: `f` sees a `Failure`. */
export const flickCatchCause =
  <E, A2, E2, R2>(f: (failure: Failure) => Layer.Layer<A2, E2, R2>) =>
  <A, R>(self: Layer.Layer<A, E, R>): Layer.Layer<A & A2, E2, R | R2> =>
    Layer.catchCause(self, (cause: Cause.Cause<E>) => f(new Failure(cause)));

/** `Layer.build_with_memo_map`: into the scope of the call. */
export const flickBuildWithMemoMap =
  (memoMap: Layer.MemoMap) =>
  <A, E, R>(self: Layer.Layer<A, E, R>): Effect.Effect<Context.Context<A>, E, R | Scope.Scope> =>
    Effect.flatMap(Effect.scope, (scope) => Layer.buildWithMemoMap(self, memoMap, scope));

/** `Layer.make_memo_map`. */
export const flickMakeMemoMap = (): Effect.Effect<Layer.MemoMap> => Layer.makeMemoMap;

/** `Layer.current_memo_map`: forked from the context, or a new one. */
export const flickCurrentMemoMap = (): Effect.Effect<Layer.MemoMap> =>
  Effect.map(Effect.context(), (context) => Layer.CurrentMemoMap.forkOrCreate(context));

type Build<A, E, R> = (memoMap: Layer.MemoMap) => Effect.Effect<Context.Context<A>, E, R>;

const scoped =
  <A, E, R>(build: Build<A, E, R>) =>
  (memoMap: Layer.MemoMap, scope: Scope.Scope): Effect.Effect<Context.Context<A>, E, Exclude<R, Scope.Scope>> =>
    Scope.provide(build(memoMap), scope);

/** `Layer.from_build`: `build` runs with the scope of the build as its `Scope`. */
export const flickFromBuild = <A, E, R>(build: Build<A, E, R>): Layer.Layer<A, E, Exclude<R, Scope.Scope>> =>
  Layer.fromBuild(scoped(build));

/** `Layer.from_build_memo`. */
export const flickFromBuildMemo = <A, E, R>(build: Build<A, E, R>): Layer.Layer<A, E, Exclude<R, Scope.Scope>> =>
  Layer.fromBuildMemo(scoped(build));

/** `Tracer.SpanOptions` as Flick holds it. */
export interface FlickTraceOptions {
  readonly attributes: ReadonlyArray<readonly [string, string]>;
  readonly links: ReadonlyArray<{
    readonly span: Tracer.AnySpan;
    readonly attributes: ReadonlyArray<readonly [string, string]>;
  }>;
  readonly parent: Option.Option<Tracer.AnySpan>;
  readonly root: boolean;
  readonly kind_: Tracer.SpanKind;
  readonly sampled: Option.Option<boolean>;
  readonly level: Option.Option<LogLevel.LogLevel>;
  readonly captureStackTrace: boolean;
  readonly disablePropagation: boolean;
}

/** `Layer.Trace.SpanOptions`; `onEnd` arrives curried, inside an `Option`. */
export interface FlickSpanOptions {
  readonly span: FlickTraceOptions;
  readonly onEnd: Option.Option<(span: Tracer.Span) => (cause: Option.Option<RawCause>) => Effect.Effect<void>>;
}

/** Effect's span options of Flick's `Tracer.SpanOptions`. */
export const traceOptions = (s: FlickTraceOptions): Tracer.SpanOptions => ({
  attributes: Object.fromEntries(s.attributes),
  links: s.links.map((link) => ({ span: link.span, attributes: Object.fromEntries(link.attributes) })),
  parent: Option.getOrUndefined(s.parent),
  root: s.root,
  kind: s.kind_,
  sampled: Option.getOrUndefined(s.sampled),
  level: Option.getOrUndefined(s.level),
  captureStackTrace: s.captureStackTrace,
  annotations: Context.make(Tracer.DisablePropagation, s.disablePropagation),
});

const spanOptions = (options: FlickSpanOptions): Layer.SpanOptions => {
  const onEnd = Option.getOrUndefined(options.onEnd);
  return {
    ...traceOptions(options.span),
    onEnd:
      onEnd === undefined
        ? undefined
        : (span, exit) => onEnd(span)(Exit.isSuccess(exit) ? Option.none() : Option.some(new RawCause(exit.cause))),
  };
};

/** `Layer.Trace.span`. */
export const flickSpan = (name: string, options: FlickSpanOptions): Layer.Layer<Tracer.ParentSpan> =>
  Layer.span(name, spanOptions(options));

/** `Layer.Trace.with_span`. */
export const flickWithSpan =
  (name: string, options: FlickSpanOptions) =>
  <A, E, R>(self: Layer.Layer<A, E, R>): Layer.Layer<A, E, Exclude<R, Tracer.ParentSpan>> =>
    Layer.withSpan(self, name, spanOptions(options));
