// The performance baseline: the time to check the check corpus, the
// language server's hover latency, the throughput of the load servers of
// tools/load at concurrency 16 and the size of the Go one, and the resident
// memory of the language server and those servers at their peak. Writes
// target/perf.json and a table (also to $GITHUB_STEP_SUMMARY) and fails
// when a metric is past its budget in budgets.json; a budget of null is
// only reported. A metric that beats its budget by a factor of more than
// 1 + 2 × tolerance prints a note that the budget can be tightened;
// loosening one takes a `reason` beside it (ratchet.mjs).
//
//   node tools/perf/index.mjs <flick binary>
import { execFileSync, spawn, spawnSync } from "node:child_process";
import { appendFileSync, mkdirSync, readdirSync, readFileSync, statSync, writeFileSync } from "node:fs";
import { availableParallelism } from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..");

/** Each metric, its unit and which way is better. */
export const METRICS = {
  "check.corpus": { unit: "ms", better: "lower" },
  "lsp.hover.p95": { unit: "ms", better: "lower" },
  "lsp.hover_after_edit.p95": { unit: "ms", better: "lower" },
  "load.json.go.rps": { unit: "req/s", better: "higher" },
  "load.json.go.binary": { unit: "bytes", better: "lower" },
  "load.ssr.rps": { unit: "req/s", better: "higher" },
  "lsp.rss.peak": { unit: "MiB", better: "lower" },
  "load.json.go.rss.peak": { unit: "MiB", better: "lower" },
  "load.ssr.rss.peak": { unit: "MiB", better: "lower" },
};

/** The peak in MiB that tools/load and tools/lsp-bench print. */
export const peakRss = (out) => Number(/^RSS, MiB: .*\bpeak ([\d.]+)/m.exec(out)?.[1]);

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

/** A note that a measurement beats its budget by a factor of more than `1 + 2 * tolerance`, or null. */
export const slack = (metric, value, { budget, tolerance }) => {
  if (budget === null) return null;
  const { unit, better } = METRICS[metric];
  const far = better === "lower" ? value * (1 + 2 * tolerance) < budget : value > budget * (1 + 2 * tolerance);
  return far ? `${metric} is ${value} ${unit}, well within its budget of ${budget} ${unit}: it can be ${better === "lower" ? "lowered" : "raised"}` : null;
};

const median = (xs) => [...xs].sort((a, b) => a - b)[Math.floor(xs.length / 2)];

/**
 * Checking the corpus: `flick check` of each area of tests/check/pass, then each
 * project of tests/check/projects, whose expected diagnostics are part of it.
 */
export const checkCorpus = (flick) => {
  const dirs = (dir) =>
    readdirSync(path.join(root, dir), { withFileTypes: true })
      .filter((entry) => entry.isDirectory())
      .map((entry) => entry.name)
      .sort();
  const runs = [
    // area by area: fixtures of different areas may declare the same module
    ...dirs("tests/check/pass").map((d) => ({ args: ["check", `tests/check/pass/${d}`], ok: [0] })),
    // 1 and 2, errors and a usage error such as a bad flick.toml, are what a project may expect;
    // 3, an internal error, or a panic is not
    ...dirs("tests/check/projects").map((p) => ({ args: ["--project", `tests/check/projects/${p}`, "check"], ok: [0, 1, 2] })),
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
  return { hover: p95("hover"), afterEdit: p95("hover after edit"), rss: peakRss(out) };
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
const throughput = async (command, { port, path: route, expect, env = {}, client = "fetch" }) => {
  const server = spawn(command[0], command.slice(1), { cwd: root, env: { ...process.env, ...env, PORT: String(port) }, stdio: "ignore" });
  try {
    await answers(`http://127.0.0.1:${port}${route.replace("{id}", "warm")}`);
    // Go JSON selects direct GETs because fetch limits its throughput even
    // with workers. SSR keeps its historical client for comparable results.
    const workers = String(Math.max(1, Math.min(8, availableParallelism() >> 1)));
    const out = execFileSync(
      "node",
      [
        "tools/load/index.mjs",
        ...["--url", `http://127.0.0.1:${port}${route}`, "--expect", expect, "--levels", "16"],
        ...["--warmup", "5", "--run", "5", "--runs", "3", "--soak", "1", "--idle", "0", "--client", client, "--workers", workers, "--pid", String(server.pid)],
      ],
      { cwd: root, encoding: "utf8" },
    );
    process.stdout.write(out);
    return { rps: Number(/^\| median \| 16 \| (\d+) \|/m.exec(out)[1]), rss: peakRss(out) };
  } finally {
    server.kill();
  }
};

const main = async () => {
  const flick = path.resolve(process.argv[2] ?? "target/release/flick");
  const build = (dir, args, env = process.env) =>
    execFileSync(flick, ["build", ...args, "--quiet", "--color", "never"], { cwd: path.join(root, dir), stdio: "inherit", env });
  const values = {};

  const errors = [];
  try {
    values["check.corpus"] = Math.round(checkCorpus(flick));
    console.log(`check corpus: median ${values["check.corpus"]} ms of ${RUNS}`);

    const { hover, afterEdit, rss } = hovers(flick);
    values["lsp.hover.p95"] = hover;
    values["lsp.hover_after_edit.p95"] = afterEdit;
    values["lsp.rss.peak"] = rss;

    // the binary's budget is that of the Go in runtime-go/go.mod, which CI sets up: a newer
    // Go builds a bigger one, so a local run builds with that version, which Go fetches once
    const version = /^go (\S+)$/m.exec(readFileSync(path.join(root, "runtime-go/go.mod"), "utf8"))[1];
    const goroot = execFileSync("go", ["env", "GOROOT"], {
      env: { ...process.env, GOTOOLCHAIN: `go${version}` },
      encoding: "utf8",
    }).trim();
    const withGo = { ...process.env, PATH: `${path.join(goroot, "bin")}${path.delimiter}${process.env.PATH}` };
    build("tools/load/json", ["--emit", "go", "--out-dir", "dist/go"], withGo);
    const go = "tools/load/json/dist/go/flick-server";
    values["load.json.go.binary"] = statSync(path.join(root, go)).size;
    const json = await throughput([go], { port: 3930, path: "/item/{id}", expect: '"item {id}"', client: "http" });
    values["load.json.go.rps"] = json.rps;
    values["load.json.go.rss.peak"] = json.rss;

    build("tools/load/ssr", []);
    execFileSync("node", ["tools/load/bundle.mjs", "tools/load/ssr/dist/flick-client.js", "tools/load/ssr/dist/client.js"], {
      cwd: root,
      stdio: "inherit",
    });
    const ssr = await throughput(["node", "tools/load/ssr/dist/flick-server.js"], {
      port: 3931,
      path: "/render/{id}",
      expect: "request {id}",
      env: { NODE_ENV: "production", CLIENT_JS: "tools/load/ssr/dist/client.js" },
    });
    values["load.ssr.rps"] = ssr.rps;
    values["load.ssr.rss.peak"] = ssr.rss;

  } catch (error) {
    errors.push(error.message);
  } finally {
    const budgets = JSON.parse(readFileSync(path.join(root, "tools/perf/budgets.json"), "utf8"));
    const result = report(values, budgets, errors);
    mkdirSync(path.join(root, "target"), { recursive: true });
    writeFileSync(path.join(root, "target/perf.json"), `${JSON.stringify(result.rows, null, 2)}\n`);
    writeFileSync(path.join(root, "target/perf-errors.json"), `${JSON.stringify(result.errors, null, 2)}\n`);
    console.log(`\n${result.table}`);
    if (process.env.GITHUB_STEP_SUMMARY) appendFileSync(process.env.GITHUB_STEP_SUMMARY, `## Performance\n\n${result.table}\n`);
    for (const n of result.notes) console.log(`note: ${n}`);
    for (const e of result.errors) console.error(`error: ${e}`);
    if (result.errors.length > 0) process.exitCode = 1;
  }
};

/** Reports completed metrics even when a later stage failed. Missing metrics fail the run. */
export const report = (values, budgets, failures = []) => {
  const rows = [];
  const errors = [...failures];
  const notes = [];
  const table = ["| metric | value | unit | budget |", "|---|---:|---|---:|"];
  for (const [metric, { unit }] of Object.entries(METRICS)) {
    const value = values[metric];
    const budget = budgets[metric];
    if (budget === undefined) errors.push(`${metric} has no entry in tools/perf/budgets.json`);
    if (!Number.isFinite(value)) {
      errors.push(`${metric} was not measured`);
      table.push(`| ${metric} | — | ${unit} | ${budget?.budget ?? "—"} (not measured) |`);
      continue;
    }
    rows.push({ metric, value, unit });
    const failed = budget && failure(metric, value, budget);
    if (failed) errors.push(failed);
    const note = budget && slack(metric, value, budget);
    if (note) notes.push(note);
    table.push(`| ${metric} | ${value} | ${unit} | ${budget?.budget ?? "—"}${failed ? " (failed)" : ""} |`);
  }
  return { rows, errors, notes, table: table.join("\n") };
};

if (process.argv[1] === fileURLToPath(import.meta.url)) await main();
