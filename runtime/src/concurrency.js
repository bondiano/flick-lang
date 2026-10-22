// Structured concurrency over cold effects. Branches run as children of the
// combinator: on a failure no new branch starts, the running ones are
// interrupted and awaited, and success values keep the order of the input.
import { Cause, Effect, Exit } from "effect";

import { exitOf } from "./cause.js";
import { err, ok } from "./data.js";

const all = (effects, concurrency) => Effect.all(effects, { concurrency });

/** `parallel a b`: both, as a tuple. */
export const parallel = (a, b) => all([a, b], "unbounded");

/** `parallel_all limit effects`: at most `limit` branches at a time. */
export const parallelAll = (limit, effects) =>
  Number.isInteger(limit) && limit > 0
    ? all(effects, limit)
    : Effect.die(
        new RangeError(`parallel_all needs a positive finite integer limit, not ${String(limit)}`),
      );

/**
 * `parallel_results`: each branch's typed failure as an `Err`; a defect or
 * an interruption still stops the group.
 */
export const parallelResults = (effects) =>
  all(
    effects.map((effect) =>
      Effect.flatMap(Effect.exit(effect), (exit) => {
        if (Exit.isSuccess(exit)) return Effect.succeed(ok(exit.value));
        const reason = Cause.hasDies(exit.cause)
          ? undefined
          : exit.cause.reasons.find(Cause.isFailReason);
        return reason ? Effect.succeed(err(reason.error)) : Effect.failCause(exit.cause);
      }),
    ),
    "unbounded",
  );

/** `parallel_exits`: the opaque exit of every branch. */
export const parallelExits = (effects) => all(effects.map(exitOf), "unbounded");
