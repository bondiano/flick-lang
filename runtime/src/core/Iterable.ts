// The `Iterable` bindings of `List` without Effect, on lists as the Effect
// adapters take and give them.
import type { Result } from "./data.ts";

/** `List.filter_map_while`: the `Ok` values up to the first `Err`. */
export const flickFilterMapWhile =
  <A, B, X>(f: (a: A) => Result<B, X>) =>
  (self: ReadonlyArray<A>): Array<B> => {
    const out: Array<B> = [];
    for (const a of self) {
      const r = f(a);
      if (r._tag === "Failure") break;
      out.push(r.success);
    }
    return out;
  };

/** `List.repeat_all`: the list repeated `n` times, and at least once. */
export const flickRepeat =
  (n: number) =>
  <A>(self: ReadonlyArray<A>): Array<A> => {
    const times = n > 0 ? Math.floor(n) : 0;
    const out: Array<A> = self.slice();
    for (let i = 1; i < times; i++) out.push(...self);
    return out;
  };
