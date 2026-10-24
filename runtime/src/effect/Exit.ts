// Effect's `Exit` for the bindings of the standard library, and the
// adapters over Flick's opaque `Exit e a`, which holds a native exit.
import { Exit, Option, Result } from "effect";

import { type Failure, type FlickExit, describeDefect } from "../cause.ts";
import { failureCause, flickExit, nativeExit } from "../stdlib/outcome.ts";
import { toFailure } from "./Cause.ts";

export * from "effect/Exit";

const native = <A>(exit: FlickExit<A>): Exit.Exit<A, unknown> => nativeExit<A, unknown>(exit);

export const flickSucceed = <A>(value: A): FlickExit<A> => flickExit(Exit.succeed(value));
export const flickFail = (error: unknown): FlickExit<never> => flickExit(Exit.fail(error));
export const flickFailCause = (failure: Failure): FlickExit<never> => flickExit(Exit.failCause(failureCause(failure)));
export const flickDie = (defect: string): FlickExit<never> => flickExit(Exit.die(defect));
export const flickInterrupt = (fiberId: Option.Option<number>): FlickExit<never> =>
  flickExit(Exit.interrupt(Option.getOrUndefined(fiberId)));
export const flickVoid: FlickExit<void> = /*#__PURE__*/ flickExit(Exit.void);

export const flickIsSuccess = <A>(exit: FlickExit<A>): boolean => Exit.isSuccess(native(exit));
export const flickIsFailure = <A>(exit: FlickExit<A>): boolean => Exit.isFailure(native(exit));
export const flickHasFails = <A>(exit: FlickExit<A>): boolean => Exit.hasFails(native(exit));
export const flickHasDies = <A>(exit: FlickExit<A>): boolean => Exit.hasDies(native(exit));
export const flickHasInterrupts = <A>(exit: FlickExit<A>): boolean => Exit.hasInterrupts(native(exit));

/** The other side of a filter stays a Flick exit. */
const keep = <A, B>(result: Result.Result<A, Exit.Exit<B, unknown>>): Result.Result<A, FlickExit<B>> =>
  Result.mapError(result, flickExit);

export const flickFilterSuccess = <A>(exit: FlickExit<A>): Result.Result<FlickExit<A>, FlickExit<A>> =>
  Result.map(keep(Exit.filterSuccess(native(exit))), flickExit);
export const flickFilterValue = <A>(exit: FlickExit<A>): Result.Result<A, FlickExit<never>> =>
  keep(Exit.filterValue(native(exit)));
export const flickFilterFailure = <A>(exit: FlickExit<A>): Result.Result<FlickExit<never>, FlickExit<A>> =>
  Result.map(keep(Exit.filterFailure(native(exit))), flickExit);
export const flickFilterCause = <A>(exit: FlickExit<A>): Result.Result<Failure, FlickExit<A>> =>
  Result.map(keep(Exit.filterCause(native(exit))), toFailure);
export const flickFindError = <A>(exit: FlickExit<A>): Result.Result<unknown, FlickExit<A>> =>
  keep(Exit.findError(native(exit)));
export const flickFindErrorOption = <A>(exit: FlickExit<A>): Option.Option<unknown> => Exit.findErrorOption(native(exit));
export const flickFindDefect = <A>(exit: FlickExit<A>): Result.Result<string, FlickExit<A>> =>
  Result.map(keep(Exit.findDefect(native(exit))), describeDefect);

export const flickMatch =
  <A, B>(onSuccess: (value: A) => B, onFailure: (failure: Failure) => B) =>
  (exit: FlickExit<A>): B =>
    Exit.match(native(exit), { onSuccess, onFailure: (cause) => onFailure(toFailure(cause)) });
export const flickMap =
  <A, B>(f: (value: A) => B) =>
  (exit: FlickExit<A>): FlickExit<B> =>
    flickExit(Exit.map(native(exit), f));
export const flickMapError =
  (f: (error: unknown) => unknown) =>
  <A>(exit: FlickExit<A>): FlickExit<A> =>
    flickExit(Exit.mapError(native(exit), f));
export const flickMapBoth =
  <A, B>(onFailure: (error: unknown) => unknown, onSuccess: (value: A) => B) =>
  (exit: FlickExit<A>): FlickExit<B> =>
    flickExit(Exit.mapBoth(native(exit), { onFailure, onSuccess }));
export const flickAsVoid = <A>(exit: FlickExit<A>): FlickExit<void> => flickExit(Exit.asVoid(native(exit)));
export const flickAsVoidAll = <A>(exits: ReadonlyArray<FlickExit<A>>): FlickExit<void> =>
  flickExit(Exit.asVoidAll(exits.map(native)));
