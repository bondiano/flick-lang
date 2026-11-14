/* oxlint-disable require-yield, unicorn/consistent-function-scoping, typescript/no-unsafe-type-assertion -- generators that return a call are what the compiler emits */
// Handlers of the kernel of `effect = "rt"`: `tail`, `abort` and `control`, deep.
import assert from "node:assert/strict";
import { test } from "node:test";

import {
  type OpInstall,
  Under,
  extend,
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
  serving,
  signalOf,
  tryWith,
  trySync,
  yieldNow,
} from "../src/rt/kernel.ts";
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

test("deferred guards suspend, continue on false, and run after body cleanup", async () => {
  const events: Array<string> = [];
  const exit = await root(() => tryWith(
    () => scopeRun(function* (): Run {
      addFinalizer(() => events.push("closed"));
      return raise(fail("body"));
    }),
    (_error, unhandled) => function* (): Run {
      events.push("guard one");
      yield sleep(1);
      const first = false;
      if (first) return "wrong";
      events.push("guard two");
      yield sleep(1);
      const second = true;
      if (second) {
        events.push("selected");
        return "handled";
      }
      return unhandled();
    },
  ));
  assert.equal(exit.value, "handled");
  assert.deepEqual(events, ["closed", "guard one", "guard two", "selected"]);
});

test("a suspended guard failure reaches only the outer try", async () => {
  const events: Array<string> = [];
  const exit = await root(() => tryWith(
    () => tryWith(
      () => raise(fail("body")),
      () => function* (): Run {
        events.push("guard");
        yield sleep(1);
        return raise(fail("guard"));
      },
    ),
    (error) => () => {
      events.push(`outer ${String(error)}`);
      return "outer";
    },
  ));
  assert.equal(exit.value, "outer");
  assert.deepEqual(events, ["guard", "outer guard"]);
});

test("deferred no-match selection rethrows the exact original cause", async () => {
  const original = { reasons: [...fail("body").reasons, ...fail("other").reasons] };
  const exit = await root(() => tryWith(
    () => raise(original),
    (_error, unhandled) => function* (): Run {
      yield sleep(1);
      return unhandled();
    },
  ));
  assert.equal(exit.cause, original);
});

test("a recovery miss is distinct from a guard or selected body raising the same payload", async () => {
  const first = { _tag: "First" };
  const second = { _tag: "Second" };
  const original = { reasons: [...fail(first).reasons, ...fail(second).reasons] };
  await Promise.all((["miss", "guard raises", "body raises", "hit"] as const).map(async (mode) => {
    const events: Array<string> = [];
    const guardFailure = fail(first);
    const exit = await root(() => tryWith(
      () => raise(original),
      (error, unhandled) => {
        assert.equal(error, first, "only the first failure is selected");
        events.push("first");
        return function* (): Run {
          events.push("guard");
          yield sleep(1);
          if (mode === "miss") return unhandled();
          if (mode === "guard raises") return raise(guardFailure);
          const selected = (): unknown => {
            events.push("body");
            return mode === "body raises" ? raise(fail(error)) : "hit";
          };
          return selected();
        };
      },
    ));
    assert.deepEqual(events, mode === "miss" || mode === "guard raises"
      ? ["first", "guard"] : ["first", "guard", "body"]);
    assert.equal(original.reasons.length, 2, "recovery never mutates the incoming cause");
    const [keptFirst, keptSecond] = original.reasons;
    assert.ok(keptFirst?._tag === "Fail" && keptSecond?._tag === "Fail");
    assert.equal(keptFirst.error, first);
    assert.equal(keptSecond.error, second);
    if (mode === "hit") {
      assert.equal(exit.cause, undefined);
      assert.equal(exit.value, "hit");
    } else if (mode === "miss") {
      assert.equal(exit.cause, original, "miss preserves the whole cause by identity");
    } else {
      assert.notEqual(exit.cause, original, "a new raise is not a selector miss");
      assert.equal(exit.cause?.reasons.length, 1, "secondary reasons are discarded");
      const reason = exit.cause?.reasons[0];
      assert.ok(reason?._tag === "Fail");
      assert.equal(reason.error, first, "the payload can be the exact same object");
      if (mode === "guard raises") assert.equal(exit.cause, guardFailure);
    }
  }));
});

test("effect handler error selectors preserve deferred no-match causes", async () => {
  const original = { reasons: [...fail("body").reasons, ...fail("other").reasons] };
  const exit = await root(() => handle([], () => raise(original), undefined,
    (_error, unhandled) => function* (): Run {
      yield sleep(1);
      return unhandled();
    },
  ));
  assert.equal(exit.cause, original);
});

test("partial handlers forward only unmentioned operations to the nearest applicable handler", async () => {
  const pair = service("Test.PartialPair");
  const left = operation(pair, "left", 1) as (u: undefined) => unknown;
  const right = operation(pair, "right", 1) as (u: undefined) => unknown;
  const exit = await root(() => handle(
    [[pair, [tail("left", () => 10), tail("right", () => 20)]]],
    () => handle(
      [[pair, [tail("left", () => 1)]]],
      () => handle(
        [[pair, [tail("right", () => 2)]]],
        () => Number(left(undefined)) + Number(right(undefined)),
        undefined, undefined,
      ),
      undefined, undefined,
    ),
    undefined, undefined,
  ));
  assert.equal(exit.cause, undefined);
  assert.equal(exit.value, 3);
});

const completeProviderBarrier = async (provider: "record" | "layer"): Promise<void> => {
  const pair = service("Test.CompletePair");
  const right = operation(pair, "right", 1) as (u: undefined) => unknown;
  let called = false;
  const exit = await root(() => handle(
    [[pair, [tail("right", () => { called = true; return 20; })]]],
    () => {
      const body = () => handle(
        [[pair, [tail("left", () => 1)]]],
        () => right(undefined),
        undefined, undefined,
      );
      return provider === "record"
        ? new Under(extend(currentCtx(), pair, {}), body)
        : serving(currentCtx(), [[pair, {}]], body);
    },
    undefined, undefined,
  ));
  assert.ok(exit.cause !== undefined, provider);
  assert.equal(called, false, provider);
};

test("a complete record provider is a barrier to missing-operation forwarding", () => completeProviderBarrier("record"));
test("a complete layer provider is a barrier to missing-operation forwarding", () => completeProviderBarrier("layer"));

test("a forwarded arm keeps its outer evidence and failures bypass inner error arms", async () => {
  const pair = service("Test.ForwardedEvidence");
  const right = operation(pair, "right", 1) as (u: undefined) => unknown;
  let caughtInside = false;
  const exit = await root(() => tryWith(() => handle(
    [[Ask, [tail("ask", () => "outer")]]],
    () => handle(
      [[pair, [tail("right", () => raise(fail(ask(undefined))))]]],
      () => handle(
        [[pair, [tail("left", () => 1)]], [Ask, [tail("ask", () => "inner")]]],
        () => right(undefined),
        undefined,
        () => () => { caughtInside = true; return "wrong"; },
      ),
      undefined, undefined,
    ),
    undefined, undefined,
  ), (error) => () => error));
  assert.equal(exit.value, "outer");
  assert.equal(caughtInside, false);
});

test("forwarded dictionary operations resolve at saturation, not partial application", async () => {
  const pair = service("Test.ForwardedCurried");
  const right = operation(pair, "right", 2, [20]) as (a: number) => (b: number) => unknown;
  const exit = await root(() => handle(
    [[pair, [["right", (dict: unknown) => (a: unknown) => (b: unknown) => Number(dict) + Number(a) + Number(b), "tail", 1, 2]]]],
    function* (): Run {
      const staged = yield handle(
        [[pair, [["right", () => () => () => -1, "tail", 1, 2]]]],
        () => right(2),
        undefined, undefined,
      );
      assert.equal(typeof staged, "function");
      return yield handle(
        [[pair, [tail("left", () => 1)]]],
        () => Reflect.apply(staged as (b: number) => unknown, undefined, [3]),
        undefined, undefined,
      );
    },
    undefined, undefined,
  ));
  assert.equal(exit.cause, undefined);
  assert.equal(exit.value, 25);
});

test("a forwarded abort closes inner scopes before its arm and skips their return arms", async () => {
  const pair = service("Test.ForwardedAbort");
  const stop = operation(pair, "stop", 1) as (u: undefined) => unknown;
  const seen: string[] = [];
  const exit = await root(() => handle(
    [[pair, [["stop", () => { seen.push("arm"); return 7; }, "abort", 0, 1]]]],
    () => handle(
      [[pair, [tail("left", () => 1)]]],
      () => scopeRun(function* (): Run {
        addFinalizer(() => seen.push("closed"));
        return yield stop(undefined);
      }),
      () => { seen.push("return"); return -1; },
      undefined,
    ),
    undefined, undefined,
  ));
  assert.equal(exit.value, 7);
  assert.deepEqual(seen, ["closed", "arm"]);
});

test("a forwarded control resumes deeply through the skipped partial handler", async () => {
  const pair = service("Test.ForwardedControl");
  const pause = operation(pair, "pause", 1) as (u: undefined) => unknown;
  const left = operation(pair, "left", 1) as (u: undefined) => unknown;
  const exit = await root(() => handle(
    [[pair, [["pause", () => function* (resume: (v: unknown) => unknown): Run {
      return yield resume(20);
    }, "control", 0, 1]]]],
    () => handle(
      [[pair, [tail("left", () => 1)]]],
      function* (): Run {
        const value = yield pause(undefined);
        return Number(value) + Number(left(undefined));
      },
      (value) => Number(value) + 100,
      undefined,
    ),
    undefined, undefined,
  ));
  assert.equal(exit.cause, undefined);
  assert.equal(exit.value, 121);
});

test("prototype-looking operation names are own arms and forward normally", async () => {
  const tag = service("Test.PrototypeOperations");
  const names = ["__proto__", "constructor", "toString"];
  const exit = await root(() => handle(
    [[tag, names.map((name, index) => tail(name, () => (index + 1) * 10))]],
    () => handle(
      [[tag, [tail("__proto__", () => 1)]]],
      () => names.map((name) => {
        const call = operation(tag, name, 1) as (u: undefined) => unknown;
        return call(undefined);
      }),
      undefined, undefined,
    ),
    undefined, undefined,
  ));
  assert.equal(exit.cause, undefined);
  assert.deepEqual(exit.value, [1, 20, 30]);
});
