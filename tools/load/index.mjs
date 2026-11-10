// A closed-loop HTTP load: `c` clients, each sending its next request when
// the last one answers. Every request carries a fresh id in place of `{id}`
// in the URL, and its answer must contain each `|`-separated text of
// `--expect` with that id, so an answer meant for another request counts
// as an error. After a warm-up it
// runs each concurrency `--runs` times and prints the medians, then holds
// the highest one for `--soak` seconds. With `--pid` it samples the
// server's resident memory (Linux, /proc) before the soak, at its peak and
// after `--idle` seconds of rest. With `--workers N` the clients are split
// over N threads to spread load generation over several cores.
// `--client http` uses direct HTTP/1 GETs with keep-alive and no redirects;
// the default `fetch` client remains available for comparisons.
//
//   node tools/load/index.mjs --url 'http://127.0.0.1:3000/render/{id}' \
//     --expect 'request {id}' [--pid N] [--levels 1,16,64] [--run 10] \
//     [--runs 3] [--warmup 10] [--soak 60] [--idle 30] [--timeout 5] [--workers 1] [--client fetch|http]
import { readFileSync } from "node:fs";
import { parseArgs } from "node:util";
import { isMainThread, parentPort, Worker, workerData } from "node:worker_threads";

import { HttpClient } from "./client.mjs";

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
    workers: { type: "string", default: "1" },
    client: { type: "string", default: "fetch" },
  },
  args: isMainThread ? process.argv.slice(2) : workerData,
  allowPositionals: !isMainThread,
});
if (!options.url?.includes("{id}")) {
  console.error("usage: index.mjs --url <url with {id}> [--expect <text with {id}>] [--pid N] ...");
  process.exit(2);
}
if (!["fetch", "http"].includes(options.client)) {
  console.error("--client must be fetch or http");
  process.exit(2);
}
const httpClient = options.client === "http" ? new HttpClient() : undefined;
const levels = options.levels.split(",").map(Number);
const expected = options.expect.split("|");
const seconds = (name) => Number(options[name]) * 1000;

const workers = Number(options.workers);
let next = 0;
const totals = { ok: 0, status: 0, body: 0, transport: 0, timeout: 0 };

const one = async () => {
  const id = `r${isMainThread ? "" : `${workerData.at(-1)}-`}${++next}`;
  const started = performance.now();
  try {
    const url = options.url.replaceAll("{id}", id);
    const signal = AbortSignal.timeout(seconds("timeout"));
    const response = httpClient ? await httpClient.get(url, { signal }) : await fetch(url, { signal });
    const body = httpClient ? response.body : await response.text();
    if (response.status !== 200) return { error: "status" };
    if (!expected.every((text) => body.includes(text.replaceAll("{id}", id)))) return { error: "body" };
    return { ms: performance.now() - started };
  } catch (error) {
    return { error: error.name === "TimeoutError" ? "timeout" : "transport" };
  }
};

/** `c` clients on this thread for `ms`: the latencies of the answers and the count of each error. */
const clients = async (c, ms) => {
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
  return { latencies, errors };
};

if (!isMainThread) {
  parentPort.on("message", async ({ c, ms }) => parentPort.postMessage(await clients(c, ms)));
}

const threads = isMainThread && workers > 1
  ? Array.from({ length: workers }, (_, i) => {
      const worker = new Worker(new URL(import.meta.url), { workerData: [...process.argv.slice(2), `w${i}`] });
      worker.unref();
      return worker;
    })
  : [];
const onThread = (worker, c, ms) =>
  new Promise((resolve) => {
    worker.once("message", resolve);
    worker.postMessage({ c, ms });
  });

/** `c` clients for `ms`, split over the worker threads: the rate, latency quantiles and errors. */
const load = async (c, ms) => {
  const parts = threads.length
    ? await Promise.all(threads.map((w, i) => onThread(w, Math.floor(c / threads.length) + (i < c % threads.length ? 1 : 0), ms)))
    : [await clients(c, ms)];
  const latencies = parts.flatMap((p) => p.latencies);
  const errors = { status: 0, body: 0, transport: 0, timeout: 0 };
  for (const p of parts) for (const [kind, n] of Object.entries(p.errors)) errors[kind] += n;
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

if (isMainThread) {
  console.log(`url ${options.url}`);
  console.log(`client ${options.client}`);
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
  httpClient?.close();
  for (const t of threads) t.terminate();
}
