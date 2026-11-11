import assert from "node:assert/strict";
import { test } from "node:test";

import { Duration, Effect, Option, Stream } from "effect";
import * as HttpRouter from "effect/http/HttpRouter";
import * as HttpServerResponse from "effect/http/HttpServerResponse";
import { tag } from "@flick-lang/runtime/tags";

import { flickAddress, flickServe, ownedServer } from "../src/effect/http/HttpServer.ts";
import { acquireRelease, foreignAsync, scope } from "../src/owner.ts";

test("successful and failed HTTP requests release their resources while the server stays open", async () => {
  const released: string[] = [];
  await Effect.runPromise(scope(() => Effect.gen(function* () {
    const server = yield* flickServe({ host: "127.0.0.1", port: 0, gracefulShutdownTimeout: Option.none() },
      (request) => Effect.flatMap(acquireRelease(
        Effect.succeed(request.url),
        (url) => Effect.sync(() => { released.push(url); }),
      ), (url) => url === "/failed" ? Effect.fail("failed request") : Effect.succeed(HttpServerResponse.text("ok"))));
    const address = flickAddress(server);
    assert.equal(address._tag, tag("Http.Server.Address.TcpAddress"));
    for (const [path, status] of [["/ok", 200], ["/failed", 500]] as const) {
      const response = yield* Effect.promise(() => fetch(`http://127.0.0.1:${address._1}${path}`));
      yield* Effect.promise(() => response.text());
      assert.equal(response.status, status);
      assert.ok(released.includes(path), `request ${path} is released before the server closes`);
    }
  })));
  assert.deepEqual(released, ["/ok", "/failed"]);
});

test("a disconnected response stream waits for foreign work before releasing its resource", async () => {
  const started = Promise.withResolvers<void>();
  const aborted = Promise.withResolvers<void>();
  const work = Promise.withResolvers<string>();
  let released = false;
  await Effect.runPromise(scope(() => Effect.gen(function* () {
    const server = yield* flickServe({ host: "127.0.0.1", port: 0, gracefulShutdownTimeout: Option.none() },
      () => Effect.gen(function* () {
        yield* acquireRelease(Effect.void, () => Effect.sync(() => { released = true; }));
        const stream = Stream.fromEffect(foreignAsync((signal) => {
          signal.addEventListener("abort", () => aborted.resolve(), { once: true });
          started.resolve();
          return work.promise;
        }));
        return HttpServerResponse.stream(Stream.map(stream, (text) => new TextEncoder().encode(text)));
      }));
    const address = flickAddress(server);
    const controller = new AbortController();
    const response = fetch(`http://127.0.0.1:${address._1}`, { signal: controller.signal }).catch(() => undefined);
    yield* Effect.promise(() => started.promise);
    controller.abort();
    yield* Effect.promise(() => aborted.promise);
    yield* Effect.promise(() => new Promise<void>((resolve) => setImmediate(resolve)));
    try {
      assert.equal(released, false, "the foreign operation still uses the response resource");
    } finally {
      work.resolve("done");
    }
    yield* Effect.promise(() => response);
  })));
  assert.equal(released, true);
});

test("an explicitly uninterruptible router route completes after the shutdown grace period", async () => {
  const started = Promise.withResolvers<void>();
  let completed = false;
  await Effect.runPromise(scope((owner) => Effect.gen(function* () {
    const router = yield* HttpRouter.make;
    yield* router.add("GET", "/", Effect.gen(function* () {
      started.resolve();
      yield* Effect.sleep(80);
      completed = true;
      return HttpServerResponse.text("done");
    }), { uninterruptible: true });
    const server = yield* ownedServer(owner.resources,
      { host: "127.0.0.1", port: 0, gracefulShutdownTimeout: Option.some(Duration.millis(20)) }, router.asHttpEffect());
    const address = flickAddress(server);
    const response = fetch(`http://127.0.0.1:${address._1}`).catch(() => undefined);
    yield* Effect.promise(() => started.promise);
    // A rejection after shutdown is observed; the request itself is intentionally still active.
    void response;
  })));
  assert.equal(completed, true, "the router's explicit mask survives shutdown interruption");
});

test("shutdown closes upgraded connections before waiting for WebSocket finalizers", { timeout: 5_000 }, async (t) => {
  let released = false;
  let socket: WebSocket | undefined;
  t.after(() => socket?.close());
  await Effect.runPromise(scope(() => Effect.gen(function* () {
    const server = yield* flickServe(
      { host: "127.0.0.1", port: 0, gracefulShutdownTimeout: Option.some(Duration.millis(20)) },
      (request) => Effect.gen(function* () {
        yield* acquireRelease(Effect.void, () => Effect.sync(() => { released = true; }));
        const connection = yield* request.upgrade;
        const reader = yield* connection.reader;
        return yield* Effect.andThen(reader.pull, Effect.never);
      }),
    );
    const address = flickAddress(server);
    yield* Effect.promise(() => new Promise<void>((resolve, reject) => {
      socket = new WebSocket(`ws://127.0.0.1:${address._1}`);
      socket.addEventListener("open", () => resolve(), { once: true });
      socket.addEventListener("error", reject, { once: true });
    }));
  })));
  assert.equal(released, true, "upgraded requests finish cleanup before their server closes");
});
