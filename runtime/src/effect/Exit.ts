// The bindings of `Exit` on the kernel: Flick's opaque `Exit e a` is the kernel's.
import { err, none, ok, type Option, type Result, some } from "../core/data.ts";
import { type Exit, concat, die, empty, fail, failed, hasDies, hasFails, hasInterrupts, interrupt, isDie, isFail, succeeded } from "../cause.ts";
import { describeDefect } from "../describe.ts";
import { Failure } from "../outcome.ts";
import { mapHead } from "./Cause.ts";

export const flickSucceed = succeeded;
export const flickFail = (error: unknown): Exit => failed(fail(error));
export const flickFailCause = (failure: Failure): Exit => failed(failure.cause);
export const flickDie = (defect: string): Exit => failed(die(defect));
export const flickInterrupt = (fiberId: Option<number>): Exit =>
  failed(interrupt(fiberId._tag === "Some" ? fiberId.value : undefined));
export const flickVoid: Exit = /*#__PURE__*/ succeeded(undefined);

export const flickIsSuccess = (exit: Exit): boolean => exit.cause === undefined;
export const flickIsFailure = (exit: Exit): boolean => exit.cause !== undefined;
export const flickHasFails = (exit: Exit): boolean => exit.cause !== undefined && hasFails(exit.cause);
export const flickHasDies = (exit: Exit): boolean => exit.cause !== undefined && hasDies(exit.cause);
export const flickHasInterrupts = (exit: Exit): boolean => exit.cause !== undefined && hasInterrupts(exit.cause);

export const flickFilterSuccess = (exit: Exit): Result<Exit, Exit> => (exit.cause === undefined ? ok(exit) : err(exit));
export const flickFilterValue = (exit: Exit): Result<unknown, Exit> => (exit.cause === undefined ? ok(exit.value) : err(exit));
export const flickFilterFailure = (exit: Exit): Result<Exit, Exit> => (exit.cause === undefined ? err(exit) : ok(exit));
export const flickFilterCause = (exit: Exit): Result<Failure, Exit> =>
  exit.cause === undefined ? err(exit) : ok(new Failure(exit.cause));
export const flickFindError = (exit: Exit): Result<unknown, Exit> => {
  const reason = exit.cause?.reasons.find(isFail);
  return reason === undefined ? err(exit) : ok(reason.error);
};
export const flickFindErrorOption = (exit: Exit): Option<unknown> => {
  const reason = exit.cause?.reasons.find(isFail);
  return reason === undefined ? none : some(reason.error);
};
export const flickFindDefect = (exit: Exit): Result<string, Exit> => {
  const reason = exit.cause?.reasons.find(isDie);
  return reason === undefined ? err(exit) : ok(describeDefect(reason.defect));
};

export const flickMatch =
  (onSuccess: (value: unknown) => unknown, onFailure: (failure: Failure) => unknown) =>
  (exit: Exit): unknown =>
    exit.cause === undefined ? onSuccess(exit.value) : onFailure(new Failure(exit.cause));
export const flickMap =
  (f: (value: unknown) => unknown) =>
  (exit: Exit): Exit =>
    exit.cause === undefined ? succeeded(f(exit.value)) : exit;
export const flickMapError =
  (f: (error: unknown) => unknown) =>
  (exit: Exit): Exit =>
    exit.cause === undefined ? exit : failed(mapHead(exit.cause, f));
export const flickMapBoth =
  (onFailure: (error: unknown) => unknown, onSuccess: (value: unknown) => unknown) =>
  (exit: Exit): Exit =>
    exit.cause === undefined ? succeeded(onSuccess(exit.value)) : failed(mapHead(exit.cause, onFailure));
export const flickAsVoid = (exit: Exit): Exit => (exit.cause === undefined ? flickVoid : exit);
export const flickAsVoidAll = (exits: ReadonlyArray<Exit>): Exit => {
  const cause = exits.reduce((c, e) => (e.cause === undefined ? c : concat(c, e.cause)), empty);
  return cause.reasons.length === 0 ? flickVoid : failed(cause);
};
