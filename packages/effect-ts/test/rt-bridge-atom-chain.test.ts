import assert from "node:assert/strict";
import { test } from "node:test";
import * as Atom from "effect/reactivity/Atom";
import * as Registry from "../src/effect/reactivity/AtomRegistry.ts";

test("adapter registry observes, updates and releases deep atom chains without stack recursion", () => {
  const registry = Registry.make();
  const state = Atom.make(0);
  let last: Atom.Atom<number> = state;
  try {
    for (let i = 0; i < 6000; i++) {
      const parent = last;
      last = Atom.keepAlive(Atom.make((get) => get(parent) + 1));
      assert.equal(registry.get(last), i + 1);
    }
    const heard: number[] = [];
    const stop = registry.subscribe(last, (value) => heard.push(value));
    registry.set(state, 1);
    assert.equal(registry.get(last), 6001);
    assert.deepEqual(heard, [6001]);
    stop();
    registry.set(state, 2);
    assert.equal(registry.get(last), 6002);
    assert.deepEqual(heard, [6001]);
  } finally {
    registry.dispose();
  }
});
