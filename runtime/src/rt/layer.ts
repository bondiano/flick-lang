// Layers (docs/formal/runtime.md §3.5): a value with an identity that builds
// services into a scope. A memo map builds each memoizable layer once, its
// scope closed when the last owner that asked for it closes. `provide layer {
// body }` builds it into the owner of the body. A nested `provide` reads the
// memo map of the one around it; sibling ones build anew.
import { type Cause, type Exit, raise } from "./cause.ts";
import { type Ctx, type Run, Suspend, Under, asExit, currentCtx, exitOf, extend, hosted, lookup, owned, raiseOf } from "./fiber.ts";
import { Owner, addTo, close, withOwner } from "./owner.ts";
import { traceEvent, traceLayer, tracing } from "./trace.ts";

/** What a layer gives: implementations by tag, later ones replacing earlier ones. */
type Services = ReadonlyArray<readonly [tag: object, impl: unknown]>;

/** How a layer builds: through the memo map, into the owner. */
type Build = (memo: MemoMap, into: Owner) => Run;

export class Layer {
  readonly build: Build;
  constructor(build: Build) {
    this.build = build;
  }
}

/** spike: what `bridge.ts` puts in; an Effect layer becomes a kernel one, its Context under `HOST`. */
export const HOST = /*#__PURE__*/ Object.freeze({ key: "flick/bridge/HostContext" });
export const layerHooks: {
  adopt: (layer: unknown) => Layer;
  merge: (a: unknown, b: unknown) => unknown;
} = {
  adopt: (layer) => {
    if (layer instanceof Layer) return layer;
    throw new TypeError("not a layer of this runtime");
  },
  merge: (_, b) => b,
};

const servicesOf = (value: unknown): Services => (Array.isArray(value) ? (value as Services) : []);

interface Entry {
  observers: number;
  readonly scope: Owner;
  built: boolean;
  services: Services;
  cause: Cause | undefined;
  readonly waiting: Array<() => void>;
}

/** What one build shares; a forked map reads the entries of its parent. */
export class MemoMap {
  readonly parent: MemoMap | undefined;
  readonly entries = new Map<Layer, Entry>();
  constructor(parent: MemoMap | undefined) {
    this.parent = parent;
  }
  find(layer: Layer): readonly [Entry, MemoMap] | undefined {
    const entry = this.entries.get(layer);
    return entry === undefined ? this.parent?.find(layer) : [entry, this];
  }
}

export const MEMO = /*#__PURE__*/ Object.freeze({ key: "effect/Layer/CurrentMemoMap" });

const held = (map: MemoMap, layer: Layer, entry: Entry, into: Owner): unknown => {
  entry.observers++;
  return addTo(
    into,
    function* (exit: Exit): Run {
      entry.observers--;
      if (entry.observers > 0) return undefined;
      if (map.entries.get(layer) === entry) map.entries.delete(layer);
      return raiseOf(asExit(yield close(entry.scope, exit)));
    },
    true,
  );
};

/** The services of a memoized layer: of its entry, or built into a new one. */
function* memoized(self: Layer, memo: MemoMap, into: Owner, build: Build): Run {
  const found = memo.find(self);
  if (found !== undefined) {
    const [entry, at] = found;
    yield held(at, self, entry, into);
    if (!entry.built) {
      yield new Suspend((resolve) => {
        entry.waiting.push(() => resolve(undefined));
      });
    }
    if (entry.cause !== undefined) return raise(entry.cause);
    return entry.services;
  }
  const entry: Entry = { observers: 0, scope: new Owner(), built: false, services: [], cause: undefined, waiting: [] };
  memo.entries.set(self, entry);
  yield held(memo, self, entry, into);
  if (tracing) {
    const id = traceLayer(self);
    traceEvent({ e: "build", l: id });
    entry.scope.finalizers.push({
      ctx: currentCtx(),
      run: () => {
        traceEvent({ e: "release", l: id });
      },
      place: 0,
    });
  }
  const built = asExit(yield exitOf(() => build(memo, entry.scope)));
  entry.built = true;
  entry.cause = built.cause;
  entry.services = built.cause === undefined ? servicesOf(built.value) : [];
  for (const wake of entry.waiting) wake();
  return raiseOf(built.cause === undefined ? built : built);
}

/** A layer whose build the memo map shares. */
export const memoizable = (build: Build): Layer => {
  const self: Layer = new Layer((memo, into) => memoized(self, memo, into, build));
  return self;
};

/** Runs `task` as a build: its owner `scope`, the map `memo` current. */
const building = (memo: MemoMap, scope: Owner, task: () => unknown): Under =>
  new Under(owned(extend(currentCtx(), MEMO, memo), scope), task);

// oxlint-disable-next-line require-yield -- what it returns is what the layer gives
function* constant(services: Services): Run {
  return services;
}

export const layerSucceed = (tag: object, impl: unknown): Layer => new Layer(() => constant([[tag, impl]]));

export const empty: Layer = /*#__PURE__*/ new Layer(() => constant([]));

/** Both layers; services `b` builds replace those of `a`. */
// shortcut: builds one after the other into one owner, not side by side into scopes of their own; upgrade when the rest of `Layer` comes
export const layerMerge = (a: Layer, b: Layer): Layer =>
  new Layer(function* (memo, into): Run {
    const first = servicesOf(yield layerHooks.adopt(a).build(memo, into));
    const second = servicesOf(yield layerHooks.adopt(b).build(memo, into));
    return [...first, ...second];
  });

export const layerOverride = layerMerge;

/** `Layer.effect tag task`: the implementation `task` computes when the layer builds. */
export const effect =
  (tag: object) =>
  (task: () => unknown): Layer =>
    memoizable(function* (memo, into): Run {
      const impl: unknown = yield building(memo, into, task);
      return [[tag, impl]];
    });

/** The evidence `ctx` with `services`, the map `memo` current. */
const provided = (ctx: Ctx, services: Services, memo: MemoMap): Ctx =>
  services.reduce(
    (c: Ctx, [tag, impl]) => (tag === HOST ? hosted(c, layerHooks.merge(c.host, impl)) : extend(c, tag, impl)),
    extend(ctx, MEMO, memo),
  );

function* provide(given: Layer, body: (unit: undefined) => unknown): Run {
  const layer = layerHooks.adopt(given);
  return yield withOwner(function* (owner: Owner): Run {
    const outer = lookup(MEMO);
    const memo = new MemoMap(outer instanceof MemoMap ? outer : undefined);
    const services = servicesOf(yield building(memo, owner, () => layer.build(memo, owner)));
    return new Under(provided(currentCtx(), services, memo), () => body(undefined));
  }, "provide");
}

/** `Layer.run layer (fun () -> body)`, which `provide layer { body }` stands for. */
export const layerRun = (layer: Layer, body: (unit: undefined) => unknown): Run => provide(layer, body);
