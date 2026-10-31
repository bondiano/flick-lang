// `ScopedRef`: a reference to a resource that `set` replaces. Its value is
// used only under a lease (`use`); the previous resource is released once
// the last lease of it ends, and the last one when the owner of the
// reference's region closes.
import { Effect, Scope, Semaphore } from "effect";

import { type Holder, acquireHolder, leased, plain, retire } from "./lease.ts";

/** `ScopedRef s a`. */
export interface FlickScopedRef<A> {
  current: Holder<A>;
  readonly lock: Semaphore.Semaphore;
}

const owned = <A>(holder: Holder<A>): Effect.Effect<FlickScopedRef<A>, never, Scope.Scope> =>
  Effect.gen(function* () {
    const ref: FlickScopedRef<A> = { current: holder, lock: Semaphore.makeUnsafe(1) };
    yield* Scope.addFinalizer(yield* Scope.Scope, Effect.suspend(() => retire(ref.current)));
    return ref;
  });

/** `ScopedRef.from_acquire acquire`. */
export const flickFromAcquire = <A, E, R>(
  acquire: Effect.Effect<A, E, R>,
): Effect.Effect<FlickScopedRef<A>, E, Exclude<R, Scope.Scope> | Scope.Scope> =>
  // the holder is registered under the mask it was acquired in, as `acquireRelease` does
  Effect.uninterruptibleMask((restore) => Effect.flatMap(acquireHolder(acquire, restore), owned));

/** `ScopedRef.make f`: a value that needs no release. */
export const flickMake = <A>(f: () => A): Effect.Effect<FlickScopedRef<A>, never, Scope.Scope> =>
  Effect.suspend(() => owned(plain(f())));

/** `ScopedRef.use ref f`. */
export const flickUse = <A, B, E, R>(
  ref: FlickScopedRef<A>,
  f: (value: A) => Effect.Effect<B, E, R>,
): Effect.Effect<B, E, R> => leased(Effect.sync(() => ref.current), f);

/** `ScopedRef.set acquire ref`: if `acquire` fails, the previous resource stays. */
export const flickSet =
  <A, E, R>(acquire: Effect.Effect<A, E, R>) =>
  (ref: FlickScopedRef<A>): Effect.Effect<void, E, Exclude<R, Scope.Scope>> =>
    ref.lock.withPermit(
      // an interruption between the swap and `retire` would leave the previous resource unreleased
      Effect.uninterruptibleMask((restore) =>
        Effect.gen(function* () {
          const next = yield* acquireHolder(acquire, restore);
          const previous = ref.current;
          ref.current = next;
          yield* retire(previous);
        }),
      ),
    );
