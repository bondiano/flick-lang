import assert from "node:assert/strict";
import { test } from "node:test";

import * as Atom from "effect/reactivity/Atom";
import * as AtomRegistry from "effect/reactivity/AtomRegistry";
import { label } from "@flick-lang/runtime/rt/atoms/atom";

test("RT debug labels preserve Effect atoms' prototype and writable behavior", () => {
  const state = label("App.counter", Atom.make(1));
  assert.equal(Atom.isAtom(state), true);
  assert.equal(typeof state.equals, "function");
  const registry = AtomRegistry.make();
  try {
    assert.equal(registry.get(state), 1);
    registry.set(state, 2);
    assert.equal(registry.get(state), 2);
  } finally {
    registry.dispose();
  }
});
