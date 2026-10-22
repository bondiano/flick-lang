// Layers. `provide` is a lexical owner: the layer is built into it before
// the body runs, one build shares each layer, and what the layer acquired
// is released by the owner's two-phase close after the body.
import { Effect, Layer } from "effect";

import { withOwner } from "./owner.js";

/** `provide layer { body }`. */
export const provide = (layer, body) =>
  withOwner((owner) =>
    Effect.flatMap(Layer.buildWithScope(layer, owner.resources), (services) =>
      Effect.provideContext(body, services),
    ),
  );

export const layerSucceed = (tag, impl) => Layer.succeed(tag, impl);

export const layerMerge = (a, b) => Layer.merge(a, b);

/** Both layers; services `b` builds replace those of `a`. */
export const layerOverride = (a, b) => Layer.merge(a, b);
