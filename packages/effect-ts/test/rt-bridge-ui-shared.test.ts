import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";
import * as native from "@flick-lang/runtime/rt/ui";
import * as adapter from "../src/ui/index.ts";

test("inert constructors selected without crossing are identical in both UI families", () => {
  const names = readFileSync(new URL("../../../flick-core/src/build/shared_ui_exports.txt", import.meta.url), "utf8").trim().split("\n");
  for (const name of names) {
    const value: unknown = Reflect.get(native, name);
    assert.notEqual(value, undefined, name);
    assert.equal(value, Reflect.get(adapter, name), name);
  }
});
