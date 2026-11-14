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
  handle,
  tryWith,
} from "../src/index.ts";
import { flickMap, flickReasons, toFailure } from "../src/effect/Cause.ts";
import { causeOf, value } from "./support.ts";

interface Tagged {
  readonly _tag: string;
}

const A: Tagged = { _tag: "A" };
const B: Tagged = { _tag: "B" };
const C: Tagged = { _tag: "C" };
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

test("a typed failure after the first has no type of the row: reasons give it as a defect, map leaves it", () => {
  const failure = toFailure(Cause.fromReasons([fail(B), fail(A)]));
  assert.deepEqual(
    flickReasons(failure).map((r) => r._tag),
    ["Fail", "Die"],
  );
  const seen: Array<unknown> = [];
  const mapped = flickMap((e) => {
    seen.push(e);
    return C;
  })(failure);
  assert.deepEqual(seen, [B]);
  assert.deepEqual(
    (nativeCause(mapped)?.reasons ?? []).map((r) => (Cause.isFailReason(r) ? r.error : r._tag)),
    [C, A],
  );
});

test("deferred guards suspend, continue on false, and run after body cleanup", async () => {
  const events: Array<string> = [];
  const body = Effect.ensuring(Effect.fail(A), Effect.sync(() => events.push("closed")));
  const exit = await Effect.runPromiseExit(tryWith(body, (_error, unhandled) =>
    Effect.gen(function* () {
      const selected = yield* Effect.gen(function* () {
        events.push("guard one");
        yield* Effect.sleep(1);
        const first = false;
        if (first) return Effect.succeed("wrong");
        events.push("guard two");
        yield* Effect.sleep(1);
        const second = true;
        if (second) return Effect.sync(() => {
          events.push("selected");
          return "handled";
        });
        return undefined;
      });
      return yield* (selected === undefined ? unhandled() : selected);
    }),
  ));
  assert.deepEqual(exit, Exit.succeed("handled"));
  assert.deepEqual(events, ["closed", "guard one", "guard two", "selected"]);
});

test("a suspended guard failure reaches only the outer try", async () => {
  const events: Array<string> = [];
  const guarded = tryWith(Effect.fail(A), () => Effect.gen(function* () {
    events.push("guard");
    yield* Effect.sleep(1);
    return yield* Effect.fail(B);
  }));
  const exit = await Effect.runPromiseExit(tryWith(guarded, (error) => {
    events.push(`outer ${error._tag}`);
    return Effect.succeed("outer");
  }));
  assert.deepEqual(exit, Exit.succeed("outer"));
  assert.deepEqual(events, ["guard", "outer B"]);
});

test("deferred no-match selection rethrows the exact original cause", async () => {
  const original = Cause.fromReasons([fail(B), fail(A), Cause.makeInterruptReason()]);
  const exit = await Effect.runPromiseExit(tryWith(Effect.failCause(original), (_error, unhandled) =>
    Effect.gen(function* () {
      const selected = yield* Effect.gen(function* () {
        yield* Effect.sleep(1);
        return undefined;
      });
      return yield* (selected === undefined ? unhandled() : selected);
    }),
  ));
  assert.equal(causeOf(exit), original);
});

test("a selector miss differs from a guard or body raising the identical payload", async () => {
  const first: Tagged = { _tag: "First" };
  const second: Tagged = { _tag: "Second" };
  const original = Cause.fromReasons([fail(first), fail(second)]);
  await Promise.all((["miss", "guard raises", "body raises", "hit"] as const).map(async (mode) => {
    const events: Array<string> = [];
    const guardFailure = Cause.fromReasons([fail(first)]);
    const exit = await Effect.runPromiseExit(tryWith(
      Effect.failCause(original),
      (error, unhandled) => {
        assert.equal(error, first, "only the first failure is selected");
        events.push("first");
        return Effect.gen(function* () {
          events.push("guard");
          yield* Effect.yieldNow;
          if (mode === "miss") return yield* unhandled();
          if (mode === "guard raises") return yield* Effect.failCause(guardFailure);
          return yield* Effect.suspend(() => {
            events.push("body");
            return mode === "body raises" ? Effect.fail(error) : Effect.succeed("hit");
          });
        });
      },
    ));
    assert.deepEqual(events, mode === "miss" || mode === "guard raises"
      ? ["first", "guard"] : ["first", "guard", "body"]);
    assert.equal(original.reasons.length, 2);
    const [keptFirst, keptSecond] = original.reasons;
    assert.ok(keptFirst && Cause.isFailReason(keptFirst));
    assert.ok(keptSecond && Cause.isFailReason(keptSecond));
    assert.equal(keptFirst.error, first);
    assert.equal(keptSecond.error, second);
    if (mode === "hit") {
      assert.deepEqual(exit, Exit.succeed("hit"));
    } else {
      assert.ok(Exit.isFailure(exit));
      const actual = causeOf(exit);
      if (mode === "miss") {
        assert.equal(actual, original, "miss preserves the whole cause by identity");
      } else {
        assert.notEqual(actual, original);
        assert.equal(actual.reasons.length, 1, "secondary reasons are discarded");
        const reason = actual.reasons[0];
        assert.ok(reason && Cause.isFailReason(reason));
        assert.equal(reason.error, first);
        if (mode === "guard raises") assert.equal(actual, guardFailure);
      }
    }
  }));
});

test("effect handler error selectors preserve deferred no-match causes", async () => {
  const original = Cause.fromReasons([fail(A), fail(B)]);
  const guarded = handle([], Effect.failCause(original), undefined,
    (_error, unhandled) => Effect.gen(function* () {
      yield* Effect.sleep(1);
      return yield* unhandled();
    }),
  );
  // oxlint-disable-next-line typescript/no-unsafe-type-assertion -- handle erases service requirements; this body and its guards require none
  const exit = await Effect.runPromiseExit(guarded as Effect.Effect<unknown, unknown>);
  assert.equal(causeOf(exit), original);
});
