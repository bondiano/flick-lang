// Effect's `Sink` for the bindings of the standard library, and the
// adapters where a Flick value differs from what Effect takes: an `End`
// keeps its leftovers in an `Option`, and failures are `Failure` and
// `Exit` values.
import { Channel, Effect, Option, Sink, Stream, type Cause, type Queue } from "effect";

import { Failure, passOn, unwinds } from "../cause.ts";
import { failureCause, flickExit } from "../stdlib/outcome.ts";
import { callEffect, durationFromEffect, inNativeRuntime } from "../bridge.ts";
import { Queue as NativeQueue, end as nativeEnd, offer as nativeOffer } from "@flick-lang/runtime/rt/effect/Queue";

export * from "effect/Sink";

/** A native queue receives each sink input and its end. */
export const fromQueue = (queue: Queue.Queue<unknown, Cause.Done> | NativeQueue): Sink.Sink<void, unknown, never, unknown, unknown> =>
  queue instanceof NativeQueue
    ? Sink.make<unknown>()((stream) => Effect.flatMap(
        Stream.runForEach(stream, (value) => callEffect(() => nativeOffer(value)(queue))),
        () => Effect.sync(() => { nativeEnd(queue); }),
      ))
    : Sink.fromQueue(queue);

/** Timings leave Effect as native durations only for the native runner. */
export const withDuration = <A, In, L, E, R>(self: Sink.Sink<A, In, L, E, R>) =>
  Sink.mapEffect(Sink.withDuration(self), ([value, duration]) =>
    Effect.map(inNativeRuntime, (native) => [value, native ? durationFromEffect(duration) : duration] as const));

export const timed = Sink.mapEffect(Sink.timed, (duration) =>
  Effect.map(inNativeRuntime, (native) => native ? durationFromEffect(duration) : duration));

type NonEmpty<A> = readonly [A, ...Array<A>];
type AnySink = Sink.Sink<unknown, unknown, unknown, unknown, unknown>;

/** `Sink.End` as Flick holds it. */
type FlickEnd<A, L> = readonly [A, Option.Option<NonEmpty<L>>];

const toEnd = <A, L>(end: FlickEnd<A, L>): Sink.End<A, L> => {
  const [value, leftover] = end;
  return Option.isSome(leftover) ? [value, leftover.value] : [value];
};

const fromEnd = <A, L>(end: Sink.End<A, L>): FlickEnd<A, L> => {
  const [value, leftover] = end;
  return [value, Option.fromUndefinedOr(leftover)];
};

/** `Sink.from_channel`. */
export const flickFromChannel = <L, In, E, A, R>(
  channel: Channel.Channel<never, E, FlickEnd<A, L>, NonEmpty<In>, never, void, R>,
): Sink.Sink<A, In, L, E, R> => Sink.fromChannel(Channel.mapDone(channel, toEnd));

/** `Sink.to_channel`. */
export const flickToChannel = <A, In, L, E, R>(self: Sink.Sink<A, In, L, E, R>) =>
  Channel.mapDone(Sink.toChannel(self), fromEnd);

/** `Sink.make`: the sink of what `f` does with the stream of its input. */
export const flickMake = <In, A, E, R>(f: (input: Stream.Stream<In>) => Effect.Effect<A, E, R>): Sink.Sink<A, In, never, E, R> =>
  Sink.make<In>()(f);

/** `Sink.from_effect_end`. */
export const flickFromEffectEnd = <A, E, R, L>(effect: Effect.Effect<FlickEnd<A, L>, E, R>) =>
  Sink.fromEffectEnd(Effect.map(effect, toEnd));

/** `Sink.fail_cause`. */
export const flickFailCause = <E>(failure: Failure) => Sink.failCause(failureCause<E>(failure));

/** `Sink.fail_cause_sync`. */
export const flickFailCauseSync = <E>(f: () => Failure) => Sink.failCauseSync(() => failureCause<E>(f()));

/** `Sink.map_end`. */
export const flickMapEnd =
  <A, L, A2, L2>(f: (end: FlickEnd<A, L>) => FlickEnd<A2, L2>) =>
  <In, E, R>(self: Sink.Sink<A, In, L, E, R>) =>
    Sink.mapEnd(self, (end) => toEnd(f(fromEnd(end))));

/** `Sink.map_effect_end`. */
export const flickMapEffectEnd =
  <A, L, A2, L2, E2, R2>(f: (end: FlickEnd<A, L>) => Effect.Effect<FlickEnd<A2, L2>, E2, R2>) =>
  <In, E, R>(self: Sink.Sink<A, In, L, E, R>) =>
    Sink.mapEffectEnd(self, (end) => Effect.map(f(fromEnd(end)), toEnd));

/** `Sink.catch_cause`: the handler gets a `Failure`. */
export const flickCatchCause = (f: (failure: Failure) => Effect.Effect<unknown, unknown, unknown>) => (self: AnySink) =>
  Sink.catchCause(self, (cause) => (unwinds(cause) ? passOn(cause) : f(new Failure(cause))));

/** `Sink.on_exit`: `f` gets a Flick `Exit`. */
export const flickOnExit =
  (f: (exit: ReturnType<typeof flickExit>) => Effect.Effect<unknown, unknown, unknown>) => (self: AnySink) =>
    Sink.onExit(self, (exit) => f(flickExit(exit)));
