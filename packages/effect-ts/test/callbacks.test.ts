import assert from "node:assert/strict";
import { getEventListeners } from "node:events";
import { test } from "node:test";

import { Cause, Effect, Exit, Fiber, Layer, Result } from "effect";

import {
  RuntimeFailure,
  acquireRelease,
  eventsAttach,
  eventsListen,
  eventsQueue,
  foreignAsync,
  promiseCallback,
  promiseFacade,
  provide,
  scope,
  service,
} from "../src/index.ts";
import { causeOf, errorsOf } from "./support.ts";

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

test("a pre-aborted Promise facade runs neither the function nor its effects", async () => {
  const controller = new AbortController();
  controller.abort();
  let calls = 0;
  let effects = 0;
  const call = promiseFacade(() => {
    calls++;
    return Effect.sync(() => ++effects);
  }, 1);
  await assert.rejects(call(undefined, { signal: controller.signal }), RuntimeFailure);
  assert.equal(calls, 0);
  assert.equal(effects, 0);
  assert.equal(getEventListeners(controller.signal, "abort").length, 0);
});

test("completed Promise facades dispose listeners on a reused AbortSignal", async () => {
  const controller = new AbortController();
  const call = promiseFacade(() => Effect.andThen(Effect.yieldNow, Effect.succeed("ready")), 1);
  for (let i = 0; i < 3; i++) {
    const pending = call(undefined, { signal: controller.signal });
    assert.equal(getEventListeners(controller.signal, "abort").length, 1);
    // oxlint-disable-next-line no-await-in-loop -- each completed call must release the signal before its reuse
    assert.deepEqual(await pending, Result.succeed("ready"));
    assert.equal(getEventListeners(controller.signal, "abort").length, 0);
  }
});

test("an aborted Promise facade rejects only after resource cleanup", { timeout: 1000 }, async () => {
  const controller = new AbortController();
  const started = Promise.withResolvers<void>();
  const closing = Promise.withResolvers<void>();
  const released = Promise.withResolvers<void>();
  const log: Array<string> = [];
  const call = promiseFacade(
    () =>
      Effect.gen(function* () {
        yield* acquireRelease(Effect.void, () =>
          Effect.gen(function* () {
            log.push("cleanup started");
            closing.resolve();
            yield* Effect.promise(() => released.promise);
            log.push("cleanup ended");
          }),
        );
        started.resolve();
        yield* Effect.never;
      }),
    1,
  );
  const rejected = assert.rejects(call(undefined, { signal: controller.signal }), RuntimeFailure).then(() => {
    log.push("rejected");
  });
  try {
    await started.promise;
    controller.abort();
    await closing.promise;
    assert.deepEqual(log, ["cleanup started"]);
  } finally {
    released.resolve();
    await rejected;
  }
  assert.deepEqual(log, ["cleanup started", "cleanup ended", "rejected"]);
  assert.equal(getEventListeners(controller.signal, "abort").length, 0);
});

for (const bodyFails of [false, true]) {
  test(`all detach failures survive a ${bodyFails ? "failed" : "successful"} body and resource cleanup`, async () => {
    const log: Array<string> = [];
    const failures = [new Error("detach failed"), new Error("next detach failed")];
    const exit = await Effect.runPromiseExit(
      scope((owner) =>
        Effect.gen(function* () {
          yield* acquireRelease(Effect.void, () => Effect.sync(() => log.push("released")));
          const listener = yield* eventsListen(owner, eventsQueue(1), () => Effect.void, () => Effect.void);
          for (const failure of failures) {
            yield* eventsAttach(
              listener,
              () => Effect.void,
              () => Effect.andThen(Effect.sync(() => log.push(failure.message)), Effect.die(failure)),
            );
          }
          if (bodyFails) yield* Effect.fail("body failed");
        }),
      ),
    );
    assert.deepEqual(log, ["detach failed", "next detach failed", "released"]);
    assert.deepEqual(causeOf(exit).reasons.filter(Cause.isDieReason).map((reason) => reason.defect), failures);
    assert.deepEqual(errorsOf(exit), bodyFails ? ["body failed"] : []);
  });
}
