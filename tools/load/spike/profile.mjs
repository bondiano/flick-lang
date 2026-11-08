// Reads the spike's measurements.
//
// `cpu`: the self time of `node --cpu-prof` profiles, by where the code is
// (the bridge, the kernel, Effect, compiled std, the program, node), with the
// busiest functions and, where the kernel ran, its busiest functions. Each
// mode's profile is the first `*.cpuprofile` in `<dir>/<mode>`.
//
// `micro`: the output of micro.sh, each measurement on both modes, the ratio
// of `js` to `rt`, and on `rt` what a crossing costs over the loop without one.
// A mode measured more than once counts by its median.
//
//   node tools/load/spike/profile.mjs cpu [--dir /tmp/prof] <mode>...
//   node tools/load/spike/profile.mjs micro [results/micro-2.txt]
import { readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { parseArgs } from "node:util";

const USAGE = "usage: profile.mjs cpu [--dir <dir>] <mode>... | profile.mjs micro [<micro.sh output>]";

const IDLE = "(idle)";
const TOP_FUNCTIONS = 14;
const TOP_KERNEL_FUNCTIONS = 12;

/** Where a frame's code is: the categories a profile is summed by. */
const categoryOf = (url, functionName) => {
  if (functionName === IDLE) return IDLE;
  if (url === "") return functionName === "(garbage collector)" || functionName === "(program)" ? "(native/gc/idle)" : "(native)";
  if (url.includes("/runtime/dist/rt/bridge")) return "bridge";
  if (url.includes("/runtime/dist/rt/")) return "kernel rt/*";
  if (url.includes("/runtime/dist/")) return "runtime (Effect side)";
  if (url.includes("/node_modules/")) {
    if (url.includes("/effect/")) return "effect";
    if (url.includes("react") || url.includes("scheduler")) return "react";
    return "other deps";
  }
  if (url.includes("/_flick/")) return "compiled std";
  if (url.includes("/dist/")) return "program";
  if (url.startsWith("node:")) return "node";
  return "other";
};

const fileOf = (url) => url.slice(url.lastIndexOf("/") + 1);

/** Adds `amount` to the total of `key`. */
const add = (totals, key, amount) => totals.set(key, (totals.get(key) ?? 0) + amount);

/** The entries of `totals`, the largest first. */
const largest = (totals) => [...totals].toSorted((a, b) => b[1] - a[1]);

const seconds = (micros) => (micros / 1e6).toFixed(2);

const readProfile = (dir, mode) => {
  const modeDir = join(dir, mode);
  let file;
  try {
    file = readdirSync(modeDir).find((name) => name.endsWith(".cpuprofile"));
  } catch {
    throw new Error(`no directory ${modeDir}`);
  }
  if (file === undefined) throw new Error(`no .cpuprofile in ${modeDir}`);
  return JSON.parse(readFileSync(join(modeDir, file), "utf8"));
};

/** The self time of each frame of `profile`, in microseconds: a sample's time goes to the frame on top. */
const selfTimes = (profile) => {
  const frames = new Map(profile.nodes.map((node) => [node.id, node.callFrame]));
  const byNode = new Map();
  profile.samples.forEach((id, i) => add(byNode, id, profile.timeDeltas[i]));
  return [...byNode].map(([id, time]) => ({ frame: frames.get(id), time }));
};

const summarize = (mode, profile) => {
  const byCategory = new Map();
  const byFunction = new Map();
  const byKernelFunction = new Map();
  for (const { frame, time } of selfTimes(profile)) {
    const category = categoryOf(frame.url, frame.functionName);
    if (category === IDLE) continue;
    const name = frame.functionName || "(anon)";
    add(byCategory, category, time);
    add(byFunction, `${category.padEnd(20)} ${name} ${fileOf(frame.url)}:${frame.lineNumber}`, time);
    if (frame.url.includes("/runtime/dist/rt/")) add(byKernelFunction, `${frame.url.split("/dist/rt/").at(-1)}  ${name}`, time);
  }
  const busy = [...byCategory.values()].reduce((sum, time) => sum + time, 0);

  console.log(`== ${mode}: busy ${seconds(busy)} s`);
  for (const [category, time] of largest(byCategory)) {
    console.log(`  ${category.padEnd(24)} ${seconds(time).padStart(6)} s ${((100 * time) / busy).toFixed(1).padStart(5)}%`);
  }
  console.log("  top functions:");
  for (const [key, time] of largest(byFunction).slice(0, TOP_FUNCTIONS)) {
    console.log(`    ${seconds(time).padStart(5)} s  ${key}`);
  }
  if (byKernelFunction.size > 0) {
    console.log("  kernel functions:");
    for (const [key, time] of largest(byKernelFunction).slice(0, TOP_KERNEL_FUNCTIONS)) {
      console.log(`    ${(time / 1e3).toFixed(1).padStart(7)} ms  ${key}`);
    }
  }
};

/** micro.sh's output: `== <mode> <run>` before each run, then `<measurement>\t<ns per iteration>`. */
const readMicro = (file) => {
  const runs = new Map();
  let mode;
  readFileSync(file, "utf8")
    .split("\n")
    .forEach((line, i) => {
      if (line.trim() === "") return;
      if (line.startsWith("==")) {
        mode = line.split(/\s+/)[1];
        return;
      }
      const [name, value] = line.split("\t");
      const ns = Number(value);
      if (mode === undefined || value === undefined || Number.isNaN(ns)) throw new Error(`${file}:${i + 1}: not a measurement: ${line}`);
      const byMode = runs.get(name) ?? new Map();
      byMode.set(mode, [...(byMode.get(mode) ?? []), ns]);
      runs.set(name, byMode);
    });
  return runs;
};

const median = (xs) => {
  const sorted = xs.toSorted((a, b) => a - b);
  const mid = Math.floor(sorted.length / 2);
  return sorted.length % 2 === 1 ? sorted[mid] : (sorted[mid - 1] + sorted[mid]) / 2;
};

/** On `rt`, what each of `names` costs over the measurement `baseline`. */
const over = (rt, baseline, names) =>
  names.map((name) => `${name} +${Math.round(rt.get(name) - rt.get(baseline))}`).join(", ");

const summarizeMicro = (file) => {
  const rt = new Map();
  for (const [name, byMode] of readMicro(file)) {
    const [r, j] = ["rt", "js"].map((mode) => byMode.get(mode));
    if (r === undefined || j === undefined) throw new Error(`${file}: ${name} is not measured on both rt and js`);
    rt.set(name, median(r));
    const ratio = median(j) / median(r);
    console.log(`${name.padEnd(24)} rt ${median(r).toFixed(0).padStart(8)} ns  js ${median(j).toFixed(0).padStart(8)} ns  js/rt x${ratio.toFixed(1).padStart(5)}`);
  }
  console.log(`rt, a call of Effect from the kernel, ns over kernel_loop_baseline: ${over(rt, "kernel_loop_baseline", ["from_effect_succeed", "from_effect_sync", "from_effect_two_steps"])}`);
  console.log(`rt, a call of Flick from Effect, ns over effect_loop_baseline: ${over(rt, "effect_loop_baseline", ["callback_plain", "callback_generator", "callback_suspends"])}`);
};

const { values: options, positionals } = parseArgs({ options: { dir: { type: "string", default: "/tmp/prof" } }, allowPositionals: true });
const [command, ...rest] = positionals;
try {
  if (command === "cpu" && rest.length > 0) for (const mode of rest) summarize(mode, readProfile(options.dir, mode));
  else if (command === "micro" && rest.length <= 1) summarizeMicro(rest[0] ?? new URL("results/micro-2.txt", import.meta.url).pathname);
  else {
    console.error(USAGE);
    process.exit(2);
  }
} catch (error) {
  console.error(`profile.mjs: ${error.message}`);
  process.exit(1);
}
