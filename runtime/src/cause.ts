// Why a computation failed: a flat list of reasons, joined by `combine` (each
// reason once) or `concat` (repeats kept). A failing computation throws a
// `Raise` holding its cause; a typed failure and the abort of a handler are the
// same thing thrown, the second a `Fail` whose error is a `Signal`.
import { equals } from "./core/data.ts";

export interface Fail {
  readonly _tag: "Fail";
  readonly error: unknown;
}
export interface Die {
  readonly _tag: "Die";
  readonly defect: unknown;
}
export interface Interrupt {
  readonly _tag: "Interrupt";
  readonly fiberId: number | undefined;
}
export type Reason = Fail | Die | Interrupt;

export interface Cause {
  readonly reasons: ReadonlyArray<Reason>;
}

export const empty: Cause = /*#__PURE__*/ Object.freeze({ reasons: Object.freeze([]) });

export const fromReasons = (reasons: ReadonlyArray<Reason>): Cause => ({ reasons });
export const failReason = (error: unknown): Fail => ({ _tag: "Fail", error });
export const dieReason = (defect: unknown): Die => ({ _tag: "Die", defect });
export const interruptReason = (fiberId: number | undefined): Interrupt => ({ _tag: "Interrupt", fiberId });

export const fail = (error: unknown): Cause => ({ reasons: [failReason(error)] });
export const die = (defect: unknown): Cause => ({ reasons: [dieReason(defect)] });
export const interrupt = (fiberId: number | undefined): Cause => ({ reasons: [interruptReason(fiberId)] });

/** Reasons are one when a typed error or a defect's value is equal; a thrown `Error` only equals itself. */
export const sameReason = (a: Reason, b: Reason): boolean => {
  if (a._tag === "Fail") return b._tag === "Fail" && equals(a.error, b.error);
  if (a._tag === "Interrupt") return b._tag === "Interrupt" && a.fiberId === b.fiberId;
  if (b._tag !== "Die") return false;
  return a.defect instanceof Error || b.defect instanceof Error ? a.defect === b.defect : equals(a.defect, b.defect);
};

/** `c ⊕ c'`: both, each reason once; an empty one gives the other as it is. */
export const combine = (a: Cause, b: Cause): Cause => {
  if (a.reasons.length === 0) return b;
  if (b.reasons.length === 0) return a;
  const out: Array<Reason> = [];
  for (const r of [...a.reasons, ...b.reasons]) {
    if (!out.some((k) => sameReason(k, r))) out.push(r);
  }
  return { reasons: out };
};

/** `c ⧺ c'`: both, repeats kept. */
export const concat = (a: Cause, b: Cause): Cause => {
  if (a.reasons.length === 0) return b;
  if (b.reasons.length === 0) return a;
  return { reasons: [...a.reasons, ...b.reasons] };
};

export const isFail = (r: Reason): r is Fail => r._tag === "Fail";
export const isDie = (r: Reason): r is Die => r._tag === "Die";
export const isInterrupt = (r: Reason): r is Interrupt => r._tag === "Interrupt";

export const hasFails = (c: Cause): boolean => c.reasons.some(isFail);
export const hasDies = (c: Cause): boolean => c.reasons.some(isDie);
export const hasInterrupts = (c: Cause): boolean => c.reasons.some(isInterrupt);
export const hasInterruptsOnly = (c: Cause): boolean => c.reasons.length > 0 && c.reasons.every(isInterrupt);

/** `dies c`: every typed failure a defect of the same value. */
export const dies = (c: Cause): Cause => ({ reasons: c.reasons.map((r) => (isFail(r) ? dieReason(r.error) : r)) });

/** `elect c`: `dies c` when `c` has a defect and a typed failure, `c` otherwise. */
export const elect = (c: Cause): Cause => (hasDies(c) && hasFails(c) ? dies(c) : c);

/** What a failing computation throws. Not an `Error`: it captures no stack. */
export class Raise {
  readonly cause: Cause;
  constructor(cause: Cause) {
    this.cause = cause;
  }
}

/** Fails with `cause`. */
export const raise = (cause: Cause): never => {
  // oxlint-disable-next-line typescript/only-throw-error -- the one thing a failure throws: no stack to capture
  throw new Raise(cause);
};

/** The cause of what a computation threw; anything but a `Raise` is a defect. */
export const causeOf = (thrown: unknown): Cause => (thrown instanceof Raise ? thrown.cause : die(thrown));

/**
 * Where a handler of effects unwinds to, as the error of a `Fail`: the `abort`
 * arm to run there, the failure of a `tail` arm, which is the handler's and not
 * its body's, or the discontinuing of its body. No `try` and no `Exit` takes it
 * for an error.
 */
export class Signal {
  readonly token: object;
  readonly arm: (() => unknown) | undefined;
  readonly cause: Cause | undefined;
  readonly discontinue: boolean;
  constructor(token: object, arm: (() => unknown) | undefined, cause: Cause | undefined, discontinue: boolean) {
    this.token = token;
    this.arm = arm;
    this.cause = cause;
    this.discontinue = discontinue;
  }
}

/** Whether `c` unwinds to a handler of effects. */
export const unwinds = (c: Cause): boolean => c.reasons.some((r) => isFail(r) && r.error instanceof Signal);

/** The signal of the handler `token` in `c`. */
export const signalOf = (c: Cause, token: object): Signal | undefined => {
  for (const r of c.reasons) {
    if (isFail(r) && r.error instanceof Signal && r.error.token === token) return r.error;
  }
  return undefined;
};

/** The first typed failure of a cause without defects, which recovery selects. */
export const recoverable = (c: Cause): Fail | undefined =>
  hasDies(c) || unwinds(c) ? undefined : c.reasons.find(isFail);

/** How a computation ended: `cause` is undefined on success. */
export class Exit {
  readonly value: unknown;
  readonly cause: Cause | undefined;
  constructor(value: unknown, cause: Cause | undefined) {
    this.value = value;
    this.cause = cause;
  }
}

export const succeeded = (value: unknown): Exit => new Exit(value, undefined);
export const failed = (cause: Cause): Exit => new Exit(undefined, cause);
