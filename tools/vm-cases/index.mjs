// Run the checked-in `tests/run/model.txt` corpus through `flick run --target vm`
// in both `debug` and `release` profiles with an empty PATH (proving no external
// `node` or `go` process is spawned), asserting exact `.stdout` and `.exit`.
//
//   node tools/vm-cases/index.mjs <flick>
import { spawn } from "node:child_process";
import { existsSync, readFileSync } from "node:fs";
import { availableParallelism } from "node:os";
import { join, resolve } from "node:path";

const [flickArg] = process.argv.slice(2);
if (!flickArg) {
  console.error("usage: index.mjs <flick>");
  process.exit(2);
}

const flick = resolve(flickArg);
const runRoot = resolve("tests/run");
const cases = readFileSync(join(runRoot, "model.txt"), "utf8")
  .split("\n")
  .map((line) => line.split("#")[0].trim())
  .filter((line) => line !== "");

const env = { ...process.env, PATH: "" };
const jobs = Math.max(1, Math.min(availableParallelism(), 8));

function runCase(rel, profile) {
  return new Promise((done) => {
    const fl = join(runRoot, `${rel}.fl`);
    const expectedStdout = readFileSync(join(runRoot, `${rel}.stdout`), "utf8");
    const exitFile = join(runRoot, `${rel}.exit`);
    const expectedExit = existsSync(exitFile)
      ? Number.parseInt(readFileSync(exitFile, "utf8").trim(), 10)
      : 0;
    const child = spawn(flick, ["run", "--target", "vm", "--profile", profile, fl], { env });
    let stdout = "";
    let stderr = "";
    child.stdout.on("data", (chunk) => {
      stdout += chunk;
    });
    child.stderr.on("data", (chunk) => {
      stderr += chunk;
    });
    child.on("close", (code) => {
      if (code === expectedExit && stdout === expectedStdout) {
        done(null);
      } else {
        done({
          rel,
          profile,
          expectedExit,
          actualExit: code,
          stdoutMatch: stdout === expectedStdout,
          stderr: stderr.slice(0, 400),
        });
      }
    });
  });
}

async function runProfile(profile) {
  let next = 0;
  const failures = [];
  async function worker() {
    while (next < cases.length) {
      const rel = cases[next++];
      const problem = await runCase(rel, profile);
      if (problem) failures.push(problem);
    }
  }
  await Promise.all(Array.from({ length: jobs }, worker));
  return failures;
}

let allFailures = [];
for (const profile of ["debug", "release"]) {
  const failures = await runProfile(profile);
  allFailures = allFailures.concat(failures);
  console.log(`vm (${profile}): ${cases.length - failures.length}/${cases.length} passed`);
}

if (allFailures.length > 0) {
  for (const f of allFailures) {
    console.error(
      `FAIL ${f.rel} (${f.profile}): exit ${f.actualExit} (expected ${f.expectedExit}), stdoutMatch=${f.stdoutMatch}\n${f.stderr}`,
    );
  }
  process.exit(1);
}
