import assert from "node:assert/strict";
import { test } from "node:test";

import { Effect } from "effect";

import { listFoldEffect } from "../src/index.ts";

test("fold_effect starts from its seed, whatever its type", async () => {
  const sum = await Effect.runPromise(listFoldEffect((acc: number, x: number) => Effect.succeed(acc + x), 10, [1, 2, 3]));
  assert.equal(sum, 16);
  const empty = await Effect.runPromise(listFoldEffect((acc: string, x: string) => Effect.succeed(acc + x), "seed", []));
  assert.equal(empty, "seed");
});
