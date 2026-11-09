// Effect's `ErrorReporter` for the bindings of the standard library: each
// report goes to a Flick event listener as a record.
import { Effect, ErrorReporter, type LogLevel } from "effect";

import { type Failure, RawCause, nativeCause } from "../cause.ts";
import { inspect } from "../host.ts";
import type { Listener } from "../stdlib/callbacks.ts";

export * from "effect/ErrorReporter";

/** `ErrorReporter.Report`. */
export interface FlickReport {
  readonly cause: RawCause;
  readonly message: string;
  readonly severity: LogLevel.Severity;
  readonly attributes: ReadonlyArray<readonly [string, string]>;
  readonly timestamp: bigint;
}

const text = (value: unknown): string => (typeof value === "string" ? value : inspect(value));

/** `ErrorReporter.make`. */
export const flickMake = (listener: Listener<FlickReport>): ErrorReporter.ErrorReporter =>
  ErrorReporter.make((options) =>
    listener({
      cause: new RawCause(options.cause),
      message: options.error.message,
      severity: options.severity,
      attributes: Object.entries(options.attributes).map(([key, value]) => [key, text(value)] as const),
      timestamp: options.timestamp,
    }),
  );

/** `ErrorReporter.layer`. */
export const flickLayer = (
  reporters: ReadonlyArray<ErrorReporter.ErrorReporter>,
  options: { readonly mergeWithExisting: boolean },
) => ErrorReporter.layer(reporters, options);

/** `ErrorReporter.report`. */
export const flickReport = (failure: Failure): Effect.Effect<void> => {
  const cause = nativeCause(failure);
  return cause === undefined ? Effect.void : ErrorReporter.report(cause);
};
