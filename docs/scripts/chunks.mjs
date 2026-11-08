// The chunks the site loads apart from its pages: each stays in its gzip budget,
// and no page loads it before it asks for it. Runs on docs/dist after a build.
import { readdirSync, readFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { gzipSync } from "node:zlib";

import { CHUNKS } from "../../tools/bundle-size/programs.mjs";

const dist = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../dist");
const assets = path.join(dist, "_astro");
const files = readdirSync(assets).filter((f) => f.endsWith(".js"));

/** The HTML pages of the site. */
const pages = (dir) =>
  readdirSync(dir, { withFileTypes: true }).flatMap((e) =>
    e.isDirectory() ? pages(path.join(dir, e.name)) : e.name.endsWith(".html") ? [path.join(dir, e.name)] : [],
  );

/** Every script a page loads before its code runs: those it names and what they import statically. */
const eager = new Set();
const visit = (file) => {
  if (eager.has(file) || !files.includes(file)) return;
  eager.add(file);
  // minified static imports: `import"./a.js"`, `from"./a.js"`; `import("./a.js")` waits until it is called
  for (const [, name] of readFileSync(path.join(assets, file), "utf8").matchAll(/(?:\bimport|\bfrom)\s*["']\.\/([^"']+\.js)["']/g)) visit(name);
};
for (const page of pages(dist)) {
  for (const [, name] of readFileSync(page, "utf8").matchAll(/\/_astro\/([^"'?]+\.js)/g)) visit(name);
}

let failed = false;
for (const { name, budget } of CHUNKS) {
  const parts = files.filter((f) => f.startsWith(`${name}.`));
  if (parts.length === 0) {
    console.error(`error: no chunk ${name} in docs/dist/_astro`);
    failed = true;
    continue;
  }
  const kb = parts.reduce((sum, f) => sum + gzipSync(readFileSync(path.join(assets, f))).length, 0) / 1024;
  console.log(`${name}: ${kb.toFixed(1)} KB gzip of ${budget} KB, in ${parts.join(", ")}`);
  if (kb > budget) {
    console.error(`error: ${name} is over its budget in tools/bundle-size/programs.mjs`);
    failed = true;
  }
  for (const part of parts.filter((p) => eager.has(p))) {
    console.error(`error: a page loads ${part} before it asks for it`);
    failed = true;
  }
}
if (failed) process.exitCode = 1;
