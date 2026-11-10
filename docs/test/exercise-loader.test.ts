import assert from "node:assert/strict";
import { mkdirSync } from "node:fs";
import path from "node:path";
import { test } from "node:test";
import { fileURLToPath, pathToFileURL } from "node:url";
import { rolldown } from "rolldown";

const repo = fileURLToPath(new URL("../../", import.meta.url));
test("exercise files survive bundling into a prerender server chunk", async () => {
  const dir = path.join(repo, "target/tmp/exercise-prerender");
  mkdirSync(dir, { recursive: true });
  const file = path.join(dir, "loader.mjs");
  const bundle = await rolldown({ input: fileURLToPath(new URL("../exercises/load.ts", import.meta.url)), platform: "node", external: (id) => id.startsWith("node:") });
  try { await bundle.write({ file, format: "esm" }); } finally { await bundle.close(); }
  const { loadExercise } = await import(pathToFileURL(file).href);
  assert.equal(loadExercise("signatures/02").check.output, "6");
  assert.throws(() => loadExercise("../other"), /Invalid exercise ID/);
});
