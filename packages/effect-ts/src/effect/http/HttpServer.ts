// Effect's `HttpServer` on Node.js for the bindings of the standard
// library. `flickServe` runs in the owner of the Flick scope it belongs to:
// Closing first stops network admission and drains requests up to the grace
// deadline, then closes the transport scope and joins all request cleanup.
import { createServer } from "node:http";
import type { Duplex } from "node:stream";

import * as NodeHttpServer from "@effect/platform-node/NodeHttpServer";
import { Context, Duration, Effect, Exit, Option, Scope } from "effect";
import * as HttpServer from "effect/http/HttpServer";
import * as HttpServerResponse from "effect/http/HttpServerResponse";
import * as NetAddress from "effect/net/NetAddress";

import { tag } from "@flick-lang/runtime/tags";
import { CurrentOwner, Owner, closeOwn } from "../../owner.ts";
import { type Handler, toApp } from "./handler.ts";
import { serveError } from "./HttpServerError.ts";
import { respondTo } from "./HttpServerResponse.ts";

export * from "effect/http/HttpServer";

/** `Http.Server.ServeOptions`. */
export interface ServeOptions {
  readonly host: string;
  readonly port: number;
  readonly gracefulShutdownTimeout: Option.Option<Duration.Duration>;
}

type Server = HttpServer.HttpServer["Service"];

/** The app of a handler, its Flick errors of `Http` answered with their status. */
const answered = <E, R>(handler: Handler<E, R>) =>
  Effect.catch(toApp(handler), (e) => {
    const response = respondTo(e);
    return response === undefined ? Effect.fail(e) : Effect.succeed(response);
  });

/** Keep Flick's request owner live until the response body has been sent. */
const requestApp = <E, R>(app: Effect.Effect<HttpServerResponse.HttpServerResponse, E, R>, admit: () => boolean) => Effect.gen(function* () {
  if (!admit()) return HttpServerResponse.empty({ status: 503 });
  const parent = yield* Effect.service(CurrentOwner);
  const owner = new Owner(yield* Scope.make(), parent?.defects);
  yield* Effect.addFinalizer((exit) => Effect.orDie(Effect.flatMap(closeOwn(owner, exit), (closed) => closed)));
  // HttpEffect's request frame restores its context after sending the body.
  // A provideService wrapper would restore it too early, when app returns.
  yield* Effect.withFiber((fiber) => Effect.sync(() => {
    fiber.setContext(Context.add(Context.add(fiber.context, CurrentOwner, owner), Scope.Scope, owner.resources));
  }));
  return yield* app;
});

/** `Http.Server.serve options handler`. */
export const flickServe = <E, R>(options: ServeOptions, handler: Handler<E, R>) =>
  Effect.gen(function* () {
    const lifetime = yield* Scope.make();
    yield* Effect.addFinalizer((exit) => Scope.close(lifetime, exit));
    return yield* Effect.mapError(ownedServer(lifetime, options, answered(handler)), serveError);
  });

/** Shared HTTP ownership for Http.Server, HttpApi and RPC's HTTP transports. */
export const ownedServer = <E, R>(
  lifetime: Scope.Closeable,
  options: ServeOptions,
  app: Effect.Effect<HttpServerResponse.HttpServerResponse, E, R>,
) => Effect.gen(function* () {
  const resources = yield* Scope.make();
  const transport = nodeTransport(options);
  let exit: Exit.Exit<unknown, unknown> = Exit.void;
  let closing = false;
  const stopped = yield* Effect.cached(Effect.ensuring(
    transport.shutdown,
    Effect.suspend(() => Scope.close(resources, exit)),
  ));
  const stop = (reason: Exit.Exit<unknown, unknown>) => Effect.suspend(() => {
    if (!closing) exit = reason;
    closing = true;
    return stopped;
  });
  yield* Scope.addFinalizerExit(lifetime, stop);
  if (closing) return yield* Effect.interrupt;
  const server = yield* Effect.provideService(Effect.gen(function* () {
    const http: Server = yield* NodeHttpServer.make(() => transport.node, {
      host: options.host, port: options.port, disablePreemptiveShutdown: true,
    });
    yield* http.serve(requestApp(app, transport.admit));
    return http;
  }), Scope.Scope, resources).pipe(Effect.onError((cause) => stop(Exit.failCause(cause))));
  const owner = yield* Effect.service(CurrentOwner);
  if (owner !== undefined) yield* Effect.orDie(owner.onClose(() => stop(Exit.void)));
  return server;
});

/** Stop network admission and drain before closing any lazy upgrade finalizers. */
const nodeTransport = (options: ServeOptions) => {
  const node = createServer();
  const upgraded = new Set<Duplex>();
  let closing = false;
  node.on("upgrade", (_request, socket) => {
    upgraded.add(socket);
    socket.once("close", () => upgraded.delete(socket));
  });
  node.prependListener("request", (_request, response) => response.once("finish", () => {
    if (closing) setImmediate(() => node.closeIdleConnections());
  }));
  const shutdown = Effect.ensuring(Effect.suspend(() => {
    closing = true;
    if (!node.listening) return Effect.void;
    // Start close even for a zero timeout; the promise never rejects after a timeout.
    const drained = new Promise<Error | undefined>((resolve) => node.close((error) => resolve(error)));
    const wait = Effect.flatMap(Effect.promise(() => drained), (error) => error === undefined ? Effect.void : Effect.die(error));
    return Effect.timeoutOrElse(Effect.interruptible(wait), {
      duration: Option.getOrElse(options.gracefulShutdownTimeout, () => Duration.seconds(20)),
      orElse: () => Effect.void,
    });
  }), Effect.sync(() => {
    node.closeAllConnections();
    for (const socket of upgraded) socket.destroy();
  }));
  return { node, shutdown, admit: () => !closing };
};

const TCP = /*#__PURE__*/ tag("Http.Server.Address.TcpAddress");
const UNIX = /*#__PURE__*/ tag("Http.Server.Address.UnixAddress");

/** `Http.Server.address server`. */
export const flickAddress = (server: Server) => {
  const a = server.address;
  return a._tag === "UnixPathAddress"
    ? { _tag: UNIX, _0: a.path }
    : { _tag: TCP, _0: NetAddress.formatIp(a.address), _1: a.port };
};

/** `Http.Server.log_address server`. */
export const flickLogAddress = (server: Server) =>
  Effect.provideService(HttpServer.logAddress, HttpServer.HttpServer, server);
