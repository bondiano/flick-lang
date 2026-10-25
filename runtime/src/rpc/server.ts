// `Rpc.listen`: a group served on Node's HTTP server for as long as the
// Flick scope that started it. A separate entry point from the client, so a
// program that only calls procedures loads no Node server.
import { spawn } from "node:child_process";
import { Duplex } from "node:stream";

import * as NodeHttpServer from "@effect/platform-node/NodeHttpServer";
import * as NodeSocket from "@effect/platform-node/NodeSocket";
import * as NodeSocketServer from "@effect/platform-node/NodeSocketServer";
import { Effect, Exit, Fiber, Layer, Scope, Stdio, Stream } from "effect";
import { RpcSerialization } from "effect/rpc/RpcSerialization";
import { type Headers, HttpRouter } from "effect/http";
import * as RpcClient from "effect/rpc/RpcClient";
import * as RpcServer from "effect/rpc/RpcServer";

import { nodeHttp } from "../host.ts";
import { service } from "../effects.ts";
import { nativeStdio } from "../effect/Stdio.ts";
import { acquireRelease } from "../owner.ts";
import { Socket, SocketServer } from "effect/socket";

import type { Client } from "./client.ts";
import { clientError, effectGroup, type Group, type RpcError, type Serialization, type Server, type ServeOptions, serveError, type Transport } from "./model.ts";

type Handler = (payload: unknown) => Effect.Effect<unknown, unknown, unknown>;

const isRoutePath = (path: string): path is `/${string}` => path.startsWith("/");

/**
 * Effect's group of `group` with `handlers`, each running with the services
 * of the caller, in a scope of its own the server closes.
 */
const prepare = (group: Group, handlers: Readonly<Record<string, Handler>>) =>
  Effect.gen(function* () {
    const context = yield* Effect.context<unknown>();
    const own: Record<string, (payload: unknown, call: { readonly headers: Headers.Headers }) => unknown> = {};
    for (const procedure of group.procedures) {
      const handler = handlers[procedure.name];
      if (handler === undefined) return yield* Effect.die(new TypeError(`no handler for ${procedure.name}`));
      // the hooks of the procedure, then the handler, with the services of `listen`
      const handle = (payload: unknown, call: { readonly headers: Headers.Headers }) =>
        Effect.provideContext(
          Effect.andThen(
            Effect.forEach(procedure.middleware, (hook) => toEffect(hook(Object.freeze({ payload, headers: call.headers }))), {
              discard: true,
            }),
            handler(payload),
          ),
          context,
        );
      // a stream's handler gives the stream as its result
      own[procedure.name] = procedure.stream
        ? (payload: unknown, call: { readonly headers: Headers.Headers }) => Stream.unwrap(Effect.map(handle(payload, call), toStream))
        : handle;
    }
    const rpcs = effectGroup(group.procedures);
    const scope = yield* Scope.make();
    // oxlint-disable-next-line typescript/no-unsafe-type-assertion -- the checker typed each handler by its procedure
    const handled = yield* Effect.provideService(rpcs.toHandlers(own as never), Scope.Scope, scope);
    return { rpcs, scope, handled };
  });

/** Whether `serialization` frames its messages, which a byte stream such as TCP needs. */
const framed = (serialization: Serialization): boolean =>
  Effect.runSync(Effect.map(Effect.provide(Effect.service(RpcSerialization), serialization), (s) => s.includesFraming));

/** How the server takes calls: a POST per call, or the messages of a WebSocket. */
type Served = "http" | "websocket";

const start = (transport: Served, options: ServeOptions, group: Group, handlers: Readonly<Record<string, Handler>>) =>
  Effect.gen(function* () {
    const { rpcs, scope, handled } = yield* prepare(group, handlers);
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

// oxlint-disable-next-line typescript/no-unsafe-type-assertion -- a hook is a runtime function of the checker's typing
const toEffect = (value: unknown): Effect.Effect<unknown, unknown, unknown> => value as Effect.Effect<unknown, unknown, unknown>;

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


/** The TCP server of `listen_tcp`: messages framed on each connection. */
const startTcp = (options: ServeOptions, group: Group, handlers: Readonly<Record<string, Handler>>) =>
  Effect.gen(function* () {
    if (!framed(options.serialization)) {
      return yield* Effect.fail(serveError("TCP needs a serialization that frames its messages, such as `Rpc.ndjson`"));
    }
    const { rpcs, scope, handled } = yield* prepare(group, handlers);
    const sockets = yield* NodeSocketServer.make({ host: options.host, port: options.port }).pipe(
      Effect.provideService(Scope.Scope, scope),
      Effect.mapError((error) => serveError(error.message)),
      Effect.onError((cause) => Scope.close(scope, Exit.failCause(cause))),
    );
    const protocol = yield* Layer.build(RpcServer.layerProtocolSocketServer).pipe(
      Effect.provideService(SocketServer.SocketServer, sockets),
      Effect.provide(options.serialization),
      Effect.provideService(Scope.Scope, scope),
    );
    yield* RpcServer.make(rpcs).pipe(
      Effect.provideContext(handled),
      Effect.provideContext(protocol),
      Effect.forkIn(scope),
    );
    const port = sockets.address._tag === "UnixPathAddress" ? 0 : sockets.address.port;
    return { port, scope } satisfies Server;
  });

/** `Rpc.listen_tcp options group handlers`: as `listen`, each client a TCP connection. */
export const rpcListenTcp = (
  options: ServeOptions,
  group: Group,
  handlers: Readonly<Record<string, Handler>>,
): Effect.Effect<Server, RpcError, unknown> =>
  acquireRelease(startTcp(options, group, handlers), (server) => Scope.close(server.scope, Exit.void));

/** `Rpc.tcp`: the messages of calls over a TCP connection to `tcp://host:port`, opened on the first call. */
export const rpcTcp: Transport = (url, serialization) => {
  const { hostname, port } = new URL(url);
  const socket = Layer.effect(Socket.Socket, NodeSocket.makeNet({ host: hostname, port: Number(port) }));
  return RpcClient.layerProtocolSocket().pipe(Layer.provide([socket, serialization]));
};

const FlickStdio = /*#__PURE__*/ service("Stdio.Stdio");

/**
 * `Rpc.serve_stdio serialization group handlers`: the calls of the messages
 * on stdin, answered on stdout, until stdin ends.
 */
export const rpcServeStdio = (serialization: Serialization, group: Group, handlers: Readonly<Record<string, Handler>>) =>
  Effect.gen(function* () {
    const stdio = nativeStdio(yield* Effect.service(FlickStdio));
    if (stdio === undefined) return yield* Effect.die(new Error("`Rpc.serve_stdio` needs the `Stdio` of `Stdio.node`"));
    const { rpcs, scope, handled } = yield* prepare(group, handlers);
    // the protocol interrupts the fiber that built it once stdin ends and stdout is written
    const serving = Effect.scoped(
      Effect.flatMap(Layer.build(RpcServer.layerProtocolStdio), (protocol) =>
        RpcServer.make(rpcs).pipe(Effect.provideContext(handled), Effect.provideContext(protocol)),
      ),
    ).pipe(Effect.provideService(Stdio.Stdio, stdio), Effect.provide(serialization));
    yield* Fiber.await(yield* Effect.forkChild(serving));
    return yield* Scope.close(scope, Exit.void);
  });

/**
 * `Rpc.spawn serialization command args`: a connection to a child process
 * whose stdin and stdout carry the messages; the scope's end kills it.
 */
export const rpcSpawn = (serialization: Serialization, command: string, args: ReadonlyArray<string>) =>
  Effect.gen(function* () {
    const pipe = yield* Effect.acquireRelease(
      Effect.try({
        try: () => {
          const child = spawn(command, [...args], { stdio: ["pipe", "pipe", "inherit"] });
          const duplex = Duplex.from({ readable: child.stdout, writable: child.stdin });
          // closing the pipes below is the end the scope asks for, not a failure
          duplex.on("error", () => undefined);
          return { child, duplex };
        },
        catch: (error) => clientError(error instanceof Error ? error.message : `cannot start ${command}`),
      }),
      // the pipes too, or they keep this process alive
      ({ child, duplex }) =>
        Effect.sync(() => {
          duplex.destroy();
          child.kill();
        }),
    );
    const socket = yield* NodeSocket.fromDuplex(Effect.succeed(pipe.duplex));
    const context = yield* Layer.build(
      RpcClient.layerProtocolSocket().pipe(Layer.provide([Layer.succeed(Socket.Socket, socket), serialization])),
    );
    return { context } satisfies Client;
  });
