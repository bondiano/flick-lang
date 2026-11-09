// Effect's `Logger` for the bindings of the standard library, and the
// adapters where a Flick value differs from what Effect takes.
import { type Duration, Effect, FileSystem, type Layer, Logger, type LogLevel, Option } from "effect";

import { RawCause } from "../cause.ts";
import type { Data } from "../data.ts";
import { inspect } from "../host.ts";
import { tag } from "@flick-lang/runtime/tags";
import { effectFileSystem, openFlag } from "./FileSystem.ts";
import { mapError } from "./PlatformError.ts";

export * from "effect/Logger";

type AnyLogger<O> = Logger.Logger<unknown, O>;

const text = (part: unknown): string => (typeof part === "string" ? part : inspect(part));

/** The parts of a message as text. */
const parts = (message: unknown): ReadonlyArray<string> =>
  Array.isArray(message) ? message.map(text) : [text(message)];

/** `Logger.make`: `f` gets `Logger.Options`. */
export const flickMake = <O>(
  f: (options: {
    readonly message: ReadonlyArray<string>;
    readonly logLevel: LogLevel.LogLevel;
    readonly cause: RawCause;
    readonly date: number;
  }) => O,
): AnyLogger<O> =>
  Logger.make((o) =>
    f({ message: parts(o.message), logLevel: o.logLevel, cause: new RawCause(o.cause), date: o.date.getTime() }),
  );

/** `Logger.format_structured`: `Logger.Structured`. */
export const flickFormatStructured = /*#__PURE__*/ Logger.map(Logger.formatStructured, (s) => ({
  level: s.level,
  fiberId: s.fiberId,
  timestamp: s.timestamp,
  message: parts(s.message),
  cause: Option.fromUndefinedOr(s.cause),
  annotations: Object.entries(s.annotations).map(([k, v]) => [k, text(v)] as const),
  spans: Object.entries(s.spans),
}));

const formatDate = (f: Option.Option<(millis: number) => string>) =>
  Option.getOrUndefined(Option.map(f, (format) => (date: Date) => format(date.getTime())));

const MODES = /*#__PURE__*/ new Map([
  [tag("Logger.PrettyMode.Auto"), "auto"],
  [tag("Logger.PrettyMode.Browser"), "browser"],
  [tag("Logger.PrettyMode.Tty"), "tty"],
] as const);

/** `Logger.console_pretty`. */
export const flickConsolePretty = (options: {
  readonly colors: Option.Option<boolean>;
  readonly formatDate: Option.Option<(millis: number) => string>;
  readonly mode: Data;
}): AnyLogger<void> =>
  Logger.consolePretty({
    colors: Option.getOrElse(options.colors, () => "auto" as const),
    formatDate: formatDate(options.formatDate),
    mode: MODES.get(options.mode._tag) ?? "auto",
  });

interface PrettyConsoleOptions {
  readonly colors: boolean;
  readonly formatDate: Option.Option<(millis: number) => string>;
}

/** `Logger.console_pretty_browser`. */
export const flickConsolePrettyBrowser = (options: PrettyConsoleOptions): AnyLogger<void> =>
  Logger.consolePrettyBrowser({ colors: options.colors, formatDate: formatDate(options.formatDate) });

/** `Logger.console_pretty_tty`. */
export const flickConsolePrettyTty = (options: PrettyConsoleOptions): AnyLogger<void> =>
  Logger.consolePrettyTty({ colors: options.colors, formatDate: formatDate(options.formatDate) });

interface LayerOptions {
  readonly mergeWithExisting: boolean;
}

/** `Logger.layer`. */
export const flickLayer = <O>(options: LayerOptions, loggers: ReadonlyArray<AnyLogger<O>>): Layer.Layer<never> =>
  Logger.layer(loggers, options);

/** `Logger.batched`, installed by a layer that owns its scope. */
export const flickBatched = <O>(
  batch: { readonly window: Duration.Duration; readonly flush: (outputs: Array<O>) => Effect.Effect<void> },
  options: LayerOptions,
  logger: AnyLogger<O>,
): Layer.Layer<never> => Logger.layer([Logger.batched(logger, batch)], options);

/** `Logger.to_file`, through the Flick `FileSystem` service, installed by a layer that owns its scope. */
export const flickToFile = (
  file: {
    readonly flag: Option.Option<Data>;
    readonly mode: Option.Option<number>;
    readonly batchWindow: Option.Option<Duration.Duration>;
  },
  options: LayerOptions,
  path: string,
  logger: AnyLogger<string>,
) =>
  Logger.layer(
    [
      Effect.flatMap(effectFileSystem, (fs) =>
        mapError(
          Effect.provideService(
            // a field set to `undefined` would replace the default flag `a+`
            Logger.toFile(logger, path, {
              ...Option.match(file.flag, { onNone: () => ({}), onSome: (flag) => ({ flag: openFlag.js(flag) }) }),
              ...Option.match(file.mode, { onNone: () => ({}), onSome: (mode) => ({ mode }) }),
              ...Option.match(file.batchWindow, { onNone: () => ({}), onSome: (batchWindow) => ({ batchWindow }) }),
            }),
            FileSystem.FileSystem,
            fs,
          ),
        ),
      ),
    ],
    options,
  );

/** `Logger.with_loggers`. */
export const flickWithLoggers =
  <O>(loggers: ReadonlyArray<AnyLogger<O>>) =>
  <A, E, R>(self: Effect.Effect<A, E, R>): Effect.Effect<A, E, R> =>
    Effect.provideService(self, Logger.CurrentLoggers, new Set(loggers));

/** `Logger.with_log_to_stderr`. */
export const flickWithLogToStderr =
  (toStderr: boolean) =>
  <A, E, R>(self: Effect.Effect<A, E, R>): Effect.Effect<A, E, R> =>
    Effect.provideService(self, Logger.LogToStderr, toStderr);

const LEVELS: Readonly<Record<string, LogLevel.Severity>> = { debug: "Debug", info: "Info", warn: "Warn", error: "Error" };

/** The `write` of `Logger.effect_log`: the `Log` level of the message as Effect's. */
export const flickLogWrite = (level: string, message: string): Effect.Effect<void> =>
  Effect.logWithLevel(LEVELS[level] ?? "Info")(message);

