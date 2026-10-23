// Effect's `References` for the bindings of the standard library: the
// adapters that read a reference or set it around a task.
import { type Context, Effect, type LogLevel, Option, References } from "effect";

export * from "effect/References";

const setting =
  <T>(reference: Context.Reference<T>) =>
  (value: T) =>
  <A, E, R>(self: Effect.Effect<A, E, R>): Effect.Effect<A, E, R> =>
    Effect.provideService(self, reference, value);

const text = (value: unknown): string => (typeof value === "string" ? value : JSON.stringify(value) ?? String(value));

export const flickWithCurrentLogLevel = setting(References.CurrentLogLevel);
export const flickWithMinimumLogLevel = setting(References.MinimumLogLevel);
export const flickWithDisablePropagation = setting(References.DisablePropagation);
export const flickWithTracerEnabled = setting(References.TracerEnabled);
export const flickWithTracerTimingEnabled = setting(References.TracerTimingEnabled);
export const flickWithUnhandledLogLevel = (level: Option.Option<LogLevel.Severity>) =>
  setting(References.UnhandledLogLevel)(Option.getOrUndefined(level));

export const flickCurrentLogLevel = (): Effect.Effect<LogLevel.Severity> => Effect.service(References.CurrentLogLevel);
export const flickCurrentLogAnnotations = (): Effect.Effect<Array<[string, string]>> =>
  Effect.map(Effect.service(References.CurrentLogAnnotations), (annotations) =>
    Object.entries(annotations).map(([key, value]): [string, string] => [key, text(value)]),
  );
export const flickCurrentLogSpans = (): Effect.Effect<Array<string>> =>
  Effect.map(Effect.service(References.CurrentLogSpans), (spans) => spans.map(([label]) => label));
export const flickCurrentStackFrame = (): Effect.Effect<Option.Option<References.StackFrame>> =>
  Effect.map(Effect.service(References.CurrentStackFrame), Option.fromNullishOr);
export const flickTracerSpanAnnotations = (): Effect.Effect<Array<[string, string]>> =>
  Effect.map(Effect.service(References.TracerSpanAnnotations), (annotations) =>
    Object.entries(annotations).map(([key, value]): [string, string] => [key, text(value)]),
  );
