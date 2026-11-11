// Effect's `HttpServer` on Node.js for the bindings of the standard
// library. `flickServe` runs in the owner of the Flick scope it belongs to:
// Effect's server adds its finalizers to that scope's resources, so closing
// the scope stops accepting connections, waits for the running requests and
// their finalizers, and interrupts what is left.
import { createServer } from "node:http";

import * as NodeHttpServer from "@effect/platform-node/NodeHttpServer";
import { type Duration, Effect, Exit, Option, Scope } from "effect";
import * as HttpServer from "effect/http/HttpServer";
import type * as HttpServerResponse from "effect/http/HttpServerResponse";
import * as NetAddress from "effect/net/NetAddress";

import { tag } from "@flick-lang/runtime/tags";
import { CurrentOwner, Owner, closeOwn, inside } from "../../owner.ts";
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
const requestApp = <E, R>(app: Effect.Effect<HttpServerResponse.HttpServerResponse, E, R>) => Effect.gen(function* () {
  const parent = yield* Effect.service(CurrentOwner);
  const owner = new Owner(yield* Scope.make(), parent?.defects);
  yield* Effect.addFinalizer((exit) => Effect.orDie(Effect.flatMap(closeOwn(owner, exit), (closed) => closed)));
  return yield* inside(owner, Effect.interruptible(app));
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
  const server = yield* Effect.provideService(makeServer(options, app), Scope.Scope, lifetime).pipe(
    Effect.onError((cause) => Scope.close(lifetime, Exit.failCause(cause))),
  );
  const owner = yield* Effect.service(CurrentOwner);
  if (owner !== undefined) yield* Effect.orDie(owner.onClose(() => Scope.close(lifetime, Exit.void)));
  return server;
});

const makeServer = <E, R>(options: ServeOptions, app: Effect.Effect<HttpServerResponse.HttpServerResponse, E, R>) =>
  Effect.gen(function* () {
    const node = createServer();
    // Runs after Effect's own finalizers: a connection still open when the
    // graceful shutdown timed out would otherwise keep the process alive.
    yield* Effect.addFinalizer(() => Effect.sync(() => node.closeAllConnections()));
    const server: Server = yield* NodeHttpServer.make(() => node, {
      host: options.host,
      port: options.port,
      gracefulShutdownTimeout: Option.getOrUndefined(options.gracefulShutdownTimeout),
    });
    yield* server.serve(requestApp(app));
    // Runs before Effect's shutdown: a connection whose request ends while
    // the server closes is closed then, not kept alive until it times out.
    let closing = false;
    node.prependListener("request", (_request, response) =>
      response.once("finish", () => {
        if (closing) setImmediate(() => node.closeIdleConnections());
      }),
    );
    yield* Effect.addFinalizer(() =>
      Effect.sync(() => {
        closing = true;
      }),
    );
    return server;
  });

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
