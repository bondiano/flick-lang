/* oxlint-disable require-yield, unicorn/consistent-function-scoping, typescript/no-unsafe-type-assertion -- generators that return a call are what the compiler emits */
// The kernel of `effect = "rt"` alone: fibers, the driver, interruption, causes.
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";

import {
  type Cause,
  type Exit,
  Raise,
  raise,
  Suspend,
  asExit,
  atMask,
  awaitFiber,
  combine,
  concat,
  currentCtx,
  currentFiber,
  dies,
  elect,
  fail,
  failed,
  fork,
  interrupt,
  runIn,
  type Run,
  succeeded,
  uninterruptible,
  yieldNow,
  die,
  exitOf,
  caught,
} from "../src/rt/kernel.ts";
import { root, sleep } from "./rt-support.ts";

test("a chain of calls deeper than the JS stack runs on heap frames", async () => {
  function* down(n: number): Run {
    if (n === 0) return 0;
    const below: unknown = yield down(n - 1);
    return Number(below) + 1;
  }
  const exit = await root(() => down(200_000));
  assert.equal(exit.value, 200_000);
});

test("a tail call replaces its frame: 10^6 of them, by self and by a group", async () => {
  function* self(n: number): Run {
    return n === 0 ? "self" : self(n - 1);
  }
  function* even(n: number): Run {
    return n === 0 ? "even" : odd(n - 1);
  }
  function* odd(n: number): Run {
    return n === 0 ? "odd" : even(n - 1);
  }
  assert.equal((await root(() => self(1_000_000))).value, "self");
  assert.equal((await root(() => even(1_000_000))).value, "even");
});

test("a call that needs no frame gives its value, and a throw fails the caller's frame", async () => {
  const plain = (x: number): number => x + 1;
  const boom = (): never => {
    throw new Error("boom");
  };
  function* body(): Run {
    const a: unknown = yield plain(1);
    try {
      yield boom();
    } catch (error) {
      return [a, error instanceof Error ? error.message : "?"];
    }
    return "not here";
  }
  assert.deepEqual((await root(body)).value, [2, "boom"]);
});

test("a fiber starts on a later turn, the one that forked goes on first", async () => {
  const seen: Array<string> = [];
  function* body(): Run {
    const child = fork(() => void seen.push("child"), currentCtx());
    seen.push("parent");
    yield awaitFiber(child);
    return seen;
  }
  assert.deepEqual((await root(body)).value, ["parent", "child"]);
});

test("a wait that ends moves its fiber on at once, on the stack that ended it", async () => {
  const seen: Array<string> = [];
  let end: (value: unknown) => void = () => {};
  function* waiter(): Run {
    const v: unknown = yield new Suspend((resolve) => {
      end = resolve;
    });
    seen.push(`woke ${String(v)}`);
    return undefined;
  }
  function* body(): Run {
    const child = fork(waiter, currentCtx());
    yield yieldNow();
    seen.push("ending");
    end(1);
    seen.push("ended");
    yield awaitFiber(child);
    return undefined;
  }
  await root(body);
  assert.deepEqual(seen, ["ending", "woke 1", "ended"]);
});

test("interruption is delivered at a suspension, and runs the frames' cleanup", async () => {
  const seen: Array<string> = [];
  function* child(): Run {
    try {
      yield sleep(1000);
      seen.push("not reached");
    } finally {
      seen.push("cleaned");
    }
    return undefined;
  }
  function* body(): Run {
    const f = fork(child, currentCtx());
    yield yieldNow();
    f.interrupt(currentFiber().id);
    const exit = asExit(yield awaitFiber(f));
    return exit;
  }
  const exit = asExit((await root(body)).value);
  assert.deepEqual(exit.cause?.reasons.map((r) => r._tag), ["Interrupt"]);
  assert.deepEqual(seen, ["cleaned"]);
});

test("a fiber asked to stop before it starts never runs", async () => {
  let ran = false;
  function* body(): Run {
    const f = fork(() => {
      ran = true;
    }, currentCtx());
    f.interrupt(currentFiber().id);
    return asExit(yield awaitFiber(f));
  }
  const exit = asExit((await root(body)).value);
  assert.equal(ran, false);
  assert.deepEqual(exit.cause?.reasons.map((r) => r._tag), ["Interrupt"]);
});

test("a mask holds the request back, and leaving it delivers the request at once", async () => {
  const seen: Array<string> = [];
  function* masked(): Run {
    yield sleep(5);
    seen.push("block done");
    return undefined;
  }
  function* child(): Run {
    yield uninterruptible(masked);
    seen.push("not reached");
    return undefined;
  }
  function* body(): Run {
    const f = fork(child, currentCtx());
    yield yieldNow();
    f.interrupt(currentFiber().id);
    return asExit(yield awaitFiber(f));
  }
  const exit = asExit((await root(body)).value);
  assert.deepEqual(seen, ["block done"]);
  assert.deepEqual(exit.cause?.reasons.map((r) => r._tag), ["Interrupt"]);
});

test("restoring the mask lets the request in", async () => {
  const seen: Array<string> = [];
  function* child(): Run {
    const fiber = currentFiber();
    fiber.mask = 1;
    try {
      yield atMask(0, () => sleep(1000));
      seen.push("not reached");
    } finally {
      fiber.mask = 0;
    }
    return undefined;
  }
  function* body(): Run {
    const f = fork(child, currentCtx());
    yield yieldNow();
    f.interrupt(currentFiber().id);
    return asExit(yield awaitFiber(f));
  }
  await root(body);
  assert.deepEqual(seen, []);
});

test("a recovery in a fiber asked to stop is skipped: failures dropped, interruption added", async () => {
  let handled = false;
  function* child(): Run {
    const fiber = currentFiber();
    fiber.mask = 1;
    yield yieldNow();
    yield yieldNow();
    fiber.mask = 0;
    const exit = asExit(
      yield caught(() => {
        return raise(fail("typed"));
      }),
    );
    handled = exit.cause !== undefined;
    return undefined;
  }
  function* body(): Run {
    const f = fork(child, currentCtx());
    yield yieldNow();
    f.interrupt(currentFiber().id);
    return asExit(yield awaitFiber(f));
  }
  const exit = asExit((await root(body)).value);
  assert.equal(handled, false);
  assert.deepEqual(exit.cause?.reasons.map((r) => r._tag), ["Interrupt"]);
});

test("a stage that runs at once runs a call to its end, and a child it needs on its stack", async () => {
  function* three(): Run {
    const a: unknown = yield 1;
    const b: unknown = yield 2;
    return Number(a) + Number(b);
  }
  assert.equal(runIn(three()), 3);
  assert.equal(runIn(7), 7);
  let ran = false;
  function* waits(): Run {
    const f = fork(() => {
      ran = true;
    }, currentCtx());
    yield awaitFiber(f);
    return "done";
  }
  assert.equal(runIn(waits()), "done");
  assert.equal(ran, true);
  assert.throws(() => runIn(sleep(1)), /has not happened/);
});

test("a thrown failure comes out of a stage that runs at once", () => {
  function* fails(): Run {
    return raise(fail("typed"));
  }
  try {
    runIn(fails());
    assert.fail("it throws");
  } catch (thrown) {
    assert.ok(thrown instanceof Raise);
  }
});

test("a fiber gives others a turn after its budget", async () => {
  let turns = 0;
  function* spin(): Run {
    for (let i = 0; i < 10_000; i++) yield undefined;
    return "spun";
  }
  function* other(): Run {
    turns++;
    return undefined;
  }
  function* body(): Run {
    const a = fork(spin, currentCtx());
    const b = fork(other, currentCtx());
    yield awaitFiber(a);
    yield awaitFiber(b);
    return turns;
  }
  assert.equal((await root(body)).value, 1);
});

const reasons = (c: Cause): ReadonlyArray<unknown> => c.reasons.map((r) => (r._tag === "Fail" ? r.error : r._tag === "Die" ? ["die", r.defect] : "int"));

test("combine keeps each reason once and is associative; concat keeps repeats", () => {
  const a = fail({ _tag: "A" });
  const a2 = fail({ _tag: "A" });
  const b = fail({ _tag: "B" });
  assert.equal(combine(a, a2).reasons.length, 1);
  assert.deepEqual(reasons(combine(combine(a, b), a2)), reasons(combine(a, combine(b, a2))));
  assert.equal(concat(a, a2).reasons.length, 2);
  assert.equal(combine({ reasons: [] }, b), b);
});

test("defects are one by value, a thrown error only by itself", () => {
  assert.equal(combine(die("x"), die("x")).reasons.length, 1);
  assert.equal(combine(die(new Error("x")), die(new Error("x"))).reasons.length, 2);
  assert.equal(combine(interrupt(1), interrupt(1)).reasons.length, 1);
  assert.equal(combine(interrupt(1), interrupt(2)).reasons.length, 2);
});

test("elect turns failures to defects only beside a defect", () => {
  const c = concat(fail("a"), die("d"));
  assert.deepEqual(elect(c), dies(c));
  assert.deepEqual(reasons(elect(c)), [["die", "a"], ["die", "d"]]);
  assert.equal(elect(fail("a")).reasons[0]?._tag, "Fail");
});

test("exits say how a task ended, with a thrown failure kept whole", async () => {
  function* body(): Run {
    const good = asExit(yield exitOf(() => 1));
    const bad = asExit(
      yield exitOf(() => {
        return raise(fail("typed"));
      }),
    );
    return [good, bad];
  }
  const [good, bad] = (await root(body)).value as [Exit, Exit];
  assert.deepEqual(good, succeeded(1));
  assert.deepEqual(bad, failed(fail("typed")));
});

test("nothing the runtime's index reaches imports Effect", () => {
  const seen = new Set<string>();
  const visit = (url: URL): void => {
    // spike: the bridge is the one module of the index that imports Effect, until it
    // moves to an entry of its own (tools/load/spike/FINDINGS.md, recommendation 5)
    if (seen.has(url.href) || url.pathname.endsWith("/rt/bridge.ts")) return;
    seen.add(url.href);
    for (const [, spec] of readFileSync(url, "utf8").matchAll(/from "([^"]+)"/g)) {
      if (spec === undefined || spec.startsWith("node:")) continue;
      assert.ok(spec.startsWith("."), `${url.pathname} imports ${spec}`);
      visit(new URL(spec, url));
    }
  };
  visit(new URL("../src/rt/index.ts", import.meta.url));
  assert.ok(seen.size > 10);
});
