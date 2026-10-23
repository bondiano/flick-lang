import assert from "node:assert/strict";
import { test } from "node:test";

import { Cause, Deferred, Effect } from "effect";

import {
  type Failure,
  failurePrimary,
  fiberJoin,
  fiberSpawn,
  reportedDefects,
  scope,
} from "../src/index.ts";
import { at, errorsOf, value } from "./support.ts";

test("the supervisor gets the failure of a child whose handle is lost", async () => {
  const seen: Array<[unknown, boolean]> = [];
  const done = Deferred.makeUnsafe<void>();
  const supervisor = (failure: Failure) =>
    Effect.andThen(
      Effect.sync(() => seen.push([value(failurePrimary(failure)), failure.shutdown])),
      Deferred.succeed(done, undefined),
    );
  await Effect.runPromise(
    scope((owner) =>
      Effect.gen(function* () {
        yield* fiberSpawn(owner, supervisor, Effect.fail({ _tag: "Lost" }));
        yield* Deferred.await(done);
      }),
    ),
  );
  assert.deepEqual(seen, [[{ _tag: "Lost" }, false]]);
});

test("a child stopped by its closing owner is a shutdown interruption", async () => {
  const seen: Array<boolean> = [];
  const supervisor = (failure: Failure) => Effect.sync(() => seen.push(failure.shutdown));
  await Effect.runPromise(
    scope((owner) => Effect.asVoid(fiberSpawn(owner, supervisor, Effect.never))),
  );
  assert.deepEqual(seen, [true]);
});

test("join restores the child's failure, not only the supervisor sees it", async () => {
  const exit = await Effect.runPromiseExit(
    scope((owner) =>
      Effect.gen(function* () {
        const fiber = yield* fiberSpawn(owner, () => Effect.void, Effect.fail({ _tag: "E" }));
        return yield* fiberJoin(fiber);
      }),
    ),
  );
  assert.deepEqual(errorsOf(exit), [{ _tag: "E" }]);
});

test("a defect of the supervisor goes to the root reporter", async () => {
  const before = reportedDefects().length;
  await Effect.runPromise(
    scope((owner) =>
      Effect.gen(function* () {
        const started = yield* Deferred.make<void>();
        const task = Effect.andThen(Deferred.succeed(started, undefined), Effect.fail("x"));
        yield* fiberSpawn(owner, () => Effect.die("supervisor broke"), task);
        yield* Deferred.await(started);
      }),
    ),
  );
  const added = reportedDefects().slice(before);
  assert.equal(added.length, 1);
  assert.ok(Cause.hasDies(at(added, 0)));
});
