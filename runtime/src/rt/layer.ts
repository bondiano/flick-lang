// Layers (docs/formal/runtime.md §3.5): a value with an identity that builds
// services into a scope. A memo map builds each memoizable layer once, its
// scope closed when the last owner that asked for it closes. `provide layer {
// body }` builds it into the owner of the body. A nested `provide` reads the
// memo map of the one around it; sibling ones build anew.
import { type Cause, type Exit, combine, empty, raise } from "./cause.ts";
import { allOf } from "./concurrency.ts";
import { mapped } from "./effects.ts";
import { type Ctx, type Run, Suspend, Under, asExit, currentCtx, exitOf, extend, hosted, lookup, raiseOf, uninterruptible } from "./fiber.ts";
import { Owner, addTo, closeOwn, finalize, inside, withOwner } from "./owner.ts";
import { traceEvent, traceLayer, tracing } from "./trace.ts";

/** Built services, a `Context`: implementations by tag, later ones replacing earlier ones. */
export type Services = ReadonlyArray<readonly [tag: object, impl: unknown]>;

/** How a layer builds: through the memo map, into the owner. What it gives, a value or a call, gives the services. */
export type Build = (memo: MemoMap, into: Owner) => unknown;

export class Layer {
  readonly build: Build;
  constructor(build: Build) {
    this.build = build;
  }
}

/** An Effect Context carried by a layer that the optional adapter supplied. */
export const HOST = /*#__PURE__*/ Object.freeze({ key: "flick/bridge/HostContext" });
export const layerHooks: {
  adopt: (layer: unknown) => Layer;
  /** The host Context of the evidence `ctx`, with a provided layer's Context `b` over it. */
  merge: (ctx: Ctx, b: unknown) => unknown;
  services: (value: unknown) => Services | undefined;
  context: (services: Services) => unknown;
} = {
  adopt: (layer) => {
    if (layer instanceof Layer) return layer;
    throw new TypeError("not a layer of this runtime");
  },
  merge: (_, b) => b,
  services: () => undefined,
  context: (services) => services,
};

export const servicesOf = (value: unknown): Services => (Array.isArray(value) ? (value as Services) : layerHooks.services(value) ?? []);

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

/** The key of the memo map of the build at hand. */
export const MEMO: object = /*#__PURE__*/ Object.freeze({ key: "effect/Layer/CurrentMemoMap" });

/** The memo map a build here uses: forked from the one of the build it runs in, or a new one. */
export const currentMemo = (): MemoMap => {
  const outer = lookup(MEMO);
  return new MemoMap(outer instanceof MemoMap ? outer : undefined);
};

/** Closes `scope` after what it held ended with `exit`: it fails as its own finalizers do. */
function* release(scope: Owner, exit: Exit): Run {
  return raiseOf(asExit(yield uninterruptible(() => closeOwn(scope, exit))));
}

/** A scope closed when `parent` closes, with how it ended. */
export const forkScope = (parent: Owner): Owner => {
  const child = new Owner(parent.defects);
  addTo(parent, (exit: Exit) => release(child, exit), true);
  return child;
};

/** A layer that builds into a scope of its own, forked from the one it is built into and closed at once if the build fails. */
export const fromBuild = (build: Build): Layer =>
  new Layer(function* (memo, into): Run {
    const inner = forkScope(into);
    const built = asExit(yield exitOf(() => build(memo, inner)));
    if (built.cause === undefined) return built.value;
    // as `on_exit`: a finalizer's defect after the failure makes its failures defects
    return raiseOf(finalize(built, asExit(yield uninterruptible(() => closeOwn(inner, built)))));
  });

/** Makes `into` hold `entry` until it closes: the last one to close releases it. */
const held = (map: MemoMap, layer: Layer, entry: Entry, into: Owner): unknown => {
  entry.observers++;
  return addTo(
    into,
    function* (exit: Exit): Run {
      entry.observers--;
      if (entry.observers > 0) return undefined;
      if (map.entries.get(layer) === entry) map.entries.delete(layer);
      return yield release(entry.scope, exit);
    },
    true,
  );
};

/** The services of a memoized layer: of its entry, or built into a new one. */
function* getOrBuild(self: Layer, memo: MemoMap, into: Owner, build: Build): Run {
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
  // a closing scope cannot hold a shared entry
  if (into.finalizing) return build(memo, into);
  const entry: Entry = { observers: 0, scope: new Owner(into.defects), built: false, services: [], cause: undefined, waiting: [] };
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
  for (const wake of entry.waiting.splice(0)) wake();
  return raiseOf(built);
}

/** A layer whose build the memo map shares. */
export const memoizable = (build: Build): Layer => {
  const self: Layer = fromBuild((memo, into) => getOrBuild(self, memo, into, build));
  return self;
};

/** Runs `task` acquiring into `scope`. */
export const scoped = (scope: Owner, task: () => unknown): Under => inside(scope, task);

/** Builds `layer` into `scope` through `memo`, which the build and what runs with its services read as the current map. */
export const buildWith = (layer: Layer, memo: MemoMap, scope: Owner): Under =>
  new Under(extend(currentCtx(), MEMO, memo), () =>
    mapped(layerHooks.adopt(layer).build(memo, scope), (services): Services => [...servicesOf(services), [MEMO, memo]]),
  );

/** Runs `body` with a layer's services as values in the caller's evidence. */
export const withServices = (services: Services, body: () => unknown): Under =>
  new Under(services.reduce((c: Ctx, [tag, impl]) =>
    tag === HOST ? hosted(c, layerHooks.merge(c, impl)) : extend(c, tag, impl), currentCtx()), body);

/** A layer of services built without effects. */
export const constant = (services: Services): Layer => new Layer(() => services);

export const layerSucceed = (tag: object, impl: unknown): Layer => constant([[tag, impl]]);

/** What `allOf` gave, known to be a list. */
const listOf = (value: unknown): ReadonlyArray<unknown> => (Array.isArray(value) ? value : []);

/** Closes the scopes side by side, the last acquired starting first, as Go's `flickrt` does: they fail as their own finalizers do, joined. */
function* closeAll(scopes: ReadonlyArray<Owner>, exit: Exit): Run {
  const causes: Array<Cause> = scopes.map(() => empty);
  yield allOf(
    scopes
      .map((scope, i) =>
        function* (): Run {
          causes[i] = asExit(yield closeOwn(scope, exit)).cause ?? empty;
          return undefined;
        },
      )
      .toReversed(),
    Infinity,
  );
  const cause = causes.reduce(combine, empty);
  return cause.reasons.length === 0 ? undefined : raise(cause);
}

/**
 * Builds the layers side by side, each into a scope of its own, which release
 * side by side when the merged layer's does; the first failure stops the other
 * builds. The services of a later layer replace those of an earlier one.
 */
export const mergeAll = (layers: ReadonlyArray<Layer>): Layer =>
  fromBuild(function* (memo, into): Run {
    const parts = layers.map((layer) => [layerHooks.adopt(layer), new Owner(into.defects)] as const);
    addTo(into, (exit: Exit) => closeAll(parts.map(([, scope]) => scope), exit), true);
    const built = listOf(
      yield allOf(
        parts.map(([layer, scope]) => () => layer.build(memo, scope)),
        Infinity,
      ),
    );
    return built.flatMap(servicesOf);
  });

/** Both layers; services `b` builds replace those of `a`. */
export const layerMerge = (a: Layer, b: Layer): Layer => mergeAll([a, b]);

export const layerOverride = layerMerge;

function* provide(layer: Layer, body: (unit: undefined) => unknown): Run {
  return yield withOwner(function* (owner: Owner): Run {
    const services = servicesOf(yield buildWith(layer, currentMemo(), owner));
    return withServices(services, () => body(undefined));
  }, "provide");
}

/** `Layer.run layer (fun () -> body)`, which `provide layer { body }` stands for. */
export const layerRun = (layer: Layer, body: (unit: undefined) => unknown): Run => provide(layer, body);
