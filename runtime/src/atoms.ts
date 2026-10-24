// Reading atoms from Flick: `Atom.Read` is the service a view's render or
// a derived atom reads through. Both run synchronously (React renders and
// Effect's registry computes in plain JS), so the effect of a read runs at
// once with `runSync`, its reads going to the reader at hand.
import { Effect } from "effect";
import * as Atom from "effect/reactivity/Atom";

import { service } from "./effects.ts";

interface ReadImpl {
  readonly get: (atom: Atom.Atom<unknown>) => Effect.Effect<unknown>;
}

/** The service `Atom.Read`. */
export const ReadService = /*#__PURE__*/ service<ReadImpl>("Atom.Read");

/** Runs `effect`, whose reads of atoms go to `read`, synchronously. */
export const runReading = <A>(effect: Effect.Effect<A, unknown, unknown>, read: (atom: Atom.Atom<unknown>) => unknown): A => {
  const reading = Effect.provideService(effect, ReadService, { get: (atom) => Effect.sync(() => read(atom)) });
  // oxlint-disable-next-line typescript/no-unsafe-type-assertion -- `Atom.Read` was its only service, and a read cannot fail
  return Effect.runSync(reading as Effect.Effect<A>);
};

/** `Atom.derived`: an atom `compute` computes, depending on the atoms it reads. */
export const atomDerived = <A>(compute: (unit: undefined) => Effect.Effect<A, unknown, unknown>): Atom.Atom<A> =>
  Atom.readable((ctx) => runReading(compute(undefined), (atom) => ctx(atom)));
