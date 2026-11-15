/* oxlint-disable require-yield, unicorn/consistent-function-scoping -- exercise compiled generator acquisitions */
import assert from "node:assert/strict";
import { test } from "node:test";

import { effect } from "../src/rt/effect/Layer.ts";
import { fromBuild, layerRun, scoped } from "../src/rt/layer.ts";
import * as rt from "../src/rt/kernel.ts";
import { root } from "./rt-support.ts";

for (const shared of [false, true]) {
  for (const failing of [false, true]) {
    test(`${shared ? "memoized" : "private"} layer acquisition keeps captured-owner finalizers on its build scope (${failing ? "failure" : "success"})`, async () => {
      const events: string[] = [];
      const exit = await root(() => rt.scopeRun((outer) => (function* (): rt.Run {
        const acquire = function* (): rt.Run {
          yield rt.within(outer, () => rt.addFinalizer(() => { events.push("released"); }));
          if (failing) return rt.raise(rt.fail("acquisition failed"));
          return {};
        };
        const layer = shared
          ? effect(rt.service("LayerAcquisition"))(acquire)
          : fromBuild((_memo, into) => scoped(into, function* (): rt.Run {
              yield acquire();
              return [];
            }));
        const built = rt.asExit(yield rt.exitOf(() => layerRun(layer, () => {
          assert.equal(rt.readOwnerEvidence(), outer);
          events.push("body");
        })));
        assert.equal(built.cause?.reasons[0]?._tag, failing ? "Fail" : undefined);
        assert.equal(outer.open, true);
        assert.equal(rt.readOwnerEvidence(), outer);
        assert.deepEqual(events, failing ? ["released"] : ["body", "released"]);
        events.push("outer body");
        return undefined;
      })()));
      assert.equal(exit.cause, undefined);
      assert.deepEqual(events, failing ? ["released", "outer body"] : ["body", "released", "outer body"]);
    });
  }
}

test("a nested layer acquisition replaces its detached caller's owner until its consumer finishes", async () => {
  const events: string[] = [];
  const exit = await root(() => rt.scopeRun((outer) => {
    const layer = effect(rt.service("OuterAcquisition"))(function* (): rt.Run {
      const first = rt.scopeCurrent();
      assert.notEqual(first, outer);
      const nested = effect(rt.service("NestedAcquisition"))(function* (): rt.Run {
        const second = rt.scopeCurrent();
        assert.notEqual(second, first);
        assert.notEqual(second, outer);
        yield rt.within(first, () => rt.addFinalizer(() => { events.push("nested released"); }));
        return {};
      });
      yield layerRun(nested, () => {
        assert.equal(rt.readOwnerEvidence(), first);
        events.push("nested body");
      });
      assert.equal(rt.scopeCurrent(), first);
      assert.equal(rt.readOwnerEvidence(), first);
      assert.deepEqual(events, ["nested body", "nested released"]);
      yield rt.within(outer, () => rt.addFinalizer(() => { events.push("outer released"); }));
      return {};
    });
    return layerRun(layer, () => {
      assert.equal(rt.readOwnerEvidence(), outer);
      events.push("outer body");
    });
  }));
  assert.equal(exit.cause, undefined);
  assert.deepEqual(events, ["nested body", "nested released", "outer body", "outer released"]);
});
