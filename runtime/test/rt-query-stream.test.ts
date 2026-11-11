/* oxlint-disable typescript/no-unsafe-type-assertion -- the values of atoms are what they were made with */
// `Atom.query_stream` on Flick's own runtime, as the adapter's atom of a
// stream: `Loading` until the first element, then the latest one; its last
// one stays when it ends; an error fails it; a stream with no element fails
// with `NoSuchElementError`; a mutation of its keys starts it anew; dropping
// it interrupts its run, whose finalizers run once.
import assert from "node:assert/strict";
import { test } from "node:test";

import { atomQueryStream } from "../src/rt/atoms/atom.ts";
import { Registry } from "../src/rt/atoms/registry.ts";
import { fail, raise } from "../src/rt/cause.ts";
import { currentCtx } from "../src/rt/kernel.ts";
import * as Deferred from "../src/rt/effect/Deferred.ts";
import { addFinalizer } from "../src/rt/owner.ts";
import type { Failure } from "../src/rt/outcome.ts";
import { tag } from "../src/tags.ts";
import { streamHooks } from "../src/rt/ui/stream_hook.ts";

const LOADING = tag("Atom.Async.Loading");
const LOADED = tag("Atom.Async.Loaded");
const FAILED = tag("Atom.Async.Failed");

/** A stream here: the task that hands each element to `element`, as the adapter's bridge runs a `Stream`. */
type Fake = (element: (value: unknown) => void) => unknown;

streamHooks.each = (stream, element) => (stream as Fake)(element);

const shown = (value: unknown): string => {
  const async = value as { _tag: string; _0?: unknown };
  if (async._tag === LOADING) return "loading";
  if (async._tag === LOADED) return `loaded ${String(async._0)}`;
  assert.equal(async._tag, FAILED);
  const [reason] = (async._0 as Failure).cause.reasons;
  return `failed ${String(Reflect.get(Object(Reflect.get(Object(reason), "error")), "_tag") ?? Reflect.get(Object(reason), "error"))}`;
};

const tick = (): Promise<void> => new Promise((resolve) => setTimeout(resolve, 5));

test("loading until the first element, then the latest; the last stays when it ends", async () => {
  const gate = Deferred.make();
  const stream: Fake = function* (element) {
    element(7);
    yield Deferred.await(gate);
    element(8);
  };
  const registry = new Registry(currentCtx());
  const atom = atomQueryStream(["stock"], stream);
  const seen: Array<string> = [];
  registry.subscribe(atom, (value) => seen.push(shown(value)), true);
  Deferred.succeed(undefined)(gate);
  await tick();
  // the end tells again with the same value, no longer waiting
  assert.deepEqual(seen, ["loaded 7", "loaded 8", "loaded 8"]);
  assert.equal(shown(registry.get(atom)), "loaded 8");
});

test("a stream that has not sent anything yet is loading", () => {
  const registry = new Registry(currentCtx());
  const atom = atomQueryStream([], () => Deferred.await(Deferred.make()));
  assert.equal(shown(registry.get(atom)), "loading");
});

test("an error of the stream fails it", async () => {
  const gate = Deferred.make();
  const stream: Fake = function* (element) {
    element(1);
    yield Deferred.await(gate);
    return raise(fail("Oops"));
  };
  const registry = new Registry(currentCtx());
  const atom = atomQueryStream([], stream);
  const seen: Array<string> = [];
  registry.subscribe(atom, (value) => seen.push(shown(value)), true);
  Deferred.succeed(undefined)(gate);
  await tick();
  assert.deepEqual(seen, ["loaded 1", "failed Oops"]);
});

test("a stream that ends with no element fails with NoSuchElementError", () => {
  const registry = new Registry(currentCtx());
  const atom = atomQueryStream([], () => undefined);
  assert.equal(shown(registry.get(atom)), `failed ${tag("Cause.NoSuchElementError.NoSuchElementError")}`);
});

test("a mutation of its keys starts it anew and interrupts the run before it, once", async () => {
  let runs = 0;
  let cleanups = 0;
  const first = Deferred.make();
  const second = Deferred.make();
  const stream: Fake = function* (element) {
    const run = runs++;
    addFinalizer(() => cleanups++);
    element(run * 10);
    yield Deferred.await(run === 0 ? first : second);
    element(run * 10 + 1);
  };
  const registry = new Registry(currentCtx());
  const atom = atomQueryStream(["stock"], stream);
  const seen: Array<string> = [];
  const off = registry.subscribe(atom, (value) => seen.push(shown(value)), true);
  registry.invalidate(["other"]);
  assert.equal(runs, 1);
  registry.invalidate(["stock"]);
  assert.equal(runs, 2);
  await tick();
  assert.equal(cleanups, 1);
  // the first run was interrupted: its next element never comes
  Deferred.succeed(undefined)(first);
  await tick();
  assert.deepEqual(seen, ["loaded 0", "loaded 10"]);
  off();
  await tick();
  assert.equal(cleanups, 2);
  Deferred.succeed(undefined)(second);
  await tick();
  assert.deepEqual(seen, ["loaded 0", "loaded 10"]);
  assert.equal(cleanups, 2);
});
