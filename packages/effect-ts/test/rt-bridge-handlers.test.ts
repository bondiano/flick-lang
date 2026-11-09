import assert from "node:assert/strict";
import { test } from "node:test";
import { Cause, Context, Effect, Option } from "effect";
import { some } from "@flick-lang/runtime/core/data";
import { runIn, currentCtx, extend, inCtx, type Run } from "@flick-lang/runtime/rt/fiber";
import { scopeRun } from "@flick-lang/runtime/rt/owner";
import { fail } from "@flick-lang/runtime/rt/cause";
import { service } from "@flick-lang/runtime/rt/handle";
import { causeToEffect, causeFromEffect, fromEffect, handlersToEffect } from "../src/bridge.ts";

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
      return (function* (): Run { return { nested: [some("Ada")] }; })();
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
