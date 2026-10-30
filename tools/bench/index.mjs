// Benchmarks of the JS and the go target, as gleam's benchmark/: each
// `tools/bench/*.fl` is built in release for both, run `runs` times after
// one warm-up, and timed as a whole process (the go target has no clock a
// program could read), so `startup` is the part of every row that is only
// starting. Both targets must print the same checksum.
//
//   node tools/bench/index.mjs target/debug/flick [runs]
import { spawnSync } from "node:child_process";
import { readdirSync, readFileSync } from "node:fs";
import { cpus } from "node:os";
import { basename, join } from "node:path";

const [flick, runsArg = "5"] = process.argv.slice(2);
if (!flick) {
  console.error("usage: index.mjs <flick> [runs]");
  process.exit(2);
}
const runs = Number(runsArg);
const here = import.meta.dirname;
const out = join(here, "../../target/bench");
const exe = process.platform === "win32" ? "flick-main.exe" : "flick-main";

const run = (command, args) => {
  const result = spawnSync(command, args, { encoding: "utf8", maxBuffer: 1 << 26 });
  if (result.status !== 0) throw new Error(`${command} ${args.join(" ")} failed:\n${result.stderr}`);
  return result.stdout;
};

/** The median wall time of `runs` runs in ms, after one warm-up, and the output. */
const time = (command, args) => {
  const output = run(command, args);
  const times = [];
  for (let i = 0; i < runs; i++) {
    const started = performance.now();
    run(command, args);
    times.push(performance.now() - started);
  }
  times.sort((a, b) => a - b);
  return { ms: times[times.length >> 1], output };
};

/** The 1, 5 and 15 minute load averages, where Linux tells them. */
const loadavg = () => {
  try {
    return readFileSync("/proc/loadavg", "utf8").split(" ").slice(0, 3).join(" ");
  } catch {
    return "unknown";
  }
};
const loadBefore = loadavg();

const programs = readdirSync(here)
  .filter((f) => f.endsWith(".fl"))
  .map((f) => basename(f, ".fl"))
  .sort((a, b) => (a === "startup" ? -1 : b === "startup" ? 1 : a.localeCompare(b)));

const rows = [];
for (const name of programs) {
  const source = join(here, `${name}.fl`);
  for (const emit of ["js", "go"]) {
    run(flick, ["build", source, "--emit", emit, "--profile", "release", "--out-dir", join(out, emit, name)]);
  }
  const js = time("node", [join(out, "js", name, "flick-main.js")]);
  const go = time(join(out, "go", name, exe), []);
  if (js.output !== go.output) throw new Error(`${name}: js printed ${js.output}, go printed ${go.output}`);
  rows.push(`| ${name} | ${js.ms.toFixed(0)} | ${go.ms.toFixed(0)} | ${(go.ms / js.ms).toFixed(2)} |`);
}

const go = run("go", ["version"]).trim().replace(/^go version /, "");
console.log(`${cpus()[0]?.model ?? "unknown CPU"}, Node ${process.version}, ${go}`);
console.log(`load average ${loadBefore} before, ${loadavg()} after`);
console.log(`median of ${runs} runs after a warm-up, whole process, release\n`);
console.log("| program | JS, ms | Go, ms | Go / JS |\n| - | -: | -: | -: |");
for (const row of rows) console.log(row);
