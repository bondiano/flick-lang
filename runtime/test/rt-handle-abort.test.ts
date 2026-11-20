/* oxlint-disable require-yield, unicorn/consistent-function-scoping, typescript/no-unsafe-type-assertion -- generators that return a call are what the compiler emits */
// Cross-fiber `abort` defects, defect-supersedes-signal stripping, `Lazy.force`
// R9/signal rules, and `Effect.Errors` signal hygiene on Flick's native `rt`.
import assert from "node:assert/strict";
import { test } from "node:test";

import {
  Signal,
  awaitFiber,
  currentCtx,
  die,
  elect,
  fail,
  fork,
  handle,
  interrupt,
  operation,
  raise,
  service,
  signalOf,
  type Exit,
  type Run,
} from "../src/rt/kernel.ts";
import { parallelExits, parallelResults } from "../src/rt/concurrency.ts";
import { ensuring, flickOnExit } from "../src/rt/effect/Effect.ts";
import { lazyForce, lazyMake } from "../src/rt/effect/Lazy.ts";

const Abort = service("Test.AbortHygiene");
const abort = operation(Abort, "abort", 1) as (value: unknown) => unknown;

const root = (task: () => unknown): Promise<Exit> =>
  new Promise((resolve) => {
    fork(task, currentCtx()).observe(resolve);
  });

test("an abort operation from a child fiber is a defect and does not run the arm", async () => {
  let armRan = false;
  const exit = await root(() =>
    handle(
      [[Abort, [["abort", () => { armRan = true; return "wrong"; }, "abort", 0, 1]]]],
      function* (): Run {
        const child = fork(() => abort("child"), currentCtx());
        return yield awaitFiber(child);
      },
      undefined,
      undefined,
    ),
  );
  assert.equal(armRan, false);
  const childExit = exit.value as { cause?: { reasons: ReadonlyArray<{ _tag: string; defect?: unknown }> } };
  assert.equal(childExit.cause?.reasons.length, 1);
  assert.equal(childExit.cause?.reasons[0]?._tag, "Die");
  assert.match(String(childExit.cause?.reasons[0]?.defect), /other than its handler/);
});

test("ensuring with body die and cleanup abort propagates only the defect", async () => {
  let armRan = false;
  const bodyDie = await root(() =>
    handle(
      [[Abort, [["abort", () => { armRan = true; return "swallowed"; }, "abort", 0, 1]]]],
      () => ensuring(() => abort("cleanup"))(() => raise(die("boom"))),
      undefined,
      undefined,
    ),
  );
  assert.equal(armRan, false);
  assert.equal(bodyDie.cause?.reasons.length, 1);
  assert.deepEqual(bodyDie.cause?.reasons[0], { _tag: "Die", defect: "boom" });

  const cleanupDie = await root(() =>
    handle(
      [[Abort, [["abort", () => { armRan = true; return "swallowed"; }, "abort", 0, 1]]]],
      () => ensuring(() => raise(die("cleanup-boom")))(() => abort("body")),
      undefined,
      undefined,
    ),
  );
  assert.equal(armRan, false);
  assert.equal(cleanupDie.cause?.reasons.length, 1);
  assert.deepEqual(cleanupDie.cause?.reasons[0], { _tag: "Die", defect: "cleanup-boom" });
});

test("parallelResults does not wrap a handler Signal as an Err", async () => {
  const token = {};
  const sig = new Signal(token, () => "arm", undefined, false);
  const exit = await root(() => parallelResults([() => raise(fail(sig))]));
  assert.ok(exit.cause !== undefined);
  assert.equal(signalOf(exit.cause, token), sig);
});

test("parallelExits strips handler signals when combined with a defect", async () => {
  const token = {};
  const sig = new Signal(token, () => "arm", undefined, false);
  const mixed = await root(() => parallelExits([
    () => raise({ reasons: [{ _tag: "Die", defect: "boom" }, { _tag: "Fail", error: sig }] }),
    () => raise({ reasons: [{ _tag: "Die", defect: "boom2" }, { _tag: "Die", defect: sig }] }),
  ]));
  assert.equal(mixed.cause, undefined);
  const exits = mixed.value as ReadonlyArray<{ cause?: { reasons: ReadonlyArray<{ _tag: string; defect?: unknown }> } }>;
  assert.equal(exits.length, 2);
  assert.deepEqual(exits[0]?.cause?.reasons, [{ _tag: "Die", defect: "boom" }]);
  assert.deepEqual(exits[1]?.cause?.reasons, [{ _tag: "Die", defect: "boom2" }]);
});

test("elect ignores Die(Signal) when no real defect is present", () => {
  const sig = new Signal({}, () => "arm", undefined, false);
  const lone = { reasons: [{ _tag: "Die" as const, defect: sig }] };
  assert.deepEqual(elect(lone), lone);
  const withFail = { reasons: [{ _tag: "Die" as const, defect: sig }, { _tag: "Fail" as const, error: "err" }] };
  assert.deepEqual(elect(withFail), withFail);
});

test("lazyForce does not cache Interrupt+Die (R9) and strips+caches Die+Signal", async () => {
  let runs1 = 0;
  const cell1 = lazyMake(() => {
    runs1 += 1;
    if (runs1 === 1) {
      return raise({ reasons: [...interrupt(undefined).reasons, ...die("int-die").reasons] });
    }
    return 42;
  });
  const first = await root(() => lazyForce(cell1));
  assert.equal(first.cause?.reasons.length, 2);
  const second = await root(() => lazyForce(cell1));
  assert.equal(second.cause, undefined);
  assert.equal(second.value, 42);
  assert.equal(runs1, 2);

  let runs2 = 0;
  const cell2 = lazyMake(() => {
    runs2 += 1;
    return ensuring(() => raise(die("fin-boom")))(() => abort("body"));
  });
  const withAbort = (task: () => unknown) =>
    handle([[Abort, [["abort", () => "swallowed", "abort", 0, 1]]]], task, undefined, undefined);
  const g1 = await root(() => withAbort(() => lazyForce(cell2)));
  const g2 = await root(() => withAbort(() => lazyForce(cell2)));
  assert.equal(runs2, 1);
  assert.deepEqual(g1.cause?.reasons, [{ _tag: "Die", defect: "fin-boom" }]);
  assert.deepEqual(g2.cause?.reasons, [{ _tag: "Die", defect: "fin-boom" }]);
});

test("flickOnExit strips handler signals before passing exit to callback", async () => {
  const sig = new Signal({}, () => "arm", undefined, false);
  let seen: ReadonlyArray<{ _tag: string; defect?: unknown }> | undefined;
  const out = await root(() =>
    flickOnExit((ex) => {
      seen = ex.cause?.reasons;
      return undefined;
    })(() => raise({ reasons: [{ _tag: "Fail", error: sig }, { _tag: "Die", defect: "fin-boom" }] })),
  );
  assert.deepEqual(seen, [{ _tag: "Die", defect: "fin-boom" }]);
  assert.deepEqual(out.cause?.reasons, [{ _tag: "Die", defect: "fin-boom" }]);
});
