import assert from "node:assert/strict";
import { test } from "node:test";
import { METRICS, budgetOn, coldStart, peakRss, report } from "./index.mjs";
import { createServer } from "node:http";

const budgets = Object.fromEntries(Object.keys(METRICS).map((key) => [key, { budget: null, tolerance: 0.25 }]));

test("a stopped run retains measurements, budget failures and its cause", () => {
  const result = report({ "check.corpus": 26271 }, { ...budgets, "check.corpus": { budget: 3400, tolerance: 0.25 } }, ["SSR build failed"]);
  assert.deepEqual(result.rows, [{ metric: "check.corpus", value: 26271, unit: "ms" }]);
  assert.ok(result.errors.includes("SSR build failed"));
  assert.ok(result.errors.some((e) => e.includes("check.corpus is 26271")));
  assert.ok(result.errors.some((e) => e.includes("load.ssr.rps was not measured")));
  assert.match(result.table, /load.ssr.rps \| — .*not measured/);
});

test("every metric must be finite and have a budget entry", () => {
  const values = Object.fromEntries(Object.keys(METRICS).map((key) => [key, 1]));
  assert.deepEqual(report(values, budgets).errors, []);
  assert.ok(report({ ...values, "check.corpus": NaN }, budgets).errors.some((e) => e.includes("check.corpus was not measured")));
  assert.ok(report(values, {}).errors.some((e) => e.includes("has no entry")));
});

test("unavailable RSS stays missing without discarding the other measurements", () => {
  assert.ok(Number.isNaN(peakRss("hover, 50 samples p95 0.3 ms")));
});

test("cold start measures fresh HTTP servers and waits for each to exit", async () => {
  const reservation = createServer();
  await new Promise((resolve) => reservation.listen(0, "127.0.0.1", resolve));
  const port = reservation.address().port;
  await new Promise((resolve) => reservation.close(resolve));
  const program = 'require("node:http").createServer((req,res)=>res.end("ready")).listen(Number(process.env.PORT),"127.0.0.1")';
  const ms = await coldStart([process.execPath, "-e", program], { port, path: "/{id}" }, 2);
  assert.ok(Number.isFinite(ms) && ms > 0);
  await assert.rejects(fetch(`http://127.0.0.1:${port}/after`));
});

test("cold start rejects an HTTP listener owned by another process", async () => {
  const server = createServer((_req, res) => res.end("unrelated"));
  await new Promise((resolve) => server.listen(0, "127.0.0.1", resolve));
  try {
    const port = server.address().port;
    await assert.rejects(coldStart([process.execPath, "-e", "process.exit(1)"], { port, path: "/" }, 1), /in use|occupied/);
  } finally {
    await new Promise((resolve) => server.close(resolve));
  }
});

test("a runner's CPU model selects its own budget, any other CPU the metric's", () => {
  const ssr = { budget: 5200, tolerance: 0.25, runners: { "AMD EPYC 7763": { budget: 3300, reason: "slower CPU" } } };
  const on = (cpu) => report({ "load.ssr.rps": 3300 }, { ...budgets, "load.ssr.rps": ssr }, [], cpu);
  const slow = on("AMD EPYC 7763 64-Core Processor");
  assert.ok(!slow.errors.some((e) => e.includes("load.ssr.rps is")), slow.errors.join("\n"));
  assert.match(slow.table, /load.ssr.rps \| 3300 \| req\/s \| 3300 on AMD EPYC 7763 \|/);
  const fast = on("Intel(R) Xeon(R) 6975P-C");
  assert.ok(fast.errors.includes("load.ssr.rps is 3300 req/s, past its budget of 5200 req/s with a tolerance of 25%"));
  assert.equal(budgetOn(ssr).budget, 5200);
  const failed = report({ "load.ssr.rps": 2000 }, { ...budgets, "load.ssr.rps": ssr }, [], "AMD EPYC 7763 64-Core Processor");
  assert.ok(failed.errors.some((e) => e.endsWith("on AMD EPYC 7763")), failed.errors.join("\n"));
});
