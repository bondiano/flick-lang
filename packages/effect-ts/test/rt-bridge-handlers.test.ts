import assert from "node:assert/strict";
import { test } from "node:test";
import { Cause, Context, Effect, Fiber, Option } from "effect";
import { some } from "@flick-lang/runtime/core/data";
import { runIn, currentCtx, extend, inCtx, type Run } from "@flick-lang/runtime/rt/fiber";
import { scopeRun } from "@flick-lang/runtime/rt/owner";
import { fail } from "@flick-lang/runtime/rt/cause";
import { service, handle as nativeHandle, operation as nativeOperation, tryWith as nativeTryWith, type OpInstall } from "@flick-lang/runtime/rt/handle";
import { flickExit, flickMatchCause } from "@flick-lang/runtime/rt/effect/Effect";
import { lazyForce, lazyMake } from "@flick-lang/runtime/rt/effect/Lazy";
import { handle as hostHandle } from "../src/handlers.ts";
import { operation as hostOperation, service as hostService } from "../src/effects.ts";
import { causeToEffect, causeFromEffect, fromEffect, handlersToEffect, callEffect } from "../src/bridge.ts";
import { flickEventually, flickFirstSuccessOf } from "../src/stdlib/effect-core.ts";


const nativeArm = (name: string, value: number): OpInstall =>
  [name, () => (function* (): Run { return yield value; })(), "tail", 0, 1];
const callNativeOperation = (operation: unknown): unknown => {
  assert.equal(typeof operation, "function");
  if (typeof operation !== "function") throw new TypeError("not an operation");
  return Reflect.apply(operation, undefined, [undefined]);
};

test("hidden handler records convert nested arguments and successful results", () => {
  const record = handlersToEffect({
    reply: (request: unknown) => {
      assert.ok(typeof request === "object" && request !== null);
      const maybe: unknown = Reflect.get(request, "maybe");
      assert.deepEqual(maybe, some("Ada"));
      assert.equal(Object.getPrototypeOf(maybe), Object.prototype);
      return { nested: [some("Ada")] };
    },
  });
  assert.ok(typeof record === "object" && record !== null);
  const reply: unknown = Reflect.get(record, "reply");
  assert.equal(typeof reply, "function");
  if (typeof reply !== "function") throw new TypeError("not a handler");
  const effect: unknown = Reflect.apply(reply, undefined, [{ maybe: Option.some("Ada") }]);
  const result: unknown = runIn(scopeRun(() => fromEffect(effect)));
  assert.ok(typeof result === "object" && result !== null);
  const nested: unknown = Reflect.get(result, "nested");
  assert.ok(Array.isArray(nested));
  const item: unknown = nested[0];
  assert.ok(Option.isOption(item) && Option.isSome(item));
  assert.equal(item.value, "Ada");
});

test("a native service exposed through Effect converts data on both sides of its operations", () => {
  const key = Context.Service<{ echo: (input: unknown) => Effect.Effect<unknown> }>("Test.NativeEcho");
  const tag = service("Test.NativeEcho");
  const implementation = {
    echo: (input: unknown) => {
      assert.deepEqual(input, some("Ada"));
      assert.equal(Object.getPrototypeOf(input), Object.prototype);
      return (function* (): Run { return yield { nested: [some("Ada")] }; })();
    },
  };
  const request = Effect.flatMap(Effect.service(key), (value) => value.echo(Option.some("Ada")));
  const result: unknown = runIn(scopeRun(() => inCtx(extend(currentCtx(), tag, implementation), () => fromEffect(request))));
  assert.ok(typeof result === "object" && result !== null);
  const nested: unknown = Reflect.get(result, "nested");
  assert.ok(Array.isArray(nested));
  const item: unknown = nested[0];
  assert.ok(Option.isOption(item) && Option.isSome(item));
  assert.equal(item.value, "Ada");
});

test("typed failures translate their nested data in both directions", () => {
  const native = fail({ nested: some("bad") });
  const host = causeToEffect(native);
  const hostError = Cause.findErrorOption(host);
  assert.ok(Option.isSome(hostError));
  const value = hostError.value;
  assert.ok(typeof value === "object" && value !== null);
  const nested: unknown = Reflect.get(value, "nested");
  assert.ok(Option.isOption(nested) && Option.isSome(nested));
  assert.equal(nested.value, "bad");
  const restored = causeFromEffect(host);
  const error = restored.reasons[0];
  assert.ok(error?._tag === "Fail");
  assert.deepEqual(error.error, { nested: some("bad") });
});

test("a native partial handler forwards unmentioned operations across an Effect crossing", () => {
  const tag = service("Test.PartialNativeBridge");
  const key = Context.Service<{ left: (u: undefined) => Effect.Effect<number>; right: (u: undefined) => Effect.Effect<number> }>("Test.PartialNativeBridge");
  const request = Effect.gen(function* () {
    const impl = yield* Effect.service(key);
    return (yield* impl.left(undefined)) + (yield* impl.right(undefined));
  });
  const result: unknown = runIn(scopeRun(() => nativeHandle(
    [[tag, [nativeArm("left", 10), nativeArm("right", 20)]]],
    () => nativeHandle([[tag, [nativeArm("left", 1)]]], () => fromEffect(request), undefined, undefined),
    undefined, undefined,
  )));
  assert.equal(result, 21);
});

test("an Effect partial handler forwards through native partial handlers without recapturing arms", async () => {
  const tag = service("Test.PartialEffectBridge");
  const key = Context.Service<object>("Test.PartialEffectBridge");
  const left: unknown = nativeOperation(tag, "left", 1);
  const right: unknown = nativeOperation(tag, "right", 1);
  const native = callEffect(() => nativeHandle(
    [[tag, [nativeArm("left", 1)]]],
    function* (): Run { return Number(yield callNativeOperation(left)) + Number(yield callNativeOperation(right)); },
    undefined, undefined,
  ));
  const request = hostHandle(
    [[key, { left: () => Effect.succeed(10), right: () => Effect.succeed(20) }, []]],
    hostHandle([[key, { left: () => Effect.succeed(2) }, []]], native, undefined, undefined),
    undefined, undefined,
  );
  // oxlint-disable-next-line typescript/no-unsafe-type-assertion -- this test supplies every required service via the erased handler ABI
  assert.equal(await Effect.runPromise(request as Effect.Effect<unknown, unknown>), 21);
});

test("a complete native provider remains a forwarding barrier at an Effect crossing", () => {
  const tag = service("Test.CompleteNativeBridge");
  const key = Context.Service<{ right: (u: undefined) => Effect.Effect<number> }>("Test.CompleteNativeBridge");
  let called = false;
  const request = Effect.flatMap(Effect.service(key), (impl) => impl.right(undefined));
  assert.throws(() => runIn(scopeRun(() => nativeHandle(
    [[tag, [["right", () => { called = true; return (function* (): Run { return yield 20; })(); }, "tail", 0, 1]]]],
    () => inCtx(extend(currentCtx(), tag, {}), () => nativeHandle(
      [[tag, [["left", () => 1, "tail", 0, 1]]]],
      () => fromEffect(request), undefined, undefined,
    )),
    undefined, undefined,
  ))));
  assert.equal(called, false);
});

test("same-fiber native abort unwinds through bridged firstSuccessOf and eventually", async () => {
  const tag = service("Test.NativeAbortBridge");
  const stop: unknown = nativeOperation(tag, "stop", 1);
  let secondRan = false;
  const first: unknown = runIn(scopeRun(() => nativeHandle(
    [[tag, [["stop", () => "aborted", "abort", 0, 1]]]],
    () => fromEffect(flickFirstSuccessOf([
      callEffect(() => callNativeOperation(stop)),
      callEffect(() => { secondRan = true; return "second"; }),
    ])),
    undefined, undefined,
  )));
  assert.equal(first, "aborted");
  assert.equal(secondRan, false);

  let attempts = 0;
  const ev: unknown = await Effect.runPromise(callEffect(() => scopeRun(() => nativeHandle(
    [[tag, [["stop", () => "stopped", "abort", 0, 1]]]],
    () => fromEffect(flickEventually(Effect.suspend(() => {
      attempts += 1;
      return attempts === 1 ? Effect.fail("retry") : callEffect(() => callNativeOperation(stop));
    }))),
    undefined, undefined,
  ))));
  assert.equal(ev, "stopped");
  assert.equal(attempts, 2);
});

test("cross-fiber native abort through Effect.forkChild dies instead of unwinding", async () => {
  const tag = service("Test.CrossFiberNativeAbortBridge");
  const stop: unknown = nativeOperation(tag, "stop", 1);
  let armRan = false;
  const program = callEffect(() => nativeHandle(
    [[tag, [["stop", () => { armRan = true; return "wrong"; }, "abort", 0, 1]]]],
    () => fromEffect(Effect.flatMap(Effect.forkChild(callEffect(() => callNativeOperation(stop))), Fiber.join)),
    undefined, undefined,
  ));
  const exit = await Effect.runPromiseExit(program);
  assert.equal(exit._tag, "Failure");
  assert.ok(Cause.hasDies(exit.cause));
  assert.match(Cause.pretty(exit.cause), /other than its handler/);
  assert.equal(armRan, false);
});

test("bridged Effect.ensuring Die(Signal) is stripped cleanly in native handle, flickExit, tryWith, matchCause, and lazyForce", async () => {
  const tag = service("Test.BridgedDieSignal");
  const stop: unknown = nativeOperation(tag, "stop", 1);
  let armRan = false;
  const dieAndAbort = () =>
    fromEffect(Effect.ensuring(Effect.die("boom"), Effect.orDie(callEffect(() => callNativeOperation(stop)))));

  const direct = await Effect.runPromiseExit(callEffect(() => scopeRun(() => nativeHandle(
    [[tag, [["stop", () => { armRan = true; return "swallowed"; }, "abort", 0, 1]]]],
    dieAndAbort,
    undefined, undefined,
  ))));
  assert.equal(direct._tag, "Failure");
  assert.equal(direct.cause.reasons.length, 1);
  const directReason = direct.cause.reasons[0];
  assert.ok(directReason !== undefined && Cause.isDieReason(directReason));
  assert.equal(directReason.defect, "boom");

  const viaExit = await Effect.runPromise(callEffect(() => scopeRun(() => nativeHandle(
    [[tag, [["stop", () => { armRan = true; return "swallowed"; }, "abort", 0, 1]]]],
    () => flickExit(dieAndAbort),
    undefined, undefined,
  ))));
  // oxlint-disable-next-line typescript/no-unsafe-type-assertion -- flickExit returns a native Exit
  const nativeExit = viaExit as { cause?: { reasons: ReadonlyArray<{ _tag: string; defect?: unknown }> } };
  assert.deepEqual(nativeExit.cause?.reasons.map((r) => [r._tag, r.defect]), [["Die", "boom"]]);

  const viaTry = await Effect.runPromiseExit(callEffect(() => scopeRun(() => nativeHandle(
    [[tag, [["stop", () => { armRan = true; return "swallowed"; }, "abort", 0, 1]]]],
    () => nativeTryWith(dieAndAbort, () => () => "caught"),
    undefined, undefined,
  ))));
  assert.equal(viaTry._tag, "Failure");
  assert.equal(viaTry.cause.reasons.length, 1);

  const viaMatch = await Effect.runPromise(callEffect(() => scopeRun(() => nativeHandle(
    [[tag, [["stop", () => { armRan = true; return "swallowed"; }, "abort", 0, 1]]]],
    () => flickMatchCause((failure) => failure.cause.reasons.map((r) => [r._tag, r._tag === "Die" ? r.defect : undefined]), () => [])(dieAndAbort),
    undefined, undefined,
  ))));
  assert.deepEqual(viaMatch, [["Die", "boom"]]);

  const cell = lazyMake(dieAndAbort);
  const viaLazy = await Effect.runPromiseExit(callEffect(() => scopeRun(() => nativeHandle(
    [[tag, [["stop", () => { armRan = true; return "swallowed"; }, "abort", 0, 1]]]],
    () => lazyForce(cell),
    undefined, undefined,
  ))));
  assert.equal(viaLazy._tag, "Failure");
  assert.equal(viaLazy.cause.reasons.length, 1);
  assert.equal(armRan, false);
});

test("reverse bridge: hostHandle abort unwinds through native flickExit, tryWith, flickMatchCause, and lazyForce", async () => {
  const hostTag = hostService("Test.ReverseBridgeHostAbort");
  // oxlint-disable-next-line typescript/no-unsafe-type-assertion -- erased handler ABI in test
  const install = hostTag as unknown as Context.Service<object, object>;
  // oxlint-disable-next-line typescript/no-unsafe-type-assertion -- erased operation ABI in test
  const stop = hostOperation(hostTag, "stop", 1) as (u: undefined) => Effect.Effect<never, never, unknown>;

  const runUnderHost = (nativeBody: () => unknown) => {
    let armRan = false;
    const effect = hostHandle(
      [[install, { stop: () => Effect.sync(() => { armRan = true; return "host-aborted"; }) }, ["stop"]]],
      callEffect(nativeBody),
      undefined, undefined,
    );
    // oxlint-disable-next-line typescript/no-unsafe-type-assertion -- hostHandle satisfies the service
    return Effect.runPromise(effect as Effect.Effect<unknown, unknown>).then((value) => ({ value, armRan }));
  };

  const viaExit = await runUnderHost(() => flickExit(() => fromEffect(stop(undefined))));
  assert.deepEqual(viaExit, { value: "host-aborted", armRan: true });

  const viaTry = await runUnderHost(() => nativeTryWith(() => fromEffect(stop(undefined)), () => () => "swallowed"));
  assert.deepEqual(viaTry, { value: "host-aborted", armRan: true });

  const viaMatch = await runUnderHost(() => flickMatchCause(() => "swallowed", () => "ok")(() => fromEffect(stop(undefined))));
  assert.deepEqual(viaMatch, { value: "host-aborted", armRan: true });

  const cell = lazyMake(() => fromEffect(stop(undefined)));
  const viaLazy = await runUnderHost(() => lazyForce(cell));
  assert.deepEqual(viaLazy, { value: "host-aborted", armRan: true });
  assert.equal(cell.exit, undefined);
});
