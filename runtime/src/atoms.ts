// Reading atoms from Flick: `Atom.Read` is the service a view's render or
// a derived atom reads through. Both run synchronously (React renders and
// Effect's registry computes in plain JS), so the effect of a read runs at
// once with `runSync`, its reads going to the reader at hand.
import { Cause, Effect, Option, Schema } from "effect";
import * as AsyncResult from "effect/reactivity/AsyncResult";
import * as Atom from "effect/reactivity/Atom";

import { Failure } from "./cause.ts";
import type { HashDict } from "./data.ts";
import { WriteService, flickState, nativeRegistry } from "./effect/reactivity/Atom.ts";
import { service } from "./effects.ts";
import { tag } from "./tags.ts";

interface ReadImpl {
  readonly get: (atom: Atom.Atom<unknown>) => Effect.Effect<unknown>;
}

/** The service `Atom.Read`. */
export const ReadService = /*#__PURE__*/ service<ReadImpl>("Atom.Read");

/**
 * Runs `effect`, whose reads of atoms go to `read`, synchronously; a read
 * of a loading task under `Atom.ready` gives `pending()` when there is one.
 */
export const runReading = <A>(
  effect: Effect.Effect<A, unknown, unknown>,
  read: (atom: Atom.Atom<unknown>) => unknown,
  pending?: () => A,
): A => {
  const reading = Effect.provideService(effect, ReadService, { get: (atom) => Effect.sync(() => read(atom)) });
  // oxlint-disable-next-line typescript/no-unsafe-type-assertion -- `Atom.Read` was its only service
  const exit = Effect.runSyncExit(reading as Effect.Effect<A, unknown>);
  if (exit._tag === "Success") return exit.value;
  if (isPending(exit.cause) && pending !== undefined) return pending();
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

/** Whether `value` is a loaded `Atom.Async`. */
const isLoaded = (value: unknown): value is { readonly _0: unknown } => Reflect.get(Object(value), "_tag") === LOADED;

/** Marks the task atoms of `Atom.serializable_task`, which hand over only a loaded value. */
const TASK = /*#__PURE__*/ Symbol.for("@flick-lang/runtime/Atom.serializable_task");

/** Whether a server hands `value` of the serializable `atom` to the client: a task's only once loaded. */
export const handedOver = (atom: Atom.Atom<unknown>, value: unknown): boolean => !Reflect.has(atom, TASK) || isLoaded(value);

/**
 * `Atom.serializable_task` under the key the compiler gives: a loaded
 * result is coded by `codec`, and the client starts from it instead of
 * running the task; one still loading or failed is not handed over.
 */
export const atomSerializableTask = <A>(key: string, codec: Schema.Codec<A, unknown>, atom: Atom.Atom<object>): Atom.Atom<object> => {
  const json = Schema.toCodecJson(codec);
  const encode = Schema.encodeSync(json);
  const decode = Schema.decodeSync(json);
  // as `Atom.serializable` marks an atom, with the coding of a loaded result; kept alive, so a
  // hydrated result is not dropped, and the task run, before the first view subscribes
  // oxlint-disable-next-line typescript/no-unsafe-return, typescript/no-unsafe-argument -- a copy of `atom` with its prototype
  return Object.assign(Object.create(Object.getPrototypeOf(atom)), {
    ...atom,
    keepAlive: true,
    label: atom.label ?? [key, ""],
    [TASK]: true,
    [Atom.SerializableTypeId]: {
      key,
      encode: (value: unknown) => {
        if (!isLoaded(value)) throw new Error(`the task atom \`${key}\` has no loaded value to hand over`);
        // oxlint-disable-next-line typescript/no-unsafe-type-assertion -- a loaded result holds the task's value
        return encode(value._0 as A);
      },
      // oxlint-disable-next-line typescript/no-unsafe-type-assertion -- the server encoded it with this codec
      decode: (encoded: unknown) => Object.freeze({ _tag: LOADED, _0: decode(encoded as Schema.Json) }),
    },
  });
};

/** `Atom.task`: the atom of a task's result, run by the registry that first reads it. */
export const atomTask = <A, E>(task: Effect.Effect<A, E>): Atom.Atom<object> => Atom.map(Atom.make(task), toAsync);

/** `Atom.derived`: an atom `compute` computes, depending on the atoms it reads. */
export const atomDerived = <A>(compute: (unit: undefined) => Effect.Effect<A, unknown, unknown>): Atom.Atom<A> =>
  Atom.readable((ctx) => runReading(compute(undefined), (atom) => ctx(atom)));

/**
 * The label the compiler gives a top-level atom, view or action,
 * `Module.name`, which debugging tools show; one it has already stays.
 */
export const label = <A extends object>(name: string, value: A): A => {
  if (Reflect.get(value, "label") !== undefined) return value;
  if (Atom.isAtom(value)) return Atom.withLabel(value, name);
  return Object.freeze({ ...value, label: name });
};

interface Entry<K, A> {
  readonly key: K;
  readonly ref: { deref(): A | undefined };
}

/** `Atom.Family k a`: the atoms of each key, held weakly. */
interface Family<K, A> {
  readonly dHash: HashDict<K>;
  readonly make: (key: K) => A;
  readonly entries: Map<number, Array<Entry<K, A>>>;
  readonly gone: FinalizationRegistry<Entry<K, A>> | undefined;
}

const forgetEntry = <K, A>(family: Family<K, A>, entry: Entry<K, A>): void => {
  const hash = family.dHash.hash(entry.key);
  const rest = (family.entries.get(hash) ?? []).filter((e) => e !== entry);
  if (rest.length > 0) family.entries.set(hash, rest);
  else family.entries.delete(hash);
};

const entryOf = <K, A>(family: Family<K, A>, key: K): Entry<K, A> | undefined =>
  family.entries.get(family.dHash.hash(key))?.find((e) => family.dHash.$Eq.eq(e.key)(key));

/** `Atom.family`: the atoms `make` makes, one per key equal by `Eq`. */
export const atomFamily = <K, A>(dHash: HashDict<K>, make: (key: K) => A): Family<K, A> => {
  const family: Family<K, A> = {
    dHash,
    make,
    entries: new Map(),
    // an atom nothing holds any more takes its entry with it
    gone: typeof FinalizationRegistry === "undefined" ? undefined : new FinalizationRegistry((entry) => forgetEntry(family, entry)),
  };
  return family;
};

/** `Atom.at`: the atom of `key`, made the first time. */
export const atomAt = <K, A>(family: Family<K, A>, key: K): A => {
  const entry = entryOf(family, key);
  const known = entry?.ref.deref();
  if (known !== undefined) return known;
  if (entry !== undefined) forgetEntry(family, entry);
  const atom = family.make(key);
  const weak = typeof atom === "object" && atom !== null && typeof WeakRef !== "undefined";
  const made: Entry<K, A> = { key, ref: weak ? new WeakRef(atom) : { deref: () => atom } };
  const hash = family.dHash.hash(key);
  family.entries.set(hash, [...(family.entries.get(hash) ?? []), made]);
  if (weak) family.gone?.register(atom, made);
  return atom;
};

/**
 * `Atom.forget`: the next `at` of `key` makes a new atom, and the registry
 * drops the old state's value once nothing reads it.
 */
export const atomForget = <K, A>(family: Family<K, A>, key: K): Effect.Effect<void, never, unknown> =>
  Effect.map(Effect.service(WriteService), (write) => {
    const entry = entryOf(family, key);
    if (entry === undefined) return;
    forgetEntry(family, entry);
    const atom = entry.ref.deref();
    if (!Atom.isAtom(atom)) return;
    // no `at` gives this atom again, so it need not outlive its readers
    Reflect.set(atom, "keepAlive", false);
    const registry = nativeRegistry(write);
    // a last reader leaving removes the node; with none, this does
    if (registry !== undefined) registry.mount(atom)();
  });

/** `Atom.search_param`: the URL's search parameter `name`; a write updates the URL. */
export const atomSearchParam = (name: string): Atom.Writable<string> => Atom.keepAlive(Atom.searchParam(name));

/** `Atom.on_server`: the browser atom, read as `value` on a server and while a root hydrates. */
export const atomOnServer = <A>(value: A, atom: Atom.Writable<A>): Atom.Writable<A> => Atom.withServerValue(atom, () => value);

/** `Atom.client_only`: the browser atom's value, `None` on a server and while a root hydrates. */
export const atomClientOnly = <A>(atom: Atom.Writable<A>): Atom.Atom<Option.Option<A>> =>
  Atom.withServerValue(Atom.map(atom, Option.some), () => Option.none());
