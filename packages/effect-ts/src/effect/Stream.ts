// Effect's `Stream` for the bindings of the standard library, and the
// adapters where a Flick value differs from what Effect takes: options
// records with `Option` fields, causes as `Failure`, bytes as lists, and
// the scope of a stream's resources, which stays the lexical one.
import {
  type ByteSize,
  Cause,
  type Duration,
  Effect,
  type Exit,
  FileSystem,
  Option,
  PlatformError,
  type Queue,
  Result,
  type Schedule,
  Scope,
  Stream,
} from "effect";

import { Failure, passOn, unwinds } from "../cause.ts";
import type { Data } from "../data.ts";
import { service } from "../effects.ts";
import { failureCause, flickExit } from "../stdlib/outcome.ts";
import { constructorName, tag } from "@flick-lang/runtime/tags";
import { effectFileSystem, info, nativeFileSystem } from "./FileSystem.ts";
import { toFlick } from "./PlatformError.ts";
import { nativeStdio } from "./Stdio.ts";

export * from "effect/Stream";
export { flickStreamWithExecutionPlan } from "./ExecutionPlan.ts";

type Strategy = "suspend" | "dropping" | "sliding";

const strategies: Readonly<Record<string, Strategy>> = { Suspend: "suspend", Dropping: "dropping", Sliding: "sliding" };

/** A `Queue.Strategy`, `None` being `suspend`. */
const strategyOf = (strategy: Option.Option<Data>): Strategy =>
  Option.match(strategy, { onNone: () => "suspend", onSome: (s) => strategies[constructorName(s._tag)] ?? "suspend" });

/** `None` is no bound. */
const concurrencyOf = (n: Option.Option<number>): number | "unbounded" => Option.getOrElse(n, () => "unbounded" as const);

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

interface SplitOptions extends BufferOptions {
  readonly replay: Option.Option<number>;
}

const splitOf = (o: SplitOptions) => ({ ...bufferOf(o), replay: Option.getOrUndefined(o.replay) });

/** A stream of Flick bytes as Effect takes them, and back. */
const toBytes = <E, R>(self: Stream.Stream<ReadonlyArray<number>, E, R>): Stream.Stream<Uint8Array, E, R> =>
  Stream.map(self, (bytes) => Uint8Array.from(bytes));
const fromBytes = <E, R>(self: Stream.Stream<Uint8Array, E, R>): Stream.Stream<ReadonlyArray<number>, E, R> =>
  Stream.map(self, (bytes) => Array.from(bytes));

/**
 * The stream `make` gives, run with the scope the stream runs in: Effect
 * would give the code of a `callback` or `unwrap` a scope of the stream's
 * own, but a Flick acquisition belongs to the lexical scope its region
 * names.
 */
const inOuterScope = <A, E, R>(
  make: (outer: <X, EX, RX>(effect: Effect.Effect<X, EX, RX>) => Effect.Effect<X, EX, RX>) => Stream.Stream<A, E, R>,
): Stream.Stream<A, E, R> =>
  Stream.flatten(
    Stream.fromEffect(
      Effect.map(Effect.serviceOption(Scope.Scope), (scope) =>
        make((effect) =>
          Option.isSome(scope) ? Effect.provideService(effect, Scope.Scope, scope.value) : effect,
        ),
      ),
    ),
  );

export const flickUnwrap = <A, E, R, E2, R2>(
  effect: Effect.Effect<Stream.Stream<A, E2, R2>, E, R>,
): Stream.Stream<A, E | E2, R | R2> => Stream.flatten(Stream.fromEffect(effect));

export const flickCallback = <A, E, R>(
  producer: (queue: Queue.Queue<A, E | Cause.Done>) => Effect.Effect<unknown, E, R>,
): Stream.Stream<A, E, R> => inOuterScope((outer) => Stream.callback<A, E, R>((queue) => outer(producer(queue))));

export const flickFailCause = <E>(failure: Failure): Stream.Stream<never, E> => Stream.failCause(failureCause<E>(failure));

export const flickDie = (message: string): Stream.Stream<never> => Stream.die(new Error(message));

export const flickFromEffectSchedule =
  <X, A, ES, RS>(schedule: Schedule.Schedule<X, A, ES, RS>) =>
  <E, R>(effect: Effect.Effect<A, E, R>): Stream.Stream<A, E | ES, R | RS> =>
    Stream.fromEffectSchedule(effect, schedule);

export const flickFromReadableStream = <E>(
  onError: (error: unknown) => E,
  readable: ReadableStream<unknown>,
): Stream.Stream<unknown, E> => Stream.fromReadableStream({ evaluate: () => readable, onError });

export const flickFromAsyncIterable = <E>(
  onError: (error: unknown) => E,
  iterable: AsyncIterable<unknown>,
): Stream.Stream<unknown, E> => Stream.fromAsyncIterable(iterable, onError);

export const flickFromEventListener = (type: string, target: Stream.EventListener): Stream.Stream<unknown> =>
  Stream.fromEventListener(target, type);

export const flickUnfold = <S, A, E, R>(
  seed: S,
  f: (s: S) => Effect.Effect<Option.Option<readonly [A, S]>, E, R>,
): Stream.Stream<A, E, R> => Stream.unfold(seed, (s) => Effect.map(f(s), Option.getOrUndefined));

interface Concurrent {
  readonly concurrency: Option.Option<number>;
  readonly unordered: boolean;
}

export const flickMapEffectPar =
  <A, B, E2, R2>(options: Concurrent, f: (a: A) => Effect.Effect<B, E2, R2>) =>
  <E, R>(self: Stream.Stream<A, E, R>): Stream.Stream<B, E | E2, R | R2> =>
    Stream.mapEffect(self, f, { concurrency: concurrencyOf(options.concurrency), unordered: options.unordered });

interface Inner {
  readonly concurrency: Option.Option<number>;
  readonly bufferSize: Option.Option<number>;
}

const innerOf = (o: Inner) => ({
  concurrency: concurrencyOf(o.concurrency),
  bufferSize: Option.getOrUndefined(o.bufferSize),
});

export const flickFlatMapPar =
  <A, B, E2, R2>(options: Inner, f: (a: A) => Stream.Stream<B, E2, R2>) =>
  <E, R>(self: Stream.Stream<A, E, R>): Stream.Stream<B, E | E2, R | R2> =>
    Stream.flatMap(self, f, innerOf(options));

export const flickMergeAll =
  (options: Inner) =>
  <A, E, R>(streams: ReadonlyArray<Stream.Stream<A, E, R>>): Stream.Stream<A, E, R> =>
    Stream.mergeAll(streams, innerOf(options));

export const flickTimeoutOrElse =
  <B, E2, R2>(duration: Duration.Duration, fallback: Stream.Stream<B, E2, R2>) =>
  <A, E, R>(self: Stream.Stream<A, E, R>): Stream.Stream<A | B, E | E2, R | R2> =>
    Stream.timeoutOrElse(self, { duration, orElse: () => fallback });

const haltStrategies: Readonly<Record<string, Stream.HaltStrategy>> = {
  Left: "left",
  Right: "right",
  Both: "both",
  Either: "either",
};

export const flickMergeWith =
  <A2, E2, R2>(strategy: Data, that: Stream.Stream<A2, E2, R2>) =>
  <A, E, R>(self: Stream.Stream<A, E, R>): Stream.Stream<A | A2, E | E2, R | R2> =>
    Stream.merge(self, that, { haltStrategy: haltStrategies[constructorName(strategy._tag)] ?? "both" });

export const flickBuffer =
  (options: BufferOptions) =>
  <A, E, R>(self: Stream.Stream<A, E, R>): Stream.Stream<A, E, R> =>
    Stream.buffer(self, bufferOf(options));

export const flickBufferArray =
  (options: BufferOptions) =>
  <A, E, R>(self: Stream.Stream<A, E, R>): Stream.Stream<A, E, R> =>
    Stream.bufferArray(self, bufferOf(options));

export const flickMapAccum =
  <S, A, B>(initial: S, f: (s: S, a: A) => readonly [S, ReadonlyArray<B>]) =>
  <E, R>(self: Stream.Stream<A, E, R>): Stream.Stream<B, E, R> =>
    Stream.mapAccum(self, () => initial, f);

export const flickMapAccumArray =
  <S, A, B>(initial: S, f: (s: S, a: readonly [A, ...Array<A>]) => readonly [S, ReadonlyArray<B>]) =>
  <E, R>(self: Stream.Stream<A, E, R>): Stream.Stream<B, E, R> =>
    Stream.mapAccumArray(self, () => initial, f);

export const flickMapAccumEffect =
  <S, A, B, E2, R2>(initial: S, f: (s: S, a: A) => Effect.Effect<readonly [S, ReadonlyArray<B>], E2, R2>) =>
  <E, R>(self: Stream.Stream<A, E, R>): Stream.Stream<B, E | E2, R | R2> =>
    Stream.mapAccumEffect(self, () => initial, f);

export const flickMapAccumArrayEffect =
  <S, A, B, E2, R2>(
    initial: S,
    f: (s: S, a: readonly [A, ...Array<A>]) => Effect.Effect<readonly [S, ReadonlyArray<B>], E2, R2>,
  ) =>
  <E, R>(self: Stream.Stream<A, E, R>): Stream.Stream<B, E | E2, R | R2> =>
    Stream.mapAccumArrayEffect(self, () => initial, f);

export const flickScan =
  <S, A>(initial: S, f: (s: S, a: A) => S) =>
  <E, R>(self: Stream.Stream<A, E, R>): Stream.Stream<S, E, R> =>
    Stream.scan(self, () => initial, f);

export const flickScanEffect =
  <S, A, E2, R2>(initial: S, f: (s: S, a: A) => Effect.Effect<S, E2, R2>) =>
  <E, R>(self: Stream.Stream<A, E, R>): Stream.Stream<S, E | E2, R | R2> =>
    Stream.scanEffect(self, () => initial, f);

interface Throttle<C> {
  readonly cost: C;
  readonly units: number;
  readonly duration: Duration.Duration;
  readonly burst: Option.Option<number>;
  readonly strategy: Option.Option<Data>;
}

const throttleOf = <C>(o: Throttle<C>) => ({
  cost: o.cost,
  units: o.units,
  duration: o.duration,
  burst: Option.getOrUndefined(o.burst),
  strategy: Option.match(o.strategy, {
    onNone: () => "enforce" as const,
    onSome: (s) => (constructorName(s._tag) === "Shape" ? ("shape" as const) : ("enforce" as const)),
  }),
});

export const flickThrottle =
  <A>(options: Throttle<(chunk: readonly [A, ...Array<A>]) => number>) =>
  <E, R>(self: Stream.Stream<A, E, R>): Stream.Stream<A, E, R> =>
    Stream.throttle(self, throttleOf(options));

export const flickThrottleEffect =
  <A, E2, R2>(options: Throttle<(chunk: readonly [A, ...Array<A>]) => Effect.Effect<number, E2, R2>>) =>
  <E, R>(self: Stream.Stream<A, E, R>): Stream.Stream<A, E | E2, R | R2> =>
    Stream.throttleEffect(self, throttleOf(options));

export const flickBroadcast =
  (options: SplitOptions) =>
  <A, E, R>(self: Stream.Stream<A, E, R>): Effect.Effect<Stream.Stream<A, E>, never, R | Scope.Scope> =>
    Stream.broadcast(self, splitOf(options));

export const flickBroadcastN =
  (n: number, options: SplitOptions) =>
  <A, E, R>(self: Stream.Stream<A, E, R>): Effect.Effect<ReadonlyArray<Stream.Stream<A, E>>, never, R | Scope.Scope> =>
    Stream.broadcastN(self, { ...splitOf(options), n });

export const flickShare =
  (options: SplitOptions, idleTimeToLive: Option.Option<Duration.Duration>) =>
  <A, E, R>(self: Stream.Stream<A, E, R>): Effect.Effect<Stream.Stream<A, E>, never, R | Scope.Scope> =>
    Stream.share(self, { ...splitOf(options), idleTimeToLive: Option.getOrUndefined(idleTimeToLive) });

export const flickCatchCause =
  <A2, E2, R2>(f: (failure: Failure) => Stream.Stream<A2, E2, R2>) =>
  <A, E, R>(self: Stream.Stream<A, E, R>): Stream.Stream<A | A2, E2, R | R2> =>
    Stream.catchCause(self, (cause) => (unwinds(cause) ? Stream.fromEffect(passOn(cause)) : f(new Failure(cause))));

export const flickCatchCauseIf =
  <A2, E2, R2>(p: (failure: Failure) => boolean, f: (failure: Failure) => Stream.Stream<A2, E2, R2>) =>
  <A, E, R>(self: Stream.Stream<A, E, R>): Stream.Stream<A | A2, E | E2, R | R2> =>
    Stream.catchCauseIf(
      self,
      (cause) => !unwinds(cause) && p(new Failure(cause)),
      (cause) => f(new Failure(cause)),
    );

export const flickCatchCauseFilter =
  <B, A2, E2, R2>(
    filter: (failure: Failure) => Result.Result<B, Failure>,
    f: (b: B, failure: Failure) => Stream.Stream<A2, E2, R2>,
  ) =>
  <A, E, R>(self: Stream.Stream<A, E, R>): Stream.Stream<A | A2, unknown, R | R2> =>
    Stream.catchCauseFilter(
      self,
      (cause: Cause.Cause<E>) =>
        unwinds(cause)
          ? Result.fail(cause)
          : Result.mapError(filter(new Failure(cause)), (refused) => failureCause<unknown>(refused)),
      (b, cause) => f(b, new Failure(cause)),
    );

export const flickCatchDefect =
  <A2, E2, R2>(f: (failure: Failure) => Stream.Stream<A2, E2, R2>) =>
  <A, E, R>(self: Stream.Stream<A, E, R>): Stream.Stream<A | A2, E | E2, R | R2> =>
    Stream.catchDefect(self, (defect) => f(new Failure(Cause.die(defect))));

export const flickTapCause =
  <X, E2, R2>(f: (failure: Failure) => Effect.Effect<X, E2, R2>) =>
  <A, E, R>(self: Stream.Stream<A, E, R>): Stream.Stream<A, E | E2, R | R2> =>
    Stream.tapCause(self, (cause) => f(new Failure(cause)));

export const flickTapDefect =
  <X, E2, R2>(f: (failure: Failure) => Effect.Effect<X, E2, R2>) =>
  <A, E, R>(self: Stream.Stream<A, E, R>): Stream.Stream<A, E | E2, R | R2> =>
    Stream.tapDefect(self, (defect) => f(new Failure(Cause.die(defect))));

export const flickOrElseIfEmpty =
  <A2, E2, R2>(fallback: Stream.Stream<A2, E2, R2>) =>
  <A, E, R>(self: Stream.Stream<A, E, R>): Stream.Stream<A | A2, E | E2, R | R2> =>
    Stream.orElseIfEmpty(self, () => fallback);

export const flickOnExit =
  <X, R2>(f: (exit: ReturnType<typeof flickExit>) => Effect.Effect<X, never, R2>) =>
  <A, E, R>(self: Stream.Stream<A, E, R>): Stream.Stream<A, E, R | R2> =>
    Stream.onExit(self, (exit: Exit.Exit<unknown, E>) => f(flickExit(exit)));

export const flickOnError =
  <X, R2>(f: (failure: Failure) => Effect.Effect<X, never, R2>) =>
  <A, E, R>(self: Stream.Stream<A, E, R>): Stream.Stream<A, E, R | R2> =>
    Stream.onError(self, (cause) => f(new Failure(cause)));

export const flickRunFold =
  <Z, A>(initial: Z, f: (z: Z, a: A) => Z) =>
  <E, R>(self: Stream.Stream<A, E, R>): Effect.Effect<Z, E, R> =>
    Stream.runFold(self, () => initial, f);

export const flickRunFoldEffect =
  <Z, A, E2, R2>(initial: Z, f: (z: Z, a: A) => Effect.Effect<Z, E2, R2>) =>
  <E, R>(self: Stream.Stream<A, E, R>): Effect.Effect<Z, E | E2, R | R2> =>
    Stream.runFoldEffect(self, () => initial, f);

export const flickMkBytes = <E, R>(self: Stream.Stream<ReadonlyArray<number>, E, R>): Effect.Effect<Array<number>, E, R> =>
  Effect.map(Stream.mkUint8Array(toBytes(self)), (bytes) => Array.from(bytes));

export const flickToPubSub =
  (options: SplitOptions) =>
  <A, E, R>(self: Stream.Stream<A, E, R>) =>
    Stream.toPubSub(self, splitOf(options));

export const flickToQueue =
  (options: BufferOptions) =>
  <A, E, R>(self: Stream.Stream<A, E, R>) =>
    Stream.toQueue(self, bufferOf(options));

export const flickDecodeText = <E, R>(self: Stream.Stream<ReadonlyArray<number>, E, R>): Stream.Stream<string, E, R> =>
  Stream.decodeText(toBytes(self));

export const flickEncodeText = <E, R>(self: Stream.Stream<string, E, R>): Stream.Stream<ReadonlyArray<number>, E, R> =>
  fromBytes(Stream.encodeText(self));

export const flickLimitBytes =
  <E, R>(limit: ByteSize.ByteSize, onLimit: Stream.Stream<ReadonlyArray<number>, E, R>) =>
  (self: Stream.Stream<ReadonlyArray<number>, E, R>): Stream.Stream<ReadonlyArray<number>, E, R> =>
    fromBytes(Stream.limitBytes(toBytes(self), limit, () => toBytes(onLimit)));

// The platform's streams: Effect's `PlatformError` becomes the Flick error,
// which the Flick operations already fail with.

const isPlatform = (e: unknown): e is PlatformError.PlatformError | PlatformError.BadArgument =>
  PlatformError.isPlatformError(e) || e instanceof PlatformError.BadArgument;

const flickErrors = <A, E, R>(self: Stream.Stream<A, E, R>) =>
  Stream.mapError(self, (e) => (isPlatform(e) ? toFlick(e) : e));

const notFound = (module: string, method: string, path: string) =>
  toFlick(
    PlatformError.systemError({
      _tag: "NotFound",
      module,
      method,
      description: "the implementation of the service has no native streams",
      pathOrDescriptor: path,
    }),
  );

const FlickFileSystem = /*#__PURE__*/ service("FileSystem.FileSystem");
const FlickStdio = /*#__PURE__*/ service("Stdio.Stdio");

const watchTag = (name: string): string => tag(`Stream.Platform.WatchEvent.${name}`);

const toFlickEvent = (event: FileSystem.WatchEvent): Data => ({ _tag: watchTag(event._tag), _0: event.path });

const watchNames: Readonly<Record<string, FileSystem.WatchEvent["_tag"]>> = {
  Create: "Create",
  Update: "Update",
  Remove: "Remove",
};

const toEffectEvent = (event: Data): FileSystem.WatchEvent => ({
  _tag: watchNames[constructorName(event._tag)] ?? "Update",
  path: String(event["_0"]),
});

type FlickStream = Stream.Stream<Data, unknown>;

const watched = (path: string, recursive: boolean, backend: Option.Option<FileSystem.WatchBackend["Service"]>) =>
  Stream.unwrap(
    Effect.map(Effect.service(FlickFileSystem), (flick) => {
      const fs = nativeFileSystem(flick);
      if (fs === undefined) return Stream.fail(notFound("FileSystem", "watch", path));
      const events = fs.watch(path, { recursive });
      const provided = Option.isSome(backend)
        ? Stream.provideService(events, FileSystem.WatchBackend, backend.value)
        : events;
      return Stream.map(flickErrors(provided), toFlickEvent);
    }),
  );

export const flickWatch = (options: { readonly recursive: boolean }, path: string) =>
  watched(path, options.recursive, Option.none());

export const flickWatchWith = (
  backend: (path: string, stat: ReturnType<typeof info>) => Option.Option<FlickStream>,
  options: { readonly recursive: boolean },
  path: string,
) =>
  watched(
    path,
    options.recursive,
    Option.some({
      register: (at: string, stat: FileSystem.File.Info) =>
        Option.map(backend(at, info(stat)), (events) =>
          // oxlint-disable-next-line typescript/no-unsafe-type-assertion -- the backend fails with the Flick form of `PlatformError`, which `watch` passes on and `flickErrors` keeps
          Stream.map(events, toEffectEvent) as Stream.Stream<FileSystem.WatchEvent, PlatformError.PlatformError>,
        ),
    }),
  );

export const flickReadFile = (
  options: {
    readonly offset: Option.Option<ByteSize.ByteSize>;
    readonly bytesToRead: Option.Option<ByteSize.ByteSize>;
    readonly chunkSize: Option.Option<number>;
  },
  path: string,
) =>
  Stream.unwrap(
    Effect.map(effectFileSystem, (fs) =>
      fromBytes(
        flickErrors(
          FileSystem.make(fs).stream(path, {
            offset: Option.getOrUndefined(options.offset),
            bytesToRead: Option.getOrUndefined(options.bytesToRead),
            chunkSize: Option.getOrUndefined(options.chunkSize),
          }),
        ),
      ),
    ),
  );

export const flickStdin = /*#__PURE__*/ Stream.unwrap(
  Effect.map(Effect.service(FlickStdio), (flick) => {
    const stdio = nativeStdio(flick);
    return stdio === undefined ? Stream.fail(notFound("Stdio", "stdin", "stdin")) : fromBytes(flickErrors(stdio.stdin));
  }),
);
