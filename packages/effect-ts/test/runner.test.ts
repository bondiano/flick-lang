import assert from "node:assert/strict";
import { test } from "node:test";

import { Effect, Option } from "effect";

import { applyRuntime, service } from "../src/effects.ts";
import { start } from "../src/entry.ts";
import { run } from "../src/runner/browser.ts";
import { provideNodeServices } from "../src/runner/host.ts";

const callOp = (key: string, op: string, args: ReadonlyArray<unknown>): Effect.Effect<unknown, unknown, unknown> =>
  Effect.flatMap(Effect.service(service(key)), (impl) => applyRuntime(Reflect.get(impl, op), args));

test("the browser runner gives main its input, whether it is pure or an effect", async () => {
  const seen: Array<unknown> = [];
  await run((input: unknown) => {
    seen.push(input);
  }, "pure");
  await run((input: unknown) => Effect.sync(() => seen.push(input)), { id: "root" });
  await run(() => {
    seen.push("no input");
  });
  assert.deepEqual(seen, ["pure", { id: "root" }, "no input"]);
});

test("the browser runner refuses what is not a function", async () => {
  await assert.rejects(run({ main: true }), TypeError);
});

test("the browser runner provides Host.Clock and Host.Random", async () => {
  const seen: Array<unknown> = [];
  await run(() =>
    Effect.gen(function* () {
      seen.push(yield* callOp("Host.Clock", "current_time_millis", [undefined]));
      seen.push(yield* callOp("Host.Random", "next", [undefined]));
    }),
  );
  assert.equal(seen.length, 2);
  const [millisValue, randomValue] = seen;
  assert.ok(typeof millisValue === "number" && Number.isFinite(millisValue) && millisValue > 0);
  assert.ok(typeof randomValue === "number" && randomValue >= 0 && randomValue < 1);
});

test("the node runner host layer provides Host.Environment and Host.Arguments", async () => {
  let envResult: unknown;
  let argsResult: unknown;
  const fiber = start(
    () =>
      Effect.gen(function* () {
        envResult = yield* callOp("Host.Environment", "env", ["PATH"]);
        argsResult = yield* callOp("Host.Arguments", "args", [undefined]);
      }),
    undefined,
    provideNodeServices,
  );
  await new Promise((resolve) => fiber.addObserver(resolve));
  assert.ok(Option.isOption(envResult) && Option.isSome(envResult));
  assert.ok(Array.isArray(argsResult));
});
