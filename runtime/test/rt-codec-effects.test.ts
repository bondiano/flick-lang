import assert from "node:assert/strict";
import { test } from "node:test";

import { codecBlock, codecDecode, codecEncode, codecTransformEffect, dslFieldEffect, dslOnMissingEffect } from "../src/rt/codec.ts";
import { Suspend } from "../src/rt/fiber.ts";
import { schemaInt } from "../src/rt/schema.ts";
import { root } from "./rt-support.ts";

test("an effectful missing field waits, then encodes as the same wire field", async () => {
  let resume: ((value: unknown) => void) | undefined;
  const codec = codecBlock("Profile", [["level", dslOnMissingEffect(
    () => new Suspend((resolve) => { resume = resolve; }), dslFieldEffect("level", schemaInt),
  )]]);
  let settled = false;
  const pending = root(() => codecDecode(codec, {})).then((exit) => { settled = true; return exit; });
  await new Promise((resolve) => setTimeout(resolve, 0));
  assert.equal(settled, false);
  assert.ok(resume);
  resume(3);
  assert.deepEqual((await pending).value, { level: 3 });
  assert.deepEqual((await root(() => codecEncode(codec, { level: 3 }))).value, { level: 3 });
});

test("an effectful transform waits for its decoder and preserves an inert input check", async () => {
  let resume: ((value: unknown) => void) | undefined;
  const codec = codecTransformEffect(() => new Suspend((resolve) => { resume = resolve; }), (n) => n, schemaInt);
  let settled = false;
  const pending = root(() => codecDecode(codec, 2)).then((exit) => { settled = true; return exit; });
  await new Promise((resolve) => setTimeout(resolve, 0));
  assert.equal(settled, false);
  assert.ok(resume);
  resume(4);
  assert.equal((await pending).value, 4);

  const active = { get secret(): number { throw new Error("read a getter"); } };
  const rejected = await root(() => codecDecode(codec, active));
  assert.equal(rejected.cause?.reasons[0]?._tag, "Fail");
});
