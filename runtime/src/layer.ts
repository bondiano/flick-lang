// Layers. `provide` is a lexical owner: the layer is built into it before
// the body runs, one build shares each layer, and what the layer acquired
// is released by the owner's two-phase close after the body.
import { type Context, Effect, Layer, Scope } from "effect";

import { withOwner } from "./owner.ts";
import { traceEvent, traceLayer, tracing } from "./trace.ts";

/** `inner`, writing to the trace each layer it builds and its release. */
const tracedMemoMap = (inner: Layer.MemoMap): Layer.MemoMap => {
  const self: Layer.MemoMap = {
    "~effect/Layer/MemoMap": "~effect/Layer/MemoMap",
    get: (layer, scope) => inner.get(layer, scope),
    getOrElseMemoize: (layer, scope, build) =>
      inner.getOrElseMemoize(layer, scope, (_, built) =>
        Effect.andThen(
          Effect.suspend(() => {
            const l = traceLayer(layer);
            traceEvent({ e: "build", l });
            return Scope.addFinalizer(built, Effect.sync(() => traceEvent({ e: "release", l })));
          }),
          // the layers it builds go through this map too
          build(self, built),
        ),
      ),
  };
  return self;
};

/** `layer` built into `scope`, as `Layer.buildWithScope`; traced when the trace is on. */
const buildInto = <ROut, E, RIn>(layer: Layer.Layer<ROut, E, RIn>, scope: Scope.Scope) =>
  tracing
    ? Effect.flatMap(Effect.context(), (context) =>
        Layer.buildWithMemoMap(layer, tracedMemoMap(Layer.CurrentMemoMap.forkOrCreate(context)), scope),
      )
    : Layer.buildWithScope(layer, scope);

/** `provide layer { body }`. */
export const provide = <ROut, E, RIn, A, E2, R>(
  layer: Layer.Layer<ROut, E, RIn>,
  body: Effect.Effect<A, E2, R>,
): Effect.Effect<A, unknown, RIn | Exclude<R, ROut>> =>
  withOwner(
    (owner) =>
      Effect.flatMap(buildInto(layer, owner.resources), (services) => Effect.provideContext(body, services)),
    "provide",
  );

export const layerSucceed = <I, S>(tag: Context.Key<I, S>, impl: S): Layer.Layer<I> => Layer.succeed(tag, impl);

export const layerMerge = <A, E, R, A2, E2, R2>(
  a: Layer.Layer<A, E, R>,
  b: Layer.Layer<A2, E2, R2>,
): Layer.Layer<A | A2, E | E2, R | R2> => Layer.merge(a, b);

/** Both layers; services `b` builds replace those of `a`. */
export const layerOverride = layerMerge;
