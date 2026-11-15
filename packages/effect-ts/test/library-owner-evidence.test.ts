import assert from "node:assert/strict";
import { test } from "node:test";
import { Context, Effect } from "effect";
import { effect, effectContext, effectDiscard } from "../src/effect/Layer.ts";
import { provide } from "../src/layer.ts";
import { acquireRelease, readOwnerEvidence, scope, scopeCurrent, within } from "../src/index.ts";

for (const curried of [false, true]) {
  test(`a ${curried ? "curried" : "direct"} layer acquisition owns its callback and releases before the outer scope`, async () => {
    const key = Context.Service<number>(`Test.LayerEvidence.${curried}`);
    let released = false;
    await Effect.runPromise(scope(() => Effect.gen(function* () {
      const outer = yield* readOwnerEvidence();
      const acquire = Effect.gen(function* () {
        const owner = yield* readOwnerEvidence();
        assert.notEqual(owner, outer);
        assert.equal(owner, yield* scopeCurrent());
        return yield* within(owner, acquireRelease(Effect.succeed(42), () => Effect.sync(() => { released = true; })));
      });
      const layer = curried ? effect(key)(acquire) : effect(key, acquire);
      const value = yield* provide(layer, Effect.service(key));
      assert.equal(value, 42);
      assert.equal(released, true);
      assert.equal(yield* readOwnerEvidence(), outer);
    })));
  });
}

for (const discard of [false, true]) {
  test(`layer ${discard ? "effectDiscard" : "effectContext"} supplies acquisition evidence`, async () => {
    await Effect.runPromise(scope(() => Effect.gen(function* () {
      const outer = yield* readOwnerEvidence();
      const acquire = Effect.gen(function* () {
        const owner = yield* readOwnerEvidence();
        assert.notEqual(owner, outer);
        assert.equal(owner, yield* scopeCurrent());
        return Context.empty();
      });
      yield* provide(discard ? effectDiscard(acquire) : effectContext(acquire), Effect.void);
      assert.equal(yield* readOwnerEvidence(), outer);
    })));
  });
}
