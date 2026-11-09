/* oxlint-disable require-yield, unicorn/consistent-function-scoping, typescript/no-unsafe-type-assertion -- generators that return a call are what the compiler emits */
// Layers on the kernel of `effect = "rt"`: a merged layer builds and releases
// its sides side by side, and the services of a layer are handlers.
import assert from "node:assert/strict";
import { test } from "node:test";

import * as Deferred from "../src/rt/effect/Deferred.ts";
import { effect } from "../src/rt/effect/Layer.ts";
import { type Layer, layerMerge, layerRun, layerSucceed } from "../src/rt/layer.ts";
import { type OpInstall, type Run, addFinalizer, handle, operation, runIn, service } from "../src/rt/kernel.ts";
import { root } from "./rt-support.ts";

const tail = (name: string, arm: unknown): OpInstall => [name, arm, "tail", 0, 1];

test("a merged layer builds its sides side by side and releases them so, the last acquired first", { timeout: 5000 }, async () => {
  // each side waits for the other: built or released one after the other, they would wait for ever
  const built = [Deferred.make(), Deferred.make()] as const;
  const released = [Deferred.make(), Deferred.make()] as const;
  const seen: Array<string> = [];
  const side = (i: 0 | 1, other: 0 | 1): Layer =>
    effect(service(`Side${i}`))(function* (): Run {
      yield addFinalizer(function* (): Run {
        seen.push(`release ${i} starts`);
        Deferred.succeed(undefined)(released[i]);
        yield Deferred.await(released[other]);
        seen.push(`released ${i}`);
        return undefined;
      });
      Deferred.succeed(undefined)(built[i]);
      yield Deferred.await(built[other]);
      return {};
    });
  const both = layerMerge(side(0, 1), side(1, 0));
  const exit = await root(() => layerRun(both, () => seen.push("body")));
  assert.equal(exit.cause, undefined);
  // as Effect and Go, the side acquired last starts its release first
  assert.deepEqual(seen.slice(0, 3), ["body", "release 1 starts", "release 0 starts"]);
  assert.deepEqual(seen.slice(3).toSorted(), ["released 0", "released 1"]);
});

test("an operation of a layer's service runs its own effects where the layer is provided", async () => {
  const Prefix = service("Prefix");
  const Greeter = service("Greeter");
  const prefixed = (prefix: string) => (body: () => unknown) => handle([[Prefix, [tail("prefix", () => prefix)]]], body, undefined, undefined);
  const prefix = operation(Prefix, "prefix", 1) as (u: undefined) => unknown;
  const greet = operation(Greeter, "greet", 1) as (name: string) => unknown;
  const impl = { greet: (name: string): string => `${String(runIn(prefix(undefined)))} ${name}` };
  const exit = await root(() =>
    prefixed("outer")(() => layerRun(layerSucceed(Greeter, impl), () => prefixed("inner")(() => greet("layer")))),
  );
  assert.equal(exit.value, "outer layer");
});
