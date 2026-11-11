import assert from "node:assert/strict";
import { test } from "node:test";

import { Effect, Option } from "effect";
import * as HttpServerResponse from "effect/http/HttpServerResponse";
import { tag } from "@flick-lang/runtime/tags";

import { flickAddress, flickServe } from "../src/effect/http/HttpServer.ts";
import { acquireRelease, scope } from "../src/owner.ts";

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
