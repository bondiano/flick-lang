// Effect's `Config` for the bindings of the standard library, and the
// adapters where a Flick value differs from what Effect takes: a failure
// to read is the Flick error `Config.ConfigError`.
import { Config, ConfigProvider, Effect, Option, Schema } from "effect";

import { tag } from "../tags.ts";

export * from "effect/Config";

const CONFIG_ERROR = /*#__PURE__*/ tag("Config.ConfigError.ConfigError");

/** The Flick error of a failure: an Effect `ConfigError`, or one a Flick callback raised. */
const toFlick = (error: unknown): unknown =>
  error instanceof Config.ConfigError ? { _tag: CONFIG_ERROR, _0: error.message } : error;

/** `Config.get`. */
export const flickGet = <A>(config: Config.Config<A>): Effect.Effect<A, unknown> =>
  Effect.mapError(config, toFlick);

/** `Config.parse`. */
export const flickParse = <A>(provider: ConfigProvider.ConfigProvider, config: Config.Config<A>): Effect.Effect<A, unknown> =>
  Effect.mapError(config.parse(provider), toFlick);

/** `Config.or_else`: `f` gets the Flick error. */
export const flickOrElse =
  <A2>(f: (error: unknown) => Config.Config<A2>) =>
  <A>(self: Config.Config<A>): Config.Config<A | A2> =>
    Config.orElse(self, (error) => f(toFlick(error)));

/** `Config.fail`: a source error with the message. */
export const flickFail = (message: string): Config.Config<never> =>
  Config.fail(new ConfigProvider.SourceError({ message }));

/** `Config.redacted`: an effect, so only code that may run on the server describes a secret. */
export const flickRedacted = (name: string): Effect.Effect<Config.Config<unknown>> => Effect.succeed(Config.Redacted(name));

/** `Config.url`: the normalized text of the URL. */
export const flickUrl = (name: string): Config.Config<string> => Config.map(Config.URL(name), (url) => url.href);

/** `Config.int`: an integer within Flick's 32-bit `Int`. */
export const flickInt = (name: string): Config.Config<number> => Config.schema(Schema.Int.check(Schema.isInt32()), name);

/** `Config.date`: milliseconds since the epoch. */
export const flickDate = (name: string): Config.Config<number> => Config.map(Config.Date(name), (d) => d.getTime());

/** `Config.array`. */
export const flickArray = <A>(
  options: { readonly separator: Option.Option<string> },
  value: Schema.ConstraintCodec<A, unknown>,
  path: string,
): Config.Config<ReadonlyArray<A>> =>
  Config.Array(value, path, { separator: Option.getOrUndefined(options.separator) });

/** `Config.record`: string keys, as pairs. */
export const flickRecord = <V>(
  options: { readonly separator: Option.Option<string>; readonly keyValueSeparator: Option.Option<string> },
  value: Schema.ConstraintCodec<V, unknown>,
  path: string,
): Config.Config<ReadonlyArray<readonly [string, V]>> =>
  Config.map(
    Config.Record(Schema.String, value, path, {
      separator: Option.getOrUndefined(options.separator),
      keyValueSeparator: Option.getOrUndefined(options.keyValueSeparator),
    }),
    (record) => Object.entries(record),
  );
