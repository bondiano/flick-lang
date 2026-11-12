// Module replacement in development on Flick's own runtime: the labelled
// states of the mounted roots carry over to the next program's registry.
import assert from "node:assert/strict";
import { test } from "node:test";

import { label, seedStates, statesOf } from "../src/rt/atoms/atom.ts";
import { Registry, readable, state } from "../src/rt/atoms/registry.ts";
import { currentCtx } from "../src/rt/kernel.ts";

test("a labelled state carries its value to a new registry, and nothing else does", () => {
  const count = label("App.count", state(0));
  const derived = label("App.double", readable((r) => r.get(count) * 2));
  const before = new Registry(currentCtx());
  before.set(count, 5);
  assert.equal(before.get(derived), 10);
  const states = statesOf(before);
  assert.deepEqual([...states], [["App.count", 5]]);
  const after = new Registry(currentCtx());
  seedStates(after, states);
  assert.equal(after.get(count), 5);
  assert.equal(after.get(derived), 10);
  // a label the next module no longer has starts afresh
  seedStates(after, new Map([["App.gone", 1]]));
  assert.equal(after.get(count), 5);
});
