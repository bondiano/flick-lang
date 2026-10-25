// Effect's `Iterable` for the bindings of `List`: the adapters take and give
// lists. Lazy sequences are `Seq`, in `core/seq.ts`.
import { Iterable } from "effect";
import type { Result } from "effect";

export * from "effect/Iterable";

/** `List.filter_map_while`: the `Ok` values up to the first `Err`, as a list. */
export const flickFilterMapWhile =
  <A, B, X>(f: (a: A) => Result.Result<B, X>) =>
  (self: ReadonlyArray<A>): Array<B> =>
    Array.from(Iterable.filterMapWhile(self, (a) => f(a)));

/** `List.repeat_all`: the list repeated `n` times, as a list. */
export const flickRepeat =
  (n: number) =>
  <A>(self: ReadonlyArray<A>): Array<A> =>
    Array.from(Iterable.repeat(self, n));
