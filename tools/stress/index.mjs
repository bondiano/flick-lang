// Stress of the runtime contract (docs/formal/runtime.md §6.1): each
// portable case whose path starts with one of `prefixes` is built for both
// targets and run `runs` times on each with FLICK_STRESS set to the run's
// seed, so that checkpoints, forks and Effect's scheduler take other
// interleavings, and the monitor of λRT checks the trace of every run. The
// first violation stops it, with the case, the target, the seed and the
// command that repeats the run. Not part of CI.
//
// `FLICK_STRESS_TARGETS` names the targets, `js,go` by default: `rt` is Flick's
// own runtime, for the cases of `tests/run/rt.txt`.
//
//   node tools/stress/index.mjs target/debug/flick target/debug/examples/rt_trace [runs] [prefix…]
import { spawnSync } from "node:child_process";
import { mkdirSync, readFileSync, rmSync } from "node:fs";
import { join } from "node:path";

const [flick, monitor, runsArg = "20", ...given] = process.argv.slice(2);
if (!flick || !monitor) {
  console.error("usage: index.mjs <flick> <rt_trace> [runs] [prefix…]");
  process.exit(2);
}
const runs = Number(runsArg);
const prefixes = given.length > 0 ? given : ["runtime/"];
const root = join(import.meta.dirname, "../..");
const out = join(root, "target/stress");
const exe = process.platform === "win32" ? "flick-main.exe" : "flick-main";

const listed = (file) =>
  readFileSync(join(root, "tests/run", file), "utf8")
    .split("\n")
    .map((l) => l.split("#")[0].trim())
    .filter((l) => l !== "" && prefixes.some((p) => l.startsWith(p)));
const portable = listed("portable.txt");
const onRt = listed("rt.txt");
const wanted = (process.env.FLICK_STRESS_TARGETS ?? "js,go").split(",");
const cases = [...new Set([...portable, ...onRt])];

// `<target> <case>`: a case that target does not hold yet, which may not even build there
const gaps = new Set(
  readFileSync(join(root, "tests/run/known_gaps.txt"), "utf8")
    .split("\n")
    .map((l) => l.split("#")[0].trim().replace(/\s+/, " "))
    .filter((l) => l !== ""),
);

const targets = {
  js: (dir) => ["node", ["--enable-source-maps", join(dir, "flick-main.js")]],
  go: (dir) => [join(dir, exe), []],
  rt: (dir) => ["node", ["--enable-source-maps", join(dir, "flick-main.js")]],
};

let failed = false;
for (const name of cases) {
  for (const [target, command] of Object.entries(targets)) {
    if (!wanted.includes(target) || !(target === "rt" ? onRt : portable).includes(name)) continue;
    if (gaps.has(`${target} ${name}`)) {
      console.log(`${name} (${target}): a known gap, skipped`);
      continue;
    }
    const dir = join(out, target, name);
    rmSync(dir, { recursive: true, force: true });
    mkdirSync(dir, { recursive: true });
    const build = spawnSync(
      flick,
      target === "rt"
        ? ["build", join(root, "tests/run", `${name}.fl`), "--out-dir", dir, "--rt"]
        : ["build", join(root, "tests/run", `${name}.fl`), "--out-dir", dir, "--emit", target],
      { encoding: "utf8" },
    );
    if (build.status !== 0) {
      console.error(`${name} (${target}): build failed\n${build.stderr}`);
      process.exit(1);
    }
    const [program, args] = command(dir);
    for (let seed = 1; seed <= runs && !failed; seed++) {
      const trace = join(dir, `trace-${seed}.jsonl`);
      const env = { ...process.env, FLICK_STRESS: String(seed), FLICK_TRACE: trace };
      const ran = spawnSync(program, args, { env, encoding: "utf8", timeout: 120_000 });
      if (ran.error) {
        console.error(`${name} (${target}, seed ${seed}): ${ran.error.message}`);
        failed = true;
        break;
      }
      const checked = spawnSync(monitor, [trace], { encoding: "utf8" });
      if (checked.status !== 0) {
        console.error(`${name} (${target}, seed ${seed}): ${checked.stderr}`);
        console.error(`again: FLICK_STRESS=${seed} FLICK_TRACE=${trace} ${[program, ...args].join(" ")}`);
        failed = true;
      } else {
        rmSync(trace);
      }
    }
    if (failed) process.exit(1);
    console.log(`${name} (${target}): ${runs} runs`);
  }
}
console.log(`stress: ${cases.length} cases, ${runs} runs each on ${wanted.join(" and ")}, no violation`);
