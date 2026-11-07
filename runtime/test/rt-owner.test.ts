/* oxlint-disable require-yield, unicorn/consistent-function-scoping, typescript/no-unsafe-type-assertion -- generators that return a call are what the compiler emits */
// Owners of the kernel of `effect = "rt"`: the phases C0–C6 of closing.
import assert from "node:assert/strict";
import { test } from "node:test";

import {
  type Failure,
  raise,
  type Run,
  acquireRelease,
  addFinalizer,
  addFinalizerExit,
  asExit,
  awaitFiber,
  bracket,
  currentCtx,
  die,
  exitOf,
  fail,
  fiberJoin,
  fiberSpawn,
  foreignAsync,
  ownedExits,
  scopeCurrent,
  scopeRun,
  type OwnedExits,
  type Owner,
  uninterruptible,
  within,
  withOwner,
  yieldNow,
} from "../src/rt/kernel.ts";
import { root, sleep } from "./rt-support.ts";

const ignoreFailure = (): undefined => undefined;

const logger = (): { seen: Array<string>; say: (line: string) => () => void } => {
  const seen: Array<string> = [];
  return {
    seen,
    say: (line) => () => {
      seen.push(line);
    },
  };
};

test("finalizers run last added first, after the body, each once", async () => {
  const { seen, say } = logger();
  await root(() =>
    scopeRun(function* (): Run {
      yield addFinalizer(say("first added"));
      yield addFinalizer(say("last added"));
      seen.push("body");
      return undefined;
    }),
  );
  assert.deepEqual(seen, ["body", "last added", "first added"]);
});

test("a finalizer sees how the body ended, and a failure of the body still closes", async () => {
  const { seen, say } = logger();
  const exit = await root(() =>
    scopeRun(function* (): Run {
      yield addFinalizerExit((e) => seen.push(e.cause === undefined ? "ok" : "failed"));
      yield addFinalizer(say("released"));
      return raise(fail("typed"));
    }),
  );
  assert.deepEqual(seen, ["released", "failed"]);
  assert.equal(exit.cause?.reasons[0]?._tag, "Fail");
});

test("closing runs the hooks, stops the children, awaits supervisors, settles calls, then finalizers", async () => {
  const { seen, say } = logger();
  await root(() =>
    scopeRun(function* (owner: Owner): Run {
      yield addFinalizer(say("finalizer"));
      owner.closing.add(say("hook"));
      fiberSpawn(
        owner,
        function* (): Run {
          yield sleep(1);
          seen.push("supervisor");
          return undefined;
        },
        function* (): Run {
          try {
            yield sleep(1000);
          } finally {
            seen.push("child stopped");
          }
          return undefined;
        },
      );
      yield foreignAsync(() =>
        new Promise((resolve) => {
          setTimeout(() => {
            seen.push("call settled");
            resolve(undefined);
          }, 5);
        }).then(() => undefined),
      );
      yield yieldNow();
      seen.push("body done");
      return undefined;
    }),
  );
  assert.deepEqual(seen, ["call settled", "body done", "hook", "child stopped", "supervisor", "finalizer"]);
});

test("a promise an interrupted call left running is waited for before the finalizers", async () => {
  const { seen, say } = logger();
  await root(() =>
    scopeRun(function* (owner: Owner): Run {
      yield addFinalizer(say("finalizer"));
      fiberSpawn(owner, ignoreFailure, () =>
        foreignAsync(
          () =>
            new Promise((resolve) => {
              setTimeout(() => {
                seen.push("late write");
                resolve(undefined);
              }, 5);
            }),
        ),
      );
      yield yieldNow();
      return undefined;
    }),
  );
  assert.deepEqual(seen, ["late write", "finalizer"]);
});

test("a closed owner refuses a child; a finalizer added once finalizers started runs at once", async () => {
  const { seen, say } = logger();
  let kept: Owner | undefined;
  const refused = await root(function* (): Run {
    yield scopeRun(function* (owner: Owner): Run {
      kept = owner;
      yield addFinalizer(function* (): Run {
        yield addFinalizer(say("late"));
        seen.push("first");
        return undefined;
      });
      return undefined;
    });
    if (kept === undefined) return undefined;
    return fiberSpawn(kept, ignoreFailure, () => undefined);
  });
  assert.deepEqual(seen, ["late", "first"]);
  assert.equal(refused.cause?.reasons[0]?._tag, "Die");
});

test("a child's finalizer goes on the stack while the owner closes, and runs after it", async () => {
  const { seen, say } = logger();
  await root(() =>
    scopeRun(function* (owner: Owner): Run {
      fiberSpawn(owner, ignoreFailure, () =>
        uninterruptible(function* (): Run {
          yield sleep(5);
          yield addFinalizer(say("close late"));
          seen.push("child uses late");
          yield sleep(1);
          seen.push("child done");
          return undefined;
        }),
      );
      yield yieldNow();
      seen.push("body done");
      return undefined;
    }),
  );
  assert.deepEqual(seen, ["body done", "child uses late", "child done", "close late"]);
});

test("a child's failure goes to its supervisor, and a supervisor's failure to the root's sink", async () => {
  const seen: Array<unknown> = [];
  const exits = await root(() =>
    ownedExits(function* (owner: Owner): Run {
      fiberSpawn(
        owner,
        (failure: Failure) => {
          seen.push(failure.cause.reasons[0]?._tag);
          return raise(die("supervisor died"));
        },
        () => {
          return raise(fail("child"));
        },
      );
      yield yieldNow();
      yield yieldNow();
      return "body";
    }),
  );
  const { exit, cleanup, defects } = exits.value as OwnedExits;
  assert.deepEqual(seen, ["Fail"]);
  assert.equal(exit.value, "body");
  assert.equal(cleanup.cause, undefined);
  assert.equal(defects[0]?.reasons[0]?._tag, "Die");
});

test("join gives the child's value or its failure; a stopped child reports a shutdown", async () => {
  const shut: Array<boolean> = [];
  const exit = await root(() =>
    scopeRun(function* (owner: Owner): Run {
      const good = fiberSpawn(owner, ignoreFailure, () => 7);
      const bad = fiberSpawn(owner, ignoreFailure, () => {
        return raise(fail("no"));
      });
      fiberSpawn(
        owner,
        (failure: Failure) => {
          shut.push(failure.shutdown);
        },
        () => sleep(1000),
      );
      const a: unknown = yield fiberJoin(good);
      const b = asExit(yield exitOf(() => fiberJoin(bad)));
      return [a, b.cause?.reasons[0]?._tag];
    }),
  );
  assert.deepEqual(exit.value, [7, "Fail"]);
  assert.deepEqual(shut, [true]);
});

test("the failures of hooks and finalizers join the body's: hooks and body by combine, finalizers by concat", async () => {
  const exit = await root(() =>
    scopeRun(function* (owner: Owner): Run {
      owner.closing.add(() => {
        return raise(die("hook"));
      });
      yield addFinalizer(() => {
        return raise(die("same"));
      });
      yield addFinalizer(() => {
        return raise(die("same"));
      });
      return raise(fail("body"));
    }),
  );
  assert.deepEqual(
    exit.cause?.reasons.map((r) => r._tag),
    ["Fail", "Die", "Die"],
  );
});

test("an outer region takes what `within` acquires, and `Scope.current` names the owner", async () => {
  const { seen, say } = logger();
  await root(() =>
    scopeRun(function* (outer: Owner): Run {
      yield scopeRun(function* (): Run {
        yield within(outer, () => addFinalizer(say("outer's")));
        yield addFinalizer(say("inner's"));
        assert.notEqual(scopeCurrent(), outer);
        return undefined;
      });
      seen.push("inner closed");
      return undefined;
    }),
  );
  assert.deepEqual(seen, ["inner's", "inner closed", "outer's"]);
});

test("acquisition and release are masked; bracket elects a defect beside a failure", async () => {
  const { seen } = logger();
  const exit = await root(() =>
    scopeRun(function* (): Run {
      const r: unknown = yield acquireRelease(
        () => "res",
        (res) => {
          seen.push(`release ${String(res)}`);
        },
      );
      seen.push(`use ${String(r)}`);
      return yield bracket(
        () => "b",
        () => {
          return raise(fail("used"));
        },
        () => {
          return raise(die("released"));
        },
      );
    }),
  );
  assert.deepEqual(seen, ["use res", "release res"]);
  assert.deepEqual(
    exit.cause?.reasons.map((r) => r._tag),
    ["Die", "Die"],
  );
});

test("an interruption of the body closes the owner, and is delivered again on leaving", async () => {
  const { seen, say } = logger();
  function* body(): Run {
    const child = fiberSpawn(
      scopeCurrent() as Owner,
      ignoreFailure,
      () =>
        scopeRun(function* (): Run {
          yield addFinalizer(say("closed"));
          yield sleep(1000);
          return undefined;
        }),
    );
    yield yieldNow();
    child.fiber.interrupt(1);
    const exit = asExit(yield awaitFiber(child.fiber));
    return exit.cause?.reasons.map((r) => r._tag);
  }
  const exit = await root(() => withOwner(body));
  assert.deepEqual(exit.value, ["Interrupt"]);
  assert.deepEqual(seen, ["closed"]);
  assert.ok(currentCtx() !== undefined);
});
