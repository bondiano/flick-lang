/* oxlint-disable require-yield, unicorn/consistent-function-scoping, typescript/no-unsafe-type-assertion -- generators that return a call are what the compiler emits */
// Handlers of the kernel of `effect = "rt"`: `tail`, `abort` and `control`, deep.
import assert from "node:assert/strict";
import { test } from "node:test";

import {
  type OpInstall,
  Raise,
  raise,
  type Run,
  addFinalizer,
  asExit,
  awaitFiber,
  currentCtx,
  currentFiber,
  exitOf,
  fail,
  fork,
  handle,
  operation,
  runIn,
  scopeRun,
  service,
  signalOf,
  tryWith,
  trySync,
  yieldNow,
} from "../src/kernel.ts";
import { root, sleep } from "./rt-support.ts";

const Ask = service("Test.Ask");
const Yield = service("Test.Yield");
const Abort = service("Test.Abort");

const ask = operation(Ask, "ask", 1) as (u: undefined) => unknown;
const yielded = operation(Yield, "yield", 1) as (x: unknown) => unknown;
const abort = operation(Abort, "abort", 1) as (x: unknown) => unknown;

const tail = (name: string, arm: unknown): OpInstall => [name, arm, "tail", 0, 1];

test("a tail operation is a direct call: no driver, in a stage that runs at once", async () => {
  let depth = 0;
  const body = (): unknown => {
    const a = ask(undefined);
    depth = 1;
    return [a, ask(undefined)];
  };
  const exit = await root(() => handle([[Ask, [tail("ask", () => 41)]]], function* (): Run {
    return body();
  }, undefined, undefined));
  assert.deepEqual(exit.value, [41, 41]);
  assert.equal(depth, 1);
});

test("a tail arm runs with the evidence of the handler, not of the caller", async () => {
  const inner = handle.bind(undefined);
  const exit = await root(() =>
    handle(
      [[Ask, [tail("ask", () => "outer")]]],
      () =>
        inner(
          [[Yield, [tail("yield", () => ask(undefined))]]],
          () =>
            handle(
              [[Ask, [tail("ask", () => "inner")]]],
              () => yielded(0),
              undefined,
              undefined,
            ),
          undefined,
          undefined,
        ),
      undefined,
      undefined,
    ),
  );
  assert.equal(exit.value, "outer");
});

test("no handler is a defect naming the effect", async () => {
  const exit = await root(() => ask(undefined));
  assert.match(String(exit.cause?.reasons[0]?._tag), /Die/);
});

test("an abort arm runs after the body unwound, its value the handler's, past a `try`", async () => {
  const seen: Array<string> = [];
  const exit = await root(() =>
    handle(
      [[Abort, [["abort", (x: unknown) => {
        seen.push("arm");
        return `aborted ${String(x)}`;
      }, "abort", 0, 1]]]],
      () =>
        scopeRun(function* (): Run {
          yield addFinalizer(() => {
            seen.push("released");
          });
          return yield tryWith(
            () => abort(1),
            () => () => "a `try` does not see it",
          );
        }),
      undefined,
      undefined,
    ),
  );
  assert.equal(exit.value, "aborted 1");
  assert.deepEqual(seen, ["released", "arm"]);
});

test("`ret` makes the handler's value; the errors arm sees the body's typed failure", async () => {
  const ok = await root(() => handle([], () => 1, (v) => Number(v) + 1, undefined));
  assert.equal(ok.value, 2);
  const failed = await root(() =>
    handle(
      [],
      () => {
        return raise(fail("oops"));
      },
      undefined,
      (e) => () => `caught ${String(e)}`,
    ),
  );
  assert.equal(failed.value, "caught oops");
});

test("the typed failure of a tail arm is the handler's: a `try` in the body does not catch it", async () => {
  const exit = await root(() =>
    tryWith(
      () =>
        handle(
          [[Ask, [tail("ask", () => {
            return raise(fail("from the arm"));
          })]]],
          () => tryWith(() => ask(undefined), () => () => "inner `try`"),
          undefined,
          undefined,
        ),
      (e) => () => `outer: ${String(e)}`,
    ),
  );
  assert.equal(exit.value, "outer: from the arm");
});

const control = (arm: (x: unknown) => (resume: (v: unknown) => unknown) => unknown): OpInstall => ["yield", arm, "control", 0, 1];

test("a control arm resumes: deep, the value of `resume` is the rest of the handler", async () => {
  function* body(): Run {
    const a: unknown = yield yielded(1);
    const b: unknown = yield yielded(2);
    return [a, b];
  }
  const collect = (x: unknown) =>
    function* (resume: (v: unknown) => unknown): Run {
      const rest: unknown = yield resume(Number(x) * 10);
      return [x, rest];
    };
  const exit = await root(() => handle([[Yield, [control(collect)]]], body, (v) => ["end", v], undefined));
  assert.deepEqual(exit.value, [1, [2, ["end", [10, 20]]]]);
});

test("10^5 operations of one handler, each resumed in tail position, run on the heap", async () => {
  function* body(n: number): Run {
    for (let i = 0; i < n; i++) yield yielded(i);
    return "done";
  }
  const arm = () =>
    function* (resume: (v: unknown) => unknown): Run {
      return resume(undefined);
    };
  const exit = await root(() => handle([[Yield, [control(arm)]]], () => body(100_000), undefined, undefined));
  assert.equal(exit.value, "done");
});

test("an arm that does not resume discontinues the body: scopes close, last added first, then its value", async () => {
  const seen: Array<string> = [];
  const say = (line: string) => () => {
    seen.push(line);
  };
  const body = (): Run =>
    scopeRun(function* (): Run {
      yield addFinalizer(say("outer added first"));
      yield addFinalizer(say("outer added last"));
      yield scopeRun(function* (): Run {
        yield addFinalizer(say("inner"));
        yield yielded(1);
        seen.push("not reached");
        return undefined;
      });
      seen.push("not reached");
      return undefined;
    });
  const arm = (x: unknown) =>
    function* (): Run {
      seen.push("arm ends");
      return x;
    };
  const exit = await root(() => handle([[Yield, [control(arm)]]], body, undefined, undefined));
  assert.equal(exit.value, 1);
  assert.deepEqual(seen, ["arm ends", "inner", "outer added last", "outer added first"]);
});

test("a second resume is a defect that closes the body once", async () => {
  const seen: Array<string> = [];
  const body = (): Run =>
    scopeRun(function* (): Run {
      yield addFinalizer(() => {
        seen.push("closed");
      });
      yield yielded(1);
      yield yielded(2);
      return undefined;
    });
  const arm = (x: unknown) =>
    function* (resume: (v: unknown) => unknown): Run {
      if (x === 2) return "second arm";
      yield resume(undefined);
      return yield resume(undefined);
    };
  const exit = await root(() => handle([[Yield, [control(arm)]]], body, undefined, undefined));
  assert.equal(exit.cause?.reasons[0]?._tag, "Die");
  assert.deepEqual(seen, ["closed"]);
});

test("a fiber stopped inside an arm closes the body the arm holds", async () => {
  const seen: Array<string> = [];
  const body = (): Run =>
    scopeRun(function* (): Run {
      yield addFinalizer(() => {
        seen.push("closed");
      });
      yield yielded(1);
      return undefined;
    });
  const arm = () =>
    function* (): Run {
      yield sleep(1000);
      return undefined;
    };
  function* main(): Run {
    const f = fork(() => handle([[Yield, [control(arm)]]], body, undefined, undefined), currentCtx());
    yield yieldNow();
    f.interrupt(currentFiber().id);
    return asExit(yield awaitFiber(f));
  }
  const exit = asExit((await root(main)).value);
  assert.equal(exit.cause?.reasons[0]?._tag, "Interrupt");
  assert.deepEqual(seen, ["closed"]);
});

test("a nested handler: an operation of the outer one captures through the inner", async () => {
  const body = function* (): Run {
    const v: unknown = yield yielded(1);
    return `${String(ask(undefined))} ${String(v)}`;
  };
  const arm = () =>
    function* (resume: (v: unknown) => unknown): Run {
      return resume("resumed");
    };
  const exit = await root(() =>
    handle(
      [[Yield, [control(arm)]]],
      () => handle([[Ask, [tail("ask", () => "inner evidence")]]], body, undefined, undefined),
      undefined,
      undefined,
    ),
  );
  assert.equal(exit.value, "inner evidence resumed");
});

test("a control operation in another fiber is a defect", async () => {
  function* body(): Run {
    const f = fork(() => yielded(1), currentCtx());
    return asExit(yield awaitFiber(f));
  }
  const arm = () =>
    function* (): Run {
      return "no";
    };
  const exit = asExit((await root(() => handle([[Yield, [control(arm)]]], body, undefined, undefined))).value);
  assert.equal(exit.cause?.reasons[0]?._tag, "Die");
});

test("in a stage that runs at once `try` catches on the stack and an arm may be a call", () => {
  const value = trySync(
    () => {
      return raise(fail("sync"));
    },
    (e) => () => `caught ${String(e)}`,
  );
  assert.equal(value, "caught sync");
  const rethrown = (): unknown =>
    trySync(
      () => {
        return raise(fail("other"));
      },
      () => undefined,
    );
  assert.throws(rethrown, (thrown) => thrown instanceof Raise);
  assert.equal(signalOf({ reasons: [] }, {}), undefined);
});

test("a handler inside a stage that runs at once works: its control arm too", () => {
  const value = runIn(
    handle(
      [[Yield, [control((x) =>
        function* (resume: (v: unknown) => unknown): Run {
          return resume(Number(x) + 1);
        },
      )]]],
      function* (): Run {
        return yield yielded(1);
      },
      undefined,
      undefined,
    ),
  );
  assert.equal(value, 2);
  const exit = runIn(exitOf(() => handle([], () => 3, undefined, undefined)));
  assert.equal(asExit(exit).value, 3);
});
