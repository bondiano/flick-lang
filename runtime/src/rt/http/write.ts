// Writing a `ServerResponse` to Node's `http.ServerResponse`, as Effect's
// `NodeHttpServer` writes one: a `HEAD` request and a status without a body
// (204, 205, 304) send only the head, 204 and 205 without a length; a body
// is sent with a strict content length; a status no final response may
// have, or headers Node refuses, are answered with an empty 500; a failure
// after the head is out aborts the response, since ending it would make a
// partial body look whole.
import type * as Http from "node:http";
import type { Readable } from "node:stream";

import { toSetCookieHeaders } from "../effect/http/Cookies.ts";
import { type ServerResponse, emptyResponse, omitsBody } from "./model.ts";

type OutHeaders = Record<string, string | Array<string>>;

const node = (): NodeJS.Process => {
  const host = typeof process === "undefined" ? undefined : process;
  if (host?.getBuiltinModule === undefined) throw new Error("an HTTP server runs only on Node.js");
  return host;
};

/** The built-in modules of Node.js, which only a server on Node needs. */
export const nodeHttp = () => node().getBuiltinModule("node:http");
export const nodeStream = () => node().getBuiltinModule("node:stream");
const nodeStreamWeb = () => node().getBuiltinModule("node:stream/web");
const nodePipeline = () => node().getBuiltinModule("node:stream/promises");

/** Requests whose `Expect: 100-continue` the server has not answered yet. */
export const pendingContinue = /*#__PURE__*/ new WeakSet<Http.IncomingMessage>();

/** Lets the client send the body it holds back for `100 Continue`. */
export const writeContinue = (request: Http.IncomingMessage, response: Http.ServerResponse): void => {
  if (pendingContinue.delete(request) && !response.headersSent) response.writeContinue();
};

// unread body discarded after the response to keep the connection; past this it closes
const maxDrainBytes = 256 * 1024;
// time the client gets to read the response after the FIN, before a destroy can reset the connection
const closeGracePeriod = 500;

/** Whether the head says `Connection: close`: a body still to come that is too long to drain. */
const closesConnection = (request: Http.IncomingMessage, streaming: boolean): boolean => {
  if (request.httpVersionMajor !== 1 || request.complete) return false;
  if (pendingContinue.has(request)) return true;
  const contentLength = request.headers["content-length"];
  return !streaming && contentLength !== undefined && Number(contentLength) > maxDrainBytes;
};

const closeSocket = (socket: Http.IncomingMessage["socket"]): void => {
  if (socket.destroyed) return;
  socket.end();
  const timer = setTimeout(() => socket.destroy(), closeGracePeriod);
  socket.once("close", () => clearTimeout(timer));
};

/**
 * After the response: drain an unread body to keep the connection, up to a
 * limit, or close it gently when it ends with this response. Node would
 * drain a whole body, or reset a closing connection with body bytes queued.
 */
export const teardownBody = (request: Http.IncomingMessage, response: Http.ServerResponse): void => {
  if (request.complete || request.destroyed) return;
  const socket = request.socket;
  // Node's internals, which its own `finish` listener reads right after this one
  if (Reflect.get(response, "_last") === true) {
    // skips Node's own dump while the parser keeps applying backpressure
    Reflect.set(request, "_consuming", true);
    Reflect.set(response, "_last", false);
    closeSocket(socket);
    return;
  }
  let bytes = 0;
  request.on("data", (chunk: Uint8Array) => {
    bytes += chunk.length;
    if (bytes > maxDrainBytes) {
      request.pause();
      closeSocket(socket);
    }
  });
  request.resume();
};

const isFinalStatus = (status: number): boolean => status >= 200 && status <= 599;

const writeHead = (request: Http.IncomingMessage, out: Http.ServerResponse, response: ServerResponse, headers: OutHeaders, streaming = false): void => {
  if (!isFinalStatus(response.status)) throw new RangeError(`Invalid final response status: ${response.status}`);
  out.writeHead(response.status, response.statusText, closesConnection(request, streaming) ? { ...headers, connection: "close" } : headers);
};

const cancelBody = (response: ServerResponse): void => {
  const body = response.body;
  if (body._tag === "Raw" && isReadable(body.body)) body.body.destroy();
};

const isReadable = (value: unknown): value is Readable =>
  typeof value === "object" && value !== null && typeof Reflect.get(value, "pipe") === "function" && typeof Reflect.get(value, "destroy") === "function";

/** Ends the response; `finish` or a disconnect settles the promise. */
const endAndAwait = (out: Http.ServerResponse, chunk?: unknown): Promise<void> | undefined => {
  if (out.destroyed) return undefined;
  return new Promise((resolve) => {
    let completed = false;
    const done = (): void => {
      if (completed) return;
      completed = true;
      out.off("close", done);
      resolve();
    };
    out.once("close", done);
    if (chunk === undefined) out.end(done);
    else out.end(chunk, done);
  });
};

/** What a write that could not finish leaves: an empty 500, or an aborted response once its head is out. */
const failed = (request: Http.IncomingMessage, out: Http.ServerResponse): void => {
  let abort = out.headersSent;
  if (!abort) {
    try {
      writeHead(request, out, emptyResponse({ status: 500 }), {});
    } catch {
      abort = true;
    }
  }
  if (out.writableEnded) return;
  if (abort) out.destroy();
  else out.end();
};

const end = (request: Http.IncomingMessage, out: Http.ServerResponse, chunk?: string | Uint8Array): undefined => {
  try {
    out.end(chunk);
  } catch {
    failed(request, out);
  }
  return undefined;
};

/** Pipes the body, failing when its length is not the one the head gave. */
const pipeTo = (readable: Readable, out: Http.ServerResponse, headers: OutHeaders, signal: AbortSignal): Promise<void> => {
  const { pipeline } = nodePipeline();
  const { Transform } = nodeStream();
  const contentLength = headers["content-length"];
  if (typeof contentLength !== "string") return pipeline(readable, out, { signal });
  let remaining = Number(contentLength);
  const mismatch = (): Error => new Error(`Response body doesn't match content-length of ${contentLength}`);
  const counted = new Transform({
    transform(chunk: Uint8Array, _encoding, callback) {
      remaining -= chunk.length;
      callback(remaining < 0 ? mismatch() : null, chunk);
    },
    flush(callback) {
      callback(remaining > 0 ? mismatch() : null);
    },
  });
  return pipeline(readable, counted, out, { signal });
};

const piped = (request: Http.IncomingMessage, out: Http.ServerResponse, sending: Promise<void>): Promise<void> =>
  sending.catch(() => failed(request, out));

/**
 * Sends `response` to the request: `undefined` when it is done, or a
 * promise of the body still going out, which `signal` stops.
 */
export const writeResponse = (
  request: Http.IncomingMessage,
  out: Http.ServerResponse,
  method: string,
  response: ServerResponse,
  signal: AbortSignal,
): Promise<void> | undefined => {
  if (out.writableEnded) return undefined;
  let headers: OutHeaders = response.headers;
  if (response.status === 204 || response.status === 205) {
    headers = Object.fromEntries(Object.entries(headers).filter(([name]) => name !== "content-length"));
  }
  const cookies = toSetCookieHeaders(response.cookies);
  if (cookies.length > 0) {
    const set = headers["set-cookie"];
    headers = { ...headers, "set-cookie": set === undefined ? [...cookies] : [...cookies, String(set)] };
  }
  try {
    return send(request, out, method, response, headers, signal);
  } catch {
    cancelBody(response);
    failed(request, out);
    return undefined;
  }
};

const send = (
  request: Http.IncomingMessage,
  out: Http.ServerResponse,
  method: string,
  response: ServerResponse,
  headers: OutHeaders,
  signal: AbortSignal,
): Promise<void> | undefined => {
  // nothing reaches a client that went away, and writing would throw or never finish
  if (out.destroyed) {
    cancelBody(response);
    return undefined;
  }
  if (method === "HEAD") {
    writeHead(request, out, response, headers);
    cancelBody(response);
    return endAndAwait(out);
  }
  if (omitsBody(response)) {
    writeHead(request, out, response, headers);
    out.end();
    cancelBody(response);
    return undefined;
  }
  out.strictContentLength = true;
  const body = response.body;
  switch (body._tag) {
    case "Empty":
      writeHead(request, out, response, headers);
      return end(request, out);
    case "Uint8Array":
      writeHead(request, out, response, headers);
      // a small body goes with the head; a large one is awaited
      if (body.contentLength < 1024 * 1024) return end(request, out, body.text ?? body.body);
      return endAndAwait(out, body.text ?? body.body)?.catch(() => failed(request, out));
    case "Raw": {
      const readable = isReadable(body.body) ? body.body : undefined;
      writeHead(request, out, response, headers, readable !== undefined);
      if (readable === undefined) return endAndAwait(out, body.body);
      return piped(request, out, pipeTo(readable, out, headers, signal));
    }
    case "FormData": {
      const web = new Response(body.formData);
      const formHeaders = { ...headers, ...Object.fromEntries(web.headers) };
      writeHead(request, out, response, formHeaders);
      const stream: unknown = web.body;
      if (!(stream instanceof nodeStreamWeb().ReadableStream)) return end(request, out);
      return piped(request, out, pipeTo(nodeStream().Readable.fromWeb(stream), out, formHeaders, signal));
    }
  }
  return undefined;
};
