// Flick's `Ordering` and Effect's -1/0/1, for the adapters of this directory.
import type { Order } from "effect";

import { EQ, GT, LT, type Ordering } from "../data.ts";

/** Effect's ordering number as Flick's `Ordering`. */
export const toOrdering = (n: number): Ordering => (n < 0 ? LT : n > 0 ? GT : EQ);

/** Flick's `Ordering` as Effect's ordering number. */
export const fromOrdering = (o: Ordering): -1 | 0 | 1 => (o._tag === LT._tag ? -1 : o._tag === EQ._tag ? 0 : 1);

/** A Flick order, `(a, b) => Ordering` by its JS arity, as Effect's `Order`. */
export const toOrder =
  <A>(compare: (self: A, that: A) => Ordering): Order.Order<A> =>
  (self, that) =>
    fromOrdering(compare(self, that));
