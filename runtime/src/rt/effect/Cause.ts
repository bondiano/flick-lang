// The bindings of `Cause` on the kernel, over Flick's opaque `Failure e`.
import { err, none, ok, type Option, type Result, some } from "../../core/data.ts";
import { describeDefect } from "../describe.ts";
import {
  type Cause,
  type Die,
  type Fail,
  type Interrupt,
  type Reason,
  combine,
  die,
  dieReason,
  empty,
  fail,
  failReason,
  fromReasons,
  hasDies,
  hasFails,
  hasInterrupts,
  hasInterruptsOnly,
  interrupt,
  interruptReason,
  isDie,
  isFail,
  isInterrupt,
} from "../cause.ts";
import { pretty } from "../entry.ts";
import { Failure } from "../outcome.ts";

const optional = <A>(option: Option<A>): A | undefined => (option._tag === "Some" ? option.value : undefined);

export const flickEmpty: Failure = /*#__PURE__*/ new Failure(empty);
export const flickFail = (error: unknown): Failure => new Failure(fail(error));
export const flickDie = (defect: string): Failure => new Failure(die(defect));
export const flickInterrupt = (fiberId: Option<number>): Failure => new Failure(interrupt(optional(fiberId)));
export const flickFromReasons = (reasons: ReadonlyArray<Reason>): Failure => new Failure(fromReasons(reasons));

/**
 * The reasons of a failure as `Failure e` types them: only its first typed
 * failure has an error of `e`; a later one is a defect here.
 */
export const flickReasons = (failure: Failure): ReadonlyArray<Reason> => {
  let head = true;
  return failure.cause.reasons.map((reason) => {
    if (!isFail(reason)) return reason;
    if (head) {
      head = false;
      return reason;
    }
    return dieReason(reason.error);
  });
};

/** The cause with its first typed error mapped by `f`: the later ones have no type to map from. */
export const mapHead = (cause: Cause, f: (error: unknown) => unknown): Cause => {
  let head = true;
  return fromReasons(
    cause.reasons.map((reason) => {
      if (!head || !isFail(reason)) return reason;
      head = false;
      return failReason(f(reason.error));
    }),
  );
};

export const makeFailReason = failReason;
export const makeDieReason = dieReason;
export const flickMakeInterruptReason = (fiberId: Option<number>): Interrupt => interruptReason(optional(fiberId));
export const isFailReason = isFail;
export const isDieReason = isDie;
export const isInterruptReason = isInterrupt;

export const flickHasInterruptsOnly = (failure: Failure): boolean => hasInterruptsOnly(failure.cause);
export const flickHasFails = (failure: Failure): boolean => hasFails(failure.cause);
export const flickHasDies = (failure: Failure): boolean => hasDies(failure.cause);
export const flickHasInterrupts = (failure: Failure): boolean => hasInterrupts(failure.cause);

export const flickMap =
  (f: (error: unknown) => unknown) =>
  (failure: Failure): Failure =>
    new Failure(mapHead(failure.cause, f));
export const flickCombine =
  (that: Failure) =>
  (failure: Failure): Failure =>
    new Failure(combine(failure.cause, that.cause));

export const flickFindFail = (failure: Failure): Result<Fail, Failure> => {
  const reason = failure.cause.reasons.find(isFail);
  return reason === undefined ? err(failure) : ok(reason);
};
export const flickFindError = (failure: Failure): Result<unknown, Failure> => {
  const reason = failure.cause.reasons.find(isFail);
  return reason === undefined ? err(failure) : ok(reason.error);
};
export const flickFindErrorOption = (failure: Failure): Option<unknown> => {
  const reason = failure.cause.reasons.find(isFail);
  return reason === undefined ? none : some(reason.error);
};
export const flickFindDie = (failure: Failure): Result<Die, Failure> => {
  const reason = failure.cause.reasons.find(isDie);
  return reason === undefined ? err(failure) : ok(reason);
};
export const flickFindDefect = (failure: Failure): Result<string, Failure> => {
  const reason = failure.cause.reasons.find(isDie);
  return reason === undefined ? err(failure) : ok(describeDefect(reason.defect));
};
export const flickFindInterrupt = (failure: Failure): Result<Interrupt, Failure> => {
  const reason = failure.cause.reasons.find(isInterrupt);
  return reason === undefined ? err(failure) : ok(reason);
};
const interruptors = (cause: Cause): Array<number> => {
  const ids = new Set<number>();
  for (const r of cause.reasons) if (isInterrupt(r) && r.fiberId !== undefined) ids.add(r.fiberId);
  return [...ids];
};
export const flickInterruptors = (failure: Failure): Array<number> => interruptors(failure.cause);
export const flickFilterInterruptors = (failure: Failure): Result<Array<number>, Failure> =>
  hasInterrupts(failure.cause) ? ok(interruptors(failure.cause)) : err(failure);

export const flickPretty = (failure: Failure): string => pretty(failure.cause);
export const flickPrettyErrors = (failure: Failure): Array<string> =>
  failure.cause.reasons.flatMap((r) => (isFail(r) ? [String(r.error)] : isDie(r) ? [describeDefect(r.defect)] : []));
