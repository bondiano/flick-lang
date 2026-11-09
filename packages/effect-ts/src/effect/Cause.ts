// Effect's `Cause` for the bindings of the standard library, and the
// adapters over Flick's opaque `Failure e`, which holds a native cause.
import { Cause, Context, Option, Result } from "effect";
import type { StackFrame } from "effect/References";

import { Failure, describeDefect } from "../cause.ts";
import type { Tagged } from "../data.ts";
import { failureCause } from "../stdlib/outcome.ts";
import { tag } from "@flick-lang/runtime/tags";

export * from "effect/Cause";

/** A Flick `Failure` of a native cause. */
export const toFailure = (cause: Cause.Cause<unknown>): Failure => new Failure(cause);

/** The native cause of a Flick `Failure`. */
export const causeOf = <E>(failure: Failure): Cause.Cause<E> => failureCause<E>(failure);

/** `Cause.NoSuchElementError`. */
export type NoSuchElement = Tagged<string, string>;

const NO_SUCH_ELEMENT = /*#__PURE__*/ tag("Cause.NoSuchElementError.NoSuchElementError");

/** Effect's `NoSuchElementError` as the Flick error of the same name; other errors as they are. */
export const noSuchElement = <E>(error: E | Cause.NoSuchElementError): E | NoSuchElement =>
  Cause.isNoSuchElementError(error) ? { _tag: NO_SUCH_ELEMENT, _0: error.message } : error;

export const flickEmpty: Failure = /*#__PURE__*/ toFailure(Cause.empty);
export const flickFail = (error: unknown): Failure => toFailure(Cause.fail(error));
export const flickDie = (defect: string): Failure => toFailure(Cause.die(defect));
export const flickInterrupt = (fiberId: Option.Option<number>): Failure =>
  toFailure(Cause.interrupt(Option.getOrUndefined(fiberId)));
export const flickFromReasons = (reasons: ReadonlyArray<Cause.Reason<unknown>>): Failure =>
  toFailure(Cause.fromReasons(reasons));
/**
 * The reasons of a failure as `Failure e` types them: only its first typed
 * failure has an error of `e`; a later one, which a narrowing `try` left
 * behind, is a defect here (docs/formal/runtime.md §2.4).
 */
export const flickReasons = (failure: Failure): ReadonlyArray<Cause.Reason<unknown>> => {
  let head = true;
  return causeOf(failure).reasons.map((reason) => {
    if (!Cause.isFailReason(reason)) return reason;
    if (head) {
      head = false;
      return reason;
    }
    return Cause.makeDieReason(reason.error);
  });
};

/** The cause with its first typed error mapped by `f`: the later ones have no type to map from. */
export const mapHead = (cause: Cause.Cause<unknown>, f: (error: unknown) => unknown): Cause.Cause<unknown> => {
  let head = true;
  return Cause.fromReasons(
    cause.reasons.map((reason) => {
      if (!head || !Cause.isFailReason(reason)) return reason;
      head = false;
      return Cause.makeFailReason(f(reason.error));
    }),
  );
};
export const flickMakeInterruptReason = (fiberId: Option.Option<number>): Cause.Interrupt =>
  Cause.makeInterruptReason(Option.getOrUndefined(fiberId));

export const flickHasInterruptsOnly = (failure: Failure): boolean => Cause.hasInterruptsOnly(causeOf(failure));
export const flickHasFails = (failure: Failure): boolean => Cause.hasFails(causeOf(failure));
export const flickHasDies = (failure: Failure): boolean => Cause.hasDies(causeOf(failure));
export const flickHasInterrupts = (failure: Failure): boolean => Cause.hasInterrupts(causeOf(failure));

export const flickMap =
  (f: (error: unknown) => unknown) =>
  (failure: Failure): Failure =>
    toFailure(mapHead(causeOf(failure), f));
export const flickCombine =
  (that: Failure) =>
  (failure: Failure): Failure =>
    toFailure(Cause.combine(causeOf(failure), causeOf(that)));

/** A native `Result` whose failure is a cause, with the cause as a Flick `Failure`. */
const failing = <A>(result: Result.Result<A, Cause.Cause<unknown>>): Result.Result<A, Failure> =>
  Result.mapError(result, toFailure);

export const flickFindFail = (failure: Failure): Result.Result<Cause.Fail<unknown>, Failure> =>
  failing(Cause.findFail(causeOf(failure)));
export const flickFindError = (failure: Failure): Result.Result<unknown, Failure> => failing(Cause.findError(causeOf(failure)));
export const flickFindErrorOption = (failure: Failure): Option.Option<unknown> => Cause.findErrorOption(causeOf(failure));
export const flickFindDie = (failure: Failure): Result.Result<Cause.Die, Failure> => failing(Cause.findDie(causeOf(failure)));
export const flickFindDefect = (failure: Failure): Result.Result<string, Failure> =>
  Result.map(failing(Cause.findDefect(causeOf(failure))), describeDefect);
export const flickFindInterrupt = (failure: Failure): Result.Result<Cause.Interrupt, Failure> =>
  failing(Cause.findInterrupt(causeOf(failure)));
export const flickInterruptors = (failure: Failure): Array<number> => [...Cause.interruptors(causeOf(failure))];
export const flickFilterInterruptors = (failure: Failure): Result.Result<Array<number>, Failure> =>
  Result.map(failing(Cause.filterInterruptors(causeOf(failure))), (ids) => [...ids]);

export const flickPretty = (failure: Failure): string => Cause.pretty(causeOf(failure));
export const flickPrettyErrors = (failure: Failure): Array<string> =>
  Cause.prettyErrors(causeOf(failure)).map((error) => `${error.name}: ${error.message}`);

/** The stack frame a context of annotations holds under `key`. */
const frame = <I>(context: Context.Context<never>, key: Context.Key<I, StackFrame>): Option.Option<StackFrame> =>
  Context.getOption(context, key);

export const flickStackTrace = (failure: Failure): Option.Option<StackFrame> =>
  frame(Cause.annotations(causeOf(failure)), Cause.StackTrace);
export const flickInterruptorStackTrace = (failure: Failure): Option.Option<StackFrame> =>
  frame(Cause.annotations(causeOf(failure)), Cause.InterruptorStackTrace);
export const flickReasonStackTrace = (reason: Cause.Reason<unknown>): Option.Option<StackFrame> =>
  frame(Cause.reasonAnnotations(reason), Cause.StackTrace);
export const flickAnnotate =
  (overwrite: boolean) =>
  (stack: StackFrame) =>
  (failure: Failure): Failure =>
    toFailure(Cause.annotate(causeOf(failure), Context.make(Cause.StackTrace, stack), { overwrite }));

export const flickFrameName = (stack: StackFrame): string => stack.name;
export const flickFrameStack = (stack: StackFrame): Option.Option<string> => Option.fromNullishOr(stack.stack());
export const flickFrameParent = (stack: StackFrame): Option.Option<StackFrame> => Option.fromNullishOr(stack.parent);
