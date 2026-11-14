import assert from "node:assert/strict";
import { test } from "node:test";
import { Cause, Context, Effect, Option } from "effect";
import { some } from "@flick-lang/runtime/core/data";
import { runIn, currentCtx, extend, inCtx, type Run } from "@flick-lang/runtime/rt/fiber";
import { scopeRun } from "@flick-lang/runtime/rt/owner";
import { fail } from "@flick-lang/runtime/rt/cause";
import { service, handle as nativeHandle, operation as nativeOperation, type OpInstall } from "@flick-lang/runtime/rt/handle";
import { handle as hostHandle } from "../src/handlers.ts";
import { causeToEffect, causeFromEffect, fromEffect, handlersToEffect, callEffect } from "../src/bridge.ts";


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
