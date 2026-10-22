import assert from "node:assert/strict";
import { test } from "node:test";

import { Context, Effect, Exit, Layer } from "effect";

import { layerMerge, provide, service } from "../src/index.js";

const Db = service("Test.Db");
const Users = service("Test.Users");
const Posts = service("Test.Posts");

test("a layer that fails to build never runs the body", async () => {
  let ran = false;
  const exit = await Effect.runPromiseExit(
    provide(
      Layer.effect(Db, Effect.fail({ _tag: "ConnectError" })),
      Effect.sync(() => {
        ran = true;
      }),
    ),
  );
  assert.ok(Exit.isFailure(exit));
  assert.equal(ran, false);
});

test("one build shares a layer; two provides build it twice", async () => {
  let builds = 0;
  const db = Layer.effect(
    Db,
    Effect.sync(() => {
      builds += 1;
      return { id: builds };
    }),
  );
  const users = Layer.effect(Users, Effect.map(Effect.service(Db), (d) => ({ db: d.id })));
  const posts = Layer.effect(Posts, Effect.map(Effect.service(Db), (d) => ({ db: d.id })));
  const graph = Layer.provide(layerMerge(users, posts), db);
  const body = Effect.all([Effect.service(Users), Effect.service(Posts)]);
  const [u, p] = await Effect.runPromise(provide(graph, body));
  assert.equal(builds, 1);
  assert.equal(u.db, p.db);
  await Effect.runPromise(provide(graph, body));
  assert.equal(builds, 2);
});

test("the body sees the outer services too, and they are restored after", async () => {
  const outer = Context.Service("Test.Outer");
  const program = Effect.gen(function* () {
    const inside = yield* provide(
      Layer.succeed(Users, "users"),
      Effect.all([Effect.service(outer), Effect.service(Users)]),
    );
    const after = yield* Effect.serviceOption(Users);
    return [inside, after._tag];
  });
  const result = await Effect.runPromise(Effect.provideService(program, outer, "outer"));
  assert.deepEqual(result, [["outer", "users"], "None"]);
});

test("a layer's finalizer runs after the body with the services it was registered with", async () => {
  const log = [];
  const Log = service("Test.Log");
  const scoped = Layer.effect(
    Db,
    Effect.acquireRelease(Effect.succeed("db"), () =>
      Effect.map(Effect.service(Log), (l) => log.push(`release with ${l}`)),
    ),
  );
  const program = provide(
    scoped,
    Effect.provideService(
      Effect.sync(() => log.push("body")),
      Log,
      "body log",
    ),
  );
  await Effect.runPromise(Effect.provideService(program, Log, "registration log"));
  assert.deepEqual(log, ["body", "release with registration log"]);
});
