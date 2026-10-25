// `Rpc.listen`: a group served on Node's HTTP server for as long as the
// Flick scope that started it. A separate entry point from the client, so a
// program that only calls procedures loads no Node server.
import * as NodeHttpServer from "@effect/platform-node/NodeHttpServer";
import { Effect, Exit, Scope, Stream } from "effect";
import { HttpRouter } from "effect/http";
import * as RpcServer from "effect/rpc/RpcServer";

import { nodeHttp } from "../host.ts";
import { acquireRelease } from "../owner.ts";
import { effectGroup, type Group, type RpcError, type Server, type ServeOptions, serveError } from "./model.ts";

type Handler = (payload: unknown) => Effect.Effect<unknown, unknown, unknown>;

const isRoutePath = (path: string): path is `/${string}` => path.startsWith("/");

/** How the server takes calls: a POST per call, or the messages of a WebSocket. */
type Transport = "http" | "websocket";

const start = (transport: Transport, options: ServeOptions, group: Group, handlers: Readonly<Record<string, Handler>>) =>
  Effect.gen(function* () {
    const context = yield* Effect.context<unknown>();
    const own: Record<string, (payload: unknown) => unknown> = {};
    for (const procedure of group.procedures) {
      const handler = handlers[procedure.name];
      if (handler === undefined) return yield* Effect.die(new TypeError(`no handler for ${procedure.name}`));
      // a handler runs with the services of `listen`; a stream's handler gives the stream as its result
      own[procedure.name] = procedure.stream
        ? (payload) => Stream.unwrap(Effect.provideContext(Effect.map(handler(payload), toStream), context))
        : (payload) => Effect.provideContext(handler(payload), context);
    }
    const rpcs = effectGroup(group.procedures);
    const scope = yield* Scope.make();
    // oxlint-disable-next-line typescript/no-unsafe-type-assertion -- the checker typed each handler by its procedure
    const handled = yield* Effect.provideService(rpcs.toHandlers(own as never), Scope.Scope, scope);
    const app = yield* (transport === "http" ? RpcServer.toHttpEffect(rpcs) : RpcServer.toHttpEffectWebsocket(rpcs)).pipe(
      Effect.provideContext(handled),
      Effect.provide(options.serialization),
      Effect.provideService(Scope.Scope, scope),
    );
    const router = yield* HttpRouter.make;
    const path: `/${string}` = isRoutePath(options.path) ? options.path : `/${options.path}`;
    yield* router.add(transport === "http" ? "POST" : "GET", path, app);
    const made = yield* NodeHttpServer.make(() => nodeHttp().createServer(), { host: options.host, port: options.port }).pipe(
      Effect.provideService(Scope.Scope, scope),
      Effect.mapError((error) => serveError(error.message)),
      Effect.onError((cause) => Scope.close(scope, Exit.failCause(cause))),
    );
    yield* made.serve(router.asHttpEffect()).pipe(Effect.provideService(Scope.Scope, scope));
    const port = made.address._tag === "UnixPathAddress" ? 0 : made.address.port;
    return { port, scope } satisfies Server;
  });

// oxlint-disable-next-line typescript/no-unsafe-type-assertion -- the checker typed the handler's result as a stream
const toStream = (value: unknown): Stream.Stream<unknown, unknown> => value as Stream.Stream<unknown, unknown>;

/** `Rpc.listen options group handlers`, owned by the innermost scope. */
export const rpcListen = (
  options: ServeOptions,
  group: Group,
  handlers: Readonly<Record<string, Handler>>,
): Effect.Effect<Server, RpcError, unknown> =>
  acquireRelease(start("http", options, group, handlers), (server) => Scope.close(server.scope, Exit.void));

/** `Rpc.listen_websocket options group handlers`: as `listen`, each connection a WebSocket at the path. */
export const rpcListenWebsocket = (
  options: ServeOptions,
  group: Group,
  handlers: Readonly<Record<string, Handler>>,
): Effect.Effect<Server, RpcError, unknown> =>
  acquireRelease(start("websocket", options, group, handlers), (server) => Scope.close(server.scope, Exit.void));

export const rpcPort = (server: Server): number => server.port;

