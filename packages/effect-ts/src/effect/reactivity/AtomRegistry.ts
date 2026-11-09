// Effect's registry ABI, with stack-safe observation and transactional writes.
import { Effect, Layer, Scope } from "effect";
import { batch } from "effect/reactivity/Atom";
import * as Registry from "effect/reactivity/AtomRegistry";

export * from "effect/reactivity/AtomRegistry";

/**
 * Effect 4.1.2's nodes recursively propagate observation through their parents.
 * Queue those notifications per registry; batch each commit so dependent values
 * settle before listeners run. These two node methods are version-pinned internals.
 * Do not modify Effect's shared prototypes or registries owned by other callers.
 */
export const make = (options?: Parameters<typeof Registry.make>[0]): Registry.AtomRegistry => {
  const registry = Registry.make(options);
  const queue: Array<() => void> = [];
  let draining = false;
  const instrument = (node: Registry.Node<unknown>): void => {
    const observed: unknown = Reflect.get(node, "observedChanged");
    const commit: unknown = Reflect.get(node, "setValue");
    if (typeof observed !== "function" || typeof commit !== "function")
      throw new TypeError("unsupported Effect registry node ABI");
    Reflect.set(node, "observedChanged", (delta: number) => {
      queue.push(() => { Reflect.apply(observed, node, [delta]); });
      if (draining) return;
      draining = true;
      try {
        for (let i = 0; i < queue.length; i++) queue[i]?.();
      } finally {
        queue.length = 0;
        draining = false;
      }
    });
    Reflect.set(node, "setValue", (value: unknown) => batch(() => { Reflect.apply(commit, node, [value]); }));
  };
  for (const node of registry.getNodes().values()) instrument(node);
  registry.onNodeAdded = instrument;
  return registry;
};

export const layerOptions = (options?: Parameters<typeof Registry.make>[0]): Layer.Layer<Registry.AtomRegistry> =>
  Layer.effect(Registry.AtomRegistry, Effect.gen(function* () {
    const scope = yield* Effect.scope;
    const registry = make(options);
    yield* Scope.addFinalizer(scope, Effect.sync(() => registry.dispose()));
    return registry;
  }));

export const layer = /*#__PURE__*/ layerOptions();
