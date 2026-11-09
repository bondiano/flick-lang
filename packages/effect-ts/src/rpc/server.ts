// `Rpc.listen`: a group served on Node's HTTP server for as long as the
// Flick scope that started it. A separate entry point from the client, so a
// program that only calls procedures loads no Node server.
import { type ChildProcess, spawn } from "node:child_process";
import { Duplex } from "node:stream";
import { Worker } from "node:worker_threads";

import * as NodeHttpServer from "@effect/platform-node/NodeHttpServer";
import * as NodeSocket from "@effect/platform-node/NodeSocket";
import * as NodeSocketServer from "@effect/platform-node/NodeSocketServer";
import * as NodeWorker from "@effect/platform-node/NodeWorker";
import * as NodeWorkerRunner from "@effect/platform-node/NodeWorkerRunner";
import { Effect, Exit, Layer, Scope, Stdio } from "effect";
import { RpcSerialization } from "effect/rpc/RpcSerialization";
import { HttpRouter } from "effect/http";
import * as RpcClient from "effect/rpc/RpcClient";
import * as RpcServer from "effect/rpc/RpcServer";

import { nodeHttp } from "../host.ts";
import { service } from "../effects.ts";
import { nativeStdio } from "../effect/Stdio.ts";
import { acquireRelease } from "../owner.ts";
import { Socket, SocketServer } from "effect/socket";

import type { Client } from "./client.ts";
import { type Served, appOf } from "./handler.ts";
import { type Handler, prepare, serveOn } from "./handlers.ts";
import { clientError, type Group, type RpcError, type Serialization, type Server, type ServeOptions, serveError, type Transport } from "./model.ts";

const isRoutePath = (path: string): path is `/${string}` => path.startsWith("/");

/** Whether `serialization` frames its messages, which a byte stream such as TCP needs. */
const framed = (serialization: Serialization): boolean =>
  Effect.runSync(Effect.map(Effect.provide(Effect.service(RpcSerialization), serialization), (s) => s.includesFraming));

const start = (transport: Served, options: ServeOptions, group: Group, handlers: Readonly<Record<string, Handler>>) =>
  Effect.gen(function* () {
    const { app, scope } = yield* appOf(transport, options.serialization, group, handlers);
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

export { rpcHandler, rpcHandlerWebsocket } from "./handler.ts";

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
    // the protocol interrupts the fiber that built it once stdin ends and stdout is written
    const layer = RpcServer.layerProtocolStdio.pipe(Layer.provide([Layer.succeed(Stdio.Stdio, stdio), serialization]));
    return yield* serveOn(layer, group, handlers);
  });

/**
 * `Rpc.serve_node_worker group handlers`: the calls of the parent thread,
 * answered until it closes this worker.
 */
export const rpcServeNodeWorker = (group: Group, handlers: Readonly<Record<string, Handler>>) =>
  serveOn(RpcServer.layerProtocolWorkerRunner.pipe(Layer.provide(NodeWorkerRunner.layer), Layer.orDie), group, handlers);

/**
 * `Rpc.node_worker path args`: a connection to a worker thread running the
 * script at `path` with `args`, whose `serve_node_worker` answers; the
 * scope's end closes it.
 */
export const rpcNodeWorker = (path: string, args: ReadonlyArray<string>) =>
  Effect.map(
    Layer.build(
      // shortcut: one worker per client; a pool size becomes a parameter when a program needs one
      RpcClient.layerProtocolWorker({ size: 1 }).pipe(Layer.provide(NodeWorker.layer(() => new Worker(path, { argv: [...args] })))),
    ),
    (context): Client => ({ context }),
  ).pipe(Effect.mapError((error) => clientError(`cannot start ${path}: ${error.message}`)));

/**
 * The child of `command`, once it started: a failure to start, such as a
 * missing executable, comes as an `error` event after `spawn` returns, and
 * is the call's `ClientError` with the child's pipes closed.
 */
const started = (command: string, args: ReadonlyArray<string>) =>
  Effect.callback<{ child: ChildProcess; duplex: Duplex }, RpcError>((resume) => {
    let child: ChildProcess;
    try {
      child = spawn(command, [...args], { stdio: ["pipe", "pipe", "inherit"] });
    } catch (error) {
      resume(Effect.fail(clientError(error instanceof Error ? error.message : `cannot start ${command}`)));
      return Effect.void;
    }
    const failed = (error: Error): void => {
      child.off("spawn", running);
      child.stdin?.destroy();
      child.stdout?.destroy();
      resume(Effect.fail(clientError(`cannot start ${command}: ${error.message}`)));
    };
    const running = (): void => {
      child.off("error", failed);
      const duplex = Duplex.from({ readable: child.stdout, writable: child.stdin });
      // closing the pipes below is the end the scope asks for, not a failure
      duplex.on("error", () => undefined);
      // a later error of the child, such as a failed kill, ends the connection
      child.on("error", () => duplex.destroy());
      resume(Effect.succeed({ child, duplex }));
    };
    child.once("error", failed);
    child.once("spawn", running);
    // interrupted while starting: the child goes with its pipes
    return Effect.sync(() => {
      child.off("error", failed);
      child.off("spawn", running);
      child.on("error", () => undefined);
      child.stdin?.destroy();
      child.stdout?.destroy();
      child.kill();
    });
  });

/**
 * `Rpc.spawn serialization command args`: a connection to a child process
 * whose stdin and stdout carry the messages; the scope's end kills it.
 */
export const rpcSpawn = (serialization: Serialization, command: string, args: ReadonlyArray<string>) =>
  Effect.gen(function* () {
    const pipe = yield* Effect.acquireRelease(started(command, args), ({ child, duplex }) =>
      // the pipes too, or they keep this process alive
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
