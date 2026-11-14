/* oxlint-disable typescript/no-unsafe-type-assertion -- the tested erased handler ABI is supplied all of its service requirements here */
import assert from "node:assert/strict";
import { test } from "node:test";
import { type Context, Effect, Exit } from "effect";
import { handle } from "../src/handlers.ts";
import { operation, service } from "../src/effects.ts";

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
