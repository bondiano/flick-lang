// The HTTP handler of an RPC group, for a route of `Http.Router`. It needs
// no Node: a browser bundle that reaches it, through a module shared with
// the server, loads.
import { Exit, Effect, Scope } from "effect";
import * as RpcServer from "effect/rpc/RpcServer";

import { toHandler } from "../effect/http/handler.ts";
import { acquireRelease } from "../owner.ts";
import { type Handler, prepare } from "./handlers.ts";
import type { Group, Serialization } from "./model.ts";

/** How the server takes calls: a POST per call, or the messages of a WebSocket. */
export type Served = "http" | "websocket";

/** Effect's HTTP app of the group with `handlers`, and the scope its calls run in. */
export const appOf = (transport: Served, serialization: Serialization, group: Group, handlers: Readonly<Record<string, Handler>>) =>
  Effect.gen(function* () {
    const { rpcs, scope, handled } = yield* prepare(group, handlers);
    const app = yield* (transport === "http" ? RpcServer.toHttpEffect(rpcs) : RpcServer.toHttpEffectWebsocket(rpcs)).pipe(
      Effect.provideContext(handled),
      Effect.provide(serialization),
      Effect.provideService(Scope.Scope, scope),
    );
    return { app, scope };
  });

/** The Flick handler of `Rpc.handler`: the group's calls, owned by the innermost scope. */
const handlerOf =
  (transport: Served) => (serialization: Serialization, group: Group, handlers: Readonly<Record<string, Handler>>) =>
    Effect.map(
      acquireRelease(appOf(transport, serialization, group, handlers), ({ scope }) => Scope.close(scope, Exit.void)),
      ({ app }) => toHandler(Effect.scoped(app)),
    );

/** `Rpc.handler serialization group handlers`: a POST per call, for a route. */
export const rpcHandler = handlerOf("http");

/** `Rpc.handler_websocket serialization group handlers`: a WebSocket per client, for a `GET` route. */
export const rpcHandlerWebsocket = handlerOf("websocket");
