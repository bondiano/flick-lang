// Effect's `effect/devtools` client for the standard library: a tracer that
// sends a program's spans to the Effect DevTools over a WebSocket.
import * as DevTools from "effect/devtools/DevTools";

export * from "effect/devtools/DevTools";

/** `DevTools.layer url`: the program's spans sent to the DevTools at `url`. */
export const devToolsLayer = (url: string) => DevTools.layer(url);
