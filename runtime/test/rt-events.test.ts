import assert from "node:assert/strict";
import { test } from "node:test";

import { eventsAttach, eventsListen, eventsQueue, type Listener } from "../src/rt/events.ts";
import { OwnedExits, addFinalizer, ownedExits } from "../src/rt/owner.ts";
import { type Run } from "../src/rt/fiber.ts";
import { root, sleep } from "./rt-support.ts";

test("a listener queues reentrant events and ignores calls after its owner closes", async () => {
  const seen: string[] = [];
  let listener: Listener<number> | undefined;
  const exit = await root(() => ownedExits((owner) => (function* (): Run {
    listener = eventsListen(owner, eventsQueue(4), () => undefined, (n) => () => {
      seen.push(`start ${n}`);
      if (n === 1) listener?.(2);
      seen.push(`end ${n}`);
    });
    listener(1);
    seen.push("source returned");
    yield sleep(5);
    return undefined;
  })()));
  assert.ok(exit.value instanceof OwnedExits);
  assert.equal(exit.value.exit.cause, undefined);
  assert.equal(exit.value.cleanup.cause, undefined);
  listener?.(3);
  await new Promise((resolve) => setTimeout(resolve, 5));
  assert.deepEqual(seen, ["source returned", "start 1", "end 1", "start 2", "end 2"]);
});

for (const bodyFails of [false, true]) {
  test(`all detach failures survive a ${bodyFails ? "failed" : "successful"} body and cleanup`, async () => {
    const seen: string[] = [];
    const failures = [new Error("first detach"), new Error("second detach")];
    const exit = await root(() => ownedExits((owner) => (function* (): Run {
      yield addFinalizer(() => { seen.push("released"); });
      const listener = eventsListen(owner, eventsQueue(1), () => undefined, () => () => undefined);
      for (const failure of failures) {
        yield eventsAttach(listener, () => undefined, () => {
          seen.push(failure.message);
          throw failure;
        });
      }
      if (bodyFails) throw new Error("body failed");
      return undefined;
    })()));
    assert.ok(exit.value instanceof OwnedExits);
    assert.deepEqual(seen, ["first detach", "second detach", "released"]);
    assert.deepEqual(exit.value.cleanup.cause?.reasons.filter((reason) => reason._tag === "Die").map((reason) => reason.defect), failures);
    assert.equal(exit.value.exit.cause === undefined, !bodyFails);
  });
}
