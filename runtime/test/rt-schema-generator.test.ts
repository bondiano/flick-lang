import assert from "node:assert/strict";
import { test } from "node:test";

import { schemaArbitrary } from "../src/rt/effect/Arbitrary.ts";
import { schemaWithGenerator } from "../src/rt/schema_gen.ts";
import { schemaList, schemaNumber } from "../src/rt/schema.ts";
import { codecJsonSchema } from "../src/rt/schema_export.ts";

test("a generator decorates only its witness and preserves the codec and nested schema", () => {
  const decorated = schemaWithGenerator(() => 4, schemaNumber);
  assert.notEqual(decorated, schemaNumber);
  assert.deepEqual(codecJsonSchema("Number", decorated), codecJsonSchema("Number", schemaNumber));
  assert.equal(schemaArbitrary(decorated)(() => 0.5), 4);
  assert.notEqual(schemaArbitrary(schemaNumber)(() => 0), 4);
  assert.deepEqual(schemaArbitrary(schemaList(decorated))(() => 0.5), [4, 4]);
});
