import assert from "node:assert/strict";
import { test } from "node:test";

import { Effect } from "effect";
import { serverServe } from "../src/effect/server/FunctionsNode.ts";

test("a mounted server handler serves each request without repeating acquisition", async () => {
  let mounts = 0;
  const requests: string[] = [];
  const mount = Effect.sync(() => {
    mounts++;
    return (request: string) => Effect.sync(() => {
      requests.push(request);
      return `served ${request}`;
    });
  });
  const output = await Effect.runPromise(Effect.gen(function* () {
    const mounted = yield* mount;
    return [yield* serverServe(mounted, "first"), yield* serverServe(mounted, "second")];
  }));
  assert.equal(mounts, 1);
  assert.deepEqual(requests, ["first", "second"]);
  assert.deepEqual(output, ["served first", "served second"]);
});
