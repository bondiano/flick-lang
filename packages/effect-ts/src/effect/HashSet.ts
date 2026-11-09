// Effect's `HashSet` for the bindings of the standard library, and the
// adapters over Flick's sets: each element is boxed with the set's witness, so
// callbacks get the element itself and results keep the witness.
import { HashSet } from "effect";

import { type FlickHashSet, HashKey, hashSetOf, type Witness } from "../stdlib/hash.ts";

export * from "effect/HashSet";

type Hs<A> = FlickHashSet<A>;

const keep = <A>(s: Hs<A>, native: HashSet.HashSet<HashKey<A>>): Hs<A> => hashSetOf(s.witness, native);

export const flickIsEmpty = <A>(s: Hs<A>): boolean => HashSet.isEmpty(s.native);
export const flickIntersection = <A>(that: Hs<A>, s: Hs<A>): Hs<A> => keep(s, HashSet.intersection(s.native, that.native));
export const flickDifference = <A>(that: Hs<A>, s: Hs<A>): Hs<A> => keep(s, HashSet.difference(s.native, that.native));
export const flickIsSubset = <A>(that: Hs<A>, s: Hs<A>): boolean => HashSet.isSubset(s.native, that.native);
export const flickMap = <A, B>(w: Witness<B>, f: (a: A) => B, s: Hs<A>): Hs<B> =>
  hashSetOf(
    w,
    HashSet.map(s.native, (k) => new HashKey(f(k.value), w)),
  );
export const flickFilter = <A>(f: (a: A) => boolean, s: Hs<A>): Hs<A> =>
  keep(
    s,
    HashSet.filter(s.native, (k) => f(k.value)),
  );
export const flickSome = <A>(f: (a: A) => boolean, s: Hs<A>): boolean => HashSet.some(s.native, (k) => f(k.value));
export const flickEvery = <A>(f: (a: A) => boolean, s: Hs<A>): boolean => HashSet.every(s.native, (k) => f(k.value));
export const flickReduce = <A, Z>(zero: Z, f: (z: Z, a: A) => Z, s: Hs<A>): Z =>
  HashSet.reduce(s.native, zero, (z, k) => f(z, k.value));
