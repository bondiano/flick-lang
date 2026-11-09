// Effect's `Chunk` for the bindings of the standard library, and the
// adapters where a Flick value differs from what Effect takes.
import { Chunk, Option } from "effect";

import type { Ordering } from "../data.ts";
import { flickOrder } from "../stdlib/list.ts";

export * from "effect/Chunk";

/** `Chunk.non_empty`: the refinement `isNonEmpty` as an `Option`. */
export const flickNonEmpty = <A>(self: Chunk.Chunk<A>): Option.Option<Chunk.NonEmptyChunk<A>> =>
  Chunk.isNonEmpty(self) ? Option.some(self) : Option.none();

/** `Chunk.sort_by`: sorted by a Flick comparison. */
export const flickSortBy =
  <A>(cmp: (a: A, b: A) => Ordering) =>
  (self: Chunk.Chunk<A>): Chunk.Chunk<A> =>
    Chunk.sort(self, flickOrder(cmp));
