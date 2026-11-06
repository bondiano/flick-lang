// Outcomes as Flick sees them: `try … with` recovers the first typed
// failure of a cause, and `Exit`, `Failure` and `RawCause` keep the native
// cause opaque. A `CauseReport` is inert data: building it never runs code
// of a payload (getters, `toString`, `toJSON`, Proxy traps).
import type { types as NodeTypes } from "node:util";

import { Cause, Effect, Exit, type Option, Result } from "effect";

import { none, some } from "./data.ts";
import { nodeUtil } from "./host.ts";
import { constructorName } from "./tags.ts";

// a Proxy is told apart only on Node.js; elsewhere a payload is read by its descriptors anyway
const types: Pick<typeof NodeTypes, "isProxy" | "isNativeError"> = /*#__PURE__*/ nodeUtil()?.types ?? {
  isProxy: (_value: unknown): _value is never => false,
  isNativeError: (value: unknown): value is Error => value instanceof Error,
};

const failsToDies = <E>(cause: Cause.Cause<E>): Cause.Cause<never> =>
  Cause.fromReasons(cause.reasons.map((r) => (Cause.isFailReason(r) ? Cause.makeDieReason(r.error) : r)).filter(
    (r): r is Cause.Die | Cause.Interrupt => !Cause.isFailReason(r),
  ));

/**
 * Where a handler of effects unwinds to (`handlers.ts`): the `abort` arm to
 * run there, or the typed failure of a `tail` arm, which is the handler's,
 * not its body's. No `try` and no `Exit` takes it for an error.
 */
export class HandlerSignal {
  readonly token: object;
  readonly arm: Effect.Effect<unknown, unknown, unknown> | undefined;
  readonly error: unknown;
  constructor(token: object, arm: Effect.Effect<unknown, unknown, unknown> | undefined, error?: unknown) {
    this.token = token;
    this.arm = arm;
    this.error = error;
    Object.freeze(this);
  }
}

/** Whether a typed failure is a handler unwinding, not an error. */
export const isHandlerSignal = (error: unknown): error is HandlerSignal => error instanceof HandlerSignal;

/** Whether `cause` unwinds to a handler of effects. */
export const unwinds = (cause: Cause.Cause<unknown>): boolean =>
  cause.reasons.some((r) => Cause.isFailReason(r) && isHandlerSignal(r.error));

/**
 * `cause` again, unchanged, where only a handler of effects takes it: it
 * is no error of the code it passes, so it has the type of none.
 */
export const passOn = (cause: Cause.Cause<unknown>): Effect.Effect<never> =>
  // oxlint-disable-next-line typescript/no-unsafe-type-assertion -- what it fails with is a handler's, of no error type of the code
  Effect.failCause(cause as Cause.Cause<never>);

/**
 * The typed error recovery may handle: the first failure of a cause without
 * defects. A defect blocks recovery, and so does unwinding to a handler;
 * interruption alone has no error.
 */
const recoverable = <E>(cause: Cause.Cause<E>): Cause.Fail<E> | undefined =>
  Cause.hasDies(cause) || unwinds(cause) ? undefined : cause.reasons.find(Cause.isFailReason);

type Selected<B> = { readonly die: Cause.Cause<never> } | { readonly handled: B };

/**
 * `try body with arms`. `handler` returns the handling effect for a typed
 * error an arm matches, `undefined` otherwise. A cause that also holds a
 * defect is not recovered and its failures become defects; without a
 * matching arm the original cause propagates.
 */
export const tryWith = <A, E, R, A2, E2, R2>(
  body: Effect.Effect<A, E, R>,
  handler: (error: E) => Effect.Effect<A2, E2, R2> | undefined,
): Effect.Effect<A | A2, E | E2, R | R2> =>
  Effect.catchCauseFilter(
    body,
    (cause): Result.Result<Selected<Effect.Effect<A2, E2, R2>>, Cause.Cause<E>> => {
      if (Cause.hasDies(cause)) {
        return Cause.hasFails(cause) ? Result.succeed({ die: failsToDies(cause) }) : Result.fail(cause);
      }
      const reason = recoverable(cause);
      const handled = reason === undefined ? undefined : handler(reason.error);
      return handled === undefined ? Result.fail(cause) : Result.succeed({ handled });
    },
    (selected) => ("die" in selected ? Effect.failCause(selected.die) : selected.handled),
  );

const causes = /*#__PURE__*/ new WeakMap<object, Cause.Cause<unknown>>();

const causeOf = (value: object): Cause.Cause<unknown> => {
  const cause = causes.get(value);
  if (cause === undefined) throw new TypeError("not a Failure or RawCause of this runtime");
  return cause;
};

/** `Failure e`: how a computation failed, as an opaque native cause. */
export class Failure {
  /** Whether it is the interruption of a closing owner. */
  readonly shutdown: boolean;
  constructor(cause: Cause.Cause<unknown>, shutdown = false) {
    causes.set(this, cause);
    this.shutdown = shutdown;
    Object.freeze(this);
  }
}

/** `RawCause`: the whole native cause, for diagnostics only. */
// oxlint-disable-next-line typescript/no-extraneous-class -- the class gives raw causes an identity of their own
export class RawCause {
  constructor(cause: Cause.Cause<unknown>) {
    causes.set(this, cause);
    Object.freeze(this);
  }
}

/** `Exit e a`: success with its value, or a `Failure`. */
export class FlickExit<A> {
  readonly #exit: Exit.Exit<A, unknown>;
  constructor(exit: Exit.Exit<A, unknown>) {
    this.#exit = exit;
    Object.freeze(this);
  }
  static value<A>(exit: FlickExit<A>): Option.Option<A> {
    return Exit.isSuccess(exit.#exit) ? some(exit.#exit.value) : none;
  }
  static failure<A>(exit: FlickExit<A>): Option.Option<Failure> {
    return Exit.isFailure(exit.#exit) ? some(new Failure(exit.#exit.cause)) : none;
  }
}

/** The exit of `effect`, which itself never fails but where it unwinds to a handler. */
export const exitOf = <A, E, R>(effect: Effect.Effect<A, E, R>): Effect.Effect<FlickExit<A>, never, R> =>
  Effect.flatMap(Effect.exit(effect), (exit) =>
    Exit.isFailure(exit) && unwinds(exit.cause)
      ? passOn(exit.cause)
      : Effect.succeed(new FlickExit(exit)),
  );
export const exitValue = <A>(exit: FlickExit<A>): Option.Option<A> => FlickExit.value(exit);
export const exitFailure = <A>(exit: FlickExit<A>): Option.Option<Failure> => FlickExit.failure(exit);

/** `Failure.primary`: the typed error recovery would select. */
export const failurePrimary = (failure: Failure): Option.Option<unknown> => {
  const reason = recoverable(causeOf(failure));
  return reason === undefined ? none : some(reason.error);
};

export const failureRawCause = (failure: Failure): RawCause => new RawCause(causeOf(failure));

/** The native cause of a `Failure`, for the runtime only. */
export const failureCause = (failure: Failure): Cause.Cause<unknown> => causeOf(failure);

/** `Failure.rethrow`: fails with the original cause, unchanged. */
export const failureRethrow = (failure: Failure): Effect.Effect<never, unknown> => Effect.failCause(causeOf(failure));

/** The native cause behind a `Failure` or `RawCause`, for the adapter itself. */
export const nativeCause = (value: Failure | RawCause): Cause.Cause<unknown> | undefined => causes.get(value);

const LIMIT = 200;

const text = (s: string): string => (s.length > LIMIT ? `${s.slice(0, LIMIT)}…` : s);

/** A data property of `value` read without running code of it. */
const own = (value: object, key: string): string | undefined => {
  const descriptor = Object.getOwnPropertyDescriptor(value, key);
  return descriptor && "value" in descriptor && typeof descriptor.value === "string" ? text(descriptor.value) : undefined;
};

/** What can be said of a payload without running any code of it. */
interface Described {
  readonly value?: string;
  readonly error?: true;
  readonly message?: string;
  readonly tag?: string | undefined;
}

const describe = (payload: unknown): Described => {
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
      if (typeof payload !== "object" || payload === null) return { value: "null" };
      if (types.isProxy(payload)) return { value: "<proxy>" };
      if (types.isNativeError(payload)) {
        return { error: true, message: own(payload, "message") ?? "<error>" };
      }
      return { tag: own(payload, "_tag"), value: "<object>" };
  }
};

/** A defect as inert text: the message of an error, a primitive, or the constructor of a tagged value. */
export const describeDefect = (defect: unknown): string => {
  const d = describe(defect);
  if (d.error) return d.message ?? "<error>";
  return d.tag === undefined ? (d.value ?? "") : constructorName(d.tag);
};

/** One reason of a `CauseReport`. */
export type ReasonReport =
  | (Described & { readonly kind: "fail" | "die" })
  | { readonly kind: "interrupt"; readonly fiber: number | null };

/** `CauseReport`: what a cause holds, as inert data. */
export interface CauseReport {
  readonly reasons: ReadonlyArray<ReasonReport>;
}

/** `fields` as an object without a prototype: nothing of it runs code when read. */
const inert = <T extends object>(fields: T): Readonly<T> => {
  const out: T = { ...fields };
  Object.setPrototypeOf(out, null);
  return Object.freeze(out);
};

/** A `CauseReport` of a native cause: kinds, safe tags and texts. */
const report = (cause: Cause.Cause<unknown>): CauseReport =>
  inert({
    reasons: Object.freeze(
      cause.reasons.map((r): ReasonReport => {
        if (Cause.isFailReason(r)) return inert({ kind: "fail" as const, ...describe(r.error) });
        if (Cause.isDieReason(r)) return inert({ kind: "die" as const, ...describe(r.defect) });
        return inert({ kind: "interrupt" as const, fiber: r.fiberId ?? null });
      }),
    ),
  });

export const failureInspect = (failure: Failure): Effect.Effect<CauseReport> => Effect.sync(() => report(causeOf(failure)));
export const rawCauseInspect = (raw: RawCause): Effect.Effect<CauseReport> => Effect.sync(() => report(causeOf(raw)));

/** One line of a `CauseReport`: the reasons, with what can be said of each. */
export const describeReport = (r: CauseReport): string =>
  r.reasons
    .map((reason) => {
      if (reason.kind === "interrupt") return "interrupt";
      const shown = reason.tag === undefined ? reason.value : constructorName(reason.tag);
      const what = reason.error ? `error ${reason.message ?? ""}` : (shown ?? "");
      return `${reason.kind} ${what}`;
    })
    .join("; ");

/** `describeReport` of a native cause, for test results and reporters. */
export const describeCause = (cause: Cause.Cause<unknown>): string => describeReport(report(cause));
