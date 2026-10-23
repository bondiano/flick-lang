// Structured concurrency over cold effects. Branches run as children of the
// combinator: on a failure no new branch starts, the running ones are
// interrupted and awaited, and success values keep the order of the input.
import { Cause, Effect, Exit, type Result } from "effect";

import { type FlickExit, exitOf } from "./cause.ts";
import { err, ok } from "./data.ts";

/** `parallel a b`: both, as a tuple. */
export const parallel = <A, B, E, R>(
  a: Effect.Effect<A, E, R>,
  b: Effect.Effect<B, E, R>,
): Effect.Effect<[A, B], E, R> => Effect.all([a, b] as const, { concurrency: "unbounded" });

/** `parallel_all limit effects`: at most `limit` branches at a time. */
export const parallelAll = <A, E, R>(
  limit: number,
  effects: ReadonlyArray<Effect.Effect<A, E, R>>,
): Effect.Effect<Array<A>, E, R> =>
  Number.isInteger(limit) && limit > 0
    ? Effect.all(effects, { concurrency: limit })
    : Effect.die(new RangeError(`parallel_all needs a positive finite integer limit, not ${String(limit)}`));

/**
 * `parallel_results`: each branch's typed failure as an `Err`; a defect or
 * an interruption still stops the group.
 */
export const parallelResults = <A, E, R>(
  effects: ReadonlyArray<Effect.Effect<A, E, R>>,
): Effect.Effect<Array<Result.Result<A, E>>, E, R> =>
  Effect.all(
    effects.map((effect) =>
      Effect.flatMap(Effect.exit(effect), (exit): Effect.Effect<Result.Result<A, E>, E> => {
        if (Exit.isSuccess(exit)) return Effect.succeed(ok(exit.value));
        const reason = Cause.hasDies(exit.cause) ? undefined : exit.cause.reasons.find(Cause.isFailReason);
        return reason ? Effect.succeed(err(reason.error)) : Effect.failCause(exit.cause);
      }),
    ),
    { concurrency: "unbounded" },
  );

/** `parallel_exits`: the opaque exit of every branch. */
export const parallelExits = <A, E, R>(
  effects: ReadonlyArray<Effect.Effect<A, E, R>>,
): Effect.Effect<Array<FlickExit<A>>, never, R> => Effect.all(effects.map(exitOf), { concurrency: "unbounded" });
