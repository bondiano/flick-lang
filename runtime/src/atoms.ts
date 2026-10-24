// Reading atoms from Flick: `Atom.Read` is the service a view's render or
// a derived atom reads through. Both run synchronously (React renders and
// Effect's registry computes in plain JS), so the effect of a read runs at
// once with `runSync`, its reads going to the reader at hand.
import { Cause, Effect } from "effect";
import * as AsyncResult from "effect/reactivity/AsyncResult";
import * as Atom from "effect/reactivity/Atom";

import { Failure } from "./cause.ts";
import { service } from "./effects.ts";
import { tag } from "./tags.ts";

interface ReadImpl {
  readonly get: (atom: Atom.Atom<unknown>) => Effect.Effect<unknown>;
}

/** The service `Atom.Read`. */
export const ReadService = /*#__PURE__*/ service<ReadImpl>("Atom.Read");

/** Runs `effect`, whose reads of atoms go to `read`, synchronously. */
export const runReading = <A>(effect: Effect.Effect<A, unknown, unknown>, read: (atom: Atom.Atom<unknown>) => unknown): A => {
  const reading = Effect.provideService(effect, ReadService, { get: (atom) => Effect.sync(() => read(atom)) });
  // oxlint-disable-next-line typescript/no-unsafe-type-assertion -- `Atom.Read` was its only service
  const exit = Effect.runSyncExit(reading as Effect.Effect<A, unknown>);
  if (exit._tag === "Success") return exit.value;
  if (isPending(exit.cause)) throw new Error("`Atom.await` read a task still loading outside `Ui.suspended`, which shows a fallback meanwhile");
  throw Cause.squash(exit.cause);
};

/** What `Atom.await` dies with while its task loads: a view of `Ui.suspended` shows its fallback. */
const PENDING = /*#__PURE__*/ Symbol.for("@flick-lang/runtime/Atom.pending");

/** Whether `cause` is a task still loading under `Atom.await`. */
export const isPending = (cause: Cause.Cause<unknown>): boolean =>
  cause.reasons.some((r) => Cause.isDieReason(r) && r.defect === PENDING);

/** `Atom.pending`: the read of a task still loading. */
export const atomPending = (): Effect.Effect<never> => Effect.die(PENDING);

const LOADING = /*#__PURE__*/ Object.freeze({ _tag: tag("Atom.Async.Loading") });
const LOADED = /*#__PURE__*/ tag("Atom.Async.Loaded");
const FAILED = /*#__PURE__*/ tag("Atom.Async.Failed");

/** Effect's `AsyncResult` as the Flick value `Atom.Async a e`. */
const toAsync = <A, E>(result: AsyncResult.AsyncResult<A, E>): object =>
  AsyncResult.match(result, {
    onInitial: () => LOADING,
    onSuccess: (s) => Object.freeze({ _tag: LOADED, _0: s.value }),
    onFailure: (f) => Object.freeze({ _tag: FAILED, _0: new Failure(f.cause) }),
  });

/** `Atom.task`: the atom of a task's result, run by the registry that first reads it. */
export const atomTask = <A, E>(task: Effect.Effect<A, E>): Atom.Atom<object> => Atom.map(Atom.make(task), toAsync);

/** `Atom.derived`: an atom `compute` computes, depending on the atoms it reads. */
export const atomDerived = <A>(compute: (unit: undefined) => Effect.Effect<A, unknown, unknown>): Atom.Atom<A> =>
  Atom.readable((ctx) => runReading(compute(undefined), (atom) => ctx(atom)));
