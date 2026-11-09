import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtempSync, readFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { test } from "node:test";

// The trace is read from FLICK_TRACE when the runtime loads: a process of its own.
test("a traced scope writes its open, its finalizer, its close phases and the run, in order", () => {
  const path = join(mkdtempSync(join(tmpdir(), "flick-trace-")), "trace");
  const result = spawnSync(
    process.execPath,
    [
      "--input-type=module",
      "--eval",
      `
        import { Effect } from "effect";
        import { scope } from "@flick-lang/effect-ts";
        import { flickAddFinalizer } from "@flick-lang/effect-ts/effect/Scope";
        await Effect.runPromise(scope(() => flickAddFinalizer(Effect.void)));
      `,
    ],
    { env: { ...process.env, FLICK_TRACE: path }, encoding: "utf8" },
  );
  assert.equal(result.status, 0, result.stderr);
  const events = readFileSync(path, "utf8")
    .trim()
    .split("\n")
    .map((line): unknown => JSON.parse(line));
  assert.deepEqual(events[0], { e: "open", o: 1, p: 0, k: "scope" });
  assert.deepEqual(
    events.map((e): unknown => {
      assert.ok(typeof e === "object" && e !== null);
      const name: unknown = Reflect.get(e, "e");
      return name === "phase" ? (Reflect.get(e, "c") as unknown) : name;
    }),
    ["open", "fin", "C0", "C1", "C2", "C3", "C4", "C5", "run", "C6"],
  );
});
