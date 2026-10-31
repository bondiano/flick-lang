import assert from "node:assert/strict";
import { test } from "node:test";

import { Cause, Deferred, Effect, Exit, Fiber } from "effect";

import { type Owner, acquireRelease, bracket, fiberSpawn, foreignAsync, scope } from "../src/index.ts";
import { causeOf, errorsOf } from "./support.ts";

const quiet = () => Effect.void;

const resource = (log: Array<string>, name: string) =>
  acquireRelease(
    Effect.sync(() => {
      log.push(`acquire ${name}`);
      return name;
    }),
    () => Effect.sync(() => log.push(`release ${name}`)),
  );

type Ending = readonly [
  outcome: string,
  end: Effect.Effect<unknown, unknown>,
  check: (exit: Exit.Exit<unknown, unknown>) => boolean,
];

const endings: ReadonlyArray<Ending> = [
  ["success", Effect.succeed(1), Exit.isSuccess],
  ["a typed error", Effect.fail({ _tag: "E" }), (e) => Exit.isFailure(e) && Cause.hasFails(e.cause)],
  ["a defect", Effect.die("boom"), (e) => Exit.isFailure(e) && Cause.hasDies(e.cause)],
  ["interruption", Effect.interrupt, (e) => Exit.isFailure(e) && Cause.hasInterruptsOnly(e.cause)],
];

for (const [outcome, end, check] of endings) {
  test(`a scope releases its resources on ${outcome}`, async () => {
    const log: Array<string> = [];
    const exit = await Effect.runPromiseExit(
      scope(() => Effect.flatMap(resource(log, "file"), () => end)),
    );
    assert.ok(check(exit), String(exit));
    assert.deepEqual(log, ["acquire file", "release file"]);
  });
}

test("a scope releases when the fiber running it is interrupted", async () => {
  const log: Array<string> = [];
  const started = Deferred.makeUnsafe();
  const fiber = Effect.runFork(
    scope(() =>
      Effect.flatMap(resource(log, "file"), () =>
        Effect.andThen(Deferred.succeed(started, undefined), Effect.never),
      ),
    ),
  );
  await Effect.runPromise(Deferred.await(started));
  await Effect.runPromise(Fiber.interrupt(fiber));
  assert.deepEqual(log, ["acquire file", "release file"]);
});

test("resources are released last first, after every child ended", async () => {
  const log: Array<string> = [];
  await Effect.runPromise(
    scope((owner) =>
      Effect.gen(function* () {
        yield* resource(log, "first");
        yield* fiberSpawn(
          owner,
          quiet,
          Effect.onInterrupt(Effect.never, () => Effect.sync(() => log.push("child interrupted"))),
        );
        yield* Effect.yieldNow;
        // registered after the child started: still released after it ended
        yield* resource(log, "second");
      }),
    ),
  );
  assert.deepEqual(log, [
    "acquire first",
    "acquire second",
    "child interrupted",
    "release second",
    "release first",
  ]);
});

test("an uninterruptible child delays the release", async () => {
  const log: Array<string> = [];
  await Effect.runPromise(
    scope((owner) =>
      Effect.gen(function* () {
        yield* resource(log, "file");
        yield* fiberSpawn(
          owner,
          quiet,
          Effect.uninterruptible(
            Effect.andThen(Effect.sleep(30), Effect.sync(() => log.push("child done"))),
          ),
        );
        yield* Effect.yieldNow;
      }),
    ),
  );
  assert.deepEqual(log, ["acquire file", "child done", "release file"]);
});

test("release waits for a foreign promise that ignores interruption", async () => {
  const log: Array<string> = [];
  const call = foreignAsync(
    () =>
      new Promise<void>((resolve) =>
        setTimeout(() => {
          log.push("promise settled");
          resolve();
        }, 30),
      ),
  );
  const fiber = Effect.runFork(
    scope((owner) =>
      Effect.gen(function* () {
        yield* resource(log, "socket");
        yield* fiberSpawn(owner, quiet, call);
        yield* Effect.yieldNow;
        yield* Effect.interrupt;
      }),
    ),
  );
  const exit = await Effect.runPromise(Fiber.await(fiber));
  assert.ok(Cause.hasInterruptsOnly(causeOf(exit)));
  assert.deepEqual(log, ["acquire socket", "promise settled", "release socket"]);
});

test("bracket releases with a typed error of its own", async () => {
  const log: Array<string> = [];
  const exit = await Effect.runPromiseExit(
    bracket(
      Effect.succeed("conn"),
      (c) => Effect.sync(() => log.push(`use ${c}`)),
      () => Effect.fail({ _tag: "CloseError" }),
    ),
  );
  assert.deepEqual(log, ["use conn"]);
  assert.deepEqual(errorsOf(exit), [{ _tag: "CloseError" }]);
});

test("a closing owner admits no new children", async () => {
  const saved: Owner = await Effect.runPromise(scope((owner) => Effect.succeed(owner)));
  const exit = await Effect.runPromiseExit(fiberSpawn(saved, quiet, Effect.void));
  assert.ok(Exit.isFailure(exit) && Cause.hasDies(exit.cause));
});

test("failed closing hooks preserve every failure and complete all shutdown stages", { timeout: 1000 }, async () => {
  const log: Array<string> = [];
  const started = Promise.withResolvers<void>();
  const supervised = Promise.withResolvers<void>();
  const settled = Promise.withResolvers<void>();
  const detachFailure = new Error("detach failed");
  const releaseFailure = new Error("release failed");
  const fiber = Effect.runFork(
    scope((owner) =>
      Effect.gen(function* () {
        yield* acquireRelease(Effect.void, () => Effect.sync(() => log.push("released last")));
        yield* acquireRelease(Effect.void, () =>
          Effect.andThen(Effect.sync(() => log.push("release failed")), Effect.die(releaseFailure)),
        );
        owner.closing.add(() => {
          log.push("detach failed");
          throw detachFailure;
        });
        owner.closing.add(() => Effect.andThen(Effect.sync(() => log.push("next hook")), Effect.fail("next hook failed")));
        yield* fiberSpawn(
          owner,
          () =>
            Effect.sync(() => {
              log.push("supervised");
              supervised.resolve();
            }),
          Effect.onInterrupt(
            foreignAsync(() => {
              started.resolve();
              return settled.promise.then(() => void log.push("foreign settled"));
            }),
            () => Effect.sync(() => log.push("interrupted")),
          ),
        );
        yield* Effect.promise(() => started.promise);
        return yield* Effect.fail("body failed");
      }),
    ),
  );
  try {
    await supervised.promise;
    assert.deepEqual(log, ["detach failed", "next hook", "interrupted", "supervised"]);
    settled.resolve();
    const exit = await Effect.runPromise(Fiber.await(fiber));
    assert.deepEqual(errorsOf(exit), ["body failed", "next hook failed"]);
    assert.deepEqual(
      causeOf(exit).reasons.filter(Cause.isDieReason).map((reason) => reason.defect),
      [detachFailure, releaseFailure],
    );
    assert.deepEqual(log, [
      "detach failed",
      "next hook",
      "interrupted",
      "supervised",
      "foreign settled",
      "release failed",
      "released last",
    ]);
  } finally {
    settled.resolve();
    await Effect.runPromise(Fiber.interrupt(fiber));
  }
});

test("a closing hook a supervisor adds while its owner closes runs before the finalizers", async () => {
  const order: Array<string> = [];
  await Effect.runPromise(
    scope((owner) =>
      Effect.gen(function* () {
        yield* acquireRelease(Effect.void, () => Effect.sync(() => order.push("finalizer")));
        const supervisor = () =>
          Effect.sync(() => owner.closing.add(() => Effect.sync(() => order.push("hook"))));
        yield* fiberSpawn(owner, supervisor, Effect.never);
      }),
    ),
  );
  assert.deepEqual(order, ["hook", "finalizer"]);
});
