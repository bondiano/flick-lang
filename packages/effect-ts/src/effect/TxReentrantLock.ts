// Transactional lock ownership follows the logical fiber across the bridge.
import { Effect, HashMap, Option, TxRef, type TxReentrantLock as Lock } from "effect";
import { dual } from "effect/Function";

import { fiberIdentity } from "../bridge.ts";

export * from "effect/TxReentrantLock";

interface State {
  readonly readers: HashMap.HashMap<number, number>;
  readonly writer: Option.Option<readonly [number, number]>;
}

// Effect 4.1.2 keeps the journaled state on this reference; its declaration
// marks it internal, so validate the reference at the pinned adapter boundary.
const stateRef = (self: Lock.TxReentrantLock): TxRef.TxRef<State> => {
  const ref: unknown = Reflect.get(self, "stateRef");
  if (typeof ref !== "object" || ref === null || !("~effect/TxRef" in ref))
    throw new TypeError("a transactional lock has no state reference");
  // oxlint-disable-next-line typescript/no-unsafe-type-assertion -- the pinned lock implementation owns this reference's state
  return ref as TxRef.TxRef<State>;
};

const acquireFor = (self: Lock.TxReentrantLock, id: number, write: boolean): Effect.Effect<number> =>
  Effect.gen(function* () {
    const ref = stateRef(self);
    const state = yield* TxRef.get(ref);
    const writer = Option.getOrUndefined(state.writer);
    if (writer !== undefined && writer[0] !== id) return yield* Effect.txRetry;
    if (write) {
      if (Array.from(state.readers).some(([reader, count]) => reader !== id && count > 0))
        return yield* Effect.txRetry;
      const count = (writer?.[1] ?? 0) + 1;
      yield* TxRef.set(ref, { ...state, writer: Option.some([id, count] as const) });
      return count;
    }
    const count = Option.getOrElse(HashMap.get(state.readers, id), () => 0) + 1;
    yield* TxRef.set(ref, { ...state, readers: HashMap.set(state.readers, id, count) });
    return count;
  }).pipe(Effect.tx);

const releaseFor = (self: Lock.TxReentrantLock, id: number, write: boolean): Effect.Effect<number> =>
  Effect.gen(function* () {
    const ref = stateRef(self);
    const state = yield* TxRef.get(ref);
    if (write) {
      const writer = Option.getOrUndefined(state.writer);
      if (writer === undefined || writer[0] !== id) return 0;
      const count = writer[1] - 1;
      yield* TxRef.set(ref, { ...state, writer: count === 0 ? Option.none() : Option.some([id, count] as const) });
      return count;
    }
    const held = Option.getOrElse(HashMap.get(state.readers, id), () => 0);
    if (held === 0) return 0;
    const count = held - 1;
    yield* TxRef.set(ref, { ...state, readers: count === 0 ? HashMap.remove(state.readers, id) : HashMap.set(state.readers, id, count) });
    return count;
  }).pipe(Effect.tx);

export const acquireRead = (self: Lock.TxReentrantLock): Effect.Effect<number> =>
  Effect.flatMap(fiberIdentity, (id) => acquireFor(self, id, false));
export const acquireWrite = (self: Lock.TxReentrantLock): Effect.Effect<number> =>
  Effect.flatMap(fiberIdentity, (id) => acquireFor(self, id, true));
export const releaseRead = (self: Lock.TxReentrantLock): Effect.Effect<number> =>
  Effect.flatMap(fiberIdentity, (id) => releaseFor(self, id, false));
export const releaseWrite = (self: Lock.TxReentrantLock): Effect.Effect<number> =>
  Effect.flatMap(fiberIdentity, (id) => releaseFor(self, id, true));

const scoped = (self: Lock.TxReentrantLock, write: boolean) =>
  Effect.flatMap(fiberIdentity, (id) => Effect.acquireRelease(acquireFor(self, id, write),
    () => releaseFor(self, id, write), { interruptible: true }));
export const readLock = (self: Lock.TxReentrantLock) => scoped(self, false);
export const writeLock = (self: Lock.TxReentrantLock) => scoped(self, true);

const withLockOf = <A, E, R>(self: Lock.TxReentrantLock, effect: Effect.Effect<A, E, R>, write: boolean): Effect.Effect<A, E, R> =>
  Effect.flatMap(fiberIdentity, (id) => Effect.uninterruptibleMask((restore) =>
    Effect.flatMap(restore(acquireFor(self, id, write)), () => Effect.ensuring(restore(effect), releaseFor(self, id, write)))));
export const withReadLock: typeof Lock.withReadLock = dual(2,
  <A, E, R>(self: Lock.TxReentrantLock, effect: Effect.Effect<A, E, R>) => withLockOf(self, effect, false));
export const withWriteLock: typeof Lock.withWriteLock = dual(2,
  <A, E, R>(self: Lock.TxReentrantLock, effect: Effect.Effect<A, E, R>) => withLockOf(self, effect, true));
export const withLock = withWriteLock;
