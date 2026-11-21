// The standard HashSet bindings over Flick's own persistent hash set.
import {
  type FlickHashSet,
  type Witness,
  hashSetInsert,
  hashSetMember,
  hashSetSize,
  hashSetToSeq,
  hashSetWith,
} from "../collections.ts";

type Set<A> = FlickHashSet<A>;
export const flickIsEmpty = <A>(set: Set<A>): boolean => hashSetSize(set) === 0;
export const flickIntersection = <A>(that: Set<A>, set: Set<A>): Set<A> => {
  let out = hashSetWith(set.witness);
  for (const value of hashSetToSeq(set)) if (hashSetMember(value, that)) out = hashSetInsert(value, out);
  return out;
};
export const flickDifference = <A>(that: Set<A>, set: Set<A>): Set<A> => {
  let out = hashSetWith(set.witness);
  for (const value of hashSetToSeq(set)) if (!hashSetMember(value, that)) out = hashSetInsert(value, out);
  return out;
};
export const flickIsSubset = <A>(that: Set<A>, set: Set<A>): boolean => {
  for (const value of hashSetToSeq(set)) if (!hashSetMember(value, that)) return false;
  return true;
};
export const flickMap = <A, B>(witness: Witness<B>, f: (value: A) => B, set: Set<A>): Set<B> => {
  let out = hashSetWith(witness);
  for (const value of hashSetToSeq(set)) out = hashSetInsert(f(value), out);
  return out;
};
export const flickFilter = <A>(f: (value: A) => boolean, set: Set<A>): Set<A> => {
  let out = hashSetWith(set.witness);
  for (const value of hashSetToSeq(set)) if (f(value)) out = hashSetInsert(value, out);
  return out;
};
export const flickSome = <A>(f: (value: A) => boolean, set: Set<A>): boolean => {
  for (const value of hashSetToSeq(set)) if (f(value)) return true;
  return false;
};
export const flickEvery = <A>(f: (value: A) => boolean, set: Set<A>): boolean => {
  for (const value of hashSetToSeq(set)) if (!f(value)) return false;
  return true;
};
export const flickReduce = <A, Z>(zero: Z, f: (acc: Z, value: A) => Z, set: Set<A>): Z => {
  let acc = zero;
  for (const value of hashSetToSeq(set)) acc = f(acc, value);
  return acc;
};
