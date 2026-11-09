// Effect's `TxHashSet` for the bindings of the standard library. A Flick
// `TxHashSet w a` keeps its witness beside Effect's set, whose elements are
// wrapped with it as in `HashSet w a`, so each operation is an adapter that
// wraps elements going in and unwraps them coming out.
import { Effect, TxHashSet } from "effect";

import { type FlickHashSet, HashKey as Key, type Witness } from "../stdlib/hash.ts";

export * from "effect/TxHashSet";

/** `TxHashSet w a`. */
export interface FlickTxHashSet<A> {
  readonly witness: Witness<A>;
  readonly native: TxHashSet.TxHashSet<Key<A>>;
}

const wrap =
  <A>(witness: Witness<A>) =>
  (native: TxHashSet.TxHashSet<Key<A>>): FlickTxHashSet<A> =>
    Object.freeze({ witness, native });

const keys = <A>(witness: Witness<A>, xs: ReadonlyArray<A>): Array<Key<A>> => xs.map((x) => new Key(x, witness));

const key = <A>(self: FlickTxHashSet<A>, x: A): Key<A> => new Key(x, self.witness);

export const flickEmpty = <A>(witness: Witness<A>): Effect.Effect<FlickTxHashSet<A>> =>
  Effect.map(TxHashSet.empty<Key<A>>(), wrap(witness));

/** `TxHashSet.make`: the elements as Effect's rest arguments. */
export const flickMake = <A>(witness: Witness<A>, xs: ReadonlyArray<A>): Effect.Effect<FlickTxHashSet<A>> =>
  Effect.map(TxHashSet.make(...keys(witness, xs)), wrap(witness));

export const flickFromIterable = <A>(witness: Witness<A>, xs: ReadonlyArray<A>): Effect.Effect<FlickTxHashSet<A>> =>
  Effect.map(TxHashSet.fromIterable(keys(witness, xs)), wrap(witness));

export const flickFromHashSet = <A>(set: FlickHashSet<A>): Effect.Effect<FlickTxHashSet<A>> =>
  Effect.map(TxHashSet.fromHashSet(set.native), wrap(set.witness));

export const flickAdd = <A>(x: A, self: FlickTxHashSet<A>): Effect.Effect<void> => TxHashSet.add(self.native, key(self, x));
export const flickRemove = <A>(x: A, self: FlickTxHashSet<A>): Effect.Effect<boolean> =>
  TxHashSet.remove(self.native, key(self, x));
export const flickHas = <A>(x: A, self: FlickTxHashSet<A>): Effect.Effect<boolean> =>
  TxHashSet.has(self.native, key(self, x));
export const flickSize = <A>(self: FlickTxHashSet<A>): Effect.Effect<number> => TxHashSet.size(self.native);
export const flickIsEmpty = <A>(self: FlickTxHashSet<A>): Effect.Effect<boolean> => TxHashSet.isEmpty(self.native);
export const flickIsNonEmpty = <A>(self: FlickTxHashSet<A>): Effect.Effect<boolean> => TxHashSet.isNonEmpty(self.native);
export const flickClear = <A>(self: FlickTxHashSet<A>): Effect.Effect<void> => TxHashSet.clear(self.native);

export const flickUnion = <A>(that: FlickTxHashSet<A>, self: FlickTxHashSet<A>): Effect.Effect<FlickTxHashSet<A>> =>
  Effect.map(TxHashSet.union(self.native, that.native), wrap(self.witness));
export const flickIntersection = <A>(that: FlickTxHashSet<A>, self: FlickTxHashSet<A>): Effect.Effect<FlickTxHashSet<A>> =>
  Effect.map(TxHashSet.intersection(self.native, that.native), wrap(self.witness));
export const flickDifference = <A>(that: FlickTxHashSet<A>, self: FlickTxHashSet<A>): Effect.Effect<FlickTxHashSet<A>> =>
  Effect.map(TxHashSet.difference(self.native, that.native), wrap(self.witness));
export const flickIsSubset = <A>(that: FlickTxHashSet<A>, self: FlickTxHashSet<A>): Effect.Effect<boolean> =>
  TxHashSet.isSubset(self.native, that.native);

export const flickSome = <A>(f: (x: A) => boolean, self: FlickTxHashSet<A>): Effect.Effect<boolean> =>
  TxHashSet.some(self.native, (k) => f(k.value));
export const flickEvery = <A>(f: (x: A) => boolean, self: FlickTxHashSet<A>): Effect.Effect<boolean> =>
  TxHashSet.every(self.native, (k) => f(k.value));
export const flickFilter = <A>(f: (x: A) => boolean, self: FlickTxHashSet<A>): Effect.Effect<FlickTxHashSet<A>> =>
  Effect.map(
    TxHashSet.filter(self.native, (k: Key<A>) => f(k.value)),
    wrap(self.witness),
  );

/** `TxHashSet.map`: the new elements are compared by the witness given. */
export const flickMap = <A, B>(
  witness: Witness<B>,
  f: (x: A) => B,
  self: FlickTxHashSet<A>,
): Effect.Effect<FlickTxHashSet<B>> =>
  Effect.map(
    TxHashSet.map(self.native, (k) => new Key(f(k.value), witness)),
    wrap(witness),
  );

export const flickReduce = <A, B>(zero: B, f: (acc: B, x: A) => B, self: FlickTxHashSet<A>): Effect.Effect<B> =>
  TxHashSet.reduce(self.native, zero, (acc, k) => f(acc, k.value));

/** `TxHashSet.to_hash_set`: a `HashSet w a` of the same witness. */
export const flickToHashSet = <A>(self: FlickTxHashSet<A>): Effect.Effect<FlickHashSet<A>> =>
  Effect.map(TxHashSet.toHashSet(self.native), (native) => Object.freeze({ witness: self.witness, native }));
