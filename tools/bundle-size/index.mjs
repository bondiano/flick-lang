// The bundle budget: builds the tracked programs with `flick build`, bundles
// each emitted `flick-main.js`, or `flick-<entry>.js` of a project's
// client entry, for the browser with rolldown (minified, React
// in production mode), prints raw/min/gzip sizes and fails when a gzip size
// exceeds its budget, a bundle keeps a module none of them uses, or a
// release bundle is larger than the debug one. It also prints what labelling
// atoms costs: release with `labels = "all"`, the default, against
// `"components"`. Host
// foreign modules (`.ts`/`.mjs` beside the source) are not Flick output and
// stay external.
import { execFileSync } from "node:child_process";
import { rmSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { gzipSync } from "node:zlib";

import { rolldown } from "rolldown";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..");

/** Modules a tracked program does not use, unless it lists its own: a bundle that keeps one is an error. */
const UNUSED = ["effect/dist/Graph.js", "effect/dist/Stream.js"];

/** Programs and their gzip budgets in KB. */
export const PROGRAMS = [
  { name: "pure", source: "tools/bundle-size/pure.fl", budget: 12 },
  // the same program with `effect = "none"`, a project of its own: no Effect at all
  { name: "none", source: "tools/bundle-size/none", budget: 2, effectFree: true },
  // UI programs render views on atoms: at most 81 KB and 18 KB for effect/reactivity (U8),
  // and 5 KB for the spans of actions and the debug log of writes;
  // the registry keeps `Stream`, which `effect/reactivity` imports for stream atoms
  { name: "counter", source: "tests/run/ui/counter.fl", budget: 104, unused: ["effect/dist/Graph.js"] },
  { name: "playground", source: "docs/src/playground/Playground.fl", budget: 112, unused: ["effect/dist/Graph.js"] },
  { name: "atoms", source: "tests/run/ui/atoms.fl", budget: 104, unused: ["effect/dist/Graph.js"] },
  // a browser client of a procedure keeps no Node server
  {
    name: "rpc",
    source: "tools/bundle-size/rpc.fl",
    budget: 52,
    unused: ["effect/dist/Graph.js", "platform-node/dist/NodeHttpServer.js", "platform-node-shared/dist/NodeSocketServer.js"],
  },
  // a template renders without `react-dom/server` (R6); its view is still a React component
  {
    name: "email",
    source: "tests/run/ui/email.fl",
    budget: 32,
    unused: ["effect/dist/Graph.js", "react-dom/cjs/react-dom-server.browser.production.js", "react-dom/cjs/react-dom-server-legacy.browser.production.js"],
  },
  // the client entry of a project with a server entry keeps no server module (F1)
  {
    name: "fullstack",
    source: "tools/bundle-size/fullstack",
    entry: "web",
    budget: 12,
    unused: [...UNUSED, "App/Db.js", "App/Api.js", "Server.js"],
  },
];

const host = (id) => id.startsWith("node:") || id === "jsdom" || (/^\.\.?\//.test(id) && /\.(m?ts|mjs)$/.test(id));

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

/** Characters of `code` per source of `map`, as source-map-explorer counts them. */
const attribute = (code, map) => {
  const bytes = new Map();
  const lines = code.split("\n");
  segments(map.mappings).forEach((line, i) => {
    line.forEach(([column, source], j) => {
      if (source < 0) return;
      const end = j + 1 < line.length ? line[j + 1][0] : (lines[i]?.length ?? column);
      const name = map.sources[source];
      bytes.set(name, (bytes.get(name) ?? 0) + end - column);
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
    for (const chunk of chunks) {
      for (const [source, n] of attribute(chunk.code, chunk.map)) {
        const file = path.resolve(dir, source);
        const own = file.startsWith(dir + path.sep) && !file.includes(`${path.sep}_flick${path.sep}`);
        if (own && !/^flick-[a-z0-9-]+\.js$/.test(path.basename(file))) app += n;
      }
    }
    return { bytes: Buffer.byteLength(code), gzip: gzipSync(code).length, modules, app };
  };
  const raw = await sizes(false);
  const min = await sizes(true);
  await bundle.close();
  return { raw: raw.bytes, min: min.bytes, gzip: min.gzip, modules: min.modules, app: min.app };
};

/**
 * Compiles `source` into `out` with `flick build`, labelling `labels` when
 * given; a directory is a project, built with its `flick.toml`.
 */
export const build = (flick, source, out, profile, labels) => {
  rmSync(out, { recursive: true, force: true });
  const options = labels === undefined ? [] : ["--labels", labels];
  const project = !source.endsWith(".fl");
  const file = project ? [] : [path.join(root, source)];
  execFileSync(flick, ["build", ...file, "--out-dir", out, "--profile", profile, ...options, "--quiet", "--color", "never"], {
    cwd: project ? path.join(root, source) : root,
    stdio: ["ignore", "inherit", "inherit"],
  });
};

/** The emitted runner a program's bundle starts from. */
const runner = (program) => `flick-${program.entry ?? "main"}.js`;

const kb = (bytes) => (bytes / 1024).toFixed(1);

const main = async () => {
  const flick = process.argv[2] ?? process.env.FLICK_BIN ?? "flick";
  const base = path.join(root, "target/bundle-size");
  const rows = [];
  for (const program of PROGRAMS) {
    const sizes = {};
    for (const profile of ["debug", "release"]) {
      const out = path.join(base, profile, program.name);
      build(flick, program.source, out, profile);
      sizes[profile] = await measure(path.join(out, runner(program)));
    }
    const components = path.join(base, "components", program.name);
    build(flick, program.source, components, "release", "components");
    sizes.components = await measure(path.join(components, runner(program)));
    rows.push({ ...program, ...sizes.release, debug: sizes.debug, components: sizes.components });
  }
  console.log("| program | raw KB | min KB | gzip KB | budget KB | app min KB (debug) | atom labels gzip B |");
  console.log("|---|---|---|---|---|---|---|");
  for (const r of rows) {
    const labels = r.gzip - r.components.gzip;
    console.log(`| ${r.name} | ${kb(r.raw)} | ${kb(r.min)} | ${kb(r.gzip)} | ${r.budget} | ${kb(r.app)} (${kb(r.debug.app)}) | ${labels} |`);
  }
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
  for (const e of errors) console.error(`error: ${e}`);
  if (errors.length > 0) process.exitCode = 1;
};

if (process.argv[1] === fileURLToPath(import.meta.url)) await main();
