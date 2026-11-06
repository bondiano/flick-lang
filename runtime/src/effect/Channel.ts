// Effect's `Channel` for the bindings of the standard library, and the
// adapters where a Flick value differs from what Effect takes: options
// records with `Option` fields, a `Failure` or `Exit` instead of a native
// cause or exit, and bytes as lists of numbers.
import { type Cause, Channel, Effect, type Exit, Option, type PubSub, type Queue } from "effect";

import { Failure, passOn, unwinds } from "../cause.ts";
import type { Data } from "../data.ts";
import { failureCause, flickExit } from "../stdlib/outcome.ts";
import { tag } from "../tags.ts";
import { type FlickSpanOptions, nativeSpanOptions } from "./Tracer.ts";

export * from "effect/Channel";

type Any = Channel.Channel<unknown, unknown, unknown, unknown, unknown, unknown, unknown>;
type NonEmpty<A> = readonly [A, ...Array<A>];
type Strategy = "suspend" | "dropping" | "sliding";

const strategies = /*#__PURE__*/ new Map<string, Strategy>([
  [tag("Queue.Strategy.Suspend"), "suspend"],
  [tag("Queue.Strategy.Dropping"), "dropping"],
  [tag("Queue.Strategy.Sliding"), "sliding"],
]);

/** `Channel.Buffer`: a capacity (`None`: unbounded) and a strategy. */
interface Buffer {
  readonly capacity: Option.Option<number>;
  readonly strategy: Option.Option<Data>;
}

const strategyOf = (strategy: Option.Option<Data>): Strategy | undefined =>
  Option.getOrUndefined(Option.map(strategy, (s) => strategies.get(s._tag) ?? "suspend"));

type QueueOptions = { readonly capacity: "unbounded" } | { readonly capacity: number; readonly strategy?: Strategy | undefined };

const queueOptions = (options: Buffer): QueueOptions =>
  Option.match(options.capacity, {
    onNone: () => ({ capacity: "unbounded" }),
    onSome: (capacity) => ({ capacity, strategy: strategyOf(options.strategy) }),
  });

const failureOf = (cause: Cause.Cause<unknown>): Failure => new Failure(cause);

/** `Channel.from_list_chunked`. */
export const flickFromListChunked =
  (size: number) =>
  <A>(list: ReadonlyArray<A>): Channel.Channel<NonEmpty<A>> =>
    Channel.fromIterableArray(list, size);

/** `Channel.fail_cause`. */
export const flickFailCause = <E>(failure: Failure): Channel.Channel<never, E, never> =>
  Channel.failCause(failureCause<E>(failure));

/** `Channel.fail_cause_sync`. */
export const flickFailCauseSync = <E>(f: () => Failure): Channel.Channel<never, E, never> =>
  Channel.failCauseSync(() => failureCause<E>(f()));

/** `Channel.callback`. */
export const flickCallback = <A, E, R>(
  options: { readonly bufferSize: Option.Option<number>; readonly strategy: Option.Option<Data> },
  f: (queue: Queue.Queue<A, E | Cause.Done>) => Effect.Effect<unknown, E, R>,
) => Channel.callback(f, { bufferSize: Option.getOrUndefined(options.bufferSize), strategy: strategyOf(options.strategy) });

/** `Channel.callback_array`. */
export const flickCallbackArray = <A, E, R>(
  options: { readonly bufferSize: Option.Option<number>; readonly strategy: Option.Option<Data> },
  f: (queue: Queue.Queue<A, E | Cause.Done>) => Effect.Effect<unknown, E, R>,
) =>
  Channel.callbackArray(f, {
    bufferSize: Option.getOrUndefined(options.bufferSize),
    strategy: strategyOf(options.strategy),
  });

/** `Channel.acquire_release`: the release gets only the value. */
export const flickAcquireRelease =
  <Z>(release: (z: Z) => Effect.Effect<unknown>) =>
  <E, R>(self: Effect.Effect<Z, E, R>) =>
    Channel.acquireRelease(self, (z) => release(z));

/** `Channel.acquire_use_release`: the release gets a Flick `Exit`. */
export const flickAcquireUseRelease = <A, E, R, OE, OX, OD, IE, IX, ID, Env>(
  acquire: Effect.Effect<A, E, R>,
  use: (a: A) => Channel.Channel<OE, OX, OD, IE, IX, ID, Env>,
  release: (a: A, exit: ReturnType<typeof flickExit>) => Effect.Effect<unknown>,
) => Channel.acquireUseRelease(acquire, use, (a, exit: Exit.Exit<OD, OX>) => release(a, flickExit(exit)));

/** `Channel.merge_all`: `None` runs every inner channel at once. */
export const flickMergeAll =
  (options: {
    readonly concurrency: Option.Option<number>;
    readonly bufferSize: Option.Option<number>;
    readonly switch: boolean;
  }) =>
  (channels: Channel.Channel<Any, unknown, unknown, unknown, unknown, unknown, unknown>) =>
    Channel.mergeAll(channels, {
      concurrency: Option.getOrElse(options.concurrency, (): "unbounded" => "unbounded"),
      bufferSize: Option.getOrUndefined(options.bufferSize),
      switch: options.switch,
    });

/** `Channel.buffer`. */
export const flickBuffer = (options: Buffer) => (self: Any) => Channel.buffer(self, queueOptions(options));

/** `Channel.buffer_array`. */
export const flickBufferArray =
  (options: Buffer) =>
  <OE, OX, OD, IE, IX, ID, Env>(self: Channel.Channel<NonEmpty<OE>, OX, OD, IE, IX, ID, Env>) =>
    Channel.bufferArray(self, queueOptions(options));

const toBytes = (chunk: NonEmpty<ReadonlyArray<number>>): NonEmpty<Uint8Array> => {
  const [first, ...rest] = chunk;
  return [Uint8Array.from(first), ...rest.map((bytes) => Uint8Array.from(bytes))];
};

const fromBytes = (chunk: NonEmpty<Uint8Array>): NonEmpty<Array<number>> => {
  const [first, ...rest] = chunk;
  return [Array.from(first), ...rest.map((bytes) => Array.from(bytes))];
};

/** `Channel.decode_text`: chunks of bytes as lists of numbers. */
export const flickDecodeText = <Err, Done>(encoding: string) =>
  Channel.mapInput(Channel.decodeText<Err, Done>(encoding), (chunk: NonEmpty<ReadonlyArray<number>>) =>
    Effect.succeed(toBytes(chunk)),
  );

/** `Channel.encode_text`: chunks of bytes as lists of numbers. */
export const flickEncodeText = <Err, Done>() => Channel.map(Channel.encodeText<Err, Done>(), fromBytes);

/** `Channel.catch_cause`: the handler gets a `Failure`. */
export const flickCatchCause = (f: (failure: Failure) => Any) => (self: Any) =>
  Channel.catchCause(self, (cause) => (unwinds(cause) ? Channel.fromEffect(passOn(cause)) : f(failureOf(cause))));

/** `Channel.tap_cause`: `f` gets a `Failure`. */
export const flickTapCause = (f: (failure: Failure) => Effect.Effect<unknown, unknown, unknown>) => (self: Any) =>
  Channel.tapCause(self, (cause) => f(failureOf(cause)));

/** `Channel.on_error`: the finalizer gets a `Failure`. */
export const flickOnError = (f: (failure: Failure) => Effect.Effect<unknown, never, unknown>) => (self: Any) =>
  Channel.onError(self, (cause) => f(failureOf(cause)));

/** `Channel.on_exit`: the finalizer gets a Flick `Exit`. */
export const flickOnExit =
  (f: (exit: ReturnType<typeof flickExit>) => Effect.Effect<unknown, never, unknown>) => (self: Any) =>
    Channel.onExit(self, (exit) => f(flickExit(exit)));

/** `Channel.merge`. */
export const flickMerge = (haltStrategy: Channel.HaltStrategy, right: Any) => (left: Any) =>
  Channel.merge(left, right, { haltStrategy });

/** `Channel.with_span`. */
export const flickWithSpan = (name: string, options: FlickSpanOptions) => (self: Any) =>
  Channel.withSpan(self, name, nativeSpanOptions(options));

/** `Channel.mk_uint8_array`: bytes as lists of numbers. */
export const flickMkUint8Array = <OX, OD, Env>(
  self: Channel.Channel<NonEmpty<ReadonlyArray<number>>, OX, OD, unknown, unknown, unknown, Env>,
): Effect.Effect<Array<number>, OX, Env> =>
  Effect.map(Channel.mkUint8Array(Channel.map(self, toBytes)), (bytes) => Array.from(bytes));

/** `Channel.to_queue`. */
export const flickToQueue =
  (options: Buffer) =>
  <OE, OX, OD, Env>(self: Channel.Channel<OE, OX, OD, unknown, unknown, unknown, Env>) =>
    Channel.toQueue(self, queueOptions(options));

/** `Channel.to_queue_array`. */
export const flickToQueueArray =
  (options: Buffer) =>
  <OE, OX, OD, Env>(self: Channel.Channel<NonEmpty<OE>, OX, OD, unknown, unknown, unknown, Env>) =>
    Channel.toQueueArray(self, queueOptions(options));

/** `Channel.HubOptions`. */
interface HubOptions extends Buffer {
  readonly replay: Option.Option<number>;
  readonly shutdownOnEnd: boolean;
}

const hubOptions = (options: HubOptions) => ({
  ...queueOptions(options),
  replay: Option.getOrUndefined(options.replay),
  shutdownOnEnd: options.shutdownOnEnd,
});

/** `Channel.to_pubsub`. */
export const flickToPubSub =
  (options: HubOptions) =>
  <OE, OX, OD, Env>(self: Channel.Channel<OE, OX, OD, unknown, unknown, unknown, Env>) =>
    Channel.toPubSub(self, hubOptions(options));

/** `Channel.to_pubsub_array`. */
export const flickToPubSubArray =
  (options: HubOptions) =>
  <OE, OX, OD, Env>(self: Channel.Channel<NonEmpty<OE>, OX, OD, unknown, unknown, unknown, Env>) =>
    Channel.toPubSubArray(self, hubOptions(options));

/** `Channel.run_into_pubsub`. */
export const flickRunIntoPubSub =
  <OE>(options: { readonly shutdownOnEnd: boolean }, pubsub: PubSub.PubSub<OE>) =>
  <OX, OD, Env>(self: Channel.Channel<OE, OX, OD, unknown, unknown, unknown, Env>) =>
    Channel.runIntoPubSub(self, pubsub, options);

/** `Channel.run_into_pubsub_array`. */
export const flickRunIntoPubSubArray =
  <OE>(options: { readonly shutdownOnEnd: boolean }, pubsub: PubSub.PubSub<OE>) =>
  <OX, OD, Env>(self: Channel.Channel<NonEmpty<OE>, OX, OD, unknown, unknown, unknown, Env>) =>
    Channel.runIntoPubSubArray(self, pubsub, options);
