// Effect's `TxPriorityQueue` for the bindings of the standard library, and
// the constructors that take the order from an `Ord` dictionary.
import { type Effect, type Order, TxPriorityQueue } from "effect";

import { EQ, LT, type OrdDict } from "../data.ts";

export * from "effect/TxPriorityQueue";

const order =
  <A>(dOrd: OrdDict<A>): Order.Order<A> =>
  (a, b) => {
    const o = dOrd.compare(a)(b);
    return o._tag === LT._tag ? -1 : o._tag === EQ._tag ? 0 : 1;
  };

/** `TxPriorityQueue.empty`. */
export const flickEmpty = <A>(dOrd: OrdDict<A>): Effect.Effect<TxPriorityQueue.TxPriorityQueue<A>> =>
  TxPriorityQueue.empty(order(dOrd));

/** `TxPriorityQueue.from_iterable`. */
export const flickFromIterable = <A>(
  dOrd: OrdDict<A>,
  elements: ReadonlyArray<A>,
): Effect.Effect<TxPriorityQueue.TxPriorityQueue<A>> => TxPriorityQueue.fromIterable(order(dOrd), elements);

/** `TxPriorityQueue.make`: the elements as Effect's rest arguments. */
export const flickMake = <A>(
  dOrd: OrdDict<A>,
  elements: ReadonlyArray<A>,
): Effect.Effect<TxPriorityQueue.TxPriorityQueue<A>> => TxPriorityQueue.make(order(dOrd))(...elements);
