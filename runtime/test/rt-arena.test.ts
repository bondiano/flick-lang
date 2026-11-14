/* oxlint-disable require-yield, unicorn/consistent-function-scoping, typescript/no-unsafe-type-assertion -- the compiler's generator protocol returns unknown */
import assert from "node:assert/strict";
import { test } from "node:test";

import { type Arena, arenaAlloc, arenaGet } from "../src/core/Arena.ts";
import { arenaMake, closeOwn, addFinalizer, scopeRun, within } from "../src/rt/owner.ts";
import { type Owner } from "../src/rt/owner.ts";
import { type Run, asExit, die, exitOf, fail, fiberSpawn, interrupt, raise, succeeded, yieldNow } from "../src/rt/kernel.ts";
import { root, sleep } from "./rt-support.ts";

test("arena slots close at the scope boundary, while ordinary read aliases survive", async () => {
  let kept: Arena | undefined;
  const alias = { text: "owned" };
  const exit = await root(function* (): Run {
    const read = yield scopeRun(function* (): Run {
      const arena = (yield arenaMake()) as Arena;
      kept = arena;
      const view = arenaAlloc(arena, alias);
      assert.equal(arena.liveSlots, 1);
      return arenaGet(view);
    });
    assert.equal(kept?.liveSlots, 0, "released before the outer continuation");
    assert.equal(read, alias);
    return undefined;
  });
  assert.equal(exit.cause, undefined);
});

test("arenas remain live through earlier, later and nested finalizers", async () => {
  let kept: Arena | undefined;
  let read = (): unknown => undefined;
  let bornInFinalizer: Arena | undefined;
  const seen: Array<unknown> = [];
  const exit = await root(() => scopeRun(function* (): Run {
    yield addFinalizer(() => seen.push(read()));
    kept = (yield arenaMake()) as Arena;
    const view = arenaAlloc(kept, "contents");
    read = () => arenaGet(view);
    yield addFinalizer(function* (): Run {
      seen.push(read());
      bornInFinalizer = (yield arenaMake()) as Arena;
      seen.push(arenaGet(arenaAlloc(bornInFinalizer, "late")));
      yield addFinalizer(() => seen.push(read()));
      return undefined;
    });
    return undefined;
  }));
  assert.equal(exit.cause, undefined);
  assert.deepEqual(seen, ["contents", "late", "contents", "contents"]);
  assert.equal(kept?.liveSlots, 0);
  assert.equal(bornInFinalizer?.liveSlots, 0);
  assert.throws(read, /closed/);
});

test("selected outer owners retain arenas across an inner scope", async () => {
  let outerArena: Arena | undefined;
  let innerArena: Arena | undefined;
  const exit = await root(() => scopeRun(function* (outer: Owner): Run {
    yield scopeRun(function* (): Run {
      outerArena = (yield within(outer, () => arenaMake())) as Arena;
      innerArena = (yield arenaMake()) as Arena;
      arenaAlloc(outerArena, "outer");
      arenaAlloc(innerArena, "inner");
      return undefined;
    });
    assert.equal(outerArena?.liveSlots, 1);
    assert.equal(innerArena?.liveSlots, 0);
    return undefined;
  }));
  assert.equal(exit.cause, undefined);
  assert.equal(outerArena?.liveSlots, 0);
});

test("selecting a retired owner is inert, while arena admission still checks that owner", async () => {
  let kept: Arena | undefined;
  let outerArena: Arena | undefined;
  const exit = await root(() => scopeRun(function* (): Run {
    outerArena = (yield arenaMake()) as Arena;
    const outerView = arenaAlloc(outerArena, "still live");
    let retired: Owner | undefined;
    yield scopeRun(function* (owner: Owner): Run {
      retired = owner;
      kept = (yield arenaMake()) as Arena;
      arenaAlloc(kept, "retired");
      return undefined;
    });
    assert.ok(retired && kept);
    assert.equal(kept.liveSlots, 0);
    assert.equal(yield within(retired, () => 42), 42);
    assert.equal(yield within(retired, () => arenaGet(outerView)), "still live");
    const refused = asExit(yield exitOf(() => within(retired, () => arenaMake())));
    assert.equal(refused.cause?.reasons.length, 1);
    const reason = refused.cause?.reasons[0];
    assert.ok(reason?._tag === "Die");
    assert.match(String(reason.defect), /owner is closed/);
    assert.equal(kept.liveSlots, 0, "selection cannot reopen retired storage");
    const after = (yield arenaMake()) as Arena;
    assert.equal(arenaGet(arenaAlloc(after, "caller restored")), "caller restored");
    return undefined;
  }));
  assert.equal(exit.cause, undefined);
  assert.equal(outerArena?.liveSlots, 0);
});

test("failure still closes every arena, stale access fails, closed-owner acquisition is refused", async () => {
  let kept: Arena | undefined;
  let owner: Owner | undefined;
  let read = (): unknown => undefined;
  const exit = await root(() => scopeRun(function* (current: Owner): Run {
    owner = current;
    kept = (yield arenaMake()) as Arena;
    const view = arenaAlloc(kept, "contents");
    read = () => arenaGet(view);
    const second = (yield arenaMake()) as Arena;
    arenaAlloc(second, "second");
    return raise(fail("typed"));
  }));
  assert.ok(exit.cause);
  assert.equal(kept?.liveSlots, 0);
  assert.throws(read, /closed/);
  assert.ok(kept && owner);
  const closedArena = kept;
  const closedOwner = owner;
  assert.throws(() => arenaAlloc(closedArena, "late"), /closed/);
  await root(() => closeOwn(closedOwner, succeeded(undefined)));
  const refused = await root(() => within(closedOwner, () => arenaMake()));
  assert.ok(refused.cause);
  assert.equal(kept.liveSlots, 0);
});

for (const [name, cause] of [["defect", die("broken")], ["interruption", interrupt(undefined)]] as const) {
  test(`arenas close on ${name}, even if an ordinary finalizer also fails`, async () => {
    let kept: Arena | undefined;
    let read = (): unknown => undefined;
    const exit = await root(() => scopeRun(function* (): Run {
      yield addFinalizer(() => assert.equal(read(), "live"));
      kept = (yield arenaMake()) as Arena;
      const view = arenaAlloc(kept, "live");
      read = () => arenaGet(view);
      yield addFinalizer(() => { throw new Error("finalizer"); });
      return raise(cause);
    }));
    assert.ok(exit.cause);
    assert.equal(kept?.liveSlots, 0);
    assert.throws(read, /closed/);
  });
}

test("owned child cleanup and pending foreign work see storage until they settle", async () => {
  let kept: Arena | undefined;
  const seen: Array<unknown> = [];
  const exit = await root(() => scopeRun(function* (owner: Owner): Run {
    kept = (yield arenaMake()) as Arena;
    const view = arenaAlloc(kept, "live");
    fiberSpawn(owner, () => undefined, function* (): Run {
      try { yield sleep(1000); } finally { seen.push(arenaGet(view)); }
      return undefined;
    });
    owner.track(new Promise<void>((resolve) => {
      setTimeout(() => { seen.push(arenaGet(view)); resolve(); }, 5);
    }));
    yield yieldNow();
    return undefined;
  }));
  assert.equal(exit.cause, undefined);
  assert.deepEqual(seen, ["live", "live"]);
  assert.equal(kept?.liveSlots, 0);
});
