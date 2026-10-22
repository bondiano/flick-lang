import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";

import * as runtime from "../src/index.js";

test("index.d.ts declares every export", () => {
  const types = readFileSync(new URL("../src/index.d.ts", import.meta.url), "utf8");
  const declared = [...types.matchAll(/export declare const (\w+)/g)].map((m) => m[1]);
  const exported = Object.keys(runtime).filter((k) => k !== "Effect");
  assert.deepEqual(declared.sort(), exported.sort());
});
