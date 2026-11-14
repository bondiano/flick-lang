/* oxlint-disable require-yield, unicorn/consistent-function-scoping -- exercise compiled generator execution */
import assert from "node:assert/strict";
import { test } from "node:test";
import * as rt from "../src/rt/kernel.ts";
import { root, sleep } from "./rt-support.ts";

test("owner evidence is inert, independent, restorable and explicitly absent", async () => {
  const candidate = new rt.Owner();
  candidate.open = false;
  const exit = await root(() => rt.withOwner((provider) => (function* (): rt.Run {
    assert.equal(rt.readOwnerEvidence(), undefined);
    yield rt.withOwnerEvidence(candidate, function* (): rt.Run {
      assert.equal(rt.scopeCurrent(), provider);
      assert.equal(rt.readOwnerEvidence(), candidate);
      yield sleep(1);
      assert.equal(rt.readOwnerEvidence(), candidate);
      yield rt.withOwnerEvidence(undefined, () => assert.equal(rt.readOwnerEvidence(), undefined));
      assert.equal(rt.readOwnerEvidence(), candidate);
      yield rt.exitOf(() => rt.withOwnerEvidence(undefined, () => { throw new Error("expected"); }));
      assert.equal(rt.readOwnerEvidence(), candidate);
      return undefined;
    });
    assert.equal(rt.readOwnerEvidence(), undefined);
    return undefined;
  })()));
  assert.equal(exit.cause, undefined);
  assert.equal(candidate.closing.size, 0);
  assert.equal(candidate.finalizers.length, 0);
});

test("overlapping fibers and fresh scopes replace only their own evidence", async () => {
  const outer = new rt.Owner();
  const seen: unknown[] = [];
  const exit = await root(() => rt.withOwnerEvidence(outer, () => rt.scopeRun((owner) => (function* (): rt.Run {
    assert.equal(rt.readOwnerEvidence(), owner);
    const child = rt.fiberSpawn(owner, () => { seen.push(rt.readOwnerEvidence()); }, function* (): rt.Run {
      yield sleep(1);
      seen.push(rt.readOwnerEvidence());
      throw new Error("supervise");
    });
    yield rt.exitOf(() => rt.fiberJoin(child));
    yield sleep(2);
    assert.deepEqual(seen, [owner, owner]);
    return undefined;
  })())));
  assert.equal(exit.cause, undefined);
});

for (const kind of ["tail", "abort", "control"] as const) {
  test(`${kind} handler uses performer evidence and installation default owner`, async () => {
    const installed = new rt.Owner();
    const performer = new rt.Owner();
    const tag = rt.service(`OwnerEvidence.${kind}`);
    const op = rt.operation(tag, "read", 1);
    const invoke = (): unknown => { if (typeof op !== "function") throw new TypeError("not an operation"); return Reflect.apply(op, undefined, [undefined]); };
    const check = (): unknown => {
      assert.equal(rt.scopeCurrent(), installed);
      assert.equal(rt.readOwnerEvidence(), performer);
      return 42;
    };
    const arm = kind === "control" ? () => (resume: (v: unknown) => unknown) => (function* (): rt.Run {
      check();
      return yield resume(42);
    })() : check;
    const exit = await root(() => rt.inside(installed, () => rt.handle([[tag, [["read", arm, kind, 0, 1]]]],
      () => rt.withOwnerEvidence(performer, function* (): rt.Run {
        const value: unknown = yield invoke();
        assert.equal(rt.readOwnerEvidence(), performer);
        return value;
      }), undefined, undefined)));
    assert.equal(exit.cause, undefined);
    assert.equal(exit.value, 42);
  });
}

test("finalizers recover registration evidence after a bracket has ended", async () => {
  const candidate = new rt.Owner();
  let seen: unknown;
  const exit = await root(() => rt.withOwner(() => rt.withOwnerEvidence(candidate, () =>
    rt.addFinalizer(() => { seen = rt.readOwnerEvidence(); }))));
  assert.equal(exit.cause, undefined);
  assert.equal(seen, candidate);
});

test("overlapping evidence brackets cannot overwrite each other", async () => {
  const first = new rt.Owner();
  const second = new rt.Owner();
  const check = (candidate: rt.Owner) => root(() => rt.withOwnerEvidence(candidate, function* (): rt.Run {
    yield sleep(1);
    assert.equal(rt.readOwnerEvidence(), candidate);
    return undefined;
  }));
  const exits = await Promise.all([check(first), check(second)]);
  for (const exit of exits) assert.equal(exit.cause, undefined);
});

test("trusted from_build callback gets its fresh build owner as evidence", async () => {
  const { flickFromBuild } = await import("../src/rt/effect/Layer.ts");
  const { buildWith, MemoMap } = await import("../src/rt/layer.ts");
  const outer = new rt.Owner();
  const exit = await root(() => rt.withOwner((provider) => rt.withOwnerEvidence(outer, () =>
    buildWith(flickFromBuild(() => {
      assert.notEqual(rt.scopeCurrent(), outer);
      assert.equal(rt.readOwnerEvidence(), rt.scopeCurrent());
      return [];
    }), new MemoMap(undefined), provider))));
  assert.equal(exit.cause, undefined);
});

test("interruption restores registration evidence while unwinding suspended work", async () => {
  const candidate = new rt.Owner();
  let started: (() => void) | undefined;
  const entered = new Promise<void>((resolve) => { started = resolve; });
  let finalizerEvidence: unknown;
  const fiber = rt.fork(() => rt.withOwner(() => rt.withOwnerEvidence(candidate, function* (): rt.Run {
    yield rt.addFinalizer(() => { finalizerEvidence = rt.readOwnerEvidence(); });
    started?.();
    return yield new rt.Suspend(() => undefined);
  })), rt.currentCtx());
  await entered;
  fiber.interrupt(999);
  const exit = await new Promise<rt.Exit>((resolve) => { fiber.observe(resolve); });
  assert.ok(exit.cause);
  assert.equal(finalizerEvidence, candidate);
});
