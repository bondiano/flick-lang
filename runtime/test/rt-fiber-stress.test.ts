import assert from "node:assert/strict";
import { test } from "node:test";

import { stressing } from "../src/rt/fiber-stress.ts";

test("stress configuration preserves disabled, seeded and fallback choices", (t) => {
  const saved = process.env["FLICK_STRESS"];
  t.after(() => {
    if (saved === undefined) delete process.env["FLICK_STRESS"];
    else process.env["FLICK_STRESS"] = saved;
  });
  delete process.env["FLICK_STRESS"];
  assert.equal(stressing(), undefined);
  process.env["FLICK_STRESS"] = "";
  assert.equal(stressing(), undefined);

  const expected = [0.6270739405881613, 0.002735721180215478, 0.5274470399599522, 0.9810509674716741];
  for (const seed of ["1", "0", "not-a-number"]) {
    process.env["FLICK_STRESS"] = seed;
    const random = stressing();
    assert.ok(random);
    assert.deepEqual(expected.map(() => random()), expected);
    assert.equal(process.env["FLICK_STRESS"], seed, "configuration does not consume the host variable");
  }
  process.env["FLICK_STRESS"] = "42";
  const first = stressing();
  const second = stressing();
  assert.ok(first);
  assert.ok(second);
  const choices = expected.map(() => first());
  assert.deepEqual(expected.map(() => second()), choices);
  assert.notDeepEqual(choices, expected);
});
