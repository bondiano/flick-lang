/* oxlint-disable typescript/no-unsafe-type-assertion -- the values of atoms are what they were made with */
// The atom registry of Flick's own runtime: a write reaches thousands of
// dependents and subscribers through a loop, so neither a long chain nor a
// wide fan-out overflows the stack, which in a fiber would hang the program.
import assert from "node:assert/strict";
import { test } from "node:test";

import { type Atom, Registry, readable, state, taskAtom } from "../src/rt/atoms/registry.ts";
import { currentCtx } from "../src/rt/kernel.ts";
import * as Deferred from "../src/rt/effect/Deferred.ts";

const LONG = 20_000;

/** `LONG` derived atoms, each one more than the one before, from `head`. */
const chainFrom = (registry: Registry, head: Atom<number>): Atom<number> => {
  let last = head;
  for (let i = 0; i < LONG; i++) {
    const before = last;
    last = readable((r) => r.get(before) + 1);
    // computed in order, each read is one step deep
    registry.get(last);
  }
  return last;
};

test("a write reaches the end of a long chain a subscriber observes", () => {
  const registry = new Registry(currentCtx());
  const head = state(0);
  const tail = chainFrom(registry, head);
  const seen: Array<number> = [];
  registry.subscribe(tail, (n) => seen.push(n));
  registry.set(head, 1);
  registry.set(head, 2);
  assert.deepEqual(seen, [LONG + 1, LONG + 2]);
});

test("a long chain no one observes computes again when its end is read", async () => {
  const registry = new Registry(currentCtx());
  const head = { ...state(0), keepAlive: true };
  let last: Atom<number> = head;
  for (let i = 0; i < LONG; i++) {
    const before = last;
    last = { ...readable((r) => r.get(before) + 1), keepAlive: true };
    registry.get(last);
  }
  registry.set(head, 5);
  assert.equal(registry.get(last), LONG + 5);
});

test("a write tells each of thousands of subscribers once", () => {
  const registry = new Registry(currentCtx());
  const head = state(0);
  const doubled = readable((r) => r.get(head) * 2);
  let calls = 0;
  for (let i = 0; i < LONG; i++) registry.subscribe(i % 2 === 0 ? head : doubled, () => calls++);
  registry.set(head, 1);
  assert.equal(calls, LONG);
});

test("subscribers that write the next state wake a long chain of each other", () => {
  const registry = new Registry(currentCtx());
  const states = Array.from({ length: LONG }, () => state(0));
  states.forEach((atom, i) => {
    const next = states[i + 1];
    registry.subscribe(atom, (n) => {
      if (next !== undefined) registry.set(next, n);
    });
  });
  registry.set(states[0] as Atom<number>, 7);
  assert.equal(registry.get(states.at(-1) as Atom<number>), 7);
});

test("thousands of task atoms on one deferred wake their subscribers when it is set", { timeout: 5000 }, async () => {
  const registry = new Registry(currentCtx());
  const gate = Deferred.make();
  const tasks = Array.from({ length: LONG }, () => taskAtom(() => () => Deferred.await(gate)));
  let loaded = 0;
  for (const atom of tasks) {
    registry.subscribe(atom, (result) => {
      if (result._tag === "Success") loaded++;
    });
  }
  assert.equal(registry.get(tasks[0] as Atom<{ _tag: string }>)._tag, "Initial");
  Deferred.succeed("done")(gate);
  await new Promise((resolve) => setTimeout(resolve, 50));
  assert.equal(loaded, LONG);
});

test("a task atom's run is interrupted when it computes again or the registry is disposed", { timeout: 5000 }, async () => {
  const registry = new Registry(currentCtx());
  const gate = Deferred.make();
  const key = state(1);
  let runs = 0;
  const atom = taskAtom((r) => {
    r.get(key);
    runs++;
    return () => Deferred.await(gate);
  });
  let results = 0;
  registry.subscribe(atom, () => results++);
  registry.set(key, 2);
  assert.equal(runs, 2);
  registry.dispose();
  Deferred.succeed("late")(gate);
  await new Promise((resolve) => setTimeout(resolve, 10));
  // it heard of the second run starting, and of no run's result: neither ended before it was interrupted
  assert.equal(results, 1);
});
