// The performance baseline: the time to check the check corpus, the
// language server's hover latency, the throughput of the load servers of
// tools/load at concurrency 16 and the size of the Go one. Writes
// target/perf.json and a table (also to $GITHUB_STEP_SUMMARY) and fails
// when a metric is past its budget in budgets.json; a budget of null is
// only reported.
//
//   node tools/perf/index.mjs <flick binary>
import { execFileSync, spawn, spawnSync } from "node:child_process";
import { appendFileSync, mkdirSync, readdirSync, readFileSync, statSync, writeFileSync } from "node:fs";
import { availableParallelism } from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..");

/** Each metric, its unit and which way is better. */
const METRICS = {
  "check.corpus": { unit: "ms", better: "lower" },
  "lsp.hover.p95": { unit: "ms", better: "lower" },
  "lsp.hover_after_edit.p95": { unit: "ms", better: "lower" },
  "load.json.go.rps": { unit: "req/s", better: "higher" },
  "load.json.go.binary": { unit: "bytes", better: "lower" },
  "load.ssr.rps": { unit: "req/s", better: "higher" },
};

const RUNS = 5;
const HOVERS = 50;

/**
 * Why a measurement fails its budget, or null: a lower-is-better value over
 * `budget * (1 + tolerance)`, a higher-is-better one under `budget * (1 - tolerance)`.
 */
export const failure = (metric, value, { budget, tolerance }) => {
  if (budget === null) return null;
  const { unit, better } = METRICS[metric];
  const limit = better === "lower" ? budget * (1 + tolerance) : budget * (1 - tolerance);
  const past = better === "lower" ? value > limit : value < limit;
  return past ? `${metric} is ${value} ${unit}, past its budget of ${budget} ${unit} with a tolerance of ${tolerance * 100}%` : null;
};

const median = (xs) => [...xs].sort((a, b) => a - b)[Math.floor(xs.length / 2)];

/**
 * Checking the corpus: `flick check tests/check/pass`, then each project of
 * tests/check/projects, whose expected diagnostics are part of it.
 */
const checkCorpus = (flick) => {
  const projects = readdirSync(path.join(root, "tests/check/projects"), { withFileTypes: true })
    .filter((entry) => entry.isDirectory())
    .map((entry) => entry.name)
    .sort();
  const runs = [
    { args: ["check", "tests/check/pass"], ok: [0] },
    // 1 and 2, errors and a usage error such as a bad flick.toml, are what a project may expect;
    // 3, an internal error, or a panic is not
    ...projects.map((p) => ({ args: ["--project", `tests/check/projects/${p}`, "check"], ok: [0, 1, 2] })),
  ];
  const once = () => {
    const started = performance.now();
    for (const { args, ok } of runs) {
      const { status } = spawnSync(flick, ["--color", "never", ...args], { cwd: root, stdio: "ignore" });
      if (!ok.includes(status)) throw new Error(`flick ${args.join(" ")} exited with ${status}`);
    }
    return performance.now() - started;
  };
  once();
  return median(Array.from({ length: RUNS }, once));
};

/** The p95 of the hovers and of the hovers after an edit, from `lsp-bench --repeat`. */
const hovers = (flick) => {
  const out = execFileSync(
    "node",
    ["tools/lsp-bench/index.mjs", "--repeat", String(HOVERS), flick, "examples/mtl/src/Mtl/StateT.fl", "22:5"],
    { cwd: root, encoding: "utf8" },
  );
  process.stdout.write(out);
  const p95 = (label) => Number(new RegExp(`^${label}, \\d+ samples +p50 +[\\d.]+ ms +p95 +([\\d.]+) ms`, "m").exec(out)[1]);
  return { hover: p95("hover"), afterEdit: p95("hover after edit") };
};

const answers = async (url) => {
  for (let i = 0; i < 3000; i++) {
    try {
      const response = await fetch(url);
      await response.text();
      if (response.status === 200) return;
    } catch {
      // not listening yet
    }
    await new Promise((resolve) => setTimeout(resolve, 10));
  }
  throw new Error(`${url} did not answer in 30 s`);
};

/** Requests per second at 16 clients, the median of tools/load's runs, of the server `command` starts. */
const throughput = async (command, { port, path: route, expect, env = {} }) => {
  const server = spawn(command[0], command.slice(1), { cwd: root, env: { ...process.env, ...env, PORT: String(port) }, stdio: "ignore" });
  try {
    await answers(`http://127.0.0.1:${port}${route.replace("{id}", "warm")}`);
    const workers = String(Math.max(1, Math.min(8, availableParallelism() >> 1)));
    const out = execFileSync(
      "node",
      [
        "tools/load/index.mjs",
        ...["--url", `http://127.0.0.1:${port}${route}`, "--expect", expect, "--levels", "16"],
        ...["--warmup", "5", "--run", "5", "--runs", "3", "--soak", "1", "--idle", "0", "--workers", workers, "--pid", String(server.pid)],
      ],
      { cwd: root, encoding: "utf8" },
    );
    process.stdout.write(out);
    return Number(/^\| median \| 16 \| (\d+) \|/m.exec(out)[1]);
  } finally {
    server.kill();
  }
};

const main = async () => {
  const flick = path.resolve(process.argv[2] ?? "target/release/flick");
  const build = (dir, ...args) =>
    execFileSync(flick, ["build", ...args, "--quiet", "--color", "never"], { cwd: path.join(root, dir), stdio: "inherit" });
  const values = {};

  values["check.corpus"] = Math.round(checkCorpus(flick));
  console.log(`check corpus: median ${values["check.corpus"]} ms of ${RUNS}`);

  const { hover, afterEdit } = hovers(flick);
  values["lsp.hover.p95"] = hover;
  values["lsp.hover_after_edit.p95"] = afterEdit;

  build("tools/load/json", "--emit", "go", "--out-dir", "dist/go");
  const go = "tools/load/json/dist/go/flick-server";
  values["load.json.go.binary"] = statSync(path.join(root, go)).size;
  values["load.json.go.rps"] = await throughput([go], { port: 3930, path: "/item/{id}", expect: '"item {id}"' });

  build("tools/load/ssr");
  execFileSync("node", ["tools/load/bundle.mjs", "tools/load/ssr/dist/flick-client.js", "tools/load/ssr/dist/client.js"], {
    cwd: root,
    stdio: "inherit",
  });
  values["load.ssr.rps"] = await throughput(["node", "tools/load/ssr/dist/flick-server.js"], {
    port: 3931,
    path: "/render/{id}",
    expect: "request {id}",
    env: { NODE_ENV: "production", CLIENT_JS: "tools/load/ssr/dist/client.js" },
  });

  const budgets = JSON.parse(readFileSync(path.join(root, "tools/perf/budgets.json"), "utf8"));
  const rows = Object.entries(METRICS).map(([metric, { unit }]) => ({ metric, value: values[metric], unit }));
  mkdirSync(path.join(root, "target"), { recursive: true });
  writeFileSync(path.join(root, "target/perf.json"), `${JSON.stringify(rows, null, 2)}\n`);

  const errors = [];
  const table = ["| metric | value | unit | budget |", "|---|---:|---|---:|"];
  for (const { metric, value, unit } of rows) {
    const budget = budgets[metric];
    if (budget === undefined) {
      errors.push(`${metric} has no entry in tools/perf/budgets.json`);
      continue;
    }
    const failed = failure(metric, value, budget);
    if (failed) errors.push(failed);
    table.push(`| ${metric} | ${value} | ${unit} | ${budget.budget ?? "—"}${failed ? " (failed)" : ""} |`);
  }
  console.log(`\n${table.join("\n")}`);
  if (process.env.GITHUB_STEP_SUMMARY) appendFileSync(process.env.GITHUB_STEP_SUMMARY, `## Performance\n\n${table.join("\n")}\n`);
  for (const e of errors) console.error(`error: ${e}`);
  if (errors.length > 0) process.exitCode = 1;
};

if (process.argv[1] === fileURLToPath(import.meta.url)) await main();
