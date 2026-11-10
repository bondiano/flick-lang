// Budgets go down: compares the budgets of the checkout, those of
// tools/perf/budgets.json and of tools/bundle-size/programs.mjs, with
// those of a base ref and prints each change. Tightening a budget needs
// nothing. Loosening one, that is raising it (lowering it for a throughput),
// widening its tolerance or setting it back to null, needs a `reason`
// beside it that the base did not have; without one the ratchet fails.
// A budget added or removed is only printed.
//
//   node tools/perf/ratchet.mjs <base ref>
import { execFileSync } from "node:child_process";
import { readFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

import { CHUNKS, PROGRAMS } from "../bundle-size/programs.mjs";
import { METRICS } from "./index.mjs";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..");
const PERF = "tools/perf/budgets.json";
const BUNDLE = "tools/bundle-size/programs.mjs";

const size = (p) => ({ better: "lower", unit: "KB gzip", budget: p.budget, tolerance: 0, reason: p.reason });

/** The budgets of both sources by name, each with which way is better, its unit and tolerance. */
export const wasmBudget = (text) => {
  const match = /test \$\(wc -c < \$wasm\) -le (\d+)/.exec(text);
  if (!match) throw new Error("missing compiler WASM budget in justfile");
  return Number(match[1]);
};

const budgets = (perf, programs, chunks = [], wasm) => ({
  ...(wasm === undefined ? {} : { "wasm.compiler.raw": { better: "lower", unit: "bytes", budget: wasm, tolerance: 0 } }),
  ...Object.fromEntries(Object.entries(perf).map(([metric, b]) => [metric, { better: "lower", unit: "", tolerance: 0, ...METRICS[metric], ...b }])),
  ...Object.fromEntries(programs.map((p) => [`bundle.${p.name}`, size(p)])),
  ...Object.fromEntries(chunks.map((c) => [`chunk.${c.name}`, size(c)])),
});

/** The worst measurement a budget lets pass, or null when it lets anything pass. */
const limit = ({ better, budget, tolerance }) => (budget === null ? null : budget * (better === "lower" ? 1 + tolerance : 1 - tolerance));

const show = ({ budget, unit, tolerance }) => `${budget === null ? "null" : `${budget} ${unit}`}${tolerance ? ` ±${tolerance * 100}%` : ""}`;

/** Each change from the budgets `base` to `head`: a line and, for a looser budget without a new reason, an error. */
export const compare = (base, head) =>
  [...new Set([...Object.keys(base), ...Object.keys(head)])].sort().flatMap((name) => {
    const [was, now] = [base[name], head[name]];
    if (now === undefined) return [{ line: `${name}: removed` }];
    if (was === undefined) return [{ line: `${name}: new, ${show(now)}` }];
    if (was.budget === now.budget && was.tolerance === now.tolerance) return [];
    const [from, to] = [limit(was), limit(now)];
    const line = `${name}: ${show(was)} -> ${show(now)}`;
    const looser = from !== null && (to === null || (now.better === "lower" ? to > from : to < from));
    if (!looser) return [{ line: to === from ? line : `${line}, tighter` }];
    const trimmed = (b) => (typeof b.reason === "string" ? b.reason.trim() : "");
    const reason = trimmed(now);
    if (reason !== "" && reason !== trimmed(was)) return [{ line: `${line}, looser: ${reason}` }];
    return [{ line: `${line}, looser`, error: `${name} has a looser budget without a new \`reason\` beside it` }];
  });

const git = (...args) => execFileSync("git", args, { cwd: root, encoding: "utf8", stdio: ["ignore", "pipe", "pipe"] });

/** The text of `file` at `ref`, or null when the ref has no such file. */
const at = (ref, file) => {
  try {
    return git("show", `${ref}:${file}`);
  } catch {
    return null;
  }
};

const main = async () => {
  const ref = process.argv[2];
  if (!ref) {
    console.error("usage: ratchet.mjs <base ref>");
    process.exit(2);
  }
  // an unknown ref fails here rather than reading as a base without budgets
  try {
    git("rev-parse", "--verify", "--quiet", `${ref}^{commit}`);
  } catch {
    console.error(`error: ${ref} is not a commit; fetch it first`);
    process.exit(2);
  }
  const perf = at(ref, PERF);
  const programs = at(ref, BUNDLE);
  // the module only defines the programs and the chunks
  const bundle = programs === null ? {} : await import(`data:text/javascript;base64,${Buffer.from(programs).toString("base64")}`);
  const base = budgets(perf === null ? {} : JSON.parse(perf), bundle.PROGRAMS ?? [], bundle.CHUNKS, wasmBudget(at(ref, "justfile") ?? ""));
  const head = budgets(JSON.parse(readFileSync(path.join(root, PERF), "utf8")), PROGRAMS, CHUNKS, wasmBudget(readFileSync(path.join(root, "justfile"), "utf8")));
  const changes = compare(base, head);
  if (changes.length === 0) console.log(`no budget changed since ${ref}`);
  for (const { line } of changes) console.log(line);
  for (const { error } of changes) if (error) console.error(`error: ${error}`);
  if (changes.some((c) => c.error)) process.exitCode = 1;
};

if (process.argv[1] === fileURLToPath(import.meta.url)) await main();
