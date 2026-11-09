// A DevTools server on Node for tests and tools: it accepts the WebSocket
// connections of `DevTools.layer` clients and keeps the names of the spans
// and span events they send. A separate entry point, so programs that only send spans do
// not load Node's sockets.
import * as NodeSocketServer from "@effect/platform-node/NodeSocketServer";
import { Effect, Exit, Fiber, Queue, Scope } from "effect";
import * as DevToolsServer from "effect/devtools/DevToolsServer";
import { SocketServer } from "effect/socket";

/** A running DevTools server: its port, the span and event names it got, and how to stop it. */
export interface Listening {
  readonly port: number;
  readonly spans: ReadonlyArray<string>;
  readonly events: ReadonlyArray<string>;
  readonly close: () => Promise<void>;
}

/** Starts a DevTools server on a free local port. */
export const devToolsListen = async (): Promise<Listening> => {
  const spans: Array<string> = [];
  const events: Array<string> = [];
  const scope = Effect.runSync(Scope.make());
  const server = await Effect.runPromise(
    Effect.provideService(NodeSocketServer.makeWebSocket({ host: "127.0.0.1", port: 0 }), Scope.Scope, scope),
  );
  const serving = Effect.runFork(
    Effect.provideService(
      DevToolsServer.run((client) =>
        Effect.forever(
          Effect.map(Queue.take(client.queue), (request) => {
            if (request._tag === "Span" && !spans.includes(request.name)) spans.push(request.name);
            if (request._tag === "SpanEvent") events.push(request.name);
          }),
        ),
      ),
      SocketServer.SocketServer,
      server,
    ),
  );
  const port = server.address._tag === "UnixPathAddress" ? 0 : server.address.port;
  const close = () => Effect.runPromise(Effect.andThen(Fiber.interrupt(serving), Scope.close(scope, Exit.void)));
  return { port, spans, events, close };
};
