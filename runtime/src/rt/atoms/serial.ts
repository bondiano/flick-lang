// Serializable atoms on Flick's own runtime: a server hands their values to
// the client as JSON under the key the compiler gives, coded by a `Codec`, and
// a client root writes them into its registry before its first render. A
// state takes the value; a task atom takes the seed of its next run instead,
// so it does not run again on the client and still reads what it reads.
import { tag } from "../../tags.ts";
import { fail, raise } from "../cause.ts";
import { Failure } from "../outcome.ts";
import type { Codec } from "../schema.ts";
import { type Atom, type Reading, type Registry, type Serial, mapped, patched, taskAtom } from "./registry.ts";
import { toAsync } from "./services.ts";

const LOADED = /*#__PURE__*/ tag("Atom.Async.Loaded");
const FAILED = /*#__PURE__*/ tag("Atom.Async.Failed");

/** How a hydrated result reaches a task atom: the next run gives it instead of running. */
const SEED = /*#__PURE__*/ Symbol.for("@flick-lang/runtime/Atom.seed");

type Seed = (run: () => unknown) => void;

const isSeed = (value: unknown): value is Seed => typeof value === "function";

/** The field `name` of `value`, if it is an object. */
const field = (value: unknown, name: string): unknown =>
  typeof value === "object" && value !== null ? Reflect.get(value, name) : undefined;

/** The task atom of `make`, whose next run is a seeded result once one is given. */
export const seededTask = (make: (reading: Reading) => () => unknown): Atom => {
  let seed: (() => unknown) | undefined;
  const atom = mapped(
    taskAtom((reading) => {
      const task = make(reading);
      const seeded = seed;
      seed = undefined;
      return seeded ?? task;
    }),
    toAsync,
  );
  return Object.assign(atom, { [SEED]: (run: () => unknown) => void (seed = run) });
};

/** `atom` with the seed of `from`, which wraps it. */
export const withSeed = (atom: Atom, from: Atom): Atom => {
  const seed: unknown = Reflect.get(from, SEED);
  return Object.assign(atom, { [SEED]: seed });
};

/** A result of `codec`, or what fails it, named by the atom's key. */
const coded = (key: string, what: string, result: { readonly _tag: string; readonly success?: unknown; readonly failure?: unknown }): unknown => {
  if (result._tag === "Success") return result.success;
  throw new Error(`the serializable atom \`${key}\` does not ${what} its value`, { cause: result.failure });
};

/** `Reactivity.Atom.serializable`: the state under `key`, coded by a JSON codec for hydration. */
export const serializable = <A>(key: string, codec: Codec<A>, atom: Atom<A>): Atom<A> => {
  const serial: Serial = {
    key,
    seeds: false,
    handedOver: () => true,
    // oxlint-disable-next-line typescript/no-unsafe-type-assertion -- the atom holds a value of the codec's type
    encode: (value) => coded(key, "encode", codec.encode(value as A)),
    decode: (wire) => coded(key, "decode", codec.decode(wire)),
  };
  return patched(atom, { serial, label: atom.label ?? key });
};

/** The error of a failure of one typed error. */
const publicError = (value: unknown): { readonly error: unknown } | undefined => {
  const failure = field(value, "_0");
  if (!(failure instanceof Failure)) return undefined;
  const [reason, ...rest] = failure.cause.reasons;
  return reason?._tag === "Fail" && rest.length === 0 ? { error: reason.error } : undefined;
};

const isLoaded = (value: unknown): boolean => field(value, "_tag") === LOADED;

/**
 * `Atom.serializable_task` under `key`: a loaded result is coded by `codec`,
 * and the client starts from it instead of running the task; one still loading
 * or failed is not handed over. With `errors`, a failure of one typed error is
 * handed over as well, coded by it, never a defect or a stack.
 */
export const serializableTask = (key: string, codec: Codec, atom: Atom<object>, errors?: Codec): Atom<object> => {
  const seed: unknown = Reflect.get(atom, SEED);
  const seedWith = (run: () => unknown): void => {
    if (isSeed(seed)) seed(run);
  };
  const serial: Serial = {
    key,
    seeds: true,
    handedOver: (value) => isLoaded(value) || (errors !== undefined && field(value, "_tag") === FAILED && publicError(value) !== undefined),
    encode: (value) => {
      if (isLoaded(value)) {
        const loaded = coded(key, "encode", codec.encode(field(value, "_0")));
        return errors === undefined ? loaded : { ok: loaded };
      }
      const error = errors === undefined ? undefined : publicError(value);
      if (errors === undefined || error === undefined) throw new Error(`the task atom \`${key}\` has no loaded value to hand over`);
      return { err: coded(key, "encode", errors.encode(error.error)) };
    },
    decode: (wire) => {
      const sent: unknown = errors === undefined ? { ok: wire } : wire;
      if (errors !== undefined && typeof sent === "object" && sent !== null && "err" in sent) {
        const error = coded(key, "decode", errors.decode(field(sent, "err")));
        seedWith(() => raise(fail(error)));
        return Object.freeze({ _tag: FAILED, _0: new Failure(fail(error)) });
      }
      const value = coded(key, "decode", codec.decode(field(sent, "ok")));
      seedWith(() => value);
      return Object.freeze({ _tag: LOADED, _0: value });
    },
  };
  // kept alive, so a hydrated result is not dropped, and the task run, before the first view subscribes
  return patched(atom, { serial, keepAlive: true, label: atom.label ?? key });
};

/** The entries the server hands over: each serializable atom of `registry` with a value to hand over. */
export const entries = (registry: Registry): Array<{ readonly key: string; readonly value: unknown }> => {
  const all: Array<{ readonly key: string; readonly value: unknown }> = [];
  for (const node of registry.all()) {
    const serial = node.atom.serial;
    // a node not yet computed has no value; a task not yet loaded is run by the client
    if (serial === undefined || node.state !== 2 || !serial.handedOver(node.value)) continue;
    all.push({ key: serial.key, value: serial.encode(node.value) });
  }
  return all;
};

/** Writes the entries a server handed over into `registry`: an atom it has takes its value, a key it has not waits for one. */
export const hydrate = (registry: Registry, list: ReadonlyArray<{ readonly key: string; readonly value: unknown }>): void => {
  const known = new Map<string, Atom>();
  for (const node of registry.all()) if (node.atom.serial !== undefined) known.set(node.atom.serial.key, node.atom);
  for (const { key, value } of list) {
    const atom = known.get(key);
    if (atom === undefined) registry.preloaded.set(key, value);
    else if (atom.serial?.seeds === true) atom.serial.decode(value);
    else registry.set(atom, atom.serial?.decode(value));
  }
};
