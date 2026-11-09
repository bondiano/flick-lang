import assert from "node:assert/strict";
import { test } from "node:test";
import * as Atom from "effect/reactivity/Atom";
import * as Registry from "../src/effect/reactivity/AtomRegistry.ts";

test("adapter registry observes, updates and releases deep atom chains without stack recursion", () => {
  const registry = Registry.make();
  registry.onNodeAdded = Registry.prepareNode;
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

test("shared ancestor observations survive overlapping subscriptions and their removal", () => {
  const registry = Registry.make();
  registry.onNodeAdded = Registry.prepareNode;
  const state = Atom.make(0);
  const left = Atom.keepAlive(Atom.map(state, (value) => value + 1));
  const right = Atom.keepAlive(Atom.map(state, (value) => value + 2));
  const sum = Atom.keepAlive(Atom.make((get) => get(left) + get(right)));
  const heard: number[] = [];
  const direct: number[] = [];
  const stop = registry.subscribe(sum, (value) => heard.push(value));
  const stopDirect = registry.subscribe(right, (value) => direct.push(value));
  try {
    registry.set(state, 1);
    assert.deepEqual(heard, [5]);
    assert.deepEqual(direct, [3]);
    stop();
    registry.set(state, 2);
    assert.deepEqual(heard, [5]);
    assert.deepEqual(direct, [3, 4]);
    assert.equal(registry.get(sum), 7);
    stopDirect();
    registry.set(state, 3);
    assert.deepEqual(direct, [3, 4]);
    assert.equal(registry.get(sum), 9);
  } finally {
    registry.dispose();
  }
});
