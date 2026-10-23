// Layers. `provide` is a lexical owner: the layer is built into it before
// the body runs, one build shares each layer, and what the layer acquired
// is released by the owner's two-phase close after the body.
import { type Context, Effect, Layer } from "effect";

import { withOwner } from "./owner.ts";

/** `provide layer { body }`. */
export const provide = <ROut, E, RIn, A, E2, R>(
  layer: Layer.Layer<ROut, E, RIn>,
  body: Effect.Effect<A, E2, R>,
): Effect.Effect<A, unknown, RIn | Exclude<R, ROut>> =>
  withOwner((owner) =>
    Effect.flatMap(Layer.buildWithScope(layer, owner.resources), (services) => Effect.provideContext(body, services)),
  );

export const layerSucceed = <I, S>(tag: Context.Key<I, S>, impl: S): Layer.Layer<I> => Layer.succeed(tag, impl);

export const layerMerge = <A, E, R, A2, E2, R2>(
  a: Layer.Layer<A, E, R>,
  b: Layer.Layer<A2, E2, R2>,
): Layer.Layer<A | A2, E | E2, R | R2> => Layer.merge(a, b);

/** Both layers; services `b` builds replace those of `a`. */
export const layerOverride = layerMerge;
