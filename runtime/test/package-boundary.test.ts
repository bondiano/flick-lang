import assert from "node:assert/strict";
import { readFileSync, readdirSync } from "node:fs";
import { test } from "node:test";

import runtime from "../package.json" with { type: "json" };

const source = new URL("../src/", import.meta.url);

const files = (dir: URL): Array<URL> => readdirSync(dir, { withFileTypes: true }).flatMap((entry) => {
  const at = new URL(entry.name + (entry.isDirectory() ? "/" : ""), dir);
  return entry.isDirectory() ? files(at) : entry.name.endsWith(".ts") ? [at] : [];
});

test("the Flick runtime package has no Effect dependency or imports", () => {
  assert.equal(Reflect.get(runtime, "dependencies"), undefined);
  for (const file of files(source)) {
    const text = readFileSync(file, "utf8");
    assert.doesNotMatch(text, /\bfrom\s+["'](?:effect(?:\/|["'])|@effect\/)/, file.pathname);
  }
});
