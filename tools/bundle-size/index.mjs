// The bundle budget: builds the tracked programs with `flick build`, bundles
// each emitted `flick-main.js`, or `flick-<entry>.js` of a project's
// client entry, for the browser with rolldown (minified, React
// in production mode), prints raw/min/gzip sizes and fails when a gzip size
// exceeds its budget in programs.mjs, a bundle keeps a module none of them
// uses, or a release bundle is larger than the debug one; a size more than
// 10% under its budget prints a note that the budget can be lowered. It
// also prints what labelling atoms costs: release with `labels = "all"`,
// the default, against `"components"`. A program with `rt` is built for
// Flick's own runtime (`--rt`). Host foreign modules (`.ts`/`.mjs`
// beside the source) are not Flick output and stay external.
import { execFileSync } from "node:child_process";
import { rmSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { gzipSync } from "node:zlib";

import { rolldown } from "rolldown";

import { PROGRAMS } from "./programs.mjs";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..");

/** Modules a tracked program does not use, unless it lists its own: a bundle that keeps one is an error. */
const UNUSED = ["effect/dist/Graph.js", "effect/dist/Stream.js"];

const host = (id) => id.startsWith("node:") || id === "jsdom" || (/^\.\.?\//.test(id) && /\.(m?ts|mjs)$/.test(id));

/** Coarse minified source attribution; gzip cannot be apportioned additively. */
export const costGroup = (file) => {
  const name = file.replaceAll("\\", "/");
  if (/\/node_modules\/(?:react|react-dom|scheduler)\//.test(name) || /\/runtime\/(?:src|dist)\/(?:rt\/)?ui\//.test(name)) return "ui";
  if (name.includes("/node_modules/effect/")) return "effect";
  if (name.includes("/packages/effect-ts/")) return "adapter";
  if (/\/runtime\/(?:src|dist)\//.test(name)) return "runtime";
  return "other";
};

const BASE64 = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/** The source-map segments of each generated line: `[column, source]`. */
const segments = (mappings) => {
  let source = 0;
  return mappings.split(";").map((line) => {
    let column = 0;
    return line
      .split(",")
      .filter((s) => s !== "")
      .map((segment) => {
        const fields = [];
        let value = 0;
        let shift = 0;
        for (const char of segment) {
          const digit = BASE64.indexOf(char);
          value += (digit & 31) << shift;
          shift += 5;
          if ((digit & 32) === 0) {
            fields.push(value & 1 ? -(value >>> 1) : value >>> 1);
            value = 0;
            shift = 0;
          }
        }
        column += fields[0];
        if (fields.length > 1) source += fields[1];
        return [column, fields.length > 1 ? source : -1];
      });
  });
};

/** UTF-8 bytes per source; source-map columns count UTF-16 code units. */
export const attribute = (code, map) => {
  const bytes = new Map();
  const lines = code.split("\n");
  segments(map.mappings).forEach((line, i) => {
    line.forEach(([column, source], j) => {
      if (source < 0) return;
      const end = j + 1 < line.length ? line[j + 1][0] : (lines[i]?.length ?? column);
      const name = map.sources[source];
      bytes.set(name, (bytes.get(name) ?? 0) + Buffer.byteLength(lines[i].slice(column, end)));
    });
  });
  return bytes;
};

/**
 * Bundles `entry` and returns its sizes in bytes, the modules it keeps and
 * the minified bytes of the app: the modules beside `entry` other than the
 * built-in ones.
 */
export const measure = async (entry) => {
  const bundle = await rolldown({
    input: entry,
    platform: "browser",
    external: host,
    transform: { define: { "process.env.NODE_ENV": '"production"' } },
    logLevel: "silent",
  });
  const sizes = async (minify) => {
    // the map's sources are relative to `dir`
    const dir = path.dirname(path.resolve(entry));
    const { output } = await bundle.generate({ dir, format: "esm", minify, codeSplitting: false, sourcemap: true });
    const chunks = output.filter((chunk) => chunk.type === "chunk");
    const code = chunks.map((chunk) => chunk.code).join("");
    const modules = chunks.flatMap((chunk) =>
      Object.entries(chunk.modules).flatMap(([id, m]) => (m.renderedLength > 0 ? [id] : [])),
    );
    let app = 0;
    const costs = { runtime: 0, adapter: 0, effect: 0, ui: 0, other: 0 };
    for (const chunk of chunks) {
      for (const [source, n] of attribute(chunk.code, chunk.map)) {
        const file = path.resolve(dir, source);
        costs[costGroup(file)] += n;
        const own = file.startsWith(dir + path.sep) && !file.includes(`${path.sep}_flick${path.sep}`);
        if (own && !/^flick-[a-z0-9-]+\.js$/.test(path.basename(file))) app += n;
      }
    }
    return { bytes: Buffer.byteLength(code), gzip: gzipSync(code).length, modules, app, costs };
  };
  const raw = await sizes(false);
  const min = await sizes(true);
  await bundle.close();
  return { raw: raw.bytes, min: min.bytes, gzip: min.gzip, modules: min.modules, app: min.app, costs: min.costs };
};

/**
 * Compiles `source` into `out` with `flick build`, labelling `labels` when
 * given; a directory is a project, built with its `flick.toml`.
 */
export const build = (flick, source, out, profile, labels, rt = false) => {
  rmSync(out, { recursive: true, force: true });
  const options = [...(labels === undefined ? [] : ["--labels", labels]), ...(rt ? ["--rt"] : [])];
  const project = !source.endsWith(".fl");
  const file = project ? [] : [path.join(root, source)];
  execFileSync(flick, ["build", ...file, "--out-dir", out, "--profile", profile, ...options, "--quiet", "--color", "never"], {
    cwd: project ? path.join(root, source) : root,
    stdio: ["ignore", "inherit", "inherit"],
  });
};

/** The emitted runner a program's bundle starts from. */
const runner = (program) => program.module ?? `flick-${program.entry ?? "main"}.js`;

const kb = (bytes) => (bytes / 1024).toFixed(1);

const main = async () => {
  const flick = process.argv[2] ?? process.env.FLICK_BIN ?? "flick";
  const base = path.join(root, "target/bundle-size");
  const rows = [];
  for (const program of PROGRAMS) {
    const sizes = {};
    for (const profile of ["debug", "release"]) {
      const out = path.join(base, profile, program.name);
      build(flick, program.source, out, profile, undefined, program.rt);
      sizes[profile] = await measure(path.join(out, runner(program)));
    }
    const components = path.join(base, "components", program.name);
    build(flick, program.source, components, "release", "components", program.rt);
    sizes.components = await measure(path.join(components, runner(program)));
    rows.push({ ...program, ...sizes.release, debug: sizes.debug, components: sizes.components });
  }
  console.log("| program | raw KB | min KB | gzip KB | budget KB | app min KB (debug) | atom labels gzip B |");
  console.log("|---|---|---|---|---|---|---|");
  for (const r of rows) {
    const labels = r.gzip - r.components.gzip;
    console.log(`| ${r.name} | ${kb(r.raw)} | ${kb(r.min)} | ${kb(r.gzip)} | ${r.budget} | ${kb(r.app)} (${kb(r.debug.app)}) | ${labels} |`);
  }
  console.log("\n| program | runtime min KB | adapter min KB | Effect min KB | UI/React min KB |");
  console.log("|---|---:|---:|---:|---:|");
  for (const r of rows) console.log(`| ${r.name} | ${kb(r.costs.runtime)} | ${kb(r.costs.adapter)} | ${kb(r.costs.effect)} | ${kb(r.costs.ui)} |`);
  const errors = rows.flatMap((r) => [
    ...(r.gzip > r.budget * 1024 ? [`${r.name} is ${kb(r.gzip)} KB gzip, over its budget of ${r.budget} KB`] : []),
    ...(r.min > r.debug.min ? [`${r.name} is ${r.min} B minified in release, more than ${r.debug.min} B in debug`] : []),
    ...(r.unused ?? UNUSED).filter((m) => r.modules.some((id) => id.replaceAll("\\", "/").endsWith(m))).map(
      (m) => `${r.name} keeps ${m}, which it does not use`,
    ),
    ...(r.effectFree && r.modules.some((id) => id.replaceAll("\\", "/").includes("/node_modules/effect/"))
      ? [`${r.name} is built without effects, but its bundle keeps Effect`]
      : []),
  ]);
  // the budgets carry a KB or so of slack; 10% hides real growth
  for (const r of rows.filter((r) => r.gzip < r.budget * 1024 * 0.9)) {
    console.log(`\nnote: ${r.name} is ${kb(r.gzip)} KB gzip, more than 10% under its budget of ${r.budget} KB: the budget can be lowered`);
  }
  for (const e of errors) console.error(`error: ${e}`);
  if (errors.length > 0) process.exitCode = 1;
};

if (process.argv[1] === fileURLToPath(import.meta.url)) await main();
