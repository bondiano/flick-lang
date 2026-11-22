import assert from "node:assert/strict";
import { test } from "node:test";
import { Cause, type Context, Effect, Exit } from "effect";

import { type Arena, arenaAlloc, arenaGet } from "@flick-lang/runtime/core/Arena";
import { type Owner, arenaMake, closeOwn, fiberSpawn, scopeRun, within } from "../src/owner.ts";
import { handle } from "../src/handlers.ts";
import { layerRun, layerSucceed } from "../src/layer.ts";
import { promiseCallback, RuntimeFailure } from "../src/stdlib/callbacks.ts";
import { operation, service } from "../src/effects.ts";

const emptyRead = (): unknown => undefined;

test("arena storage closes before continuation, without invalidating a retained ordinary value", async () => {
  let kept: Arena | undefined;
  const alias = { text: "ordinary" };
  const value = await Effect.runPromise(scopeRun(() => Effect.gen(function* () {
    kept = yield* arenaMake();
    return arenaGet(arenaAlloc(kept, alias));
  })));
  assert.equal(value, alias);
  assert.equal(kept?.liveSlots, 0);
});

test("earlier and later finalizers read live storage; arenas created while finalizing also close", async () => {
  let kept: Arena | undefined;
  let late: Arena | undefined;
  let read = emptyRead;
  const seen: Array<unknown> = [];
  await Effect.runPromise(scopeRun(() => Effect.gen(function* () {
    yield* Effect.addFinalizer(() => Effect.sync(() => seen.push(read())));
    kept = yield* arenaMake();
    const view = arenaAlloc(kept, "live");
    read = () => arenaGet(view);
    yield* Effect.addFinalizer(() => Effect.gen(function* () {
      seen.push(read());
      late = yield* arenaMake();
      seen.push(arenaGet(arenaAlloc(late, "late")));
      yield* Effect.addFinalizer(() => Effect.sync(() => seen.push(read())));
    }));
  })));
  assert.deepEqual(seen, ["live", "late", "live", "live"]);
  assert.equal(kept?.liveSlots, 0);
  assert.equal(late?.liveSlots, 0);
  assert.throws(read, /closed/);
});

test("selected outer ownership is independent of the innermost scope", async () => {
  let outerArena: Arena | undefined;
  let innerArena: Arena | undefined;
  await Effect.runPromise(scopeRun((outer) => Effect.gen(function* () {
    yield* scopeRun(() => Effect.gen(function* () {
      outerArena = yield* within(outer, arenaMake());
      innerArena = yield* arenaMake();
      arenaAlloc(outerArena, "outer");
      arenaAlloc(innerArena, "inner");
    }));
    assert.equal(outerArena?.liveSlots, 1);
    assert.equal(innerArena?.liveSlots, 0);
  })));
  assert.equal(outerArena?.liveSlots, 0);
});

test("retired owner selection is inert, but real arena admission remains closed", async () => {
  let kept: Arena | undefined;
  let outerArena: Arena | undefined;
  await Effect.runPromise(scopeRun(() => Effect.gen(function* () {
    outerArena = yield* arenaMake();
    const outerView = arenaAlloc(outerArena, "still live");
    let retired: Owner | undefined;
    yield* scopeRun((owner) => Effect.gen(function* () {
      retired = owner;
      kept = yield* arenaMake();
      arenaAlloc(kept, "retired");
    }));
    assert.ok(retired && kept);
    assert.equal(kept.liveSlots, 0);
    assert.equal(yield* within(retired, Effect.succeed(42)), 42);
    assert.equal(yield* within(retired, Effect.sync(() => arenaGet(outerView))), "still live");
    const refused = yield* Effect.exit(within(retired, arenaMake()));
    assert.ok(Exit.isFailure(refused));
    assert.equal(refused.cause.reasons.length, 1);
    const reason = refused.cause.reasons[0];
    assert.ok(reason && Cause.isDieReason(reason));
    assert.match(String(reason.defect), /owner is closed/);
    assert.equal(kept.liveSlots, 0, "selection cannot reopen retired storage");
    const after = yield* arenaMake();
    assert.equal(arenaGet(arenaAlloc(after, "caller restored")), "caller restored");
  })));
  assert.equal(outerArena?.liveSlots, 0);
});

for (const [name, end] of [
  ["typed failure", Effect.fail("typed")],
  ["defect", Effect.die("defect")],
  ["interruption", Effect.interrupt],
] as const) {
  test(`arena closes on ${name}, stale reads fail`, async () => {
    let kept: Arena | undefined;
    let read = emptyRead;
    const exit = await Effect.runPromiseExit(scopeRun(() => Effect.gen(function* () {
      kept = yield* arenaMake();
      const view = arenaAlloc(kept, "contents");
      read = () => arenaGet(view);
      return yield* end;
    })));
    assert.ok(Exit.isFailure(exit));
    assert.equal(kept?.liveSlots, 0);
    assert.throws(read, /closed/);
  });
}

test("close is idempotent for arenas and an ended owner refuses arena acquisition", async () => {
  let kept: Arena | undefined;
  let owner: Owner | undefined;
  await Effect.runPromise(scopeRun((current) => Effect.gen(function* () {
    owner = current;
    kept = yield* arenaMake();
    arenaAlloc(kept, "live");
  })));
  assert.ok(owner && kept);
  const closedArena = kept;
  await Effect.runPromise(closeOwn(owner, Exit.void));
  assert.equal(kept.liveSlots, 0);
  assert.throws(() => arenaAlloc(closedArena, "late"), /closed/);
  const refused = await Effect.runPromiseExit(within(owner, arenaMake()));
  assert.ok(Exit.isFailure(refused));
});

test("owned child cleanup and pending foreign work retain arena access until they settle", async () => {
  let kept: Arena | undefined;
  const seen: Array<unknown> = [];
  await Effect.runPromise(scopeRun((owner) => Effect.gen(function* () {
    kept = yield* arenaMake();
    const view = arenaAlloc(kept, "live");
    yield* fiberSpawn(owner, () => Effect.void,
      Effect.ensuring(Effect.never, Effect.sync(() => seen.push(arenaGet(view)))));
    owner.track(new Promise<void>((resolve) => {
      setTimeout(() => { seen.push(arenaGet(view)); resolve(); }, 5);
    }));
    yield* Effect.yieldNow;
  })));
  assert.deepEqual(seen, ["live", "live"]);
  assert.equal(kept?.liveSlots, 0);
});

test("abort and failing tail handlers close inner arena scopes after LIFO finalizers", async () => {
  // 1. abort arm
  let abortArena: Arena | undefined;
  const abortOrder: Array<string> = [];
  const escapeTag = service("Test.ArenaEscape");
  // oxlint-disable-next-line typescript/no-unsafe-type-assertion -- erased handler ABI tag
  const escapeInstall = escapeTag as unknown as Context.Service<object, object>;
  // oxlint-disable-next-line typescript/no-unsafe-type-assertion -- erased handler op ABI
  const escapeOp = operation(escapeTag, "escape", 1) as (m: string) => Effect.Effect<string, unknown, unknown>;
  const escapeImpl = {
    escape: (msg: string) => Effect.sync(() => { abortOrder.push(`arm:${msg}`); return `escaped:${msg}`; }),
  };
  const abortProg = handle(
    [[escapeInstall, escapeImpl, ["escape"]]],
    scopeRun(() => Effect.gen(function* () {
      abortArena = yield* arenaMake();
      const view = arenaAlloc(abortArena, "abort-live");
      yield* Effect.addFinalizer(() => Effect.sync(() => abortOrder.push(`fin1:${String(arenaGet(view))}`)));
      yield* Effect.addFinalizer(() => Effect.sync(() => abortOrder.push(`fin2:${String(arenaGet(view))}`)));
      return yield* escapeOp("now");
    })),
    undefined,
    undefined,
  );
  // oxlint-disable-next-line typescript/no-unsafe-type-assertion -- handler discharges escapeTag
  const abortRes = await Effect.runPromise(abortProg as Effect.Effect<unknown, unknown>);
  assert.equal(abortRes, "escaped:now");
  assert.deepEqual(abortOrder, ["fin2:abort-live", "fin1:abort-live", "arm:now"]);
  assert.equal(abortArena?.liveSlots, 0);

  // 2. failing tail arm
  let tailArena: Arena | undefined;
  const tailOrder: Array<string> = [];
  const askTag = service("Test.ArenaAsk");
  // oxlint-disable-next-line typescript/no-unsafe-type-assertion -- erased handler ABI tag
  const askInstall = askTag as unknown as Context.Service<object, object>;
  // oxlint-disable-next-line typescript/no-unsafe-type-assertion -- erased handler op ABI
  const askOp = operation(askTag, "ask", 1) as (u: undefined) => Effect.Effect<string, unknown, unknown>;
  const askImpl = {
    ask: (_u: undefined) => Effect.fail("tail-boom"),
  };
  const tailProg = handle(
    [[askInstall, askImpl, []]],
    scopeRun(() => Effect.gen(function* () {
      tailArena = yield* arenaMake();
      const view = arenaAlloc(tailArena, "tail-live");
      yield* Effect.addFinalizer(() => Effect.sync(() => tailOrder.push(`fin1:${String(arenaGet(view))}`)));
      yield* Effect.addFinalizer(() => Effect.sync(() => tailOrder.push(`fin2:${String(arenaGet(view))}`)));
      return yield* askOp(undefined);
    })),
    undefined,
    undefined,
  );
  // oxlint-disable-next-line typescript/no-unsafe-type-assertion -- handler discharges askTag
  const tailExit = await Effect.runPromiseExit(tailProg as Effect.Effect<unknown, unknown>);
  assert.ok(Exit.isFailure(tailExit));
  assert.deepEqual(tailOrder, ["fin2:tail-live", "fin1:tail-live"]);
  assert.equal(tailArena?.liveSlots, 0);
});

test("layerRun preserves selected outer arena and promiseCallback rejects late calls after close", async () => {
  let outerArena: Arena | undefined;
  let pcb: ((arg: string) => Promise<string>) | undefined;
  let readAfter: (() => unknown) | undefined;
  const svcTag = service("Test.ArenaSvc");
  const openResult = await Effect.runPromise(
    scopeRun((outer) => Effect.gen(function* () {
      const layer = layerSucceed(svcTag, { label: "ctx" });
      const view = yield* layerRun(layer, () => Effect.gen(function* () {
        outerArena = yield* within(outer, arenaMake());
        return arenaAlloc(outerArena, "outer-live");
      }));
      assert.equal(outerArena?.liveSlots, 1);
      readAfter = () => arenaGet(view);
      const cb = yield* promiseCallback((tag: string) => Effect.sync(() => `${tag}:${String(arenaGet(view))}`));
      pcb = cb;
      return yield* Effect.promise(() => cb("open"));
    })),
  );
  assert.equal(openResult, "open:outer-live");
  assert.equal(outerArena?.liveSlots, 0);
  assert.ok(pcb);
  const lateCb = pcb;
  await assert.rejects(() => lateCb("late"), RuntimeFailure);
  assert.throws(() => readAfter?.(), /closed/);
});
