/* oxlint-disable require-yield, unicorn/consistent-function-scoping, typescript/no-unsafe-type-assertion -- the compiler's generator protocol returns unknown */
import assert from "node:assert/strict";
import { test } from "node:test";

import { type Arena, arenaAlloc, arenaGet } from "../src/core/Arena.ts";
import { arenaMake, closeOwn, addFinalizer, scopeRun, within } from "../src/rt/owner.ts";
import { type Owner } from "../src/rt/owner.ts";
import { handle, operation, service } from "../src/rt/handle.ts";
import { layerRun, layerSucceed } from "../src/rt/layer.ts";
import { eventsListen, eventsQueue } from "../src/rt/events.ts";
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

test("abort, control discontinue, and failing tail arms close inner arenas after LIFO finalizers", async () => {
  // 1. abort unwinds inner scope, runs LIFO finalizers while arena is live, then retires arena
  let abortArena: Arena | undefined;
  const abortOrder: Array<unknown> = [];
  const escapeTag = service("Test.Escape");
  const abortExit = await root(() =>
    handle(
      [[escapeTag, [["escape", (msg: unknown) => { abortOrder.push(`arm:${String(msg)}`); return `escaped:${String(msg)}`; }, "abort", 0, 1]]]],
      () =>
        scopeRun(function* (): Run {
          abortArena = (yield arenaMake()) as Arena;
          const view = arenaAlloc(abortArena, "abort-live");
          yield addFinalizer(() => abortOrder.push(`fin1:${String(arenaGet(view))}`));
          yield addFinalizer(() => abortOrder.push(`fin2:${String(arenaGet(view))}`));
          const perform = operation(escapeTag, "escape", 1) as (m: string) => unknown;
          return yield perform("now");
        }),
      undefined,
      undefined,
    ),
  );
  assert.equal(abortExit.cause, undefined);
  assert.equal(abortExit.value, "escaped:now");
  assert.deepEqual(abortOrder, ["fin2:abort-live", "fin1:abort-live", "arm:now"]);
  assert.equal(abortArena?.liveSlots, 0);

  // 2. control discontinue closes inner arena scope in LIFO order
  let ctrlArena: Arena | undefined;
  const ctrlOrder: Array<unknown> = [];
  const yieldTag = service("Test.Yield");
  const ctrlExit = await root(() =>
    handle(
      [[yieldTag, [["yield_", (x: unknown) => () => { ctrlOrder.push("ctrl-arm"); return x; }, "control", 0, 1]]]],
      () =>
        scopeRun(function* (): Run {
          ctrlArena = (yield arenaMake()) as Arena;
          const view = arenaAlloc(ctrlArena, "ctrl-live");
          yield addFinalizer(() => ctrlOrder.push(`fin1:${String(arenaGet(view))}`));
          yield addFinalizer(() => ctrlOrder.push(`fin2:${String(arenaGet(view))}`));
          const perform = operation(yieldTag, "yield_", 1) as (m: string) => unknown;
          return yield perform("dropped");
        }),
      undefined,
      undefined,
    ),
  );
  assert.equal(ctrlExit.cause, undefined);
  assert.equal(ctrlExit.value, "dropped");
  assert.deepEqual(ctrlOrder, ["ctrl-arm", "fin2:ctrl-live", "fin1:ctrl-live"]);
  assert.equal(ctrlArena?.liveSlots, 0);

  // 3. failing tail arm unwinds inner arena scope and runs LIFO finalizers before failure propagates
  let tailArena: Arena | undefined;
  const tailOrder: Array<unknown> = [];
  const askTag = service("Test.Ask");
  const tailExit = await root(() =>
    handle(
      [[askTag, [["ask", () => raise(fail("tail-boom")), "tail", 0, 1]]]],
      () =>
        scopeRun(function* (): Run {
          tailArena = (yield arenaMake()) as Arena;
          const view = arenaAlloc(tailArena, "tail-live");
          yield addFinalizer(() => tailOrder.push(`fin1:${String(arenaGet(view))}`));
          yield addFinalizer(() => tailOrder.push(`fin2:${String(arenaGet(view))}`));
          const perform = operation(askTag, "ask", 1) as (u: undefined) => unknown;
          return yield perform(undefined);
        }),
      undefined,
      undefined,
    ),
  );
  assert.ok(tailExit.cause);
  assert.deepEqual(tailOrder, ["fin2:tail-live", "fin1:tail-live"]);
  assert.equal(tailArena?.liveSlots, 0);
});

test("provider layers preserve selected outer arena and late async callbacks see closed storage", async () => {
  let outerArena: Arena | undefined;
  let listener: ((ev: string) => void) | undefined;
  let lateRead: (() => unknown) | undefined;
  const seen: Array<unknown> = [];
  const svcTag = service("Test.Svc");
  const exit = await root(() =>
    scopeRun(function* (outer: Owner): Run {
      const layer = layerSucceed(svcTag, { get: () => "svc" });
      const view = yield layerRun(layer, function* (): Run {
        outerArena = (yield within(outer, () => arenaMake())) as Arena;
        return arenaAlloc(outerArena, "outer-via-layer");
      });
      assert.equal(outerArena?.liveSlots, 1);
      lateRead = () => arenaGet(view as never);
      listener = eventsListen<string>(
        outer,
        eventsQueue(4),
        () => undefined,
        (ev) => () => seen.push(`${ev}:${String(arenaGet(view as never))}`),
      );
      listener("open");
      yield sleep(5);
      return undefined;
    }),
  );
  assert.equal(exit.cause, undefined);
  assert.equal(outerArena?.liveSlots, 0);
  // Late event after owner close is dropped cleanly; direct late read throws closed
  listener?.("after-close");
  await new Promise((r) => setTimeout(r, 10));
  assert.deepEqual(seen, ["open:outer-via-layer"]);
  assert.throws(() => lateRead?.(), /closed/);
});
