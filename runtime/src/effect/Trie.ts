// Effect's `Trie` for the bindings of the standard library, and the adapters
// that turn its iterators into lists.
import { Trie } from "effect";

export * from "effect/Trie";

/** `Trie.empty`: one empty trie for every value type. */
export const flickEmpty: Trie.Trie<never> = /*#__PURE__*/ Trie.empty();

export const flickKeys = <V>(self: Trie.Trie<V>): Array<string> => Array.from(Trie.keys(self));
export const flickValues = <V>(self: Trie.Trie<V>): Array<V> => Array.from(Trie.values(self));

export const flickKeysWithPrefix =
  (prefix: string) =>
  <V>(self: Trie.Trie<V>): Array<string> =>
    Array.from(Trie.keysWithPrefix(self, prefix));

export const flickValuesWithPrefix =
  (prefix: string) =>
  <V>(self: Trie.Trie<V>): Array<V> =>
    Array.from(Trie.valuesWithPrefix(self, prefix));
