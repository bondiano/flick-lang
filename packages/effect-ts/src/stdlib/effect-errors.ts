// Adapters of Effect's `Effect` for the part `Effect.Errors`: a native
// cause reaches Flick as an opaque `Failure`, a native exit as an `Exit`,
// and a defect only as inert text.
import { Cause, Effect, Exit, type LogLevel, Option, Result } from "effect";

import { type Failure, type FlickExit, RawCause, describeDefect, dropSignalsOnDie, hasRealDie, hasSignal, passOn, stripSignals, unwinds } from "../cause.ts";
import { none, some } from "../data.ts";
import { causeOf, toFailure } from "../effect/Cause.ts";
import { flickExit } from "./outcome.ts";

type Run<A, E = unknown, R = unknown> = (value: A) => Effect.Effect<unknown, E, R>;
type Filtered<B> = (failure: Failure) => Result.Result<B, Failure>;

const cleanCause = <E>(cause: Cause.Cause<E>): Cause.Cause<E> =>
  hasRealDie(cause) && hasSignal(cause) ? stripSignals(cause) : cause;

const isUnwinding = (cause: Cause.Cause<unknown>): boolean =>
  !hasRealDie(cause) && unwinds(cause);

const cleanExit = <A, E>(exit: Exit.Exit<A, E>): FlickExit<A> =>
  Exit.isFailure(exit) && hasRealDie(exit.cause) && hasSignal(exit.cause)
    ? flickExit(Exit.failCause(stripSignals(exit.cause)))
    : flickExit(exit);

/** A Flick filter of failures as a filter of native causes; one unwinding to a handler of effects is no failure of the code. */
const causeFilter =
  <B>(filter: Filtered<B>) =>
  (cause: Cause.Cause<unknown>): Result.Result<B, Cause.Cause<unknown>> =>
    isUnwinding(cause) ? Result.fail(cause) : Result.mapError(filter(toFailure(cleanCause(cause))), (failure) => causeOf(failure));

export const flickCatchCause =
  <A2, E2, R2>(f: (failure: Failure) => Effect.Effect<A2, E2, R2>) =>
  <A, E, R>(self: Effect.Effect<A, E, R>) =>
    Effect.catchCause(self, (cause) => (isUnwinding(cause) ? passOn(cause) : f(toFailure(cleanCause(cause)))));

export const flickCatchCauseIf =
  <A2, E2, R2>(predicate: (failure: Failure) => boolean, f: (failure: Failure) => Effect.Effect<A2, E2, R2>) =>
  <A, E, R>(self: Effect.Effect<A, E, R>) =>
    Effect.catchCauseIf(
      self,
      (cause) => !isUnwinding(cause) && predicate(toFailure(cleanCause(cause))),
      (cause) => f(toFailure(cleanCause(cause))),
    );

export const flickCatchCauseFilter =
  <B, A2, E2, R2>(filter: Filtered<B>, f: (value: B, failure: Failure) => Effect.Effect<A2, E2, R2>) =>
  <A, E, R>(self: Effect.Effect<A, E, R>) =>
    Effect.catchCauseFilter(self, causeFilter(filter), (value, cause) => f(value, toFailure(cleanCause(cause))));

export const flickCatchDefect =
  <A2, E2, R2>(f: (defect: string) => Effect.Effect<A2, E2, R2>) =>
  <A, E, R>(self: Effect.Effect<A, E, R>): Effect.Effect<A | A2, E | E2, R | R2> =>
    Effect.catchCause(self, (cause): Effect.Effect<A2, E | E2, R2> => {
      if (isUnwinding(cause)) return passOn(cause);
      const cleaned = cleanCause(cause);
      const die = cleaned.reasons.find(Cause.isDieReason);
      return die === undefined ? Effect.failCause(cleaned) : f(describeDefect(die.defect));
    });

export const flickTapCause =
  <E2, R2>(f: Run<Failure, E2, R2>) =>
  <A, E, R>(self: Effect.Effect<A, E, R>) =>
    dropSignalsOnDie(
      Effect.tapCause(self, (cause) => (isUnwinding(cause) ? Effect.void : f(toFailure(cleanCause(cause))))),
    );

export const flickTapCauseIf =
  <E2, R2>(predicate: (failure: Failure) => boolean, f: Run<Failure, E2, R2>) =>
  <A, E, R>(self: Effect.Effect<A, E, R>) =>
    dropSignalsOnDie(
      Effect.tapCauseIf(
        self,
        (cause) => !isUnwinding(cause) && predicate(toFailure(cleanCause(cause))),
        (cause) => f(toFailure(cleanCause(cause))),
      ),
    );

export const flickTapCauseFilter =
  <B, E2, R2>(filter: Filtered<B>, f: (value: B, failure: Failure) => Effect.Effect<unknown, E2, R2>) =>
  <A, E, R>(self: Effect.Effect<A, E, R>) =>
    dropSignalsOnDie(
      Effect.tapCauseFilter(self, causeFilter(filter), (value, cause) => f(value, toFailure(cleanCause(cause)))),
    );

export const flickTapDefect =
  <E2, R2>(f: Run<string, E2, R2>) =>
  <A, E, R>(self: Effect.Effect<A, E, R>) =>
    dropSignalsOnDie(
      Effect.tapCause(self, (cause) => {
        if (isUnwinding(cause)) return Effect.void;
        const die = cleanCause(cause).reasons.find(Cause.isDieReason);
        return die === undefined ? Effect.void : f(describeDefect(die.defect));
      }),
    );

export const flickMatchCause =
  <A, B>(onFailure: (failure: Failure) => B, onSuccess: (value: A) => B) =>
  <E, R>(self: Effect.Effect<A, E, R>) =>
    Effect.matchCauseEffect(self, {
      onFailure: (cause) => (isUnwinding(cause) ? passOn(cause) : Effect.succeed(onFailure(toFailure(cleanCause(cause))))),
      onSuccess: (value) => Effect.succeed(onSuccess(value)),
    });

export const flickMatchCauseEager =
  <A, B>(onFailure: (failure: Failure) => B, onSuccess: (value: A) => B) =>
  <E, R>(self: Effect.Effect<A, E, R>) =>
    Effect.matchCauseEffectEager(self, {
      onFailure: (cause) => (isUnwinding(cause) ? passOn(cause) : Effect.succeed(onFailure(toFailure(cleanCause(cause))))),
      onSuccess: (value) => Effect.succeed(onSuccess(value)),
    });

export const flickMatchCauseEffect =
  <A, B, E2, R2, E3, R3>(
    onFailure: (failure: Failure) => Effect.Effect<B, E2, R2>,
    onSuccess: (value: A) => Effect.Effect<B, E3, R3>,
  ) =>
  <E, R>(self: Effect.Effect<A, E, R>) =>
    Effect.matchCauseEffect(self, {
      onFailure: (cause) => (isUnwinding(cause) ? passOn(cause) : onFailure(toFailure(cleanCause(cause)))),
      onSuccess,
    });

export const flickMatchCauseEffectEager =
  <A, B, E2, R2, E3, R3>(
    onFailure: (failure: Failure) => Effect.Effect<B, E2, R2>,
    onSuccess: (value: A) => Effect.Effect<B, E3, R3>,
  ) =>
  <E, R>(self: Effect.Effect<A, E, R>) =>
    Effect.matchCauseEffectEager(self, {
      onFailure: (cause) => (isUnwinding(cause) ? passOn(cause) : onFailure(toFailure(cleanCause(cause)))),
      onSuccess,
    });

const logOption = (level: Option.Option<LogLevel.Severity>): { readonly log: boolean | LogLevel.Severity } => ({
  log: Option.getOrElse(level, (): boolean | LogLevel.Severity => false),
});

/** `Effect.or_die`: turns typed failures into defects while letting handler unwinding pass through. */
export const flickOrDie = <A, E, R>(self: Effect.Effect<A, E, R>): Effect.Effect<A, never, R> =>
  Effect.catchCause(self, (cause) =>
    isUnwinding(cause) ? passOn(cause) : Effect.orDie(Effect.failCause(cleanCause(cause))),
  );

export const flickIgnore =
  (level: Option.Option<LogLevel.Severity>) =>
  <A, E, R>(self: Effect.Effect<A, E, R>) =>
    Effect.catchCause(Effect.asVoid(self), (cause) =>
      isUnwinding(cause)
        ? passOn(cause)
        : Effect.ignore(Effect.failCause(cleanCause(cause)), logOption(level)),
    );

export const flickIgnoreCause =
  (level: Option.Option<LogLevel.Severity>) =>
  <A, E, R>(self: Effect.Effect<A, E, R>) =>
    Effect.catchCause(Effect.asVoid(self), (cause) =>
      isUnwinding(cause)
        ? passOn(cause)
        : Effect.ignoreCause(Effect.failCause(cleanCause(cause)), logOption(level)),
    );

export const flickOnError =
  <R2>(f: (failure: Failure) => Effect.Effect<unknown, never, R2>) =>
  <A, E, R>(self: Effect.Effect<A, E, R>) =>
    dropSignalsOnDie(
      Effect.onError(self, (cause) => (isUnwinding(cause) ? Effect.void : f(toFailure(cleanCause(cause))))),
    );

export const flickOnErrorIf =
  <E2, R2>(predicate: (failure: Failure) => boolean, f: (failure: Failure) => Effect.Effect<void, E2, R2>) =>
  <A, E, R>(self: Effect.Effect<A, E, R>) =>
    dropSignalsOnDie(Effect.onErrorIf(
      self,
      (cause) => !isUnwinding(cause) && predicate(toFailure(cleanCause(cause))),
      (cause) => f(toFailure(cleanCause(cause))),
    ));

export const flickOnErrorFilter =
  <B, E2, R2>(filter: Filtered<B>, f: (value: B, failure: Failure) => Effect.Effect<void, E2, R2>) =>
  <A, E, R>(self: Effect.Effect<A, E, R>) =>
    dropSignalsOnDie(Effect.onErrorFilter(self, causeFilter(filter), (value, cause) => f(value, toFailure(cleanCause(cause)))));

export const flickOnExit =
  <A, E2, R2>(f: (exit: FlickExit<A>) => Effect.Effect<void, E2, R2>) =>
  <E, R>(self: Effect.Effect<A, E, R>) =>
    dropSignalsOnDie(Effect.onExit(self, (exit) => f(cleanExit(exit))));

export const flickOnExitIf =
  <A, E2, R2>(predicate: (exit: FlickExit<A>) => boolean, f: (exit: FlickExit<A>) => Effect.Effect<void, E2, R2>) =>
  <E, R>(self: Effect.Effect<A, E, R>) =>
    dropSignalsOnDie(Effect.onExitIf(
      self,
      (exit) => predicate(cleanExit(exit)),
      (exit) => f(cleanExit(exit)),
    ));

export const flickOnExitFilter =
  <A, B, X, E2, R2>(
    filter: (exit: FlickExit<A>) => Result.Result<B, X>,
    f: (value: B, exit: FlickExit<A>) => Effect.Effect<void, E2, R2>,
  ) =>
  <E, R>(self: Effect.Effect<A, E, R>) =>
    dropSignalsOnDie(Effect.onExitFilter(
      self,
      (exit) => filter(cleanExit(exit)),
      (value, exit) => f(value, cleanExit(exit)),
    ));

export const flickOnExitPrimitive =
  <A, E2, R2>(interruptible: boolean, f: (exit: FlickExit<A>) => Option.Option<Effect.Effect<void, E2, R2>>) =>
  <E, R>(self: Effect.Effect<A, E, R>) =>
    dropSignalsOnDie(Effect.onExitPrimitive(self, (exit) => Option.getOrUndefined(f(cleanExit(exit))), interruptible));

export const flickOnInterrupt =
  <E2, R2>(f: (interruptors: ReadonlyArray<number>) => Effect.Effect<void, E2, R2>) =>
  <A, E, R>(self: Effect.Effect<A, E, R>) =>
    dropSignalsOnDie(Effect.onInterrupt(self, (ids) => f([...ids])));

export const flickInterrupt = (): Effect.Effect<never> => Effect.interrupt;

/** `Effect.add_finalizer`: the finalizer gets the cause the scope closed with, if it failed. */
export const flickAddFinalizer = <R>(f: (cause: Option.Option<RawCause>) => Effect.Effect<void, never, R>) =>
  Effect.addFinalizer((exit) => f(Exit.isSuccess(exit) ? none : some(new RawCause(cleanCause(exit.cause)))));

export const flickWithErrorReporting =
  (defectsOnly: boolean) =>
  <A, E, R>(self: Effect.Effect<A, E, R>) =>
    Effect.withErrorReporting(self, { defectsOnly });
