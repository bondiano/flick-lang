import assert from "node:assert/strict";
import { test } from "node:test";

import { Effect, Option } from "effect";

import { RegistryService, flickLayer } from "../src/effect/reactivity/Atom.ts";
import { ReadService, atomState } from "../src/index.ts";
import { uiField, uiRender, uiText, uiView } from "../src/ui/react/index.ts";
import { uiServerRender } from "../src/ui/server.ts";

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
