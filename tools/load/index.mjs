// A closed-loop HTTP load: `c` clients, each sending its next request when
// the last one answers. Every request carries a fresh id in place of `{id}`
// in the URL, and its answer must contain each `|`-separated text of
// `--expect` with that id, so an answer meant for another request counts
// as an error. After a warm-up it
// runs each concurrency `--runs` times and prints the medians, then holds
// the highest one for `--soak` seconds. With `--pid` it samples the
// server's resident memory (Linux, /proc) before the soak, at its peak and
// after `--idle` seconds of rest.
//
//   node tools/load/index.mjs --url 'http://127.0.0.1:3000/render/{id}' \
//     --expect 'request {id}' [--pid N] [--levels 1,16,64] [--run 10] \
//     [--runs 3] [--warmup 10] [--soak 60] [--idle 30] [--timeout 5]
import { readFileSync } from "node:fs";
import { parseArgs } from "node:util";

const { values: options } = parseArgs({
  options: {
    url: { type: "string" },
    expect: { type: "string", default: "" },
    pid: { type: "string" },
    levels: { type: "string", default: "1,16,64" },
    run: { type: "string", default: "10" },
    runs: { type: "string", default: "3" },
    warmup: { type: "string", default: "10" },
    soak: { type: "string", default: "60" },
    idle: { type: "string", default: "30" },
    timeout: { type: "string", default: "5" },
  },
});
if (!options.url?.includes("{id}")) {
  console.error("usage: index.mjs --url <url with {id}> [--expect <text with {id}>] [--pid N] ...");
  process.exit(2);
}
const levels = options.levels.split(",").map(Number);
const expected = options.expect.split("|");
const seconds = (name) => Number(options[name]) * 1000;

let next = 0;
const totals = { ok: 0, status: 0, body: 0, transport: 0, timeout: 0 };

const one = async () => {
  const id = `r${++next}`;
  const started = performance.now();
  try {
    const response = await fetch(options.url.replaceAll("{id}", id), { signal: AbortSignal.timeout(seconds("timeout")) });
    const body = await response.text();
    if (response.status !== 200) return { error: "status" };
    if (!expected.every((text) => body.includes(text.replaceAll("{id}", id)))) return { error: "body" };
    return { ms: performance.now() - started };
  } catch (error) {
    return { error: error.name === "TimeoutError" ? "timeout" : "transport" };
  }
};

/** `c` clients for `ms`: the latencies of the answers and the count of each error. */
const load = async (c, ms) => {
  const until = performance.now() + ms;
  const latencies = [];
  const errors = { status: 0, body: 0, transport: 0, timeout: 0 };
  const client = async () => {
    while (performance.now() < until) {
      const result = await one();
      if (result.error) errors[result.error]++;
      else latencies.push(result.ms);
    }
  };
  await Promise.all(Array.from({ length: c }, client));
  totals.ok += latencies.length;
  for (const [kind, n] of Object.entries(errors)) totals[kind] += n;
  latencies.sort((a, b) => a - b);
  const at = (q) => latencies[Math.min(latencies.length - 1, Math.floor(q * latencies.length))] ?? NaN;
  return { rps: latencies.length / (ms / 1000), p50: at(0.5), p95: at(0.95), p99: at(0.99), errors };
};

const median = (xs) => [...xs].sort((a, b) => a - b)[Math.floor(xs.length / 2)];
const rss = () =>
  options.pid ? Number(/VmRSS:\s+(\d+)/.exec(readFileSync(`/proc/${options.pid}/status`, "utf8"))[1]) / 1024 : NaN;
const row = (label, c, r) =>
  console.log(
    `| ${label} | ${c} | ${r.rps.toFixed(0)} | ${r.p50.toFixed(2)} | ${r.p95.toFixed(2)} | ${r.p99.toFixed(2)} |`,
  );

console.log(`url ${options.url}`);
await load(16, seconds("warmup"));
console.log("| path | c | req/s | p50, ms | p95, ms | p99, ms |\n|---|---:|---:|---:|---:|---:|");
for (const c of levels) {
  const runs = [];
  for (let i = 0; i < Number(options.runs); i++) runs.push(await load(c, seconds("run")));
  row(
    "median",
    c,
    Object.fromEntries(["rps", "p50", "p95", "p99"].map((k) => [k, median(runs.map((r) => r[k]))])),
  );
}
const before = rss();
let peak = before;
const sampler = setInterval(() => (peak = Math.max(peak, rss())), 1000);
const top = Math.max(...levels);
row(`soak ${options.soak} s`, top, await load(top, seconds("soak")));
clearInterval(sampler);
await new Promise((resolve) => setTimeout(resolve, seconds("idle")));
console.log(`RSS, MiB: before soak ${before.toFixed(1)}, peak ${peak.toFixed(1)}, after ${options.idle} s idle ${rss().toFixed(1)}`);
console.log(`answers ${totals.ok}; errors: ${JSON.stringify({ ...totals, ok: undefined })}`);
process.exitCode = totals.status + totals.body + totals.transport + totals.timeout === 0 ? 0 : 1;
