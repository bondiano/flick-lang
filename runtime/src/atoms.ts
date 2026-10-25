// Reading atoms from Flick: `Atom.Read` is the service a view's render or
// a derived atom reads through. Both run synchronously (React renders and
// Effect's registry computes in plain JS), so the effect of a read runs at
// once with `runSync`, its reads going to the reader at hand.
import { Cause, Effect, Option } from "effect";
import * as AsyncResult from "effect/reactivity/AsyncResult";
import * as Atom from "effect/reactivity/Atom";

import { Failure } from "./cause.ts";
import type { HashDict } from "./data.ts";
import { flickState } from "./effect/reactivity/Atom.ts";
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

/** `Atom.make`: a state whose value the registry keeps while it lives, read or not, so a write before the first read stays. */
export const atomState = <A>(value: A): Atom.Writable<A> => Atom.keepAlive(flickState(value));

/** `Atom.task`: the atom of a task's result, run by the registry that first reads it. */
export const atomTask = <A, E>(task: Effect.Effect<A, E>): Atom.Atom<object> => Atom.map(Atom.make(task), toAsync);

/** `Atom.derived`: an atom `compute` computes, depending on the atoms it reads. */
export const atomDerived = <A>(compute: (unit: undefined) => Effect.Effect<A, unknown, unknown>): Atom.Atom<A> =>
  Atom.readable((ctx) => runReading(compute(undefined), (atom) => ctx(atom)));

/**
 * `Atom.family`: one atom per key, made by `f` the first time and the same
 * for every key equal by `Eq` after.
 */
export const atomFamily = <K, A>(dHash: HashDict<K>, f: (key: K) => A): ((key: K) => A) => {
  // shortcut: keeps every atom it made, unlike Effect's `Atom.family`, which forgets one no one holds; upgrade if families of many short-lived keys appear
  const made = new Map<number, Array<readonly [K, A]>>();
  return (key) => {
    const hash = dHash.hash(key);
    const bucket = made.get(hash) ?? [];
    const known = bucket.find(([k]) => dHash.$Eq.eq(k)(key));
    if (known !== undefined) return known[1];
    const atom = f(key);
    bucket.push([key, atom]);
    made.set(hash, bucket);
    return atom;
  };
};

/** `Atom.search_param`: the URL's search parameter `name`; a write updates the URL. */
export const atomSearchParam = (name: string): Atom.Writable<string> => Atom.keepAlive(Atom.searchParam(name));

/** `Atom.on_server`: the browser atom, read as `value` on a server and while a root hydrates. */
export const atomOnServer = <A>(value: A, atom: Atom.Writable<A>): Atom.Writable<A> => Atom.withServerValue(atom, () => value);

/** `Atom.client_only`: the browser atom's value, `None` on a server and while a root hydrates. */
export const atomClientOnly = <A>(atom: Atom.Writable<A>): Atom.Atom<Option.Option<A>> =>
  Atom.withServerValue(Atom.map(atom, Option.some), () => Option.none());
