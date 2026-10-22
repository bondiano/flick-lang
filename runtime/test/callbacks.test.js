import assert from "node:assert/strict";
import { test } from "node:test";

import { Effect, Exit, Fiber, Layer } from "effect";

import {
  RuntimeFailure,
  eventsAttach,
  eventsListen,
  eventsQueue,
  foreignAsync,
  promiseCallback,
  provide,
  scope,
  service,
} from "../src/index.js";

const Greeter = service("Test.Greeter");

test("a Promise callback runs with the services of where it was made", async () => {
  const layer = Layer.succeed(Greeter, { greet: (n) => Effect.succeed(`hi ${n}`) });
  const callback = await Effect.runPromise(
    provide(layer, scope(() => promiseCallback((n) => Effect.flatMap(Effect.service(Greeter), (g) => g.greet(n))))),
  );
  // made inside a scope that has closed: later calls are rejected
  await assert.rejects(callback("ada"), RuntimeFailure);
  const result = await Effect.runPromise(
    provide(
      layer,
      scope(() =>
        Effect.gen(function* () {
          const cb = yield* promiseCallback((n) => Effect.flatMap(Effect.service(Greeter), (g) => g.greet(n)));
          return yield* Effect.promise(() => cb("bob"));
        }),
      ),
    ),
  );
  assert.equal(result, "hi bob");
});

test("a Promise callback rejects with a safe report when the task fails", async () => {
  const failure = await Effect.runPromise(
    scope(() =>
      Effect.gen(function* () {
        const cb = yield* promiseCallback(() => Effect.die(new Error("broken")));
        return yield* Effect.promise(() => cb(1).catch((e) => e));
      }),
    ),
  );
  assert.ok(failure instanceof RuntimeFailure);
  assert.match(failure.message, /die error broken/);
});

test("a listener ignores late calls and the owner removes the very function it registered", async () => {
  const log = [];
  let registered;
  let stored;
  await Effect.runPromise(
    scope((owner) =>
      Effect.gen(function* () {
        const listener = yield* eventsListen(
          owner,
          eventsQueue(4),
          () => Effect.sync(() => log.push("supervised")),
          (n) => Effect.sync(() => log.push(`handled ${n}`)),
        );
        stored = listener;
        yield* eventsAttach(
          listener,
          (fn) => Effect.sync(() => (registered = fn)),
          (fn) => Effect.sync(() => log.push(`removed same: ${fn === registered}`)),
        );
        listener(1);
        yield* Effect.sleep(5);
      }),
    ),
  );
  stored(2);
  await new Promise((r) => setTimeout(r, 5));
  assert.deepEqual(log, ["handled 1", "removed same: true"]);
});

test("an async foreign call gets a signal that interruption aborts", async () => {
  let aborted = false;
  const fiber = Effect.runFork(
    scope(() =>
      foreignAsync(
        (signal) =>
          new Promise((resolve) => {
            signal.addEventListener("abort", () => {
              aborted = true;
              resolve();
            });
          }),
      ),
    ),
  );
  await new Promise((r) => setTimeout(r, 5));
  await Effect.runPromise(Fiber.interrupt(fiber));
  const result = await Effect.runPromise(Fiber.await(fiber));
  assert.ok(Exit.isFailure(result));
  assert.ok(aborted);
});
