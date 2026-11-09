// NonEmpty uses the same array representation as List. These are the pure
// `effect/Array` operations its built-in module binds on the own runtime.
import { item } from "../../internal.ts";
import { GT, LT, type Ordering } from "../../core/data.ts";

export * from "../../core/Array.ts";

type List<A> = ReadonlyArray<A>;
type Eqv<A> = (a: A, b: A) => boolean;
const count = (n: number): number => n > 0 ? Math.floor(n) : 0;

export const append = <A>(value: A) => (self: List<A>): Array<A> => [...self, value];
export const makeBy = <A>(n: number, f: (i: number) => A): Array<A> =>
  Array.from({ length: Math.max(1, count(n)) }, (_, i) => f(i));
export const range = (start: number, end: number): Array<number> =>
  Array.from({ length: Math.max(1, count(end - start + 1)) }, (_, i) => start + i);
export const replicate = (n: number) => <A>(value: A): Array<A> =>
  Array.from({ length: Math.max(1, count(n)) }, () => value);
export const lastNonEmpty = <A>(self: List<A>): A => item(self, self.length - 1);
export const tailNonEmpty = <A>(self: List<A>): Array<A> => self.slice(1);
export const initNonEmpty = <A>(self: List<A>): Array<A> => self.slice(0, -1);
export const modifyHeadNonEmpty = <A>(f: (value: A) => A) => (self: List<A>): Array<A> =>
  [f(item(self, 0)), ...self.slice(1)];
export const setHeadNonEmpty = <A>(value: A) => (self: List<A>): Array<A> =>
  [value, ...self.slice(1)];
export const modifyLastNonEmpty = <A>(f: (value: A) => A) => (self: List<A>): Array<A> =>
  [...self.slice(0, -1), f(item(self, self.length - 1))];
export const setLastNonEmpty = <A>(value: A) => (self: List<A>): Array<A> =>
  [...self.slice(0, -1), value];
export const splitAtNonEmpty = (n: number) => <A>(self: List<A>): [Array<A>, Array<A>] =>
  [self.slice(0, Math.max(1, count(n))), self.slice(Math.max(1, count(n)))];
export const unappend = <A>(self: List<A>): [Array<A>, A] =>
  [self.slice(0, -1), item(self, self.length - 1)];
export const unprepend = <A>(self: List<A>): [A, Array<A>] =>
  [item(self, 0), self.slice(1)];
export const groupWith = <A>(same: Eqv<A>) => (self: List<A>): Array<Array<A>> => {
  const groups: Array<Array<A>> = [];
  for (const value of self) {
    const group = groups.at(-1);
    if (group && same(item(group, 0), value)) group.push(value);
    else groups.push([value]);
  }
  return groups;
};
export const flickMax = <A>(compare: (a: A, b: A) => Ordering) => (self: List<A>): A => {
  let best = item(self, 0);
  for (const value of self.slice(1)) if (compare(value, best)._tag === GT._tag) best = value;
  return best;
};
export const flickMin = <A>(compare: (a: A, b: A) => Ordering) => (self: List<A>): A => {
  let best = item(self, 0);
  for (const value of self.slice(1)) if (compare(value, best)._tag === LT._tag) best = value;
  return best;
};
