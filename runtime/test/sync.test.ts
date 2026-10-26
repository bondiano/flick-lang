// Sync stages read the services of the context set around them: by `syncIn`
// outside a fiber, by `sync` inside one; a runtime call runs at once.
import assert from "node:assert/strict";
import { test } from "node:test";

import { Context, Effect } from "effect";

import { runIn, service, sync, syncIn, syncOperation } from "../src/index.ts";

interface NameImpl {
  readonly name: (greeting: string) => unknown;
}
const Name = service<NameImpl>("App.Name");
// oxlint-disable-next-line typescript/no-unsafe-type-assertion -- the operation takes one argument
const greet = syncOperation(Name, "name", 1) as (greeting: string) => string;

test("a Sync operation reads the context set around it, which is restored after", () => {
  const ann = Context.make(Name, { name: (g: string) => `${g} Ann` });
  const bob = Context.make(Name, { name: (g: string) => `${g} Bob` });
  assert.equal(
    syncIn(ann, () => `${greet("hi")}, ${syncIn(bob, () => greet("hey"))}, ${greet("bye")}`),
    "hi Ann, hey Bob, bye Ann",
  );
});

test("inside a fiber a Sync call reads the fiber's services, and a runtime call in it runs at once", () => {
  const impl = { name: (g: string) => Effect.succeed(`${g} Cy`) };
  const program = Effect.provideService(
    sync(() => `${greet("yo")}, ${runIn(Effect.map(Effect.service(Name), () => "ran"))}`),
    Name,
    impl,
  );
  assert.equal(Effect.runSync(program), "yo Cy, ran");
});
