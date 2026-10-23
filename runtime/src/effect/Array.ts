// Effect's `Array` for the bindings of the standard library, and the
// adapters where a Flick value differs from what Effect takes.
import { Array as Arr } from "effect";

import type { Ordering } from "../data.ts";
import { flickOrder } from "../stdlib/list.ts";

export * from "effect/Array";

/** `NonEmpty.max_by`: the greatest element by a Flick comparison. */
export const flickMax =
  <A>(cmp: (a: A, b: A) => Ordering) =>
  (self: Arr.NonEmptyReadonlyArray<A>): A =>
    Arr.max(self, flickOrder(cmp));

/** `NonEmpty.min_by`: the least element by a Flick comparison. */
export const flickMin =
  <A>(cmp: (a: A, b: A) => Ordering) =>
  (self: Arr.NonEmptyReadonlyArray<A>): A =>
    Arr.min(self, flickOrder(cmp));

/** `List.match_left`: `onNonEmpty` is a curried Flick function of the head and the tail. */
export const flickMatchLeft =
  <A, B>(options: { readonly onEmpty: (u: undefined) => B; readonly onNonEmpty: (head: A) => (tail: Array<A>) => B }) =>
  (self: ReadonlyArray<A>): B =>
    Arr.matchLeft(self, {
      onEmpty: () => options.onEmpty(undefined),
      onNonEmpty: (head, tail) => options.onNonEmpty(head)(tail),
    });

/** `List.match_right`: `onNonEmpty` is a curried Flick function of the init and the last. */
export const flickMatchRight =
  <A, B>(options: { readonly onEmpty: (u: undefined) => B; readonly onNonEmpty: (init: Array<A>) => (last: A) => B }) =>
  (self: ReadonlyArray<A>): B =>
    Arr.matchRight(self, {
      onEmpty: () => options.onEmpty(undefined),
      onNonEmpty: (init, last) => options.onNonEmpty(init)(last),
    });
