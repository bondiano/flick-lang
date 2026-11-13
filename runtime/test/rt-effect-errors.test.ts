import assert from "node:assert/strict";
import { test } from "node:test";

import { combine, die, fail, interrupt, raise, Signal } from "../src/rt/cause.ts";
import { flickCatchDefect } from "../src/rt/effect/Effect.ts";
import { root } from "./rt-support.ts";

const unexpectedRecovery = (): never => { throw new Error("unexpected recovery"); };

test("catch_defect recovers the first defect as inert text", async () => {
  const defect = Object.defineProperty({}, "message", { get() { throw new Error("getter ran"); } });
  const exit = await root(() => flickCatchDefect((text) => `caught ${text}`)(() => raise(combine(die(defect), die("later")))));
  assert.equal(exit.value, "caught <object>");
  assert.equal((await root(() => flickCatchDefect((text) => `caught ${text}`)(() => raise(die("broken"))))).value, "caught broken");
});

test("catch_defect preserves success, typed failures, interruption and handler unwinding", async () => {
  assert.equal((await root(() => flickCatchDefect(unexpectedRecovery)(() => "success"))).value, "success");
  const signal = new Signal({}, undefined, undefined, false);
  await Promise.all([fail("typed"), interrupt(42), fail(signal)].map(async (cause) => {
    const exit = await root(() => flickCatchDefect(unexpectedRecovery)(() => raise(cause)));
    assert.equal(exit.cause, cause);
  }));
});
