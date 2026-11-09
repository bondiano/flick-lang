// The services of atoms on Flick's own runtime: `Reactivity.Atom.Registry`
// and `Atom.Write`, which a registry's layer provides, and `Atom.Read`, which
// a render or a derived atom reads through. All of them run at once.
import { tag } from "../../tags.ts";
import { type Cause, die, raise } from "../cause.ts";
import { currentCtx, extend, inCtx, runIn } from "../fiber.ts";
import { implementation, service } from "../handle.ts";
import { type Layer, memoizable } from "../layer.ts";
import { Failure } from "../outcome.ts";
import { addTo } from "../owner.ts";
import { type AsyncResult, type Atom, Registry } from "./registry.ts";

export const READ = /*#__PURE__*/ service("Atom.Read");
export const WRITE = /*#__PURE__*/ service("Atom.Write");
export const REGISTRY = /*#__PURE__*/ service("Reactivity.Atom.Registry");

/** The operations of `Reactivity.Atom.Registry` over `registry`. */
const registryImpl = (registry: Registry) => ({
  registry,
  get: (atom: Atom) => registry.get(atom),
  set: (atom: Atom) => (value: unknown) => registry.set(atom, value),
  update: (atom: Atom) => (f: (a: unknown) => unknown) => registry.update(atom, f),
  modify: (atom: Atom) => (f: (a: unknown) => readonly [unknown, unknown]) => registry.modify(atom, f),
  refresh: (atom: Atom) => registry.refresh(atom),
});

/** The operations of `Atom.Write`: the writes, each at its place in the source, and reads without tracking. */
const writeImpl = (registry: Registry) => ({
  registry,
  set: (_place: string) => (atom: Atom) => (value: unknown) => registry.set(atom, value),
  update: (_place: string) => (atom: Atom) => (f: (a: unknown) => unknown) => registry.update(atom, f),
  modify: (_place: string) => (atom: Atom) => (f: (a: unknown) => readonly [unknown, unknown]) => registry.modify(atom, f),
  current: (atom: Atom) => registry.get(atom),
  refresh: (atom: Atom) => registry.refresh(atom),
});

/** The registry of an implementation of `Reactivity.Atom.Registry` or `Atom.Write`, if a layer here built it. */
export const registryOf = (impl: unknown): Registry | undefined => {
  const registry: unknown = typeof impl === "object" && impl !== null ? Reflect.get(impl, "registry") : undefined;
  return registry instanceof Registry ? registry : undefined;
};

/** The registry of the service `key` in effect here. */
export const registryHere = (key: object = REGISTRY): Registry => {
  const registry = registryOf(implementation(key));
  if (registry === undefined) throw new Error("`Reactivity.Atom.Registry` was not built by its `layer`");
  return registry;
};

/** A layer of a fresh registry, released with the layer: its services of it are what `services` makes. */
const registryLayer = (services: (registry: Registry) => ReadonlyArray<readonly [object, unknown]>): Layer =>
  memoizable((_memo, into) => {
    const registry = new Registry(currentCtx());
    addTo(into, () => registry.dispose(), true);
    return services(registry);
  });

/** `Atom.layer`: a fresh registry as `Reactivity.Atom.Registry` and as `Atom.Write`. */
export const atomLayer: Layer = /*#__PURE__*/ registryLayer((registry) => [
  [REGISTRY, registryImpl(registry)],
  [WRITE, writeImpl(registry)],
]);

/** `Reactivity.Atom.layer`: a fresh registry as the service `key`. */
export const flickLayer = (key: object): Layer => registryLayer((registry) => [[key, registryImpl(registry)]]);

/** What `Atom.pending` dies with while a task loads: a view of `Ui.suspended` shows its fallback. */
const PENDING = /*#__PURE__*/ Symbol.for("@flick-lang/runtime/Atom.pending");

/** `Atom.pending`: the read of a task still loading. */
export const atomPending = (): never => raise(die(PENDING));

/** Whether `cause` is a task still loading under `Atom.ready`. */
export const isPending = (cause: Cause): boolean => cause.reasons.some((r) => r._tag === "Die" && r.defect === PENDING);

/** The Sync `f`, its reads of atoms going to `read`. */
export const reading = <A>(read: (atom: Atom) => unknown, f: () => A): A =>
  // oxlint-disable-next-line typescript/no-unsafe-type-assertion -- what the Sync stage gives
  inCtx(extend(currentCtx(), READ, { get: read }), () => runIn(f()) as A);

const LOADING = /*#__PURE__*/ Object.freeze({ _tag: tag("Atom.Async.Loading") });
const LOADED = /*#__PURE__*/ tag("Atom.Async.Loaded");
const FAILED = /*#__PURE__*/ tag("Atom.Async.Failed");

/** A task's result as the Flick value `Atom.Async a e`. */
export const toAsync = (result: AsyncResult): object => {
  if (result._tag === "Initial") return LOADING;
  if (result._tag === "Success") return Object.freeze({ _tag: LOADED, _0: result.value });
  return Object.freeze({ _tag: FAILED, _0: new Failure(result.cause) });
};
