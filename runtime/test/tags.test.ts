import assert from "node:assert/strict";
import { test } from "node:test";

import { constructorName, tag } from "../src/tags.ts";

test("tags are the ones the compiler writes", () => {
  // the compiler's tag of this id, from its interface snapshot
  assert.equal(tag("@acme/shapes/App.Shapes.Shape.Circle"), "Circle#7c57f5571734");
  assert.notEqual(tag("App.A.T.Some"), tag("App.B.T.Some"));
  assert.equal(constructorName("Circle#7c57f5571734"), "Circle");
  assert.equal(constructorName("Some"), "Some");
});
