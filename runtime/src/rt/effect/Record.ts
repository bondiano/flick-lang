// Effect's ReadonlyRecord operations on plain JavaScript objects. Values keep
// ECMAScript own-key order, and updates return a new dictionary.
// oxlint-disable typescript/no-unsafe-type-assertion -- every keyed read follows hasOwn
import { none, some as someOption, type Option, type Result } from "../../core/data.ts";

type Dict<A> = Readonly<Record<string, A>>;
type Pair<A> = readonly [string, A];
interface Combiner<A> { readonly combine: (a: A, b: A) => A }
interface Reducer<A> extends Combiner<A> { readonly initial: A }
const entries = <A>(record: Dict<A>): Array<[string, A]> => Object.entries(record);
const hasKey = <A>(record: Dict<A>, key: string): boolean => Object.hasOwn(record, key);
const fromPairs = <A>(pairs: Iterable<Pair<A>>): Record<string, A> => Object.fromEntries(pairs);

export const empty = <A>(): Dict<A> => ({});
export const isEmptyReadonlyRecord = <A>(record: Dict<A>): boolean => Object.keys(record).length === 0;
export const fromEntries = <A>(pairs: ReadonlyArray<Pair<A>>): Dict<A> => fromPairs(pairs);
export const fromIterableWith = <A, B>(f: (value: A) => Pair<B>) =>
  (values: ReadonlyArray<A>): Dict<B> => fromPairs(values.map(f));
export const fromIterableBy = <A>(f: (value: A) => string) =>
  (values: ReadonlyArray<A>): Dict<A> => fromPairs(values.map((value) => [f(value), value]));
export const singleton = <A>(key: string, value: A): Dict<A> => ({ [key]: value });
export const collect = <A, B>(f: (key: string, value: A) => B) =>
  (record: Dict<A>): Array<B> => entries(record).map(([key, value]) => f(key, value));
export { entries as toEntries };
export const keys = <A>(record: Dict<A>): Array<string> => Object.keys(record);
export const values = <A>(record: Dict<A>): Array<A> => Object.values(record);
export const size = <A>(record: Dict<A>): number => Object.keys(record).length;
export const has = (key: string) => <A>(record: Dict<A>): boolean => hasKey(record, key);
export const get = (key: string) => <A>(record: Dict<A>): Option<A> =>
  hasKey(record, key) ? someOption(record[key] as A) : none;
export const set = <A>(key: string, value: A) => (record: Dict<A>): Dict<A> => ({ ...record, [key]: value });
export const remove = (key: string) => <A>(record: Dict<A>): Dict<A> => {
  if (!hasKey(record, key)) return record;
  return fromPairs(entries(record).filter(([name]) => name !== key));
};
export const modify = <A>(key: string, f: (value: A) => A) => (record: Dict<A>): Option<Dict<A>> =>
  hasKey(record, key) ? someOption(set(key, f(record[key] as A))(record)) : none;
export const replace = <A>(key: string, value: A) => (record: Dict<A>): Option<Dict<A>> =>
  hasKey(record, key) ? someOption(set(key, value)(record)) : none;
export const pop = (key: string) => <A>(record: Dict<A>): Option<readonly [A, Dict<A>]> =>
  hasKey(record, key) ? someOption([record[key] as A, remove(key)(record)]) : none;
export const map = <A, B>(f: (value: A, key: string) => B) =>
  (record: Dict<A>): Dict<B> => fromPairs(entries(record).map(([key, value]) => [key, f(value, key)]));
export const mapKeys = <A>(f: (key: string, value: A) => string) =>
  (record: Dict<A>): Dict<A> => fromPairs(entries(record).map(([key, value]) => [f(key, value), value]));
export const mapEntries = <A, B>(f: (value: A, key: string) => Pair<B>) =>
  (record: Dict<A>): Dict<B> => fromPairs(entries(record).map(([key, value]) => f(value, key)));
export const filter = <A>(predicate: (value: A, key: string) => boolean) =>
  (record: Dict<A>): Dict<A> => fromPairs(entries(record).filter(([key, value]) => predicate(value, key)));
export const filterMap = <A, B, E>(f: (value: A, key: string) => Result<B, E>) =>
  (record: Dict<A>): Dict<B> => fromPairs(entries(record).flatMap(([key, value]) => {
    const result = f(value, key);
    return result._tag === "Success" ? [[key, result.success] as const] : [];
  }));
export const partition = <A, B, E>(f: (value: A, key: string) => Result<B, E>) =>
  (record: Dict<A>): readonly [Dict<B>, Dict<E>] => {
    const successes: Array<Pair<B>> = [];
    const failures: Array<Pair<E>> = [];
    for (const [key, value] of entries(record)) {
      const result = f(value, key);
      if (result._tag === "Success") successes.push([key, result.success]);
      else failures.push([key, result.failure]);
    }
    return [fromPairs(successes), fromPairs(failures)];
  };
export const getSomes = <A>(record: Dict<Option<A>>): Dict<A> => fromPairs(entries(record).flatMap(([key, value]) =>
  value._tag === "Some" ? [[key, value.value] as const] : []));
export const getSuccesses = <A, E>(record: Dict<Result<A, E>>): Dict<A> => fromPairs(entries(record).flatMap(([key, value]) =>
  value._tag === "Success" ? [[key, value.success] as const] : []));
export const getFailures = <A, E>(record: Dict<Result<A, E>>): Dict<E> => fromPairs(entries(record).flatMap(([key, value]) =>
  value._tag === "Failure" ? [[key, value.failure] as const] : []));
export const separate = <A, E>(record: Dict<Result<A, E>>): readonly [Dict<A>, Dict<E>] =>
  [getSuccesses(record), getFailures(record)];
export const reduce = <A, B>(initial: B, f: (acc: B, value: A, key: string) => B) =>
  (record: Dict<A>): B => entries(record).reduce((acc, [key, value]) => f(acc, value, key), initial);
export const every = <A>(predicate: (value: A, key: string) => boolean) =>
  (record: Dict<A>): boolean => entries(record).every(([key, value]) => predicate(value, key));
export const some = <A>(predicate: (value: A, key: string) => boolean) =>
  (record: Dict<A>): boolean => entries(record).some(([key, value]) => predicate(value, key));
export const findFirst = <A>(predicate: (value: A, key: string) => boolean) =>
  (record: Dict<A>): Option<readonly [string, A]> => {
    const found = entries(record).find(([key, value]) => predicate(value, key));
    return found === undefined ? none : someOption(found);
  };
export const union = <A>(that: Dict<A>, combine: (self: A, that: A) => A) =>
  (self: Dict<A>): Dict<A> => {
    const output: Record<string, A> = { ...self };
    for (const [key, value] of entries(that)) output[key] = hasKey(self, key) ? combine(self[key] as A, value) : value;
    return output;
  };
export const intersection = <A, B, C>(that: Dict<B>, combine: (self: A, that: B) => C) =>
  (self: Dict<A>): Dict<C> => fromPairs(entries(self).flatMap(([key, value]) =>
    hasKey(that, key) ? [[key, combine(value, that[key] as B)] as const] : []));
export const difference = <A>(that: Dict<A>) =>
  (self: Dict<A>): Dict<A> => fromPairs([
    ...entries(self).filter(([key]) => !hasKey(that, key)),
    ...entries(that).filter(([key]) => !hasKey(self, key)),
  ]);
export const isSubrecordBy = <A>(same: (self: A, that: A) => boolean) =>
  (that: Dict<A>) => (self: Dict<A>): boolean =>
    entries(self).every(([key, value]) => hasKey(that, key) && same(value, that[key] as A));
export const flickMakeEquivalence = <A>(same: (a: A, b: A) => boolean) =>
  (self: Dict<A>) => (that: Dict<A>): boolean =>
    size(self) === size(that) && isSubrecordBy(same)(that)(self);
export const makeReducerUnion = <A>(combiner: Combiner<A>): Reducer<Dict<A>> => ({
  initial: {}, combine: (a, b) => union(b, combiner.combine)(a),
});
export const makeReducerIntersection = <A>(combiner: Combiner<A>): Reducer<Dict<A>> => ({
  initial: {}, combine: (a, b) => intersection(b, combiner.combine)(a),
});
