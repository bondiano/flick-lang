// An HTTP server on Node.js, owned as the adapter owns one: `ownedServer`
// listens for the owner of the caller's scope. Each request runs in a fiber
// of its own, with the evidence of the call and an owner of the request that
// closes once the response is sent. Closing the scope's owner first stops
// admission (a new request gets 503) and waits for the requests that are
// running, at most the grace period on the fiber's clock; then it closes the
// connections, interrupts what is left and waits for its cleanup, all in the
// owner's closing phase, before its finalizers release anything.
import type * as Http from "node:http";

import { type Option, none, some } from "../../core/data.ts";
import { type Exit, Raise, fail } from "../cause.ts";
import { type Clock, clock } from "../clock.ts";
import { fromInput } from "../effect/http/Headers.ts";
import {
  type Fiber,
  type Run,
  Suspend,
  Under,
  asExit,
  atMask,
  awaitFiber,
  currentCtx,
  currentFiber,
  exitOf,
  fork,
  owned,
} from "../fiber.ts";
import { Owner, closeOwn } from "../owner.ts";
import { CLIENT_ABORT, type RequestSource, type ServerRequest, ServerResponse, causeResponse, emptyResponse, serveError } from "./model.ts";
import { nodeHttp, nodeStream, pendingContinue, teardownBody, writeContinue, writeResponse } from "./write.ts";

/** Where a server listens, and how long closing waits for running requests. */
export interface ServeOptions {
  readonly host: string;
  readonly port: number;
  readonly graceMillis: number;
}

/** A listening server: where it listens. */
export interface Server {
  readonly host: string;
  readonly port: number;
}

/** What answers a request: a response, or a call that gives one. */
export type App = (request: ServerRequest) => unknown;

const concat = (chunks: ReadonlyArray<Uint8Array>, length: number): Uint8Array => {
  if (chunks.length === 1 && chunks[0] !== undefined) return chunks[0];
  const out = new Uint8Array(length);
  let at = 0;
  for (const chunk of chunks) {
    out.set(chunk, at);
    at += chunk.length;
  }
  return out;
};

/** Reads the body once; past the limit it drains the rest, so the response can still go out. */
const readAll = (request: Http.IncomingMessage, response: Http.ServerResponse, maxBytes: number | undefined, signal: AbortSignal): Promise<Uint8Array> =>
  new Promise((resolve, reject) => {
    writeContinue(request, response);
    const { finished } = nodeStream();
    const chunks: Array<Uint8Array> = [];
    let bytes = 0;
    const cleanup = finished(request, { readable: true, writable: false }, (error) => {
      request.off("data", onData);
      signal.removeEventListener("abort", onAbort);
      cleanup();
      if (error) {
        request.destroy();
        reject(error);
        return;
      }
      resolve(concat(chunks, bytes));
    });
    function stop(error: unknown): void {
      request.off("data", onData);
      signal.removeEventListener("abort", onAbort);
      cleanup();
      const done = finished(request, { readable: true, writable: false }, () => done());
      request.resume();
      reject(error instanceof Error ? error : new Error(String(error)));
    }
    function onAbort(): void {
      stop(signal.reason);
    }
    function onData(chunk: Uint8Array): void {
      chunks.push(chunk);
      bytes += chunk.length;
      if (maxBytes !== undefined && bytes > maxBytes) stop(new Error("maxBytes exceeded"));
    }
    signal.addEventListener("abort", onAbort, { once: true });
    if (signal.aborted) {
      onAbort();
      return;
    }
    request.on("data", onData);
  });

const nodeSource = (request: Http.IncomingMessage, response: Http.ServerResponse): RequestSource => {
  let read: Promise<Uint8Array> | undefined;
  return Object.freeze({ read: (maxBytes: number | undefined, signal: AbortSignal) => (read ??= readAll(request, response, maxBytes, signal)) });
};

const absoluteTarget = (url: string): URL | undefined => {
  if (!/^https?:\/\//i.test(url)) return undefined;
  return URL.canParse(url) ? new URL(url) : undefined;
};

/** The request as the handler sees it; an absolute target's authority wins over `Host`. */
export const fromNode = (request: Http.IncomingMessage, response: Http.ServerResponse): ServerRequest => {
  const raw = request.url ?? "/";
  const target = absoluteTarget(raw);
  const pairs = Object.entries(request.headers).flatMap(([name, value]): Array<[string, string]> =>
    value === undefined ? [] : [[name, Array.isArray(value) ? value.join(", ") : value]]);
  const host = request.headers.host;
  const encrypted = Reflect.get(Object(request.socket), "encrypted") === true;
  const remote = request.socket.remoteAddress;
  const remoteAddress: Option<string> = remote === undefined ? none : some(remote);
  return Object.freeze({
    method: (request.method ?? "GET").toUpperCase(),
    url: target === undefined ? raw : target.pathname + target.search,
    originalUrl: raw[0] !== "/" || host === undefined ? raw : `${encrypted ? "https" : "http"}://${host}${raw}`,
    headers: target === undefined ? fromInput(pairs) : fromInput([...pairs, ["host", target.host]]),
    remoteAddress,
    source: nodeSource(request, response),
  });
};

/** Sends the response; ends when it is out, the client went away, or the fiber is interrupted while it streams. */
function* send(request: Http.IncomingMessage, out: Http.ServerResponse, method: string, response: ServerResponse): Run {
  const controller = new AbortController();
  const sending = writeResponse(request, out, method, response, controller.signal);
  if (sending === undefined) return undefined;
  return yield atMask(0, () =>
    new Suspend((resolve) => {
      void sending.then(resolve);
      return () => controller.abort();
    }));
}

/** Answers the request with `app` in an owner of its own, and closes that owner once the response is out. */
function* respond(app: App, request: ServerRequest, req: Http.IncomingMessage, out: Http.ServerResponse, defects: Owner["defects"]): Run {
  const owner = new Owner(defects);
  const exit: Exit = asExit(yield exitOf(() => new Under(owned(currentCtx(), owner), () => atMask(0, () => app(request)))));
  const answer =
    exit.cause !== undefined
      ? causeResponse(exit.cause)
      : exit.value instanceof ServerResponse
        ? exit.value
        : emptyResponse({ status: 500 });
  // an interruption stops a body still going out, and the owner closes all the same
  yield exitOf(() => new Under(owned(currentCtx(), owner), () => send(req, out, request.method, answer)));
  // a failure of the request's cleanup has no one to go to, as with the adapter
  yield closeOwn(owner, exit);
  return undefined;
}

/** Waits for `close` to end every connection, at most `millis` on the fiber's clock. */
const drained = (node: Http.Server, millis: number, servingClock: Clock): Suspend =>
  new Suspend((resolve, reject) => {
    let stop: (() => void) | undefined;
    node.close((error) => {
      stop?.();
      if (error === undefined) resolve(undefined);
      else reject(error);
    });
    if (millis <= 0) resolve(undefined);
    else if (Number.isFinite(millis)) stop = servingClock.timer(millis, () => resolve(undefined));
    return () => stop?.();
  });

/**
 * Listens for `app` until the owner of the call closes. A failure to listen
 * is a typed `ServeError`.
 */
export function* ownedServer(options: ServeOptions, app: App): Run {
  const ctx = currentCtx();
  const owner = ctx.owner;
  if (owner === undefined) throw new TypeError("a server needs the owner of a scope");
  if (!owner.open) throw new Error("the owner is closing: it admits no new servers");
  const servingClock = clock();
  const servingFiber = currentFiber();
  servingFiber.mask++;
  try {
    const { createServer } = nodeHttp();
    const node = createServer();
    const requests = new Set<Fiber>();
    let closing = false;
    node.prependListener("request", (_request, response) =>
      response.once("finish", () => {
        if (closing) setImmediate(() => node.closeIdleConnections());
      }));
    const handle = (req: Http.IncomingMessage, out: Http.ServerResponse): void => {
      if (req.httpVersionMajor === 1) out.prependListener("finish", () => teardownBody(req, out));
      const request = fromNode(req, out);
      if (closing) {
        void writeResponse(req, out, request.method, emptyResponse({ status: 503 }), new AbortController().signal);
        return;
      }
      const fiber = fork(() => respond(app, request, req, out, owner.defects), ctx, 1);
      requests.add(fiber);
      fiber.observe(() => requests.delete(fiber));
      out.on("close", () => {
        if (!out.writableEnded) fiber.interrupt(CLIENT_ABORT);
      });
    };
    node.on("request", handle);
    node.on("checkContinue", (req: Http.IncomingMessage, out: Http.ServerResponse) => {
      pendingContinue.add(req);
      handle(req, out);
    });
    yield new Suspend((resolve, reject) => {
      const onError = (error: Error): void => reject(new Raise(fail(serveError(error))));
      node.once("error", onError);
      node.listen({ host: options.host, port: options.port }, () => {
        node.off("error", onError);
        resolve(undefined);
      });
      return () => {
        node.off("error", onError);
        node.close();
      };
    });
    owner.closing.add(function* shutdown(): Run {
      if (closing) return undefined;
      closing = true;
      if (node.listening) yield drained(node, options.graceMillis, servingClock);
      node.closeAllConnections();
      const by = currentFiber().id;
      for (const fiber of requests) fiber.interrupt(by);
      for (const fiber of requests) yield awaitFiber(fiber);
      return undefined;
    });
    const address = node.address();
    return Object.freeze(
      typeof address === "object" && address !== null ? { host: address.address, port: address.port } : { host: String(address), port: 0 },
    );
  } finally {
    servingFiber.mask--;
  }
}
