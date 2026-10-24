// The bundle budget: builds the tracked programs with `flick build`, bundles
// each emitted `flick-main.js` for the browser with rolldown (minified, React
// in production mode), prints raw/min/gzip sizes and fails when a gzip size
// exceeds its budget. Host foreign modules (`.ts`/`.mjs` beside the source)
// are not Flick output and stay external.
import { execFileSync } from "node:child_process";
import { rmSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { gzipSync } from "node:zlib";

import { rolldown } from "rolldown";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..");

/** Programs and their gzip budgets in KB. */
export const PROGRAMS = [
  { name: "pure", source: "tools/bundle-size/pure.fl", budget: 240 },
  { name: "counter", source: "tests/run/ui/counter.fl", budget: 310 },
  { name: "playground", source: "docs/src/playground/Playground.fl", budget: 316 },
];

const host = (id) => id.startsWith("node:") || id === "jsdom" || (/^\.\.?\//.test(id) && /\.(m?ts|mjs)$/.test(id));

/** Bundles `entry` and returns its sizes in bytes. */
export const measure = async (entry) => {
  const bundle = await rolldown({
    input: entry,
    platform: "browser",
    external: host,
    transform: { define: { "process.env.NODE_ENV": '"production"' } },
    logLevel: "silent",
  });
  const sizes = async (minify) => {
    const { output } = await bundle.generate({ format: "esm", minify, codeSplitting: false });
    const code = output.map((chunk) => (chunk.type === "chunk" ? chunk.code : "")).join("");
    return { bytes: Buffer.byteLength(code), gzip: gzipSync(code).length };
  };
  const raw = await sizes(false);
  const min = await sizes(true);
  await bundle.close();
  return { raw: raw.bytes, min: min.bytes, gzip: min.gzip };
};

/** Compiles `source` into `out` with `flick build`. */
export const build = (flick, source, out, profile) => {
  rmSync(out, { recursive: true, force: true });
  execFileSync(flick, ["build", path.join(root, source), "--out-dir", out, "--profile", profile, "--quiet", "--color", "never"], {
    cwd: root,
    stdio: ["ignore", "inherit", "inherit"],
  });
};

const kb = (bytes) => (bytes / 1024).toFixed(1);

const main = async () => {
  const flick = process.argv[2] ?? process.env.FLICK_BIN ?? "flick";
  const base = path.join(root, "target/bundle-size");
  const rows = [];
  for (const program of PROGRAMS) {
    const out = path.join(base, program.name);
    build(flick, program.source, out, "release");
    rows.push({ ...program, ...(await measure(path.join(out, "flick-main.js"))) });
  }
  console.log("| program | raw KB | min KB | gzip KB | budget KB |");
  console.log("|---|---|---|---|---|");
  for (const r of rows) console.log(`| ${r.name} | ${kb(r.raw)} | ${kb(r.min)} | ${kb(r.gzip)} | ${r.budget} |`);
  const over = rows.filter((r) => r.gzip > r.budget * 1024);
  for (const r of over) console.error(`error: ${r.name} is ${kb(r.gzip)} KB gzip, over its budget of ${r.budget} KB`);
  if (over.length > 0) process.exitCode = 1;
};

if (process.argv[1] === fileURLToPath(import.meta.url)) await main();
