// Lazy sequences on Effect: the pure ones of `core/seq.ts`, the searches
// with Effect's `Option` and the runtime traversals.
import { Effect, type Option } from "effect";

import { none, some } from "../data.ts";
import type { Seq } from "@flick-lang/runtime/core/seq";

export {
  seq,
  seqOfList,
  seqFrom,
  seqRangeBy,
  seqRange,
  seqRangeTo,
  seqIterate,
  seqRepeat,
  seqCycle,
  seqUnfold,
  seqAppend,
  seqMap2,
  seqMap,
  seqFilter,
  seqFilterMap,
  seqFlatMap,
  seqTake,
  seqTakeWhile,
  seqDrop,
  seqDropWhile,
  seqZipWith,
  seqZip,
  seqEnumerate,
  seqScan,
  seqChunks,
  seqWindows,
  seqToList,
  seqFold,
  seqAny,
  seqAll,
} from "@flick-lang/runtime/core/seq";

export const seqFind = <A>(p: (a: A) => boolean, xs: Seq<A>): Option.Option<A> => {
  for (const x of xs) if (p(x)) return some(x);
  return none;
};

export const seqFirst = <A>(xs: Seq<A>): Option.Option<A> => {
  for (const x of xs) return some(x);
  return none;
};

export const seqEachEffect = <A, E, R>(f: (a: A) => Effect.Effect<unknown, E, R>, xs: Seq<A>): Effect.Effect<void, E, R> =>
  Effect.forEach(xs, (x) => f(x), { discard: true });

export const seqFoldEffect = <A, B, E, R>(
  f: (acc: B, a: A) => Effect.Effect<B, E, R>,
  init: B,
  xs: Seq<A>,
): Effect.Effect<B, E, R> => Effect.reduce(xs, () => init, (acc, x) => f(acc, x));
