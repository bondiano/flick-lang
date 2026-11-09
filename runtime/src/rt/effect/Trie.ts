// Immutable string-key trie operations. A Map stores values; traversal sorts
// keys, preserving Effect Trie's lexical order and persistent update semantics.
import { none, some, type Option, type Result } from "../../core/data.ts";
type Trie<A> = ReadonlyMap<string, A>;
type Pair<A> = readonly [string, A];
const ordered = <A>(trie: Trie<A>): Array<[string, A]> => [...trie].toSorted((a, b) => a[0] < b[0] ? -1 : a[0] > b[0] ? 1 : 0);
const present = <A>(trie: Trie<A>, key: string): A => {
  if (!trie.has(key)) throw new Error("missing trie key");
  // oxlint-disable-next-line typescript/no-unsafe-type-assertion -- has checked above; values may be undefined
  return trie.get(key) as A;
};
export const flickEmpty: Trie<never> = new Map<string, never>();
export const fromIterable = <A>(values: Iterable<Pair<A>>): Trie<A> => new Map(values);
export const insert = <A>(key: string, value: A) => (trie: Trie<A>): Trie<A> => new Map([...trie, [key, value]]);
export const insertMany = <A>(values: Iterable<Pair<A>>) => (trie: Trie<A>): Trie<A> =>
  new Map([...trie, ...values]);
export const remove = (key: string) => <A>(trie: Trie<A>): Trie<A> => {
  const copy = new Map(trie);
  copy.delete(key);
  return copy;
};
export const removeMany = (keys: Iterable<string>) => <A>(trie: Trie<A>): Trie<A> => {
  const copy = new Map(trie);
  for (const key of keys) copy.delete(key);
  return copy;
};
export const get = (key: string) => <A>(trie: Trie<A>): Option<A> => trie.has(key) ? some(present(trie, key)) : none;
export const has = (key: string) => <A>(trie: Trie<A>): boolean => trie.has(key);
export const size = <A>(trie: Trie<A>): number => trie.size;
export const isEmpty = <A>(trie: Trie<A>): boolean => trie.size === 0;
export const toEntries = ordered;
export const flickKeys = <A>(trie: Trie<A>): Array<string> => ordered(trie).map(([key]) => key);
export const flickValues = <A>(trie: Trie<A>): Array<A> => ordered(trie).map(([, value]) => value);
export const toEntriesWithPrefix = (prefix: string) => <A>(trie: Trie<A>): Array<[string, A]> =>
  ordered(trie).filter(([key]) => key.startsWith(prefix));
export const flickKeysWithPrefix = (prefix: string) => <A>(trie: Trie<A>): Array<string> =>
  toEntriesWithPrefix(prefix)(trie).map(([key]) => key);
export const flickValuesWithPrefix = (prefix: string) => <A>(trie: Trie<A>): Array<A> =>
  toEntriesWithPrefix(prefix)(trie).map(([, value]) => value);
export const longestPrefixOf = (key: string) => <A>(trie: Trie<A>): Option<[string, A]> => {
  for (let length = key.length; length >= 0; length--) {
    const prefix = key.slice(0, length);
    if (trie.has(prefix)) return some([prefix, present(trie, prefix)]);
  }
  return none;
};
export const modify = <A>(key: string, f: (value: A) => A) => (trie: Trie<A>): Trie<A> =>
  trie.has(key) ? insert(key, f(present(trie, key)))(trie) : trie;
export const reduce = <A, B>(initial: B, f: (acc: B, value: A, key: string) => B) =>
  (trie: Trie<A>): B => ordered(trie).reduce((acc, [key, value]) => f(acc, value, key), initial);
export const map = <A, B>(f: (value: A, key: string) => B) => (trie: Trie<A>): Trie<B> =>
  fromIterable(ordered(trie).map(([key, value]) => [key, f(value, key)]));
export const filter = <A>(f: (value: A, key: string) => boolean) => (trie: Trie<A>): Trie<A> =>
  fromIterable(ordered(trie).filter(([key, value]) => f(value, key)));
export const filterMap = <A, B, E>(f: (value: A, key: string) => Result<B, E>) => (trie: Trie<A>): Trie<B> =>
  fromIterable(ordered(trie).flatMap(([key, value]) => {
    const result = f(value, key);
    return result._tag === "Success" ? [[key, result.success] as const] : [];
  }));
export const compact = <A>(trie: Trie<Option<A>>): Trie<A> =>
  fromIterable(ordered(trie).flatMap(([key, value]) => value._tag === "Some" ? [[key, value.value] as const] : []));
