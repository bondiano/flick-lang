// Effect's `PlatformError` for the bindings of the standard library, and the
// adapter that turns it into the Flick error `PlatformError.PlatformError`.
import { Effect, Option, PlatformError } from "effect";

import type { Tagged } from "../data.ts";
import { tag } from "@flick-lang/runtime/tags";

export * from "effect/PlatformError";

type Reason = Tagged<string, Readonly<Record<string, unknown>>>;

/** `PlatformError.PlatformError` as Flick code sees it. */
export type FlickPlatformError = Tagged<string, Reason>;

const ERROR = /*#__PURE__*/ tag("PlatformError.PlatformError.PlatformError");
const BAD_ARGUMENT = /*#__PURE__*/ tag("PlatformError.Reason.Bad");
const SYSTEM_ERROR = /*#__PURE__*/ tag("PlatformError.Reason.System");
const UNKNOWN = "UnknownError";

const text = (value: string | number | undefined): Option.Option<string> =>
  value === undefined ? Option.none() : Option.some(String(value));

/** The Flick value of an Effect `PlatformError`. */
export const toFlick = (error: PlatformError.PlatformError | PlatformError.BadArgument): FlickPlatformError => {
  const reason = error instanceof PlatformError.BadArgument ? error : error.reason;
  const common = { module_: reason.module, method: reason.method, description: text(reason.description) };
  return {
    _tag: ERROR,
    _0:
      reason._tag === "BadArgument"
        ? { _tag: BAD_ARGUMENT, _0: common }
        : {
            _tag: SYSTEM_ERROR,
            _0: {
              tag: { _tag: tag(`PlatformError.SystemErrorTag.${reason._tag === "Unknown" ? UNKNOWN : reason._tag}`) },
              ...common,
              syscall: text(reason.syscall),
              pathOrDescriptor: text(reason.pathOrDescriptor),
            },
          },
  };
};

type Platform = PlatformError.PlatformError | PlatformError.BadArgument;

const isPlatform = (e: unknown): e is Platform =>
  PlatformError.isPlatformError(e) || e instanceof PlatformError.BadArgument;

/**
 * Typed failures of `effect` that are an Effect `PlatformError`, or the
 * bare `BadArgument` of `Path`, become the Flick error.
 */
export const mapError = <A, E, R>(effect: Effect.Effect<A, E, R>): Effect.Effect<A, Exclude<E, Platform> | FlickPlatformError, R> =>
  Effect.mapError(effect, (e) =>
    // oxlint-disable-next-line typescript/no-unsafe-type-assertion -- the guard just excluded both error classes
    isPlatform(e) ? toFlick(e) : (e as Exclude<E, Platform>),
  );
