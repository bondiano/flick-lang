// Leases on the values of library owners (`ScopedRef`, `Resource`,
// `ScopedCache`): a value is used only inside a lease, and a release the
// library starts (a replacement, a refresh, an eviction) waits until no lease
// holds the value, so a value is never released while it is in use.
import { Effect, Exit, Scope } from "effect";

/** A value and the scope of what its acquisition registered. */
export interface Holder<A> {
  readonly value: A;
  readonly scope: Scope.Closeable | undefined;
  leases: number;
  retired: boolean;
  closed: boolean;
}

/** A value that needs no release. */
export const plain = <A>(value: A): Holder<A> => ({
  value,
  scope: undefined,
  leases: 0,
  retired: false,
  closed: false,
});

/** Runs `acquire` with a fresh scope as its `Scope`: what it acquires belongs to the holder. */
export const acquireHolder = <A, E, R>(
  acquire: Effect.Effect<A, E, R>,
): Effect.Effect<Holder<A>, E, Exclude<R, Scope.Scope>> =>
  Effect.uninterruptibleMask((restore) =>
    Effect.gen(function* () {
      const scope = yield* Scope.make();
      const exit = yield* Effect.exit(restore(Scope.provide(acquire, scope)));
      if (Exit.isFailure(exit)) {
        yield* Scope.close(scope, exit);
        return yield* Effect.failCause(exit.cause);
      }
      return { value: exit.value, scope, leases: 0, retired: false, closed: false };
    }),
  );

const release = <A>(holder: Holder<A>): Effect.Effect<void> => {
  holder.closed = true;
  return holder.scope === undefined ? Effect.void : Scope.close(holder.scope, Exit.void);
};

/** The owner no longer offers `holder`: it is released once no lease holds it. */
export const retire = <A>(holder: Holder<A>): Effect.Effect<void> =>
  Effect.suspend(() => {
    if (holder.retired) return Effect.void;
    holder.retired = true;
    return holder.leases === 0 ? release(holder) : Effect.void;
  });

/** Runs `use` with the value of `holder` under a lease, or `closed` if it is released already. */
export const lease = <A, B, E, R, E2, R2>(
  holder: Holder<A>,
  use: (value: A) => Effect.Effect<B, E, R>,
  closed: () => Effect.Effect<B, E2, R2>,
): Effect.Effect<B, E | E2, R | R2> =>
  Effect.acquireUseRelease(
    Effect.sync(() => {
      if (holder.closed) return false;
      holder.leases += 1;
      return true;
    }),
    (held): Effect.Effect<B, E | E2, R | R2> => (held ? use(holder.value) : closed()),
    (held) =>
      Effect.suspend(() => {
        if (!held) return Effect.void;
        holder.leases -= 1;
        return holder.retired && holder.leases === 0 ? release(holder) : Effect.void;
      }),
  );

/** Runs `use` under a lease of the holder `get` gives; one released meanwhile is asked for again. */
export const leased = <A, B, E, R, E2, R2>(
  get: Effect.Effect<Holder<A>, E2, R2>,
  use: (value: A) => Effect.Effect<B, E, R>,
): Effect.Effect<B, E | E2, R | R2> =>
  Effect.flatMap(get, (holder) => lease(holder, use, () => leased(get, use)));
