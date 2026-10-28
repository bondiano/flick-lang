// Reading atoms from Flick: `Atom.Read` is the service a view's render or
// a derived atom reads through. Both run synchronously (React renders and
// Effect's registry computes in plain JS): a render of `Atom.Read` alone is
// a Sync function, called with the reader at hand as its context; one that
// may fail, as under `Ui.suspended`, an effect run at once with `runSync`.
import { Cause, Context, Effect, Option, Schema, type Stream } from "effect";
import * as AsyncResult from "effect/reactivity/AsyncResult";
import * as Atom from "effect/reactivity/Atom";
import * as Reactivity from "effect/reactivity/Reactivity";

import { Failure, failureCause } from "./cause.ts";
import type { HashDict } from "./data.ts";
import { WriteService, flickState, keepState, nativeRegistry, taskAtom, taskAtomOf } from "./effect/reactivity/Atom.ts";
import { service } from "./effects.ts";
import { type UrlCodec, routeParse, routePrint } from "./route.ts";
import { syncAbi, syncIn } from "./sync.ts";
import { tag } from "./tags.ts";

interface ReadImpl {
  readonly get: (atom: Atom.Atom<unknown>) => unknown;
  readonly [syncAbi]?: true;
}

const reader = (read: (atom: Atom.Atom<unknown>) => unknown): ReadImpl => ({ get: read, [syncAbi]: true });

/** The service `Atom.Read`, a Sync one: `get` returns the value. */
export const ReadService = /*#__PURE__*/ service<ReadImpl>("Atom.Read");

const STILL_LOADING = "`Atom.await` read a task still loading outside `Ui.suspended`, which shows a fallback meanwhile";

/**
 * Runs `effect`, whose reads of atoms go to `read`, synchronously; a read
 * of a loading task under `Atom.ready` gives `pending()` when there is one.
 */
export const runReading = <A>(
  effect: Effect.Effect<A, unknown, unknown>,
  read: (atom: Atom.Atom<unknown>) => unknown,
  pending?: () => A,
): A => {
  const reading = Effect.provideService(effect, ReadService, reader(read));
  // oxlint-disable-next-line typescript/no-unsafe-type-assertion -- `Atom.Read` was its only service
  const exit = Effect.runSyncExit(reading as Effect.Effect<A, unknown>);
  if (exit._tag === "Success") return exit.value;
  if (isPending(exit.cause) && pending !== undefined) return pending();
  if (isPending(exit.cause)) throw new Error(STILL_LOADING);
  throw Cause.squash(exit.cause);
};

/**
 * Calls the Sync `render`, whose reads of atoms go to `read`; a read of a
 * loading task gives `pending()` when there is one.
 */
export const syncReading = <A>(render: () => A, read: (atom: Atom.Atom<unknown>) => unknown, pending?: () => A): A => {
  try {
    return syncIn(Context.make(ReadService, reader(read)), render);
  } catch (defect) {
    if (defect !== PENDING) throw defect;
    if (pending !== undefined) return pending();
    throw new Error(STILL_LOADING, { cause: defect });
  }
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

/**
 * `Atom.fresh`: a state the registry forgets once nothing holds it; the
 * instance of `Atom.local` holds it while it is mounted.
 */
export const atomLocal = <A>(value: A): Atom.Writable<A> => flickState(value);

/** Whether `value` is a loaded `Atom.Async`. */
const isLoaded = (value: unknown): value is { readonly _0: unknown } => Reflect.get(Object(value), "_tag") === LOADED;

/** The one typed error of a failed `Atom.Async`, none for a defect, an interruption or several errors. */
const publicError = (value: unknown): Option.Option<unknown> => {
  if (Reflect.get(Object(value), "_tag") !== FAILED) return Option.none();
  const failure: unknown = Reflect.get(Object(value), "_0");
  if (!(failure instanceof Failure)) return Option.none();
  const [reason, ...rest] = failureCause(failure).reasons;
  return reason !== undefined && rest.length === 0 && Cause.isFailReason(reason) ? Option.some(reason.error) : Option.none();
};

/**
 * Marks the task atoms of `Atom.serializable_task`, which hand over only a
 * loaded value, and of `Atom.serializable_result`, which also hand over a
 * failure of one typed error: the mark is whether it does.
 */
const TASK = /*#__PURE__*/ Symbol.for("@flick-lang/runtime/Atom.serializable_task");

/** Whether a server hands `value` of the serializable `atom` to the client: a task's only once loaded, or failed with a coded error. */
export const handedOver = (atom: Atom.Atom<unknown>, value: unknown): boolean =>
  !Reflect.has(atom, TASK) || isLoaded(value) || (Reflect.get(atom, TASK) === true && Option.isSome(publicError(value)));

/**
 * Marks the atoms of `Atom.query` with how a hydrated result reaches their
 * task: its first run gives the result instead of running, as a task atom's
 * hydrated value is not run again.
 */
const QUERY = /*#__PURE__*/ Symbol.for("@flick-lang/runtime/Atom.query");

const isHydrate = (value: unknown): value is (result: Effect.Effect<unknown, unknown>) => void => typeof value === "function";

/** What `Atom.serializable_result` sends: the value or the error. */
interface Wire {
  readonly ok?: unknown;
  readonly err?: unknown;
}

/**
 * `Atom.serializable_task` under the key the compiler gives: a loaded
 * result is coded by `codec`, and the client starts from it instead of
 * running the task; one still loading or failed is not handed over. With
 * `errors`, a failure of one typed error is handed over as well, coded by it,
 * never a defect or a stack.
 */
export const atomSerializableTask = <A>(
  key: string,
  codec: Schema.Codec<A, unknown>,
  atom: Atom.Atom<object>,
  errors?: Schema.Codec<unknown, unknown>,
): Atom.Atom<object> => {
  const hydrate: unknown = Reflect.get(atom, QUERY);
  const json = Schema.toCodecJson(codec);
  const encode = Schema.encodeSync(json);
  const decode = Schema.decodeSync(json);
  const errorJson = errors === undefined ? undefined : Schema.toCodecJson(errors);
  const encodeValue = (value: unknown): unknown => {
    // oxlint-disable-next-line typescript/no-unsafe-type-assertion -- a loaded result holds the task's value
    if (isLoaded(value)) return errorJson === undefined ? encode(value._0 as A) : { ok: encode(value._0 as A) };
    const error = publicError(value);
    if (errorJson !== undefined && Option.isSome(error)) return { err: Schema.encodeSync(errorJson)(error.value) };
    throw new Error(`the task atom \`${key}\` has no loaded value to hand over`);
  };
  const decodeValue = (encoded: unknown): object => {
    // oxlint-disable-next-line typescript/no-unsafe-type-assertion -- the server encoded it with these codecs
    const wire = (errorJson === undefined ? { ok: encoded } : encoded) as Wire;
    if (errorJson !== undefined && "err" in wire) {
      // oxlint-disable-next-line typescript/no-unsafe-type-assertion -- the server encoded it with this codec
      const error = Schema.decodeSync(errorJson)(wire.err as Schema.Json);
      if (isHydrate(hydrate)) hydrate(Effect.fail(error));
      return Object.freeze({ _tag: FAILED, _0: new Failure(Cause.fail(error)) });
    }
    // oxlint-disable-next-line typescript/no-unsafe-type-assertion -- the server encoded it with this codec
    const value = decode(wire.ok as Schema.Json);
    if (isHydrate(hydrate)) hydrate(Effect.succeed(value));
    return Object.freeze({ _tag: LOADED, _0: value });
  };
  // as `Atom.serializable` marks an atom, with the coding of a loaded result; kept alive, so a
  // hydrated result is not dropped, and the task run, before the first view subscribes
  // oxlint-disable-next-line typescript/no-unsafe-return, typescript/no-unsafe-argument -- a copy of `atom` with its prototype
  return Object.assign(Object.create(Object.getPrototypeOf(atom)), {
    ...atom,
    keepAlive: true,
    label: atom.label ?? [key, ""],
    [TASK]: errorJson !== undefined,
    [Atom.SerializableTypeId]: { key, encode: encodeValue, decode: decodeValue },
  });
};

/** `Atom.serializable_result` under the key the compiler gives: `serializable_task` that also hands over a typed failure. */
export const atomSerializableResult = <A>(
  key: string,
  codec: Schema.Codec<A, unknown>,
  errors: Schema.Codec<unknown, unknown>,
  atom: Atom.Atom<object>,
): Atom.Atom<object> => atomSerializableTask(key, codec, atom, errors);

/** `Atom.task`: the atom of a task's result, run by the registry that first reads it. */
export const atomTask = <A, E>(task: Effect.Effect<A, E>): Atom.Atom<object> => Atom.map(taskAtom(task), toAsync);

/** `Atom.query keys task`: a task atom that a mutation of one of `keys` runs anew. */
export const atomQuery = <A, E>(keys: ReadonlyArray<string>, task: Effect.Effect<A, E>): Atom.Atom<object> => {
  let hydrated: Option.Option<Effect.Effect<A, E>> = Option.none();
  const first = Effect.suspend(() => {
    const value = hydrated;
    hydrated = Option.none();
    return Option.isSome(value) ? value.value : task;
  });
  const hydrate = (result: Effect.Effect<A, E>): void => {
    hydrated = Option.some(result);
  };
  return Object.assign(Atom.withReactivity(keys)(atomTask(first)), { [QUERY]: hydrate });
};

/**
 * `Atom.loader route load`: the task atom of `load` of the route's value,
 * run anew when the route changes. A hydrated result is its first run's, and
 * reading the route there makes later changes run it.
 */
export const atomLoader = <R, A, E>(route: Atom.Atom<R>, load: (value: R) => Effect.Effect<A, E>): Atom.Atom<object> => {
  let hydrated: Option.Option<Effect.Effect<A, E>> = Option.none();
  const result = Atom.map(
    taskAtomOf((get) => {
      const value = get(route);
      const seeded = hydrated;
      hydrated = Option.none();
      return Option.isSome(seeded) ? seeded.value : load(value);
    }),
    toAsync,
  );
  const hydrate = (seed: Effect.Effect<A, E>): void => {
    hydrated = Option.some(seed);
  };
  // a hydrated value is the initial one of `result`, which still builds and so reads the route
  return Object.assign(Atom.transform(result, (get) => get(result), { initialValueTarget: result }), { [QUERY]: hydrate });
};

/** `Atom.query_stream keys stream`: the latest element of `stream`, which a mutation of one of `keys` starts anew. */
export const atomQueryStream = <A, E>(keys: ReadonlyArray<string>, stream: Stream.Stream<A, E>): Atom.Atom<object> =>
  Atom.withReactivity(keys)(Atom.map(Atom.make(stream), toAsync));

// the registry's `Reactivity`, the one `withReactivity` registers queries with: both build it with the registry's memo map
const reactive = /*#__PURE__*/ Atom.runtime(Reactivity.layer);

/** `Atom.mutation keys f payload`: `f payload`, then, when it succeeds, the queries of `keys` run anew. */
export const atomMutation = <P>(
  keys: ReadonlyArray<string>,
  f: (payload: P) => Effect.Effect<unknown, unknown, unknown>,
  payload: P,
): Effect.Effect<unknown, unknown, unknown> =>
  Effect.tap(f(payload), () =>
    Effect.map(Effect.service(WriteService), (write) => {
      const registry = nativeRegistry(write);
      if (registry === undefined) return;
      const built = registry.get(reactive);
      if (!AsyncResult.isSuccess(built)) return;
      // one batch, so a query of several of the keys runs anew once
      Atom.batch(() => Context.get(built.value, Reactivity.Reactivity).invalidateUnsafe(keys));
    }),
  );

/** `Atom.derived`: an atom the Sync `compute` computes, depending on the atoms it reads. */
export const atomDerived = <A>(compute: (unit: undefined) => A): Atom.Atom<A> =>
  Atom.readable((ctx) => syncReading(() => compute(undefined), (atom) => ctx(atom)));

/**
 * The label the compiler gives a top-level atom, view or action,
 * `Module.name`, which debugging tools show; one it has already stays.
 */
export const label = <A extends object>(name: string, value: A): A => {
  if (Reflect.get(value, "label") !== undefined) return value;
  if (Atom.isAtom(value)) {
    const labelled = Atom.withLabel(value, name);
    if (Atom.isWritable(labelled)) keepState(name, labelled);
    return labelled;
  }
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

/** The browser's URL, none on a server. */
const location = (): string | undefined => (typeof window === "undefined" ? undefined : window.location.href);

/** The URL a server renders, which `Atom.serving` sets in a request's registry. */
const requestUrl = /*#__PURE__*/ Atom.keepAlive(Atom.make<string | undefined>(undefined));

/** `Atom.serving url`: the URL the registry renders, which `Atom.route` reads on a server. */
export const atomServing = (url: string): Effect.Effect<void, never, unknown> =>
  Effect.map(Effect.service(WriteService), (write) => nativeRegistry(write)?.set(requestUrl, url));

/**
 * `Atom.route url fallback`: the value of the page's URL, `fallback` when no
 * route matches; a write pushes the value's URL to the history. A server, and
 * a root while it hydrates, read the URL `Atom.serving` gave, else the
 * browser's location.
 */
export const atomRoute = <A>(codec: UrlCodec, fallback: A): Atom.Writable<A> => {
  // the same URL gives the same value: a server's stream compares what a view read by identity
  let last: { readonly url: string | undefined; readonly value: A } | undefined;
  const parse = (url: string | undefined): A => {
    if (last !== undefined && last.url === url) return last.value;
    const parsed = url === undefined ? undefined : routeParse(codec, url);
    // oxlint-disable-next-line typescript/no-unsafe-type-assertion -- `parse` gives a value of the route type
    const value = parsed === undefined ? fallback : (parsed as A);
    last = { url, value };
    return value;
  };
  const atom = Atom.writable<A, A>(
    (get) => {
      const served = get(requestUrl);
      if (served !== undefined || typeof window === "undefined") return parse(served);
      const update = () => get.setSelf(parse(location()));
      window.addEventListener("popstate", update);
      get.addFinalizer(() => window.removeEventListener("popstate", update));
      return parse(location());
    },
    (ctx, value) => {
      if (typeof window !== "undefined") {
        // oxlint-disable-next-line typescript/no-unsafe-type-assertion -- a value of the route type
        const url = routePrint(codec, value as Readonly<Record<string, unknown>>);
        if (url !== window.location.pathname + window.location.search) window.history.pushState(null, "", url);
      }
      ctx.setSelf(value);
    },
  );
  return Atom.keepAlive(Atom.withServerValue(atom, (get) => parse(get(requestUrl) ?? location())));
};

/** `Atom.on_server`: the browser atom, read as `value` on a server and while a root hydrates. */
export const atomOnServer = <A>(value: A, atom: Atom.Writable<A>): Atom.Writable<A> => Atom.withServerValue(atom, () => value);

/** `Atom.client_only`: the browser atom's value, `None` on a server and while a root hydrates. */
export const atomClientOnly = <A>(atom: Atom.Writable<A>): Atom.Atom<Option.Option<A>> =>
  Atom.withServerValue(Atom.map(atom, Option.some), () => Option.none());
