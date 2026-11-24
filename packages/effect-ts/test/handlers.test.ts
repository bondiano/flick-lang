/* oxlint-disable typescript/no-unsafe-type-assertion -- the tested erased handler ABI is supplied all of its service requirements here */
import assert from "node:assert/strict";
import { test } from "node:test";
import { Cause, type Context, Effect, Exit, Fiber } from "effect";
import { HandlerSignal, tryWith } from "../src/cause.ts";
import { parallelResults } from "../src/concurrency.ts";
import { handle } from "../src/handlers.ts";
import { operation, service } from "../src/effects.ts";
import { none } from "../src/data.ts";
import { type LazyCell, type LocalLazyCell, lazyForce, lazyMake, lazyMakeLocal } from "../src/effect/Lazy.ts";
import { flickEnsuring, flickEventually, flickFirstSuccessOf } from "../src/stdlib/effect-core.ts";
import {
  flickCatchDefect,
  flickIgnore,
  flickIgnoreCause,
  flickOnError,
  flickOnErrorIf,
  flickOnExit,
  flickOrDie,
  flickTapCause,
  flickTapCauseIf,
  flickTapDefect,
} from "../src/stdlib/effect-errors.ts";
import { exitFailure, failureInspect } from "../src/cause.ts";

const pair = service("Test.PartialPair");
const pairTag = pair as unknown as Context.Service<object, object>;
const left = operation(pair, "left", 1) as (u: undefined) => Effect.Effect<number, unknown, unknown>;
const right = operation(pair, "right", 1) as (u: undefined) => Effect.Effect<number, unknown, unknown>;
const body = Effect.gen(function* () {
  return (yield* left(undefined)) + (yield* right(undefined));
});
const run = (effect: Effect.Effect<unknown, unknown, unknown>) =>
  Effect.runPromiseExit(effect as Effect.Effect<unknown, unknown>);

test("partial handlers forward unmentioned operations through nested handlers", async () => {
  const nested = handle(
    [[pairTag, { left: () => Effect.succeed(10), right: () => Effect.succeed(20) }, []]],
    handle(
      [[pairTag, { left: () => Effect.succeed(1) }, []]],
      handle([[pairTag, { right: () => Effect.succeed(2) }, []]], body, undefined, undefined),
      undefined, undefined,
    ),
    undefined, undefined,
  );
  assert.deepEqual(await run(nested), Exit.succeed(3));
});

test("a complete provider shadows outer operations even when its implementation is malformed", async () => {
  let called = false;
  const nested = handle(
    [[pairTag, { right: () => { called = true; return Effect.succeed(20); } }, []]],
    Effect.provideService(
      handle([[pairTag, { left: () => Effect.succeed(1) }, []]], right(undefined), undefined, undefined),
      pair,
      {},
    ),
    undefined, undefined,
  );
  assert.ok(Exit.isFailure(await run(nested)));
  assert.equal(called, false);
});

test("forwarded tail arms retain outer evidence and bypass inner error selectors", async () => {
  const ask = service("Test.ForwardedEvidence");
  const askTag = ask as unknown as Context.Service<object, object>;
  const read = operation(ask, "read", 1) as (u: undefined) => Effect.Effect<string, unknown, unknown>;
  let caughtInside = false;
  const failing = handle(
    [[askTag, { read: () => Effect.succeed("outer") }, []]],
    handle(
      [[pairTag, { right: () => Effect.flatMap(read(undefined), Effect.fail) }, []]],
      handle(
        [[pairTag, { left: () => Effect.succeed(1) }, []], [askTag, { read: () => Effect.succeed("inner") }, []]],
        right(undefined),
        undefined,
        () => { caughtInside = true; return Effect.succeed("wrong"); },
      ),
      undefined, undefined,
    ),
    undefined, undefined,
  );
  const exit = await run(Effect.catch(failing, (error) => Effect.succeed(error)));
  assert.deepEqual(exit, Exit.succeed("outer"));
  assert.equal(caughtInside, false);
});

test("forwarded dictionaries and curried calls select the provider at execution", async () => {
  const curried = operation(pair, "right", 2, [20]) as (a: number) => (b: number) => Effect.Effect<number, unknown, unknown>;
  const staged = curried(2);
  const nested = handle(
    [[pairTag, { right: (dict: number) => (a: number) => (b: number) => Effect.succeed(dict + a + b) }, []]],
    handle([[pairTag, { left: () => Effect.succeed(1) }, []]], staged(3), undefined, undefined),
    undefined, undefined,
  );
  assert.deepEqual(await run(nested), Exit.succeed(25));
});

test("forwarded aborts bypass partial return and error arms", async () => {
  const stop = operation(pair, "stop", 1) as (u: undefined) => Effect.Effect<number, unknown, unknown>;
  const seen: string[] = [];
  const nested = handle(
    [[pairTag, { stop: () => Effect.sync(() => { seen.push("arm"); return 7; }) }, ["stop"]]],
    handle(
      [[pairTag, { left: () => Effect.succeed(1) }, []]],
      Effect.ensuring(stop(undefined), Effect.sync(() => seen.push("closed"))),
      () => Effect.sync(() => { seen.push("return"); return -1; }),
      () => Effect.sync(() => { seen.push("error"); return -1; }),
    ),
    undefined, undefined,
  );
  assert.deepEqual(await run(nested), Exit.succeed(7));
  assert.deepEqual(seen, ["closed", "arm"]);
});

test("prototype-looking operation names are own arms and forward normally", async () => {
  const tag = service("Test.PrototypeOperations");
  const install = tag as unknown as Context.Service<object, object>;
  const names = ["__proto__", "constructor", "toString"];
  const arms = Object.fromEntries(names.map((name, index) => [name, () => Effect.succeed((index + 1) * 10)]));
  const request = Effect.gen(function* () {
    const values = [];
    for (const name of names) {
      const call = operation(tag, name, 1) as (u: undefined) => Effect.Effect<number, unknown, unknown>;
      values.push(yield* call(undefined));
    }
    return values;
  });
  const nested = handle(
    [[install, arms, []]],
    handle([[install, Object.fromEntries([["__proto__", () => Effect.succeed(1)]]), []]], request, undefined, undefined),
    undefined, undefined,
  );
  assert.deepEqual(await run(nested), Exit.succeed([1, 20, 30]));
});

test("an abort operation from a child fiber dies instead of running the arm", async () => {
  let armRan = false;
  const stop = operation(pair, "stop", 1) as (u: undefined) => Effect.Effect<never, unknown, unknown>;
  const program = handle(
    [[pairTag, { stop: () => Effect.sync(() => { armRan = true; return "wrong"; }) }, ["stop"]]],
    Effect.flatMap(Effect.forkChild(stop(undefined)), Fiber.await),
    undefined, undefined,
  );
  const outer = await run(program);
  assert.ok(Exit.isSuccess(outer));
  const childExit = outer.value as Exit.Exit<unknown, unknown>;
  assert.ok(Exit.isFailure(childExit));
  assert.ok(Cause.hasDies(childExit.cause));
  assert.equal(childExit.cause.reasons.length, 1);
  assert.match(Cause.pretty(childExit.cause), /other than its handler/);
  assert.equal(armRan, false);
});

test("ensuring with die and abort propagates only the defect without leaking HandlerSignal", async () => {
  let armRan = false;
  const stop = operation(pair, "stop", 1) as (u: undefined) => Effect.Effect<never, never, unknown>;
  const cases = [
    ["flickEnsuring body=die cleanup=abort", flickEnsuring(stop(undefined))(Effect.die("boom"))],
    ["flickEnsuring body=abort cleanup=die", flickEnsuring(Effect.die("boom"))(stop(undefined))],
    ["raw Effect.ensuring body=die cleanup=abort", Effect.ensuring(Effect.die("boom"), stop(undefined))],
  ] as const;
  const exits = await Promise.all(
    cases.map(([, expr]) =>
      run(
        handle(
          [[pairTag, { stop: () => Effect.sync(() => { armRan = true; return "swallowed"; }) }, ["stop"]]],
          expr,
          undefined, undefined,
        ),
      ),
    ),
  );
  for (const [index, exit] of exits.entries()) {
    const label = cases[index]?.[0];
    assert.ok(Exit.isFailure(exit), label);
    assert.equal(exit.cause.reasons.length, 1, label);
    const reason = exit.cause.reasons[0];
    assert.ok(reason !== undefined && Cause.isDieReason(reason), label);
    assert.equal(reason.defect, "boom", label);
  }
  assert.equal(armRan, false);

  const okAbort = await run(
    handle(
      [[pairTag, { stop: () => Effect.succeed("cleanup-aborted") }, ["stop"]]],
      flickEnsuring(stop(undefined))(Effect.succeed("ok")),
      undefined, undefined,
    ),
  );
  assert.deepEqual(okAbort, Exit.succeed("cleanup-aborted"));
});

test("parallelResults does not wrap a HandlerSignal as an Err", async () => {
  const token = {};
  const signal = new HandlerSignal(token, Effect.succeed("arm"));
  const exit = await run(parallelResults([Effect.fail(signal)]));
  assert.ok(Exit.isFailure(exit));
  assert.equal(exit.cause.reasons.length, 1);
  const reason = exit.cause.reasons[0];
  assert.ok(reason !== undefined && Cause.isFailReason(reason));
  assert.equal(reason.error, signal);
});

test("firstSuccessOf and eventually propagate abort instead of swallowing it", async () => {
  const stop = operation(pair, "stop", 1) as (u: undefined) => Effect.Effect<never, unknown, unknown>;
  let secondRan = false;
  const first = await run(
    handle(
      [[pairTag, { stop: () => Effect.succeed("aborted") }, ["stop"]]],
      flickFirstSuccessOf([stop(undefined), Effect.sync(() => { secondRan = true; return "second"; })]),
      undefined, undefined,
    ),
  );
  assert.deepEqual(first, Exit.succeed("aborted"));
  assert.equal(secondRan, false);

  let attempts = 0;
  const ev = await run(
    handle(
      [[pairTag, { stop: () => Effect.succeed("stopped") }, ["stop"]]],
      flickEventually(Effect.suspend(() => {
        attempts += 1;
        return attempts === 1 ? Effect.fail("retry") : stop(undefined);
      })),
      undefined, undefined,
    ),
  );
  assert.deepEqual(ev, Exit.succeed("stopped"));
  assert.equal(attempts, 2);
});

const stopOp = operation(pair, "stop", 1) as (u: string) => Effect.Effect<never, unknown, unknown>;
const withStopTag = (inner: Effect.Effect<unknown, unknown, unknown>) =>
  handle([[pairTag, { stop: (s: string) => Effect.succeed(`stopped:${s}`) }, ["stop"]]], inner, undefined, undefined);

test("orDie, ignore, tap/catch defect, tapCause, onError, onExit handle abort and Die(Signal)", async () => {
  // orDie lets pure abort unwind cleanly (never strips to empty).
  assert.deepEqual(await run(withStopTag(flickOrDie(stopOp("halt")))), Exit.succeed("stopped:halt"));

  // ignore_ and ignore_cause let pure abort unwind cleanly.
  assert.deepEqual(await run(withStopTag(flickIgnore(none)(stopOp("ig")))), Exit.succeed("stopped:ig"));
  assert.deepEqual(await run(withStopTag(flickIgnoreCause(none)(stopOp("igc")))), Exit.succeed("stopped:igc"));

  // flickCatchDefect and flickTapDefect ignore Die(HandlerSignal) and see the real defect.
  const sig = new HandlerSignal({}, Effect.succeed("arm"));
  const dieSigAndBoom = Cause.combine(Cause.die(sig), Cause.die("boom"));
  const caught = await run(flickCatchDefect((d) => Effect.succeed(`caught:${d}`))(Effect.failCause(dieSigAndBoom)));
  assert.deepEqual(caught, Exit.succeed("caught:boom"));

  const tapped: Array<string> = [];
  const tappedExit = await run(
    flickTapDefect((d) => Effect.sync(() => { tapped.push(d); }))(Effect.failCause(dieSigAndBoom)),
  );
  assert.deepEqual(tapped, ["boom"]);
  assert.ok(Exit.isFailure(tappedExit) && tappedExit.cause.reasons.length === 1);

  // tryWith on lone [Die(HandlerSignal)] or [Die(HandlerSignal), Fail("err")] never strips to empty or elects Fail.
  const loneDieSig = await run(tryWith(Effect.failCause(Cause.die(sig)), () => undefined));
  assert.ok(Exit.isFailure(loneDieSig) && loneDieSig.cause.reasons.length === 1);
  const dieSigAndFail = await run(
    tryWith(Effect.failCause(Cause.combine(Cause.die(sig), Cause.fail("err"))), (e) => Effect.succeed(`rec:${e}`)),
  );
  assert.deepEqual(dieSigAndFail, Exit.succeed("rec:err"));

  // tapCause / tapCauseIf / onError / onErrorIf skip pure abort.
  let ran = false;
  assert.deepEqual(
    await run(withStopTag(flickTapCause(() => Effect.sync(() => { ran = true; }))(stopOp("tc")))),
    Exit.succeed("stopped:tc"),
  );
  assert.deepEqual(
    await run(withStopTag(flickTapCauseIf(() => true, () => Effect.sync(() => { ran = true; }))(stopOp("tci")))),
    Exit.succeed("stopped:tci"),
  );
  assert.deepEqual(
    await run(withStopTag(flickOnError(() => Effect.sync(() => { ran = true; }))(stopOp("oe")))),
    Exit.succeed("stopped:oe"),
  );
  assert.deepEqual(
    await run(withStopTag(flickOnErrorIf(() => true, () => Effect.sync(() => { ran = true; }))(stopOp("oei")))),
    Exit.succeed("stopped:oei"),
  );
  assert.equal(ran, false);

  // onExit sees stripped Exit on Die + Signal.
  let exitReasons = -1;
  const onExitOut = await run(
    withStopTag(
      flickOnExit((ex) =>
        Effect.gen(function* () {
          const f = exitFailure(ex);
          if (f._tag === "Some") {
            const rep = yield* failureInspect(f.value);
            exitReasons = rep.reasons.length;
          }
        }),
      )(Effect.failCause(Cause.combine(Cause.fail(sig), Cause.die("fin-boom")))),
    ),
  );
  assert.equal(exitReasons, 1);
  assert.ok(Exit.isFailure(onExitOut) && onExitOut.cause.reasons.length === 1);
});

const checkLazyR9 = async (
  make: <A, E, R>(task: Effect.Effect<A, E, R>) => Effect.Effect<LazyCell<A, E, R> | LocalLazyCell<A, E, R>>,
) => {
  let runs1 = 0;
  const cell1 = await Effect.runPromise(
    make(
      Effect.suspend(() => {
        runs1 += 1;
        return runs1 === 1
          ? Effect.failCause(Cause.combine(Cause.interrupt(), Cause.die("int-die")))
          : Effect.succeed(42);
      }),
    ),
  );
  const f1 = await run(lazyForce(cell1));
  assert.ok(Exit.isFailure(f1));
  const f2 = await run(lazyForce(cell1));
  assert.deepEqual(f2, Exit.succeed(42));
  assert.equal(runs1, 2);

  let runs2 = 0;
  const cell2 = await Effect.runPromise(
    make(
      Effect.suspend(() => {
        runs2 += 1;
        return flickEnsuring(Effect.die("fin-boom"))(stopOp("body-abort"));
      }),
    ),
  );
  const g1 = await run(withStopTag(lazyForce(cell2)));
  const g2 = await run(withStopTag(lazyForce(cell2)));
  assert.equal(runs2, 1);
  const r1 = Exit.isFailure(g1) ? g1.cause.reasons : [];
  const r2 = Exit.isFailure(g2) ? g2.cause.reasons : [];
  assert.ok(r1.length === 1 && r1[0] !== undefined && Cause.isDieReason(r1[0]));
  assert.ok(r2.length === 1 && r2[0] !== undefined && Cause.isDieReason(r2[0]));
};

test("lazyForce follows R9 on Interrupt+Die and strips+caches Die+HandlerSignal", async () => {
  await Promise.all([checkLazyR9(lazyMake), checkLazyR9(lazyMakeLocal)]);
});
