// The bindings of `Atom` on Flick's own runtime, which compiled code imports
// from `@flick-lang/runtime/rt`.
import type { HashDict } from "../../core/data.ts";
import { none, some } from "../../core/data.ts";
import { absent } from "../builtins.ts";
import type { Run } from "../fiber.ts";
import { implementation } from "../handle.ts";
import type { Codec } from "../schema.ts";
import { type UrlCodec, routeParse, routePrint } from "../route.ts";
import { type Atom, type Reading, isAtom, mapped, patched, readable, state } from "./registry.ts";
import { seededTask, serializableTask, withSeed } from "./serial.ts";
import { WRITE, reading, registryHere, registryOf } from "./services.ts";

export { atomPending } from "./services.ts";

/** `Atom.make`: a state whose value the registry keeps while it lives, read or not. */
export const atomState = <A>(value: A): Atom<A> => patched(state(value), { keepAlive: true });

/** `Atom.fresh`: a state the registry forgets once nothing holds it. */
export const atomLocal = <A>(value: A): Atom<A> => state(value);

/** `Atom.derived`: an atom the Sync `compute` computes, depending on the atoms it reads. */
export const atomDerived = <A>(compute: (unit: undefined) => A): Atom<A> =>
  readable((r) => reading((atom) => r.get(atom), () => compute(undefined)));

/** `Atom.task`: the atom of a task's result, run by the registry that first reads it. */
export const atomTask = (task: () => unknown): Atom => seededTask(() => task);

/** `atom`, which a mutation of one of `keys` refreshes in the registry that reads it. */
const withReactivity = (keys: ReadonlyArray<string>, atom: Atom): Atom =>
  readable(
    (r: Reading) => {
      r.addFinalizer(r.registry.register(keys, () => r.registry.refresh(atom)));
      return r.get(atom);
    },
    (registry) => registry.refresh(atom),
  );

/** `Atom.query keys task`: a task atom that a mutation of one of `keys` runs anew. */
export const atomQuery = (keys: ReadonlyArray<string>, task: () => unknown): Atom => {
  const result = atomTask(task);
  return withSeed(withReactivity(keys, result), result);
};

/**
 * `Atom.loader route load`: the task atom of `load` of the route's value, run
 * anew when the route changes. A hydrated result is its first run's, which
 * still reads the route, so later changes run it.
 */
export const atomLoader = <R>(route: Atom<R>, load: (value: R) => () => unknown): Atom => seededTask((r) => load(r.get(route)));

/** `Atom.mutation keys f payload`: `f payload`, then, when it succeeds, the queries of `keys` run anew. */
export function* atomMutation(keys: ReadonlyArray<string>, f: (payload: unknown) => unknown, payload: unknown): Run {
  const value: unknown = yield f(payload);
  registryOf(implementation(WRITE))?.invalidate(keys);
  return value;
}

/**
 * The label the compiler gives a top-level atom, view or action,
 * `Module.name`, which debugging tools show; one it has already stays.
 */
export const label = <A extends object>(name: string, value: A): A => {
  if (Reflect.get(value, "label") !== undefined) return value;
  // oxlint-disable-next-line typescript/no-unsafe-type-assertion -- an atom labelled is an atom
  if (isAtom(value)) return patched(value, { label: name }) as unknown as A;
  const labelled = { ...value, label: name };
  const prototype: unknown = Object.getPrototypeOf(value);
  if (prototype === null || typeof prototype === "object" || typeof prototype === "function")
    Object.setPrototypeOf(labelled, prototype);
  return Object.freeze(labelled);
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
export const atomForget = <K, A>(family: Family<K, A>, key: K): undefined => {
  const entry = entryOf(family, key);
  if (entry === undefined) return undefined;
  forgetEntry(family, entry);
  const atom = entry.ref.deref();
  if (!isAtom(atom)) return undefined;
  // no `at` gives this atom again, so it need not outlive its readers
  atom.keepAlive = false;
  // a last reader leaving drops the node; with none, this does
  registryHere(WRITE).mount(atom)();
  return undefined;
};

/** The URL a server renders, which `Atom.serving` sets in a request's registry. */
const requestUrl = /*#__PURE__*/ atomState<string | undefined>(undefined);

/** `Atom.serving url`: the URL the registry renders, which `Atom.route` reads on a server. */
export const atomServing = (url: string): undefined => {
  registryHere(WRITE).set(requestUrl, url);
  return undefined;
};

/** `Atom.on_server`: the browser atom, read as `value` on a server and while a root hydrates. */
export const atomOnServer = <A>(value: A, atom: Atom<A>): Atom<A> => patched(atom, { server: () => value });

/** `Atom.client_only`: the browser atom's value, `None` on a server and while a root hydrates. */
export const atomClientOnly = <A>(atom: Atom<A>): Atom =>
  patched(
    mapped(atom, (a) => some(a)),
    { server: () => none },
  );

/** The writes of search parameters not yet in the URL: they go in together, after a pause. */
const searchWrites = new Map<string, string>();
let searchTimer: ReturnType<typeof setTimeout> | undefined;
let pushing = false;

const pushSearch = (): void => {
  searchTimer = undefined;
  const params = new URLSearchParams(window.location.search);
  for (const [name, value] of searchWrites) {
    if (value.length > 0) params.set(name, value);
    else params.delete(name);
  }
  searchWrites.clear();
  const search = params.toString();
  pushing = true;
  try {
    window.history.pushState({}, "", `${window.location.pathname}${search.length > 0 ? `?${search}` : ""}${window.location.hash}`);
  } finally {
    pushing = false;
  }
};

/** `Atom.search_param name`: the URL's search parameter `name`, `""` without one; a write updates the URL. */
export const atomSearchParam = (name: string): Atom<string> => {
  const param = (): string => new URLSearchParams(window.location.search).get(name) ?? "";
  return {
    ...readable((r) => {
      if (typeof window === "undefined") return "";
      const changed = (): void => {
        if (!pushing) r.setSelf(param());
      };
      window.addEventListener("popstate", changed);
      r.addFinalizer(() => window.removeEventListener("popstate", changed));
      return param();
    }),
    keepAlive: true,
    write: (value, setSelf) => {
      setSelf(value);
      if (typeof window === "undefined") return;
      searchWrites.set(name, value);
      if (searchTimer !== undefined) clearTimeout(searchTimer);
      searchTimer = setTimeout(pushSearch, 500);
    },
  };
};

/** `Atom.stored key`: the text the browser's local storage keeps under `key`. */
export const atomStored = (key: string): Atom => {
  const read = () => {
    const text = typeof localStorage === "undefined" ? null : localStorage.getItem(key);
    return text === null ? none : some(text);
  };
  return {
    ...readable((r) => {
      if (typeof window !== "undefined") {
        // another tab's write
        const changed = (event: StorageEvent): void => {
          if (event.key === key || event.key === null) r.setSelf(read());
        };
        window.addEventListener("storage", changed);
        r.addFinalizer(() => window.removeEventListener("storage", changed));
      }
      return read();
    }),
    keepAlive: true,
    write: (value, setSelf) => {
      if (typeof localStorage !== "undefined") {
        const text: unknown = Reflect.get(Object(value), "value");
        if (Reflect.get(Object(value), "_tag") === "Some") localStorage.setItem(key, String(text));
        else localStorage.removeItem(key);
      }
      setSelf(value);
    },
  };
};

/** `Atom.serializable_task` under the key the compiler gives. */
export const atomSerializableTask = (key: string, codec: Codec, atom: Atom<object>): Atom<object> => serializableTask(key, codec, atom);

/** `Atom.serializable_result` under the key the compiler gives: `serializable_task` that also hands over a typed failure. */
export const atomSerializableResult = (key: string, codec: Codec, errors: Codec, atom: Atom<object>): Atom<object> =>
  serializableTask(key, codec, atom, errors);

const location = (): string | undefined => (typeof window === "undefined" ? undefined : window.location.href);

/**
 * `Atom.route codec fallback`: the state of the URL, `fallback` when it does
 * not parse. A server reads the URL `Atom.serving` set; a browser the page's,
 * again on back and forward; a write pushes its URL without a reload.
 */
export const atomRoute = <A>(codec: UrlCodec, fallback: A): Atom<A> => {
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
  return {
    ...readable((r) => {
      const served = r.get(requestUrl);
      if (served !== undefined || typeof window === "undefined") return parse(served);
      const update = (): void => r.setSelf(parse(location()));
      window.addEventListener("popstate", update);
      r.addFinalizer(() => window.removeEventListener("popstate", update));
      return parse(location());
    }),
    keepAlive: true,
    write: (value, setSelf) => {
      if (typeof window !== "undefined") {
        // oxlint-disable-next-line typescript/no-unsafe-type-assertion -- a value of the route type
        const url = routePrint(codec, value as Readonly<Record<string, unknown>>);
        if (url !== window.location.pathname + window.location.search) window.history.pushState(null, "", url);
      }
      setSelf(value);
    },
    server: (get) => {
      const served = get(requestUrl);
      return parse(typeof served === "string" ? served : location());
    },
  };
};

// a stream query reads a `Stream`, which this runtime does not have yet
export const atomQueryStream = /*#__PURE__*/ absent("@flick-lang/runtime", "atomQueryStream");
