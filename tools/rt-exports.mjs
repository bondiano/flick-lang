// The compiler selects an RT binding only when this manifest contains the
// export. Generate it from the built runtime so missing names use the bridge.
import { readFile, readdir, writeFile } from "node:fs/promises";
import { fileURLToPath, pathToFileURL } from "node:url";
import { dirname, join, relative } from "node:path";

const repo = dirname(dirname(fileURLToPath(import.meta.url)));
const root = join(repo, "runtime/dist");
const manifest = join(repo, "flick-core/src/build/rt_exports.txt");
const stubManifest = join(repo, "flick-core/src/build/rt_stubs.txt");
const bridgeStubManifest = join(repo, "flick-core/src/build/rt_bridge_stubs.txt");
// These exports exist on RT, but the Effect adapter must own their ABI when
// the same program also uses Effect's generators or duration values.
const bridgeOverrides = [
  "@flick-lang/runtime/rt durationFormat",
  "@flick-lang/runtime/rt redactedMake",
  "@flick-lang/runtime/rt redactedReveal",
  "@flick-lang/runtime/rt/effect/Arbitrary Constant",
  "@flick-lang/runtime/rt/effect/Arbitrary filter",
  "@flick-lang/runtime/rt/effect/Arbitrary flickCheckEffect",
  "@flick-lang/runtime/rt/effect/Arbitrary flickFormatCheckFailure",
  "@flick-lang/runtime/rt/effect/Arbitrary flickSchema",
  "@flick-lang/runtime/rt/effect/Arbitrary schema",
  "@flick-lang/runtime/rt/effect/Redacted flickEquivalent",
];
// An installed tracer/exporter belongs to Effect's Context. Keep every span
// operation on that side so creation, annotation and completion see it.
const bridgeModules = new Set(["rt/effect/Tracer"]);
const bridgeExport = (module) => bridgeModules.has(module);

async function files(dir) {
  const out = [];
  for (const entry of await readdir(dir, { withFileTypes: true })) {
    const path = join(dir, entry.name);
    if (entry.isDirectory()) out.push(...await files(path));
    else if (entry.name.endsWith(".js")) out.push(path);
  }
  return out;
}

const selected = (await files(root)).filter((path) =>
  path === join(root, "rt/index.js") ||
  path.startsWith(join(root, "rt/effect/")) ||
  path.startsWith(join(root, "rt/ui")) ||
  path.startsWith(join(root, "core/"))
);
const lines = [];
const bridgeStubs = [];
for (const path of selected) {
  const exports = await import(pathToFileURL(path).href);
  const name = relative(root, path).replace(/\.js$/, "");
  const spec = `@flick-lang/runtime/${name === "rt/index" ? "rt" : name}`;
  for (const exportName of Object.keys(exports)) {
    const line = `${spec} ${exportName}`;
    lines.push(line);
    if (bridgeExport(name, exportName) ||
        (typeof exports[exportName] === "function" && exports[exportName][Symbol.for("flick/absent")] === true))
      bridgeStubs.push(line);
  }
}
lines.sort();
for (const line of bridgeOverrides) {
  if (!lines.includes(line)) throw new Error(`missing bridge override: ${line}`);
  bridgeStubs.push(line);
}
const uniqueBridgeStubs = [...new Set(bridgeStubs)].sort();
const contents = `${lines.join("\n")}\n`;
const bridgeStubContents = `${uniqueBridgeStubs.join("\n")}\n`;
const rt = await import(pathToFileURL(join(root, "rt/index.js")).href);
const stubs = await import(pathToFileURL(join(root, "core/stubs.js")).href);
const stubNames = Object.keys(stubs).filter((name) => rt[name] === stubs[name]).sort();
const stubContents = `${stubNames.join("\n")}\n`;
if (process.argv.includes("--check")) {
  if (contents !== await readFile(manifest, "utf8") ||
      stubContents !== await readFile(stubManifest, "utf8") ||
      bridgeStubContents !== await readFile(bridgeStubManifest, "utf8")) {
    console.error("RT exports changed; run pnpm -C runtime build && node tools/rt-exports.mjs");
    process.exitCode = 1;
  }
} else {
  await writeFile(manifest, contents);
  await writeFile(stubManifest, stubContents);
  await writeFile(bridgeStubManifest, bridgeStubContents);
  console.log(`${selected.length} modules, ${lines.length} exports, ${stubNames.length} RT stubs, ${uniqueBridgeStubs.length} bridge stubs`);
}
