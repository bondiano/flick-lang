// Outcomes as Flick sees them: `try … with` recovers the first typed
// failure of a cause, and `Exit`, `Failure` and `RawCause` keep the native
// cause opaque. A `CauseReport` is inert data: building it never runs code
// of a payload (getters, `toString`, `toJSON`, Proxy traps).
import { types } from "node:util";

import { Cause, Effect, Exit, Result } from "effect";

import { none, some } from "./data.js";

const failsToDies = (cause) =>
  Cause.fromReasons(
    cause.reasons.map((r) => (Cause.isFailReason(r) ? Cause.makeDieReason(r.error) : r)),
  );

/**
 * The typed error recovery may handle: the first failure of a cause without
 * defects. A defect blocks recovery; interruption alone has no error.
 */
const recoverable = (cause) => {
  if (Cause.hasDies(cause)) return undefined;
  return cause.reasons.find(Cause.isFailReason);
};

/**
 * `try body with arms`. `handler` returns the handling effect for a typed
 * error an arm matches, `undefined` otherwise. A cause that also holds a
 * defect is not recovered and its failures become defects; without a
 * matching arm the original cause propagates.
 */
export const tryWith = (body, handler) =>
  Effect.catchCauseFilter(
    body,
    (cause) => {
      if (Cause.hasDies(cause)) {
        return Cause.hasFails(cause)
          ? Result.succeed({ die: failsToDies(cause) })
          : Result.fail(cause);
      }
      const reason = recoverable(cause);
      const handled = reason === undefined ? undefined : handler(reason.error);
      return handled === undefined ? Result.fail(cause) : Result.succeed({ handled });
    },
    (selected) => (selected.die ? Effect.failCause(selected.die) : selected.handled),
  );

const causes = new WeakMap();

/** `Failure e`: how a computation failed, as an opaque native cause. */
export class Failure {
  constructor(cause, shutdown = false) {
    causes.set(this, cause);
    /** Whether it is the interruption of a closing owner. */
    this.shutdown = shutdown;
    Object.freeze(this);
  }
}

/** `RawCause`: the whole native cause, for diagnostics only. */
export class RawCause {
  constructor(cause) {
    causes.set(this, cause);
    Object.freeze(this);
  }
}

/** `Exit e a`: success with its value, or a `Failure`. */
export class FlickExit {
  #exit;
  constructor(exit) {
    this.#exit = exit;
    Object.freeze(this);
  }
  static value(exit) {
    return Exit.isSuccess(exit.#exit) ? some(exit.#exit.value) : none;
  }
  static failure(exit) {
    return Exit.isFailure(exit.#exit) ? some(new Failure(exit.#exit.cause)) : none;
  }
}

/** The exit of `effect`, which itself never fails. */
export const exitOf = (effect) => Effect.map(Effect.exit(effect), (exit) => new FlickExit(exit));
export const exitValue = (exit) => FlickExit.value(exit);
export const exitFailure = (exit) => FlickExit.failure(exit);

/** `Failure.primary`: the typed error recovery would select. */
export const failurePrimary = (failure) => {
  const reason = recoverable(causes.get(failure));
  return reason === undefined ? none : some(reason.error);
};

export const failureRawCause = (failure) => new RawCause(causes.get(failure));

/** `Failure.rethrow`: fails with the original cause, unchanged. */
export const failureRethrow = (failure) => Effect.failCause(causes.get(failure));

/** The native cause behind a `Failure` or `RawCause`, for the adapter itself. */
export const nativeCause = (value) => causes.get(value);

const LIMIT = 200;

const text = (s) => (s.length > LIMIT ? `${s.slice(0, LIMIT)}…` : s);

/** A data property of `value` read without running code of it. */
const own = (value, key) => {
  const descriptor = Object.getOwnPropertyDescriptor(value, key);
  return descriptor && "value" in descriptor && typeof descriptor.value === "string"
    ? text(descriptor.value)
    : undefined;
};

/** What can be said of a payload without running any code of it. */
const describe = (payload) => {
  switch (typeof payload) {
    case "string":
      return { value: text(payload) };
    case "number":
    case "boolean":
    case "bigint":
    case "undefined":
      return { value: String(payload) };
    case "symbol":
      return { value: "<symbol>" };
    case "function":
      return { value: "<function>" };
    default:
      if (payload === null) return { value: "null" };
      if (types.isProxy(payload)) return { value: "<proxy>" };
      if (types.isNativeError(payload)) {
        return { error: true, message: own(payload, "message") ?? "<error>" };
      }
      return { tag: own(payload, "_tag"), value: "<object>" };
  }
};

const inert = (fields) => Object.freeze(Object.assign(Object.create(null), fields));

/** A `CauseReport` of a native cause: kinds, safe tags and texts. */
const report = (cause) =>
  inert({
    reasons: Object.freeze(
      cause.reasons.map((r) => {
        if (Cause.isFailReason(r)) return inert({ kind: "fail", ...describe(r.error) });
        if (Cause.isDieReason(r)) return inert({ kind: "die", ...describe(r.defect) });
        return inert({ kind: "interrupt", fiber: r.fiberId ?? null });
      }),
    ),
  });

export const failureInspect = (failure) => Effect.sync(() => report(causes.get(failure)));
export const rawCauseInspect = (raw) => Effect.sync(() => report(causes.get(raw)));

/** One line of a `CauseReport`: the reasons, with what can be said of each. */
export const describeReport = (r) =>
  r.reasons
    .map((reason) => {
      if (reason.kind === "interrupt") return "interrupt";
      const what = reason.error ? `error ${reason.message}` : (reason.tag ?? reason.value);
      return `${reason.kind} ${what}`;
    })
    .join("; ");

/** `describeReport` of a native cause, for test results and reporters. */
export const describeCause = (cause) => describeReport(report(cause));
