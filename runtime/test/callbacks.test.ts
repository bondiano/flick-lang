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
} from "../src/index.ts";

interface Greeter {
  readonly greet: (name: string) => Effect.Effect<string>;
}

const Greeter = service<Greeter>("Test.Greeter");

const greet = (n: string) => Effect.flatMap(Effect.service(Greeter), (g) => g.greet(n));

test("a Promise callback runs with the services of where it was made", async () => {
  const layer = Layer.succeed(Greeter, { greet: (n: string) => Effect.succeed(`hi ${n}`) });
  const callback = await Effect.runPromise(
    provide(layer, scope(() => promiseCallback(greet))),
  );
  // made inside a scope that has closed: later calls are rejected
  await assert.rejects(callback("ada"), RuntimeFailure);
  const result = await Effect.runPromise(
    provide(
      layer,
      scope(() =>
        Effect.gen(function* () {
          const cb = yield* promiseCallback(greet);
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
        return yield* Effect.promise(() => cb(1).catch((e: unknown) => e));
      }),
    ),
  );
  assert.ok(failure instanceof RuntimeFailure);
  assert.match(failure.message, /die error broken/);
});

test("a listener ignores late calls and the owner removes the very function it registered", async () => {
  const log: Array<string> = [];
  let registered: unknown;
  let stored: ((event: number) => void) | undefined;
  await Effect.runPromise(
    scope((owner) =>
      Effect.gen(function* () {
        const listener = yield* eventsListen(
          owner,
          eventsQueue(4),
          () => Effect.sync(() => log.push("supervised")),
          (n: number) => Effect.sync(() => log.push(`handled ${n}`)),
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
  stored?.(2);
  await new Promise((r) => setTimeout(r, 5));
  assert.deepEqual(log, ["handled 1", "removed same: true"]);
});

test("an async foreign call gets a signal that interruption aborts", async () => {
  let aborted = false;
  const fiber = Effect.runFork(
    scope(() =>
      foreignAsync(
        (signal) =>
          new Promise<void>((resolve) => {
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

test("a Promise callback takes one argument whatever `this` and extra arguments JS passes", async () => {
  const received = await Effect.runPromise(
    scope(() =>
      Effect.gen(function* () {
        const cb = yield* promiseCallback((...args: ReadonlyArray<unknown>) => Effect.succeed(args));
        const call: (this: unknown, ...args: ReadonlyArray<unknown>) => Promise<ReadonlyArray<unknown>> = cb;
        return yield* Effect.promise(() => call.call({ self: true }, "ada", "extra"));
      }),
    ),
  );
  assert.deepEqual(received, ["ada"]);
});

test("a Promise callback may be called again from its own task", async () => {
  const result = await Effect.runPromise(
    scope(() =>
      Effect.gen(function* () {
        const self: { call?: (n: number) => Promise<number> } = {};
        const call = yield* promiseCallback((n: number) =>
          n === 0 ? Effect.succeed(0) : Effect.map(Effect.promise(() => self.call?.(n - 1) ?? Promise.resolve(0)), (m) => m + n),
        );
        self.call = call;
        return yield* Effect.promise(() => call(3));
      }),
    ),
  );
  assert.equal(result, 6);
});

test("a listener called from its own handler queues the event instead of nesting", async () => {
  const log: Array<string> = [];
  await Effect.runPromise(
    scope((owner) =>
      Effect.gen(function* () {
        const self: { listener?: (n: number) => void } = {};
        const listener = yield* eventsListen(
          owner,
          eventsQueue(4),
          () => Effect.void,
          (n: number) =>
            Effect.sync(() => {
              log.push(`start ${n}`);
              if (n === 1) self.listener?.(2);
              log.push(`end ${n}`);
            }),
        );
        self.listener = listener;
        listener(1);
        log.push("source returned");
        yield* Effect.sleep(5);
      }),
    ),
  );
  assert.deepEqual(log, ["source returned", "start 1", "end 1", "start 2", "end 2"]);
});
