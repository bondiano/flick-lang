// The HTTP server of Flick's own runtime against what the adapter answers:
// each expected value was read from the same handlers served by
// `@flick-lang/effect-ts` (Effect 4.1.2 on Node), requests sent raw.
import assert from "node:assert/strict";
import { connect } from "node:net";
import { test } from "node:test";

import { type Option, none, some } from "../src/core/data.ts";
import { fail, raise } from "../src/rt/cause.ts";
import { type Run, Suspend } from "../src/rt/fiber.ts";
import { operation, service } from "../src/rt/handle.ts";
import { flickRoute, flickRouter, prefixRoute } from "../src/rt/effect/http/HttpRouter.ts";
import { flickText as requestText, flickWithMaxBodySize } from "../src/rt/effect/http/HttpServerRequest.ts";
import { flickEmpty, flickText, removeHeader, setHeader, setStatus } from "../src/rt/effect/http/HttpServerResponse.ts";
import { type ServerRequest, ServerResponse, httpServerError, serveError } from "../src/rt/http/model.ts";
import { type App, ownedServer } from "../src/rt/http/node.ts";
import { withOwner } from "../src/rt/owner.ts";
import { tag } from "../src/tags.ts";
import { root, sleep } from "./rt-support.ts";

const defaults = { status: none, statusText: none, headers: none, cookies: none, contentType: none };
const text = (body: string): ServerResponse => flickText(body, defaults);

/** Serves `app` while `use` runs, then closes the server's owner. */
const serving = async <T>(app: App, use: (url: string) => Promise<T>, graceMillis = 1000): Promise<T> => {
  const { promise: result, resolve: settle } = Promise.withResolvers<T>();
  const exit = await root(() =>
    withOwner(function* (): Run {
      const server: unknown = yield ownedServer({ host: "127.0.0.1", port: 0, graceMillis }, app, serveError);
      const url = `http://127.0.0.1:${String(Reflect.get(Object(server), "port"))}`;
      yield new Suspend((resolve) => void use(url).then((value) => {
        settle(value);
        resolve(undefined);
      }));
      return undefined;
    }));
  assert.equal(exit.cause, undefined);
  return result;
};

/** The responses to the requests, written at their delays: status line, sorted headers but the volatile ones, body. */
const exchange = (url: string, parts: ReadonlyArray<readonly [number, string]>, millis = 400): Promise<ReadonlyArray<string>> =>
  new Promise((resolve) => {
    const { hostname, port } = new URL(url);
    const socket = connect(Number(port), hostname);
    let raw = "";
    socket.on("data", (d: Buffer) => {
      raw += d.toString("latin1");
    });
    socket.on("error", () => {});
    const done = (): void => {
      socket.destroy();
      resolve(raw.split(/(?=HTTP\/1\.1 )/).filter((r) => r !== "").map((r) => {
        const [head = "", ...rest] = r.split("\r\n\r\n");
        const [line, ...headers] = head.split("\r\n");
        const kept = headers.filter((h) => !/^(date|connection|keep-alive|transfer-encoding):/i.test(h)).toSorted();
        return `${line} | ${kept.join("; ")} | ${JSON.stringify(rest.join("\r\n\r\n"))}`;
      }));
    };
    socket.on("close", done);
    for (const [delay, request] of parts) setTimeout(() => socket.write(request), delay);
    setTimeout(done, millis);
  });

const request = (method: string, path: string, body = "", close = true): string =>
  `${method} ${path} HTTP/1.1\r\nHost: x\r\n${close ? "Connection: close\r\n" : ""}` +
  `${body === "" ? "" : `Content-Length: ${Buffer.byteLength(body)}\r\nContent-Type: application/json\r\n`}\r\n${body}`;

const one = async (url: string, method: string, path: string, body = ""): Promise<string> =>
  (await exchange(url, [[0, request(method, path, body)]]))[0] ?? "";

const ROUTE_CONTEXT = service("Http.Router.RouteContext");

/** A route's handler that answers its parameters and the URL it sees. */
const params = function* (req: ServerRequest): Run {
  const read = operation(ROUTE_CONTEXT, "params", 1);
  assert.equal(typeof read, "function");
  const pairs: unknown = typeof read === "function" ? yield Reflect.apply(read, undefined, [undefined]) : undefined;
  return text(`${JSON.stringify(pairs)} ${req.url}`);
};

const GET: Option<{ _tag: string }> = some({ _tag: tag("Http.Method.Method.GET") });

const failing = (error: unknown) => (): never => raise(fail(error));

const small = (req: ServerRequest): unknown => flickWithMaxBodySize(4n, () => requestText(req));

const shown = (e: unknown): string => JSON.stringify(e, (_k, v: unknown) => (typeof v === "string" ? v.replace(/#.*$/, "") : v));

test("a request that comes while the server closes gets 503; the one running finishes", async () => {
  const { promise: begun, resolve: started } = Promise.withResolvers<void>();
  const app = flickRouter(
    { ignoreTrailingSlash: none, ignoreDuplicateSlashes: none, caseSensitive: none, maxParamLength: none },
    [
      flickRoute(GET, "/slow", function* (): Run {
        started();
        yield sleep(400);
        return text("slow");
      }),
      flickRoute(GET, "/users/:id", params),
    ],
  );
  let pending: Promise<ReadonlyArray<string>> | undefined;
  await serving(app, async (url) => {
    pending = exchange(url, [[0, request("GET", "/slow", "", false)], [150, request("GET", "/users/1")]], 1500);
    await begun;
  });
  assert.deepEqual(await pending, [
    'HTTP/1.1 200 OK | content-length: 4; content-type: text/plain | "slow"',
    'HTTP/1.1 503 Service Unavailable |  | "0\\r\\n\\r\\n"',
  ]);
});

test("a typed failure is answered with its status, any other failure and a defect with 500", async () => {
  const bodyTooLarge = { _tag: tag("Http.Multipart.MultipartError.MultipartError"), _0: { _tag: tag("Http.Multipart.MultipartErrorReason.BodyTooLarge") } };
  const app = flickRouter(
    { ignoreTrailingSlash: none, ignoreDuplicateSlashes: none, caseSensitive: none, maxParamLength: none },
    [
      flickRoute(GET, "/typed", (req: ServerRequest) => raise(fail(httpServerError("InternalError", req)))),
      flickRoute(GET, "/multipart", failing(bodyTooLarge)),
      flickRoute(GET, "/other", failing({ _tag: tag("Http.ServerError.ServeError.ServeError"), _0: "x" })),
      flickRoute(GET, "/defect", () => {
        throw new Error("broken");
      }),
      flickRoute(GET, "/status", () => flickEmpty({ ...defaults, status: some(99) })),
    ],
  );
  const statuses = await serving(app, async (url) => [
    await one(url, "GET", "/typed"),
    await one(url, "GET", "/multipart"),
    await one(url, "GET", "/other"),
    await one(url, "GET", "/defect"),
    await one(url, "GET", "/status"),
    await one(url, "GET", "/nowhere"),
  ]);
  assert.deepEqual(statuses.map((s) => s.split(" | ")[0]), [
    "HTTP/1.1 500 Internal Server Error",
    "HTTP/1.1 413 Payload Too Large",
    "HTTP/1.1 500 Internal Server Error",
    "HTTP/1.1 500 Internal Server Error",
    "HTTP/1.1 500 Internal Server Error",
    "HTTP/1.1 404 Not Found",
  ]);
});

test("HEAD gets the head of GET's response without its body", async () => {
  const app = flickRouter(
    { ignoreTrailingSlash: none, ignoreDuplicateSlashes: none, caseSensitive: none, maxParamLength: none },
    [flickRoute(GET, "/text", () => text("hello"))],
  );
  const answers = await serving(app, async (url) => [await one(url, "HEAD", "/text"), await one(url, "GET", "/text")]);
  assert.deepEqual(answers, [
    'HTTP/1.1 200 OK | content-length: 5; content-type: text/plain | ""',
    'HTTP/1.1 200 OK | content-length: 5; content-type: text/plain | "hello"',
  ]);
});

test("a body over the size limit fails with RequestParseError, answered with 400", async () => {
  let caught = "";
  const app: App = function* (req): Run {
    if (req.url !== "/caught") return text(String(yield small(req)));
    try {
      return text(String(yield small(req)));
    } catch (thrown) {
      caught = shown(Reflect.get(Object(thrown), "cause"));
      return text("caught");
    }
  };
  const answers = await serving(app, async (url) => [
    await one(url, "POST", "/uncaught", '{"a": 12345}'),
    await one(url, "POST", "/caught", '{"a": 12345}'),
    await one(url, "POST", "/fits", "{}"),
  ]);
  assert.deepEqual(answers.map((a) => a.split(" | ")[0]), ["HTTP/1.1 400 Bad Request", "HTTP/1.1 200 OK", "HTTP/1.1 200 OK"]);
  assert.equal(answers[2]?.split(" | ")[2], '"{}"');
  assert.match(caught, /"_tag":"HttpServerError","_0":\{"_tag":"RequestParseError","_0":\{"method":\{"_tag":"POST"\},"url":"\/caught","description":\{"_tag":"Some","value":"maxBytes exceeded"\}\}\}/);
});

test("routes match parameters, wildcards and their prefix, and HEAD falls back to GET", async () => {
  const app = flickRouter(
    { ignoreTrailingSlash: none, ignoreDuplicateSlashes: none, caseSensitive: none, maxParamLength: none },
    [
      flickRoute(GET, "/users/:id", params),
      flickRoute(GET, "/files/*", params),
      flickRoute(GET, "/u/:name", params),
      prefixRoute("/api")(flickRoute(none, "/v1/*", params)),
    ],
  );
  const bodies = await serving(app, async (url) => [
    await one(url, "GET", "/users/7"),
    await one(url, "GET", "/files/a/b.txt"),
    await one(url, "GET", "/files"),
    await one(url, "GET", "/api/v1/x?q=1"),
    await one(url, "HEAD", "/users/7"),
    await one(url, "POST", "/users/7"),
    await one(url, "GET", "/nowhere"),
    await one(url, "GET", "/u/" + "x".repeat(120)),
  ]);
  assert.deepEqual(bodies.map((b) => b.split(" | ")[2]), [
    JSON.stringify('[["id","7"]] /users/7'),
    JSON.stringify('[["*","a/b.txt"]] /files/a/b.txt'),
    JSON.stringify("[] /files"),
    JSON.stringify('[["*","x"]] /v1/x?q=1'),
    '""',
    '"0\\r\\n\\r\\n"',
    '"0\\r\\n\\r\\n"',
    '"0\\r\\n\\r\\n"',
  ]);
  assert.deepEqual(bodies.slice(4).map((b) => b.split(" | ")[0]), [
    "HTTP/1.1 200 OK",
    "HTTP/1.1 404 Not Found",
    "HTTP/1.1 404 Not Found",
    "HTTP/1.1 404 Not Found",
  ]);
});

test("a request no route matches fails with RouteNotFound, saying when a parameter is too long", async () => {
  const app = flickRouter(
    { ignoreTrailingSlash: none, ignoreDuplicateSlashes: none, caseSensitive: none, maxParamLength: none },
    [flickRoute(GET, "/inner/u/:name", params)],
  );
  const failure = async (url: string): Promise<string> => {
    const req: ServerRequest = { method: "GET", url, originalUrl: url, headers: {}, remoteAddress: none, source: { read: () => Promise.resolve(new Uint8Array()) } };
    const exit = await root(() => app(req));
    return shown(exit.cause?.reasons[0]);
  };
  assert.equal(
    await failure("/inner/nowhere?token=1"),
    JSON.stringify({ _tag: "Fail", error: { _tag: "HttpServerError", _0: { _tag: "RouteNotFound", _0: { method: { _tag: "GET" }, url: "/inner/nowhere?token=1", description: { _tag: "None" } } } } }),
  );
  assert.match(await failure("/inner/u/" + "y".repeat(120)), /"description":\{"_tag":"Some","value":"path parameter is longer than maxParamLength \(100\)"\}/);
});

test("a header set over the body's content type gives way to the body when the response is made again", () => {
  const response = removeHeader("x-a")(setStatus(201, "Made")(setHeader("content-type", "text/x")(setHeader("X-A", "1")(text("h")))));
  assert.ok(response instanceof ServerResponse);
  assert.equal(response.status, 201);
  assert.equal(response.statusText, "Made");
  assert.deepEqual(Object.entries(response.headers), [["content-type", "text/plain"], ["content-length", "1"]]);
});
