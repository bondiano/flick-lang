import assert from "node:assert/strict";
import { test } from "node:test";

import { Cause, Deferred, Effect, Exit, Fiber } from "effect";

import { acquireRelease, bracket, fiberSpawn, foreignAsync, scope } from "../src/index.js";

const quiet = () => Effect.void;

const resource = (log, name) =>
  acquireRelease(
    Effect.sync(() => {
      log.push(`acquire ${name}`);
      return name;
    }),
    () => Effect.sync(() => log.push(`release ${name}`)),
  );

for (const [outcome, end, check] of [
  ["success", Effect.succeed(1), Exit.isSuccess],
  ["a typed error", Effect.fail({ _tag: "E" }), (e) => Exit.isFailure(e) && Cause.hasFails(e.cause)],
  ["a defect", Effect.die("boom"), (e) => Exit.isFailure(e) && Cause.hasDies(e.cause)],
  ["interruption", Effect.interrupt, (e) => Exit.isFailure(e) && Cause.hasInterruptsOnly(e.cause)],
]) {
  test(`a scope releases its resources on ${outcome}`, async () => {
    const log = [];
    const exit = await Effect.runPromiseExit(
      scope(() => Effect.flatMap(resource(log, "file"), () => end)),
    );
    assert.ok(check(exit), String(exit));
    assert.deepEqual(log, ["acquire file", "release file"]);
  });
}

test("a scope releases when the fiber running it is interrupted", async () => {
  const log = [];
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
  const log = [];
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
  const log = [];
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
  const log = [];
  const call = foreignAsync(
    () =>
      new Promise((resolve) =>
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
  assert.ok(Cause.hasInterruptsOnly(exit.cause));
  assert.deepEqual(log, ["acquire socket", "promise settled", "release socket"]);
});

test("bracket releases with a typed error of its own", async () => {
  const log = [];
  const exit = await Effect.runPromiseExit(
    bracket(
      Effect.succeed("conn"),
      (c) => Effect.sync(() => log.push(`use ${c}`)),
      () => Effect.fail({ _tag: "CloseError" }),
    ),
  );
  assert.deepEqual(log, ["use conn"]);
  assert.deepEqual(exit.cause.reasons.map((r) => r.error), [{ _tag: "CloseError" }]);
});

test("a closing owner admits no new children", async () => {
  let saved;
  await Effect.runPromise(scope((owner) => Effect.sync(() => (saved = owner))));
  const exit = await Effect.runPromiseExit(fiberSpawn(saved, quiet, Effect.void));
  assert.ok(Exit.isFailure(exit) && Cause.hasDies(exit.cause));
});
