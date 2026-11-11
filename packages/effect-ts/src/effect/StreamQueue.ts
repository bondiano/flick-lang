// Stream-to-queue combinators for the standard library's bindings: native
// consumers need native queues, so each reaches the kernel's queue when the
// program runs on Flick's own runtime and Effect's queue otherwise.
import { Effect, Exit, type Filter, Option, Result, type Queue, type Scope, Stream } from "effect";

import type { Data } from "../data.ts";
import { callEffect, causeFromEffect, inNativeRuntime } from "../bridge.ts";
import { constructorName } from "@flick-lang/runtime/tags";
import { Queue as NativeQueue, end as nativeEnd, flickTake as nativeTake, offer as nativeOffer } from "@flick-lang/runtime/rt/effect/Queue";
import { runIntoQueueArray as channelRunIntoQueueArray } from "./Channel.ts";

type Strategy = "suspend" | "dropping" | "sliding";

const strategies: Readonly<Record<string, Strategy>> = { Suspend: "suspend", Dropping: "dropping", Sliding: "sliding" };

/** A `Queue.Strategy`, `None` being `suspend`. */
const strategyOf = (strategy: Option.Option<Data>): Strategy =>
  Option.match(strategy, { onNone: () => "suspend", onSome: (s) => strategies[constructorName(s._tag)] ?? "suspend" });

interface BufferOptions {
  readonly capacity: Option.Option<number>;
  readonly strategy: Option.Option<Data>;
}

const bufferOf = (
  o: BufferOptions,
): { readonly capacity: "unbounded" } | { readonly capacity: number; readonly strategy: Strategy } =>
  Option.match(o.capacity, {
    onNone: () => ({ capacity: "unbounded" as const }),
    onSome: (capacity) => ({ capacity, strategy: strategyOf(o.strategy) }),
  });

/** Drain a native queue one message at a time while preserving its end. */
export const fromQueue = (queue: Queue.Queue<unknown, unknown> | NativeQueue) =>
  queue instanceof NativeQueue
    ? Stream.unfold(queue, (q) => Effect.map(callEffect(() => nativeTake(q)), (taken) =>
      Reflect.get(Object(taken), "_tag") === "Some" ? [Reflect.get(Object(taken), "value"), q] as const : undefined))
    : Stream.fromQueue(queue);

export const runIntoQueue = (queue: Queue.Queue<unknown, unknown> | NativeQueue) => (self: Stream.Stream<unknown, unknown, unknown>) =>
  queue instanceof NativeQueue ? channelRunIntoQueueArray(queue)(Stream.toChannel(self)) : Stream.runIntoQueue(self, queue);

export const flickToQueue =
  (options: BufferOptions) =>
  <A, E, R>(self: Stream.Stream<A, E, R>) =>
    Effect.flatMap(inNativeRuntime, (native) => native
      ? Effect.map(Effect.gen(function* () {
          const queue = new NativeQueue(Option.getOrElse(options.capacity, () => Infinity), strategyOf(options.strategy));
          yield* Effect.forkScoped(Effect.flatMap(
            Effect.exit(Stream.runForEach(self, (value) => callEffect(() => nativeOffer(value)(queue)))),
            (exit) => Effect.sync(() => {
              if (Exit.isSuccess(exit)) nativeEnd(queue);
              else queue.failCause(causeFromEffect(exit.cause));
            }),
          ));
          return queue;
        }), (queue): unknown => queue)
      : Effect.map(Stream.toQueue(self, bufferOf(options)), (queue): unknown => queue));

/** Native consumers need native queues, including the two ends of a partition. */
export const partitionQueue =
  <A, Pass, Fail>(filter: Filter.Filter<A, Pass, Fail>) =>
  <E, R>(self: Stream.Stream<A, E, R>): Effect.Effect<unknown, never, R | Scope.Scope> =>
    Effect.flatMap(inNativeRuntime, (native) => native
      ? Effect.map(Effect.gen(function* () {
          const passed = new NativeQueue(Infinity, "suspend");
          const failed = new NativeQueue(Infinity, "suspend");
          yield* Effect.forkScoped(Effect.flatMap(
            Effect.exit(Stream.runForEach(self, (value) => {
              const result = filter(value);
              return callEffect(() => nativeOffer(Result.isSuccess(result) ? result.success : result.failure)(
                Result.isSuccess(result) ? passed : failed,
              ));
            })),
            (exit) => Effect.sync(() => {
              for (const queue of [passed, failed]) {
                if (Exit.isSuccess(exit)) nativeEnd(queue);
                else queue.failCause(causeFromEffect(exit.cause));
              }
            }),
          ));
          return [passed, failed] as const;
        }), (queues): unknown => queues)
      : Effect.map(Stream.partitionQueue(self, filter), (queues): unknown => queues));
