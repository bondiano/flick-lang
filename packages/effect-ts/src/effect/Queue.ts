// Effect's `Queue` for the bindings of the standard library, and the
// adapters where a Flick value differs from what Effect takes: the end of a
// queue (`Cause.Done`) reaches Flick as `None`, never as an error.
import { type Cause, Effect, Option, Queue } from "effect";

import type { Failure } from "../cause.ts";
import type { Data } from "../data.ts";
import { failureCause, orDone } from "../stdlib/outcome.ts";
import { tag } from "@flick-lang/runtime/tags";

export * from "effect/Queue";

/** A Flick queue: its error row and the end of the queue. */
type FlickQueue<A, E> = Queue.Queue<A, E | Cause.Done>;

const strategies = /*#__PURE__*/ new Map<string, "suspend" | "dropping" | "sliding">([
  [tag("Queue.Strategy.Suspend"), "suspend"],
  [tag("Queue.Strategy.Dropping"), "dropping"],
  [tag("Queue.Strategy.Sliding"), "sliding"],
]);

/** `Queue.make`: the options record with `Option` fields. */
export const flickMake = <A, E>(options: {
  readonly capacity: Option.Option<number>;
  readonly strategy: Option.Option<Data>;
}): Effect.Effect<FlickQueue<A, E>> =>
  Queue.make<A, E | Cause.Done>({
    capacity: Option.getOrUndefined(options.capacity),
    strategy: Option.getOrUndefined(Option.map(options.strategy, (s) => strategies.get(s._tag) ?? "suspend")),
  });

/** `Queue.fail_cause`: fails with the cause behind a Flick `Failure`. */
export const flickFailCause =
  (failure: Failure) =>
  <A, E>(self: FlickQueue<A, E>): Effect.Effect<boolean> =>
    Effect.suspend(() => Queue.failCause(self, failureCause<E>(failure)));

export const flickTake = <A, E>(self: FlickQueue<A, E>): Effect.Effect<Option.Option<A>, E> => orDone(Queue.take(self));

export const flickTakeAll = <A, E>(self: FlickQueue<A, E>): Effect.Effect<Option.Option<Array<A>>, E> =>
  orDone(Queue.takeAll(self));

export const flickTakeN =
  (n: number) =>
  <A, E>(self: FlickQueue<A, E>): Effect.Effect<Option.Option<Array<A>>, E> =>
    orDone(Queue.takeN(self, n));

export const flickTakeBetween =
  (min: number, max: number) =>
  <A, E>(self: FlickQueue<A, E>): Effect.Effect<Option.Option<Array<A>>, E> =>
    orDone(Queue.takeBetween(self, min, max));

export const flickPeek = <A, E>(self: FlickQueue<A, E>): Effect.Effect<Option.Option<A>, E> => orDone(Queue.peek(self));
