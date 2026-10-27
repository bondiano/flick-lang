// Preloaded by `flick run --devtools` (`node --import`): the DevTools at
// `FLICK_DEVTOOLS` get the spans of the program and, with the level at
// Debug, the log of each atom write as an event of its span. A module of
// its own, so no built program carries the DevTools.
import { Effect, Layer, References } from "effect";

import { devToolsLayer } from "../effect/devtools/DevTools.ts";
import { aroundMain } from "../entry.ts";
import { env } from "../host.ts";

const layer = Layer.merge(
  devToolsLayer(env("FLICK_DEVTOOLS") ?? "ws://localhost:34437"),
  Layer.succeed(References.MinimumLogLevel, "Debug"),
);

aroundMain((program) => Effect.provide(program, layer));
