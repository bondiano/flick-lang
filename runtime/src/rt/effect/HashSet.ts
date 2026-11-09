// The standard HashSet bindings over Flick's own persistent hash set.
import {
  type FlickHashSet,
  type Witness,
  hashSetFromList,
  hashSetMember,
  hashSetSize,
  hashSetToList,
} from "../collections.ts";

type Set<A> = FlickHashSet<A>;
export const flickIsEmpty = <A>(set: Set<A>): boolean => hashSetSize(set) === 0;
export const flickIntersection = <A>(that: Set<A>, set: Set<A>): Set<A> =>
  hashSetFromList(set.witness, hashSetToList(set).filter((value) => hashSetMember(value, that)));
export const flickDifference = <A>(that: Set<A>, set: Set<A>): Set<A> =>
  hashSetFromList(set.witness, hashSetToList(set).filter((value) => !hashSetMember(value, that)));
export const flickIsSubset = <A>(that: Set<A>, set: Set<A>): boolean =>
  hashSetToList(set).every((value) => hashSetMember(value, that));
export const flickMap = <A, B>(witness: Witness<B>, f: (value: A) => B, set: Set<A>): Set<B> =>
  hashSetFromList(witness, hashSetToList(set).map(f));
export const flickFilter = <A>(f: (value: A) => boolean, set: Set<A>): Set<A> =>
  hashSetFromList(set.witness, hashSetToList(set).filter(f));
export const flickSome = <A>(f: (value: A) => boolean, set: Set<A>): boolean => hashSetToList(set).some(f);
export const flickEvery = <A>(f: (value: A) => boolean, set: Set<A>): boolean => hashSetToList(set).every(f);
export const flickReduce = <A, Z>(zero: Z, f: (acc: Z, value: A) => Z, set: Set<A>): Z =>
  hashSetToList(set).reduce(f, zero);
