import assert from "node:assert/strict";
import { test } from "node:test";

import { Cause, Effect, Exit } from "effect";

import {
  type Failure,
  exitFailure,
  exitOf,
  failureInspect,
  failurePrimary,
  failureRethrow,
  nativeCause,
  rawCauseInspect,
  failureRawCause,
  tryWith,
} from "../src/index.ts";
import { causeOf, value } from "./support.ts";

interface Tagged {
  readonly _tag: string;
}

const A: Tagged = { _tag: "A" };
const B: Tagged = { _tag: "B" };
const fails = (...reasons: Array<Cause.Reason<Tagged>>) => Effect.failCause(Cause.fromReasons(reasons));
const fail = (e: Tagged) => Cause.makeFailReason(e);
const handled = (body: Effect.Effect<never, Tagged>, tags: ReadonlyArray<string>) =>
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
    causeOf(exit).reasons.map((r) => [r._tag, Cause.isDieReason(r) ? r.defect : undefined]),
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
    causeOf(exit).reasons.map((r) => (Cause.isFailReason(r) ? r.error : undefined)),
    [B, A],
  );
});

const failureOf = async (effect: Effect.Effect<unknown, unknown>): Promise<Failure> => {
  const exit = await Effect.runPromise(exitOf(effect));
  return value(exitFailure(exit));
};

test("Failure.rethrow keeps the original cause", async () => {
  const original = Cause.fromReasons([fail(B), fail(A)]);
  const failure = await failureOf(Effect.failCause(original));
  assert.equal(nativeCause(failure), original);
  const exit = await Effect.runPromiseExit(failureRethrow(failure));
  assert.equal(causeOf(exit), original);
  assert.equal(value(failurePrimary(failure)), B);
});

test("Failure.primary is the error recovery would select", async () => {
  const withDefect = await failureOf(fails(fail(A), Cause.makeDieReason("D")));
  assert.equal(failurePrimary(withDefect)._tag, "None");
  const interrupted = await failureOf(fails(Cause.makeInterruptReason()));
  assert.equal(failurePrimary(interrupted)._tag, "None");
});

test("inspecting a cause never runs code of its payloads", async () => {
  const calls: Array<string> = [];
  const spy =
    <R>(name: string, result: R) =>
    (): R => {
      calls.push(name);
      return result;
    };
  const getter: Tagged & { toString(): string; toJSON(): string } = {
    get _tag() {
      return spy("_tag getter", "spied")();
    },
    toString: spy("toString", "spied"),
    toJSON: spy("toJSON", "spied"),
  };
  const proxy = new Proxy<Tagged>(
    { _tag: "proxy" },
    {
      get: spy("proxy get", "spied"),
      getOwnPropertyDescriptor: spy("proxy descriptor", undefined),
      getPrototypeOf: spy("proxy prototype", null),
    },
  );
  const error = new Error("boom");
  Object.defineProperty(error, "stack", { get: spy("stack", "spied") });
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
  assert.equal(Reflect.get(report, "toString"), undefined);
  assert.ok(Object.isFrozen(report));
});
