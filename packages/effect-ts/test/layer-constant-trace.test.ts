import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { test } from "node:test";

const layerModule = new URL("../src/layer.ts", import.meta.url).href;

// FLICK_TRACE is consumed at module load, so exercise the real traced boundary
// in a fresh process. Constants have no acquisition or memo scope in λRT §3.5.
const traced = (program: string): ReadonlyArray<Readonly<Record<string, unknown>>> => {
  const directory = mkdtempSync(join(tmpdir(), "flick-constant-layer-"));
  const path = join(directory, "trace");
  try {
    const result = spawnSync(process.execPath, ["--input-type=module", "--eval", `
      import assert from "node:assert/strict";
      import { Context, Effect, Exit, Layer } from "effect";
      import { layerSucceed, layerMerge, layerOverride, provide } from ${JSON.stringify(layerModule)};
      ${program}
    `], { cwd: new URL("..", import.meta.url), env: { ...process.env, FLICK_TRACE: path }, encoding: "utf8" });
    assert.equal(result.status, 0, result.stderr);
    return readFileSync(path, "utf8").trim().split("\n").map((line) => {
      const event: unknown = JSON.parse(line);
      assert.ok(typeof event === "object" && event !== null);
      return Object.fromEntries(Object.entries(event));
    });
  } finally {
    rmSync(directory, { recursive: true, force: true });
  }
};

test("shared constant merges retain right override without memo builds or releases", () => {
  const events = traced(`
    const key = Context.Service("Test.Constant");
    const other = Context.Service("Test.Other");
    let factories = 0;
    let reads = 0;
    const make = () => { factories++; return { get value() { reads++; return "other"; } }; };
    const implementation = make();
    const left = layerSucceed(key, "wrong");
    const right = layerSucceed(key, "right");
    const graph = layerMerge(right, layerSucceed(other, implementation));
    const shared = layerOverride(graph, graph);
    const merged = layerOverride(left, shared);
    const read = Effect.all([Effect.service(key), Effect.service(other)]);
    assert.equal(factories, 1);
    assert.equal(reads, 0);
    assert.deepEqual(await Effect.runPromise(provide(merged, provide(graph, read))), ["right", implementation]);
    assert.deepEqual(await Effect.runPromise(provide(graph, read)), ["right", implementation]);
    assert.ok(Exit.isFailure(await Effect.runPromiseExit(provide(shared, Effect.fail("expected")))));
    assert.equal(factories, 1);
    assert.equal(reads, 0);
  `);
  assert.deepEqual(events.filter((event) => event["e"] === "build" || event["e"] === "release"), []);
  assert.deepEqual(events.filter((event) => event["e"] === "open").map((event) => event["k"]),
    ["provide", "provide", "provide", "provide"]);
});

for (const outcome of ["success", "failure", "interrupt"]) {
  test(`constant merge optimization preserves real acquisition traces on ${outcome}`, () => {
    const events = traced(`
      const key = Context.Service("Test.Resource");
      const other = Context.Service("Test.Constant");
      const log = [];
      const resource = Layer.effect(key, Effect.acquireRelease(
        Effect.sync(() => { log.push("acquire"); return "resource"; }),
        () => Effect.sync(() => { log.push("release"); }),
      ));
      const constants = layerMerge(layerSucceed(other, "wrong"), layerSucceed(other, "constant"));
      const graph = layerMerge(resource, constants);
      const shared = layerMerge(graph, graph);
      const body = Effect.andThen(Effect.sync(() => log.push("body")),
        ${outcome === "success" ? "Effect.all([Effect.service(key), Effect.service(other)])" : outcome === "failure" ? "Effect.fail('expected')" : "Effect.interrupt"});
      for (let i = 0; i < 2; i++) {
        const exit = await Effect.runPromiseExit(provide(shared, provide(graph, body)));
        assert.equal(Exit.isSuccess(exit), ${outcome === "success"});
        ${outcome === "success" ? 'assert.deepEqual(exit.value, ["resource", "constant"]);' : ""}
      }
      assert.deepEqual(log, ["acquire", "body", "release", "acquire", "body", "release"]);
    `);
    // Both mixed composites and the resource are still memoized; only the
    // constant-only subgraph has no build scope. Shared/nested consumers reuse
    // each build, while sibling provides rebuild all three.
    const identities = (kind: string): Array<number> => events.filter((event) => event["e"] === kind).map((event) => {
      const id = event["l"];
      assert.ok(typeof id === "number");
      return id;
    });
    const builds = identities("build");
    const releases = identities("release");
    assert.equal(builds.length, 6);
    assert.equal(new Set(builds).size, 3);
    assert.deepEqual(releases.toSorted((a, b) => a - b), builds.toSorted((a, b) => a - b));
  });
}
