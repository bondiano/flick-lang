import assert from "node:assert/strict";
import { test } from "node:test";

import { Cause, Effect, Exit, Option, Schema } from "effect";
import * as Atom from "effect/reactivity/Atom";

import { RegistryService, flickLayer } from "../src/effect/reactivity/Atom.ts";
import { ReadService, atomState } from "../src/index.ts";
import { uiField, uiRender, uiText, uiView } from "../src/ui/react/index.ts";
import { uiServerRender, uiServerState } from "../src/ui/server.ts";

test("requests rendered at once each read the atoms of their own registry", async () => {
  const count = atomState(0);
  const name = atomState("");
  const view = uiView(() =>
    Effect.map(Effect.flatMap(Effect.service(ReadService), (read) => read.get(count)), (n) => uiText(`count ${String(n)}`)),
  );
  const page = uiRender(view, undefined);
  const request = (n: number) =>
    Effect.provide(
      Effect.gen(function* () {
        const registry = yield* Effect.service(RegistryService);
        yield* registry.set(count)(n);
        yield* registry.set(name)(`user ${n}`);
        yield* Effect.sleep(5);
        return [yield* uiServerRender(page), yield* uiServerRender(uiField({ bind: name, placeholder: "", on_input: Option.none() }))];
      }),
      flickLayer(RegistryService),
    );
  // oxlint-disable-next-line typescript/no-unsafe-type-assertion -- the layer provides the request's one service
  const both = await Effect.runPromise(Effect.all([request(1), request(2)], { concurrency: 2 }) as Effect.Effect<unknown>);
  assert.deepEqual(both, [
    ["count 1", '<input placeholder="" value="user 1"/>'],
    ["count 2", '<input placeholder="" value="user 2"/>'],
  ]);
});

test("the state script holds each serializable atom of the registry, and one that does not encode is a defect", async () => {
  const count = Atom.serializable(Atom.make(0), { key: "count", schema: Schema.Number });
  const whole = Atom.serializable(Atom.make(0), { key: "whole", schema: Schema.Int });
  const state = (n: number) =>
    Effect.provide(
      Effect.gen(function* () {
        const registry = yield* Effect.service(RegistryService);
        yield* registry.set(count)(n);
        yield* registry.set(whole)(n);
        return yield* uiServerState();
      }),
      flickLayer(RegistryService),
    );
  // oxlint-disable-next-line typescript/no-unsafe-type-assertion -- the layer provides the request's one service
  const run = (n: number) => Effect.runPromiseExit(state(n) as Effect.Effect<string>);
  const kept = await run(2);
  assert.ok(Exit.isSuccess(kept));
  assert.equal(
    kept.value,
    '<script type="application/json" id="flick-state">[{"key":"count","value":2},{"key":"whole","value":2}]</script>',
  );
  const refused = await run(1.5);
  assert.ok(Exit.isFailure(refused));
  assert.match(Cause.pretty(refused.cause), /the serializable atom `whole` does not encode its value/);
});
