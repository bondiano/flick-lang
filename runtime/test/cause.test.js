import assert from "node:assert/strict";
import { test } from "node:test";

import { Cause, Effect, Exit } from "effect";

import {
  exitFailure,
  exitOf,
  failureInspect,
  failurePrimary,
  failureRethrow,
  nativeCause,
  rawCauseInspect,
  failureRawCause,
  tryWith,
} from "../src/index.js";

const A = { _tag: "A" };
const B = { _tag: "B" };
const fails = (...reasons) => Effect.failCause(Cause.fromReasons(reasons));
const fail = (e) => Cause.makeFailReason(e);
const handled = (body, tags) =>
  Effect.runPromiseExit(
    tryWith(body, (e) => (tags.includes(e._tag) ? Effect.succeed(`handled ${e._tag}`) : undefined)),
  );

test("one Fail(A) is recovered", async () => {
  const exit = await handled(fails(fail(A)), ["A"]);
  assert.deepEqual(exit, Exit.succeed("handled A"));
});

test("Fail(A), Fail(B): the first failure is selected", async () => {
  const exit = await handled(fails(fail(A), fail(B)), ["A", "B"]);
  assert.deepEqual(exit, Exit.succeed("handled A"));
});

test("Fail(A), Interrupt: the interruption does not block selecting A", async () => {
  const exit = await handled(fails(fail(A), Cause.makeInterruptReason()), ["A"]);
  assert.deepEqual(exit, Exit.succeed("handled A"));
});

test("Fail(A), Die(D): recovery is blocked and failures become defects", async () => {
  const exit = await handled(fails(fail(A), Cause.makeDieReason("D")), ["A"]);
  assert.ok(Exit.isFailure(exit));
  assert.deepEqual(
    exit.cause.reasons.map((r) => [r._tag, r.defect]),
    [
      ["Die", A],
      ["Die", "D"],
    ],
  );
});

test("Interrupt only: the handler gets no error", async () => {
  let called = false;
  const exit = await Effect.runPromiseExit(
    tryWith(fails(Cause.makeInterruptReason()), () => {
      called = true;
      return Effect.succeed(0);
    }),
  );
  assert.equal(called, false);
  assert.ok(Exit.isFailure(exit) && Cause.hasInterruptsOnly(exit.cause));
});

test("Fail(B), Fail(A) with a handler for A only: the original cause propagates", async () => {
  const original = Cause.fromReasons([fail(B), fail(A)]);
  const exit = await handled(Effect.failCause(original), ["A"]);
  assert.ok(Exit.isFailure(exit));
  assert.deepEqual(
    exit.cause.reasons.map((r) => r.error),
    [B, A],
  );
});

const failureOf = async (effect) => {
  const exit = await Effect.runPromise(exitOf(effect));
  return exitFailure(exit).value;
};

test("Failure.rethrow keeps the original cause", async () => {
  const original = Cause.fromReasons([fail(B), fail(A)]);
  const failure = await failureOf(Effect.failCause(original));
  assert.equal(nativeCause(failure), original);
  const exit = await Effect.runPromiseExit(failureRethrow(failure));
  assert.equal(exit.cause, original);
  assert.equal(failurePrimary(failure).value, B);
});

test("Failure.primary is the error recovery would select", async () => {
  const withDefect = await failureOf(fails(fail(A), Cause.makeDieReason("D")));
  assert.equal(failurePrimary(withDefect)._tag, "None");
  const interrupted = await failureOf(fails(Cause.makeInterruptReason()));
  assert.equal(failurePrimary(interrupted)._tag, "None");
});

test("inspecting a cause never runs code of its payloads", async () => {
  const calls = [];
  const spy = (name) => () => {
    calls.push(name);
    return "spied";
  };
  const getter = {};
  Object.defineProperty(getter, "_tag", { get: spy("_tag getter"), enumerable: true });
  getter.toString = spy("toString");
  getter.toJSON = spy("toJSON");
  const proxy = new Proxy(
    {},
    {
      get: spy("proxy get"),
      getOwnPropertyDescriptor: spy("proxy descriptor"),
      getPrototypeOf: spy("proxy prototype"),
    },
  );
  const error = new Error("boom");
  Object.defineProperty(error, "stack", { get: spy("stack") });
  const failure = await failureOf(
    fails(fail(getter), fail({ _tag: "NotFound" }), Cause.makeDieReason(proxy), Cause.makeDieReason(error)),
  );
  const report = await Effect.runPromise(failureInspect(failure));
  const raw = await Effect.runPromise(rawCauseInspect(failureRawCause(failure)));
  assert.deepEqual(calls, []);
  assert.deepEqual(JSON.parse(JSON.stringify(raw)), JSON.parse(JSON.stringify(report)));
  assert.deepEqual(
    report.reasons.map((r) => ({ ...r })),
    [
      { kind: "fail", tag: undefined, value: "<object>" },
      { kind: "fail", tag: "NotFound", value: "<object>" },
      { kind: "die", value: "<proxy>" },
      { kind: "die", error: true, message: "boom" },
    ],
  );
  assert.equal(Object.getPrototypeOf(report), null, "the report is inert data");
  assert.equal(report.toString, undefined);
  assert.ok(Object.isFrozen(report));
});
