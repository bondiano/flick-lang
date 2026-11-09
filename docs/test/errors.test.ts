import { strict as assert } from "node:assert";
import { test } from "node:test";
import { expectedPages, problems } from "../scripts/errors.mjs";

test("every registered compiler error has current English and Russian pages", () => {
  assert.ok(expectedPages().size > 400);
  assert.deepEqual(problems(), []);
});
