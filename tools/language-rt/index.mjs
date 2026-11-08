// The language tests on Flick's own runtime: runs `flick test --rt` in
// tests/language with the seed of `just language`, and holds its report to
// the one on Effect, but for the tests tests/language/rt-gaps.txt lists. A
// line of it is a module, which the run leaves out (it does not load on the
// kernel), or a prefix of full test names, which must fail there: a gap that
// passes, or names nothing, is reported, so its line goes.
//
//   node tools/language-rt/index.mjs <flick> <seed> <report on Effect>
import { spawnSync } from "node:child_process";
import { readFileSync, readdirSync } from "node:fs";
import { join, resolve } from "node:path";

const [flick, seed, effectReport] = process.argv.slice(2);
if (!flick || !seed || !effectReport) {
  console.error("usage: index.mjs <flick> <seed> <report on Effect>");
  process.exit(2);
}
const dir = "tests/language";
const gaps = readFileSync(join(dir, "rt-gaps.txt"), "utf8")
  .split("\n")
  .map((line) => line.split("#")[0].trim())
  .filter((line) => line !== "");
const modules = new Set(gaps.filter((gap) => !gap.includes(" > ")));
const files = readdirSync(join(dir, "test/Language"))
  .filter((file) => file.endsWith(".fl") && !modules.has(`Language.${file.slice(0, -3)}`))
  .map((file) => `test/Language/${file}`);
const run = spawnSync(resolve(flick), ["test", "--rt", "--emit", "js", "--seed", seed, "--reporter", "json", ...files], {
  cwd: dir,
  encoding: "utf8",
});
let rt;
try {
  rt = JSON.parse(run.stdout);
} catch {
  console.error(`flick test --rt wrote no report (exit ${run.status}):\n${run.stdout}${run.stderr}`);
  process.exit(1);
}
const effect = JSON.parse(readFileSync(effectReport, "utf8"));
const gapOf = (name) => gaps.find((gap) => name === gap || name.startsWith(`${gap} > `));
const problems = [];
for (const gap of gaps.filter((g) => g.includes(" > "))) {
  const named = rt.tests.filter((t) => gapOf(t.name) === gap);
  if (named.length === 0) problems.push(`rt-gaps.txt: \`${gap}\` names no test`);
  for (const t of named.filter((t) => t.status === "passed")) problems.push(`${t.name} passes on the kernel: remove its line from rt-gaps.txt`);
}
const kept = (report) => JSON.stringify(report.tests.filter((t) => gapOf(t.name) === undefined), null, 1);
const [onEffect, onRt] = [kept(effect), kept(rt)];
if (rt.seed !== effect.seed) problems.push(`seed ${rt.seed} on the kernel, ${effect.seed} on Effect`);
if (onEffect !== onRt) {
  const a = onEffect.split("\n");
  const b = onRt.split("\n");
  const at = a.findIndex((line, i) => line !== b[i]);
  problems.push(`the reports differ from line ${at + 1}:\n  Effect: ${a.slice(at, at + 6).join(" ")}\n  rt:     ${b.slice(at, at + 6).join(" ")}`);
}
if (problems.length > 0) {
  console.error(problems.join("\n"));
  process.exit(1);
}
const left = effect.tests.length - JSON.parse(onEffect).length;
console.log(`rt: ${JSON.parse(onRt).length} tests as on Effect; ${left} in rt-gaps.txt`);
