import assert from "node:assert/strict";
import { test } from "node:test";

import { Effect } from "effect";

import { serverCall, serverWithOrigin } from "../src/effect/server/Functions.ts";
import { clientError, rpcMake } from "../src/rpc/model.ts";

const place = rpcMake("place");

/** The failure of `call`, or the success as a failure of the test. */
const failure = async (call: Effect.Effect<unknown, unknown, unknown>): Promise<unknown> =>
  // oxlint-disable-next-line typescript/no-unsafe-type-assertion -- a call that is not served in this process needs no service
  Effect.runPromise(Effect.flip(call as Effect.Effect<unknown, unknown>));

test("a server function call outside a browser and outside Server.with_origin is a client error", async () => {
  assert.deepEqual(
    await failure(serverCall(place, null)),
    clientError("no server for place: call it within Server.with_origin"),
  );
});

test("an origin that is not a URL is a client error, not a defect", async () => {
  assert.deepEqual(
    await failure(serverWithOrigin("not a url")(serverCall(place, null))),
    clientError('no server for place: "not a url" is not an origin'),
  );
});
