import assert from "node:assert/strict";
import { fileURLToPath } from "node:url";
import { test } from "node:test";

import { measure } from "../index.mjs";

test("importing only Effect from the runtime keeps no Schema", async () => {
  const { modules } = await measure(fileURLToPath(new URL("effect-only.mjs", import.meta.url)));
  assert.ok(modules.some((id) => id.includes("effect/dist/Effect.js")));
  assert.deepEqual(modules.filter((id) => /Schema|runtime\/dist\/schema\//.test(id)), []);
});
