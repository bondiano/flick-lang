#!/usr/bin/env node
// Moves the repository to another Effect release: takes the snapshot of its
// surface, reports what was added, removed or changed against the current
// one and what the runtime's capability matrix lacks, writes the snapshot
// and the pins. The coverage linter stays red until the manifest is reviewed.
// Usage: bump.mjs <version> [--package-dir <dir>] [--root <repo>]
import { execFileSync } from "node:child_process";
import { mkdtempSync, readFileSync, readdirSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";

import { render, snapshot } from "./surface.mjs";

/** The files that pin the Effect release, and how each spells it. */
const PINS = [
  ["runtime/package.json", /("effect": ")[^"]+(")/],
  ["runtime/src/capabilities.ts", /(EFFECT_VERSION = ")[^"]+(")/],
  ["runtime/test/effect-version.test.ts", /(PINNED_EFFECT = ")[^"]+(")/],
  ["flick/src/toolchain.rs", /(pub const EFFECT: &str = ")[^"]+(")/],
];

/** Downloads `effect@version` and unpacks it into a fresh directory. */
const download = (version) => {
  const dir = mkdtempSync(path.join(tmpdir(), "effect-bump-"));
  execFileSync("npm", ["pack", `effect@${version}`, "--pack-destination", dir], { stdio: ["ignore", "ignore", "inherit"] });
  const tarball = readdirSync(dir).find((f) => f.endsWith(".tgz"));
  // fingerprints are printed declarations: the package's own dependencies are not needed
  execFileSync("tar", ["-xzf", path.join(dir, tarball), "-C", dir]);
  return path.join(dir, "package");
};

/** The `Module.op` names of the capability matrix in `capabilities.ts`. */
const matrix = (text) => {
  const body = text.slice(text.indexOf("const MATRIX"), text.indexOf("};", text.indexOf("const MATRIX")));
  return [...body.matchAll(/(\w+): \[([^\]]*)\]/g)].flatMap(([, module, ops]) =>
    [...ops.matchAll(/"(\w+)"/g)].map(([, op]) => `${module}.${op}`));
};

/** The report of the move from snapshot `old` to `next`. */
export const report = (old, next, capabilities) => {
  const lines = [`effect ${old.version} -> ${next.version}`];
  const list = (title, items) => {
    if (items.length > 0) lines.push(`${title} (${items.length}):`, ...items.map((i) => `  ${i}`));
  };
  list("entrypoints added", next.entrypoints.filter((e) => !old.entrypoints.includes(e)));
  list("entrypoints removed", old.entrypoints.filter((e) => !next.entrypoints.includes(e)));
  const flat = (snap) =>
    new Map(Object.entries(snap.modules).flatMap(([entry, exports]) =>
      Object.entries(exports).map(([key, value]) => [`${entry} ${key}`, value])));
  const before = flat(old);
  const after = flat(next);
  list("exports added", [...after.keys()].filter((k) => !before.has(k)));
  list("exports removed", [...before.keys()].filter((k) => !after.has(k)));
  list("exports changed", [...after.keys()].filter((k) => before.has(k) && before.get(k) !== after.get(k)));
  const core = next.modules.effect ?? {};
  list("capability matrix missing", capabilities.filter((c) => !(core[c] ?? "").startsWith("value")));
  return `${lines.join("\n")}\n`;
};

const main = (args) => {
  const version = args[0];
  if (version === undefined || version.startsWith("--")) throw new Error("usage: bump.mjs <version> [--package-dir <dir>] [--root <repo>]");
  const option = (name) => {
    const i = args.indexOf(name);
    return i === -1 ? undefined : args[i + 1];
  };
  const root = option("--root") ?? path.join(path.dirname(fileURLToPath(import.meta.url)), "../..");
  const dir = option("--package-dir") ?? download(version);
  const next = snapshot(dir);
  if (next.version !== version) throw new Error(`${dir} holds effect ${next.version}, not ${version}`);
  const file = path.join(root, "stdlib/Effect/snapshot.json");
  const old = JSON.parse(readFileSync(file, "utf8"));
  const capabilities = matrix(readFileSync(path.join(root, "runtime/src/capabilities.ts"), "utf8"));
  process.stdout.write(report(old, next, capabilities));
  writeFileSync(file, render(next));
  for (const [rel, pattern] of PINS) {
    const pin = path.join(root, rel);
    writeFileSync(pin, readFileSync(pin, "utf8").replace(pattern, `$1${version}$2`));
  }
  process.stdout.write(
    "\nwrote the snapshot and the pins; next: `pnpm install`, review stdlib/Effect/surface/ " +
      "until `cargo test -p flick-core --test effect_surface` passes\n",
  );
};

if (process.argv[1] === fileURLToPath(import.meta.url)) main(process.argv.slice(2));
