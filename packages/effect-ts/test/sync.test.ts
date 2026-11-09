// Sync stages read the services of the context set around them: by `syncIn`
// outside a fiber, by `sync` inside one; a runtime call runs at once.
import assert from "node:assert/strict";
import { test } from "node:test";

import { Context, Effect } from "effect";

import { runIn, seqIn, service, sync, syncAbi, syncIn, syncOperation } from "../src/index.ts";

interface NameImpl {
  readonly name: (greeting: string) => unknown;
  readonly [syncAbi]?: true;
}
const Name = service<NameImpl>("App.Name");
// oxlint-disable-next-line typescript/no-unsafe-type-assertion -- the operation takes one argument
const greet = syncOperation(Name, "name", 1) as (greeting: string) => string;

test("a Sync operation reads the context set around it, which is restored after", () => {
  const ann = Context.make(Name, { name: (g: string) => `${g} Ann`, [syncAbi]: true });
  const bob = Context.make(Name, { name: (g: string) => `${g} Bob`, [syncAbi]: true });
  assert.equal(
    syncIn(ann, () => `${greet("hi")}, ${syncIn(bob, () => greet("hey"))}, ${greet("bye")}`),
    "hi Ann, hey Bob, bye Ann",
  );
});

test("an operation of a JS implementation returns its value, even an Effect, which stays cold", () => {
  let runs = 0;
  const task = Effect.sync(() => ++runs);
  const js = Context.make(Name, { name: () => task, [syncAbi]: true });
  assert.equal(syncIn(js, () => greet("hi")), task);
  assert.equal(runs, 0);
});

test("inside a fiber a Sync call reads the fiber's services, and a Flick implementation's effect runs at once", () => {
  const impl = { name: (g: string) => Effect.succeed(`${g} Cy`) };
  const program = Effect.provideService(
    sync(() => `${greet("yo")}, ${runIn(Effect.map(Effect.service(Name), () => "ran"))}`),
    Name,
    impl,
  );
  assert.equal(Effect.runSync(program), "yo Cy, ran");
});

test("each step of a Sync sequence reads the context it was built with, again on each iteration", () => {
  let names = ["Ann"];
  const context = Context.make(Name, { name: (g: string) => `${g} ${names.join("+")}`, [syncAbi]: true });
  let ran = 0;
  const steps = seqIn(context, function* () {
    ran++;
    yield greet("hi");
    yield greet("bye");
  });
  assert.deepEqual([...steps], ["hi Ann", "bye Ann"]);
  names = ["Ann", "Bob"];
  // a step outside any context still reads the one the sequence holds
  for (const first of steps) {
    assert.equal(first, "hi Ann+Bob");
    break;
  }
  assert.equal(ran, 2);
});
