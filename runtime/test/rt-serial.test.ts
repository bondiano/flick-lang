/* oxlint-disable typescript/no-unsafe-type-assertion -- the values of atoms are what they were made with */
// Serializable atoms on Flick's own runtime: what a server hands over and what
// a client registry starts from. A hydrated task is not run again, and a value
// that does not code is a defect naming its key, as in the Effect adapter.
import assert from "node:assert/strict";
import { test } from "node:test";

import { atomLoader, atomSerializableResult, atomSerializableTask, atomState, atomTask } from "../src/rt/atoms/atom.ts";
import { Registry, type Atom } from "../src/rt/atoms/registry.ts";
import { entries, hydrate, serializable } from "../src/rt/atoms/serial.ts";
import { fail, raise } from "../src/rt/cause.ts";
import { currentCtx } from "../src/rt/kernel.ts";
import { schemaNumber, schemaString } from "../src/rt/schema.ts";
import { tag } from "../src/tags.ts";

const LOADED = tag("Atom.Async.Loaded");
const FAILED = tag("Atom.Async.Failed");

const failing = (): never => raise(fail("missing"));

test("a server hands over each serializable atom by key, coded, and no other", () => {
  const registry = new Registry(currentCtx());
  const count = serializable("App.count", schemaNumber, atomState(0));
  const local = atomState(1);
  registry.set(count, 3);
  registry.set(local, 2);
  assert.deepEqual(entries(registry), [{ key: "App.count", value: 3 }]);
});

test("a client state starts from what the server handed over, whether or not its atom exists yet", () => {
  const before = new Registry(currentCtx());
  const early = serializable("App.early", schemaString, atomState(""));
  before.get(early);
  hydrate(before, [{ key: "App.early", value: "hi" }, { key: "App.late", value: "there" }]);
  assert.equal(before.get(early), "hi");
  const late = serializable("App.late", schemaString, atomState(""));
  assert.equal(before.get(late), "there");
});

test("a hydrated task gives the server's value without running, and a later change of its route runs it", () => {
  const registry = new Registry(currentCtx());
  const route = atomState("a");
  let runs = 0;
  const item = atomSerializableTask(
    "App.item",
    schemaString,
    atomLoader(route, (key) => () => {
      runs++;
      return `item ${key}`;
    }) as Atom<object>,
  );
  hydrate(registry, [{ key: "App.item", value: "from server" }]);
  assert.deepEqual(registry.get(item), { _tag: LOADED, _0: "from server" });
  assert.equal(runs, 0);
  registry.subscribe(item, () => undefined);
  registry.set(route, "b");
  assert.deepEqual(registry.get(item), { _tag: LOADED, _0: "item b" });
  assert.equal(runs, 1);
});

test("a task hands over only a loaded value; a result also a failure of its typed error", () => {
  const registry = new Registry(currentCtx());
  const task = atomSerializableTask("App.task", schemaString, atomTask(failing) as Atom<object>);
  const result = atomSerializableResult("App.result", schemaString, schemaString, atomTask(failing) as Atom<object>);
  assert.equal(Reflect.get(registry.get(task), "_tag"), FAILED);
  assert.equal(Reflect.get(registry.get(result), "_tag"), FAILED);
  assert.deepEqual(entries(registry), [{ key: "App.result", value: { err: "missing" } }]);
  const client = new Registry(currentCtx());
  hydrate(client, entries(registry));
  const again = atomSerializableResult("App.result", schemaString, schemaString, atomTask(() => "ran") as Atom<object>);
  assert.equal(Reflect.get(client.get(again), "_tag"), FAILED);
});

test("a value its codec does not encode is a defect naming the atom's key", () => {
  const registry = new Registry(currentCtx());
  const count = serializable("App.count", schemaNumber, atomState<unknown>(0));
  registry.set(count, "not a number");
  assert.throws(() => entries(registry), /the serializable atom `App.count` does not encode its value/);
});
