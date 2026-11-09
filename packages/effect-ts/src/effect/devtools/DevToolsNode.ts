// The DevTools client with handshake headers, which only Node's WebSocket
// (the `ws` package) sends: a separate entry point, so `DevTools.layer` in
// a browser loads none of it.
import * as NodeWS from "@effect/platform-node/NodeWS";
import { Layer } from "effect";
import * as DevTools from "effect/devtools/DevTools";
import * as Socket from "effect/socket/Socket";

/** `DevTools.layer_with { url; headers }`. */
export const devToolsLayerWith = (options: { readonly url: string; readonly headers: ReadonlyArray<readonly [string, string]> }) =>
  DevTools.layerSocket.pipe(
    Layer.provide(Socket.layerWebSocket(options.url, { headers: Object.fromEntries(options.headers) })),
    Layer.provide(NodeWS.layerWebSocketConstructor),
  );
