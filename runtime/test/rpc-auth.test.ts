import assert from "node:assert/strict";
import { test } from "node:test";

import { Context, Effect, Schema } from "effect";
import * as RpcClient from "effect/rpc/RpcClient";

import { rpcCallLocal } from "../src/rpc/local.ts";
import { type Group, rpcMake, rpcWithAuth, rpcWithErrors, rpcWithSuccess } from "../src/rpc/model.ts";

const Session = Context.Service<{ readonly user: string }>("Session");

const refused = { _tag: "Refused" };

/** `me`: the session's user, made from the `authorization` header of each call. */
const me = rpcWithAuth(
  Session,
  (call) => {
    const user = Reflect.get(Reflect.get(Object(call), "headers"), "authorization");
    return typeof user === "string" ? Effect.succeed({ user }) : Effect.fail(refused);
  },
  rpcWithErrors([Schema.Struct({ _tag: Schema.Literal("Refused") })], rpcWithSuccess(Schema.String, rpcMake("me"))),
);

const group: Group = { procedures: [me] };

let runs = 0;
const handlers = {
  // slow, so concurrent calls overlap
  me: () =>
    Effect.flatMap(Effect.sleep("10 millis"), () =>
      Effect.map(Effect.service(Session), (s) => {
        runs += 1;
        return s.user;
      }),
    ),
};

const callAs = (user: string | undefined) => {
  const call = rpcCallLocal(group, handlers, me, undefined);
  return user === undefined ? call : RpcClient.withHeaders({ authorization: user })(call);
};

test("concurrent calls each see the service their own check made, and a refused one runs no handler", async () => {
  const all = Effect.all([callAs("ada"), callAs("bob"), Effect.flip(callAs(undefined))], { concurrency: "unbounded" });
  // oxlint-disable-next-line typescript/no-unsafe-type-assertion -- the handlers need only the session each check makes
  const [ada, bob, nobody] = await Effect.runPromise(all as Effect.Effect<[unknown, unknown, unknown], unknown>);
  assert.equal(ada, "ada");
  assert.equal(bob, "bob");
  assert.deepEqual(nobody, refused);
  assert.equal(runs, 2);
});
