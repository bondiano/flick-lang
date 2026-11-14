/* oxlint-disable typescript/no-unsafe-type-assertion, require-yield, unicorn/consistent-function-scoping -- compiler-style generator callbacks */
import assert from "node:assert/strict";
import { test } from "node:test";
import { Context, Effect, Scope } from "effect";
import * as native from "@flick-lang/runtime/rt/kernel";
import * as host from "../src/owner.ts";
import { callEffect, callSync, fromEffect, inSyncContext } from "../src/bridge_kernel.ts";
import { sync } from "../src/sync.ts";
import { handle } from "../src/handlers.ts";

const owner = (): host.Owner => new host.Owner(Scope.makeUnsafe());

test("Effect evidence is deferred, independent, explicit absence and task-as-data", async () => {
  const candidate = owner();
  candidate.open = false;
  let constructed = 0;
  let executed = 0;
  const task = Effect.sync(() => { executed++; });
  const wrapped = host.withOwnerEvidence(candidate, () => { constructed++; return Effect.succeed(task); });
  assert.equal(constructed, 0);
  const returned = Effect.runSync(wrapped);
  assert.equal(returned, task);
  assert.equal(constructed, 1);
  assert.equal(executed, 0);
  await Effect.runPromise(host.withOwner((provider) => host.withOwnerEvidence(candidate, () => Effect.gen(function* () {
    assert.equal(yield* host.scopeCurrent(), provider);
    assert.equal(yield* host.readOwnerEvidence(), candidate);
    yield* Effect.yieldNow;
    assert.equal(yield* host.readOwnerEvidence(), candidate);
    assert.equal(yield* host.withOwnerEvidence(undefined, () => host.readOwnerEvidence()), undefined);
    yield* Effect.exit(host.withOwnerEvidence(undefined, () => Effect.die("expected")));
    assert.equal(yield* host.readOwnerEvidence(), candidate);
  }))));
  assert.equal(candidate.closing.size, 0);
  assert.equal(candidate.resources.state._tag, "Empty");
});

test("bridge transports inert identity both ways and overrides stale base absence", () => {
  const candidate = new native.Owner();
  candidate.open = false;
  native.runIn(native.withOwnerEvidence(candidate, function* (): native.Run {
    assert.equal(yield fromEffect(host.readOwnerEvidence()), candidate);
    assert.equal(yield fromEffect(host.withOwnerEvidence(undefined, () => callEffect(() => native.readOwnerEvidence()))), undefined);
    assert.equal(yield fromEffect(callEffect(() => native.readOwnerEvidence())), candidate);
    assert.equal(inSyncContext(() => callSync(() => native.readOwnerEvidence())), candidate);
    return undefined;
  }));
  const foreign = owner();
  const readLater = () => callEffect(() => native.readOwnerEvidence());
  assert.equal(Effect.runSync(host.withOwnerEvidence(foreign, readLater)), foreign);
  assert.equal(Effect.runSync(host.withOwnerEvidence(undefined, readLater)), undefined);
  assert.equal(candidate.finalizers.length, 0);
  assert.equal(candidate.closing.size, 0);
  assert.equal(foreign.resources.state._tag, "Empty");
});

for (const abort of [false, true]) {
  test(`Effect ${abort ? "abort" : "tail"} handler keeps performer C and installation D`, () => {
    const installed = owner();
    const performer = owner();
    const tag = Context.Service<{ read: () => Effect.Effect<unknown> }>(`Test.Evidence.${abort}`);
    const arm = () => Effect.gen(function* () {
      assert.equal(yield* host.scopeCurrent(), installed);
      assert.equal(yield* host.readOwnerEvidence(), performer);
      return 42;
    });
    const body = host.withOwnerEvidence(performer, () => Effect.flatMap(Effect.service(tag), (s) => s.read()));
    const run = handle([[tag as unknown as Context.Service<object, object>, { read: arm }, abort ? ["read"] : []]], body, undefined, undefined);
    assert.equal(Effect.runSync(host.inside(installed, run) as Effect.Effect<unknown, unknown>), 42);
  });
}

test("Sync evidence observes execution rather than effect construction", () => {
  const candidate = owner();
  const cold = sync(() => callSync(() => native.readOwnerEvidence()));
  assert.equal(Effect.runSync(host.withOwnerEvidence(candidate, () => cold)), candidate);
  assert.equal(Effect.runSync(cold), undefined);
});

test("raw activation alone materializes a native candidate on Effect side", () => {
  const candidate = new native.Owner();
  const arena = Effect.runSync(host.withOwnerEvidence(candidate, () => Effect.flatMap(host.readOwnerEvidence(), (evidence) => host.within(evidence, host.arenaMake()))));
  assert.ok(arena);
  assert.equal(candidate.finalizers.length, 1);
  assert.equal(candidate.closing.size, 0);
  native.runIn(native.close(candidate, native.succeeded(undefined)));
});

test("raw activation of an Effect owner uses its scope, not the crossing's scope", () => {
  const candidate = owner();
  const arena = Effect.runSync(host.withOwnerEvidence(candidate, () => callEffect(() => native.within(native.readOwnerEvidence(), () => native.arenaMake()))));
  assert.ok(arena);
  assert.equal(candidate.resources.state._tag, "Open");
  Effect.runSync(host.close(candidate, Effect.runSync(Effect.exit(Effect.void))));
});

test("Effect finalizers capture registration evidence", () => {
  const candidate = owner();
  let seen: unknown;
  Effect.runSync(host.withOwner(() => host.withOwnerEvidence(candidate, () => Effect.addFinalizer(() =>
    Effect.map(host.readOwnerEvidence(), (evidence) => { seen = evidence; })))));
  assert.equal(seen, candidate);
});

test("Effect scopes and child/supervisor callbacks establish their own evidence", async () => {
  const outer = owner();
  const seen: unknown[] = [];
  await Effect.runPromise(host.withOwnerEvidence(outer, () => host.scopeRun((fresh) => Effect.gen(function* () {
    assert.equal(yield* host.readOwnerEvidence(), fresh);
    const child = yield* host.fiberSpawn(fresh,
      () => Effect.map(host.readOwnerEvidence(), (evidence) => { seen.push(evidence); }),
      Effect.flatMap(host.readOwnerEvidence(), (evidence) => { seen.push(evidence); return Effect.fail("expected"); }));
    yield* Effect.exit(host.fiberJoin(child));
    yield* Effect.yieldNow;
    assert.deepEqual(seen, [fresh, fresh]);
  }))));
});

test("overlapping Effect evidence brackets do not leak", async () => {
  const first = owner();
  const second = owner();
  const check = (candidate: host.Owner) => host.withOwnerEvidence(candidate, () => Effect.gen(function* () {
    yield* Effect.yieldNow;
    assert.equal(yield* host.readOwnerEvidence(), candidate);
  }));
  await Promise.all([Effect.runPromise(check(first)), Effect.runPromise(check(second))]);
});

test("trusted Effect from_build callback gets fresh ownership evidence", async () => {
  const { flickFromBuild } = await import("../src/effect/Layer.ts");
  const { Layer } = await import("effect");
  const unrelated = owner();
  Effect.runSync(host.withOwner((provider) => host.withOwnerEvidence(unrelated, () => Layer.buildWithScope(flickFromBuild(() =>
    Effect.gen(function* () {
      const fresh = yield* host.scopeCurrent();
      assert.notEqual(fresh, unrelated);
      assert.notEqual(fresh, provider);
      assert.equal(yield* host.readOwnerEvidence(), fresh);
      return Context.empty();
    })), provider.resources))));
});

test("closed identity transports without hooks but cannot reopen on raw activation", () => {
  const candidate = new native.Owner();
  native.runIn(native.close(candidate, native.succeeded(undefined)));
  assert.equal(Effect.runSync(host.withOwnerEvidence(candidate, () => callEffect(() => native.readOwnerEvidence()))), candidate);
  assert.equal(candidate.finalizers.length, 0);
  assert.equal(candidate.closing.size, 0);
  assert.throws(() => Effect.runSync(host.within(candidate, host.arenaMake())), /closed/);
});

test("pure host combinators store executables without capturing construction evidence", () => {
  const construction = new native.Owner();
  const execution = new native.Owner();
  const hold = (callback: () => unknown) => Effect.suspend(() => callEffect(callback));
  const stored = native.runIn(native.withOwnerEvidence(construction, () => [hold(() => native.readOwnerEvidence())]));
  assert.ok(Array.isArray(stored));
  const delayed: unknown = stored[0];
  assert.equal(native.runIn(native.withOwnerEvidence(execution, () => fromEffect(delayed))), execution);
  assert.equal(native.runIn(native.withOwnerEvidence(undefined, () => fromEffect(delayed))), undefined);

  const callback = Effect.runSync(host.withOwnerEvidence(construction, () => Effect.sync(() =>
    () => fromEffect(host.readOwnerEvidence()))));
  assert.equal(native.runIn(native.withOwnerEvidence(execution, callback)), execution);
  assert.equal(native.runIn(native.withOwnerEvidence(undefined, callback)), undefined);
  for (const candidate of [construction, execution]) {
    assert.equal(candidate.finalizers.length, 0);
    assert.equal(candidate.closing.size, 0);
  }
});

test("remembered host context deltas cannot replay stale owner evidence", async () => {
  const { rememberContext, updatedContext } = await import("../src/bridge_context.ts");
  const { OwnerEvidenceContext } = await import("../src/owner_evidence.ts");
  const fiber = new native.Fiber();
  const at = native.currentCtx();
  const previous = owner();
  const current = owner();
  const initial = Context.make(OwnerEvidenceContext, undefined);
  Effect.runSync(Effect.provideService(Effect.withFiber((hostFiber) => Effect.sync(() => {
    rememberContext(fiber, at, initial, hostFiber);
  })), OwnerEvidenceContext, previous));
  assert.equal(Context.get(updatedContext(fiber, at, Context.make(OwnerEvidenceContext, current)), OwnerEvidenceContext), current);
});

test("fresh Effect build ownership preserves cleanup defects", async () => {
  const { flickFromBuild } = await import("../src/effect/Layer.ts");
  const { Layer, Exit } = await import("effect");
  const built = flickFromBuild(() => Effect.andThen(Effect.addFinalizer(() => Effect.die("build cleanup")), Effect.succeed(Context.empty())));
  const result = Effect.runSyncExit(host.withOwner((provider) => Layer.buildWithScope(built, provider.resources)));
  assert.ok(Exit.isFailure(result));
});

test("Layer.run changes default ownership but preserves lexical evidence", async () => {
  const { Layer } = await import("effect");
  const { layerRun } = await import("../src/layer.ts");
  const candidate = owner();
  const tag = Context.Service<object>("Test.Evidence.Provider");
  Effect.runSync(host.withOwnerEvidence(candidate, () => layerRun(Layer.succeed(tag, {}), () => Effect.gen(function* () {
    assert.notEqual(yield* host.scopeCurrent(), candidate);
    assert.equal(yield* host.readOwnerEvidence(), candidate);
  }))));
});

for (const absent of [false, true]) {
  test(`native handler through Effect reads execution evidence (${absent ? "absent" : "present"})`, () => {
    const installed = new native.Owner();
    const performer = absent ? undefined : new native.Owner();
    const tag = native.service(`Test.CrossEvidence.${absent}`);
    const hostTag = Context.Service<{ read: (u: undefined) => Effect.Effect<unknown> }>(`Test.CrossEvidence.${absent}`);
    const out = native.runIn(native.withOwnerEvidence(installed, () => native.handle([[tag, [["read", () => (function* (): native.Run {
      return native.readOwnerEvidence();
    })(), "tail", 0, 1]]]], () => fromEffect(host.withOwnerEvidence(performer, () =>
      Effect.flatMap(Effect.service(hostTag), (s) => s.read(undefined)))), undefined, undefined)));
    assert.equal(out, performer);
  });
}

test("native partial forwarding through suspended Effect execution carries performer evidence", async () => {
  const performer = new native.Owner();
  const tag = native.service("Test.CrossEvidence.Forwarded");
  const hostTag = Context.Service<{ read: (u: undefined) => Effect.Effect<unknown> }>("Test.CrossEvidence.Forwarded");
  const request = host.withOwnerEvidence(performer, () => Effect.andThen(Effect.yieldNow,
    Effect.flatMap(Effect.service(hostTag), (s) => s.read(undefined))));
  const result = await new Promise<native.Exit>((resolve) => {
    native.fork(() => native.handle([[tag, [["read", () => (function* (): native.Run {
      yield new native.Suspend((resume) => { queueMicrotask(() => resume(undefined)); });
      return native.readOwnerEvidence();
    })(), "tail", 0, 1]]]], () => native.handle([[tag, [["other", () => 0, "tail", 0, 1]]]],
      () => fromEffect(request), undefined, undefined), undefined, undefined), native.currentCtx()).observe(resolve);
  });
  assert.equal(result.cause, undefined);
  assert.equal(result.value, performer);
});

test("untagged legacy native runtime factories see the executing Effect context", () => {
  const performer = new native.Owner();
  const tag = native.service("Test.CrossEvidence.Legacy");
  const hostTag = Context.Service<{ read: () => Effect.Effect<unknown> }>("Test.CrossEvidence.Legacy");
  const impl = { read: () => native.withOwnerEvidence(native.readOwnerEvidence(), () => native.readOwnerEvidence()) };
  const out = native.runIn(new native.Under(native.extend(native.currentCtx(), tag, impl), () =>
    fromEffect(host.withOwnerEvidence(performer, () => Effect.flatMap(Effect.service(hostTag), (s) => s.read())))));
  assert.equal(out, performer);
});

test("native handler dictionary and pure argument stages stay immediate; terminal invocation stays cold", () => {
  const tag = native.service("Test.CrossEvidence.Curried");
  const hostTag = Context.Service<object>("Test.CrossEvidence.Curried");
  let executions = 0;
  let taskRuns = 0;
  const task = () => { taskRuns++; return 99; };
  const arm = (dict: unknown) => (first: unknown) => (last: unknown) => {
    executions++;
    assert.equal(dict, "dictionary");
    assert.equal(first, "first");
    assert.equal(last, "last");
    return task;
  };
  const request = Effect.flatMap(Effect.service(hostTag), (impl) => {
    const dictionary: unknown = Reflect.get(impl, "read");
    assert.equal(typeof dictionary, "function");
    const pure: unknown = Reflect.apply(dictionary as Function, undefined, ["dictionary"]);
    assert.equal(typeof pure, "function");
    const terminal: unknown = Reflect.apply(pure as Function, undefined, ["first"]);
    assert.equal(typeof terminal, "function");
    const result: unknown = Reflect.apply(terminal as Function, undefined, ["last"]);
    assert.equal(executions, 0);
    assert.ok(Effect.isEffect(result));
    return result as Effect.Effect<unknown>;
  });
  const returned = native.runIn(native.handle([[tag, [["read", arm, "tail", 1, 2]]]], () => fromEffect(request), undefined, undefined));
  assert.equal(executions, 1);
  assert.equal(returned, task);
  assert.equal(taskRuns, 0);
});

test("native handler Sync invocation uses explicit Sync context immediately", async () => {
  const { syncIn, syncContext } = await import("../src/sync.ts");
  const { OwnerEvidenceContext } = await import("../src/owner_evidence.ts");
  const candidate = new native.Owner();
  const tag = native.service("Test.CrossEvidence.Sync");
  const hostTag = Context.Service<{ read: () => unknown }>("Test.CrossEvidence.Sync");
  const out = native.runIn(native.handle([[tag, [["read", () => native.readOwnerEvidence(), "tail", 0, 1]]]],
    () => fromEffect(Effect.flatMap(Effect.service(hostTag), (impl) => sync(() =>
      syncIn(Context.add(syncContext(), OwnerEvidenceContext, candidate), () => impl.read())))), undefined, undefined));
  assert.equal(out, candidate);
});
