// Builds the extension's VSIX and checks what it holds before the release job
// attaches it: the bundled language client (not just the grammar), the
// manifest's version, and the files VS Code loads. Nothing is published to a
// marketplace.
//
//   node editors/vscode/scripts/package.mjs [--out FILE]
//
// With `FLICK_EXPECTED_VERSION` set, the manifest must carry that version: the
// release job sets it to the tag without its `v`.

import { mkdirSync, readFileSync, statSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

import { build, root, run } from "./bundle.mjs";

/** The packager, pinned as the bundler is. */
export const VSCE = "4.1.0";

/** The smallest a VSIX that carries the client can be. */
const MIN_BYTES = 50_000;

/** What VS Code loads, by the names a zip holds in its entry headers. */
export const REQUIRED = [
  "extension/package.json",
  "extension/dist/extension.js",
  "extension/syntaxes/flick.tmLanguage.json",
  "extension/language-configuration.json",
];

/** The `--out` argument, or the ignored build directory of the checkout. */
export const output = (args) => {
  const at = args.indexOf("--out");
  if (at !== -1 && args[at + 1] !== undefined) return path.resolve(args[at + 1]);
  const manifest = JSON.parse(readFileSync(path.join(root, "package.json"), "utf8"));
  return path.resolve(root, "..", "..", "target", "vsix", `flick-${manifest.version}.vsix`);
};

/** The version the manifest names, checked against the release when given. */
export const version = () => {
  const manifest = JSON.parse(readFileSync(path.join(root, "package.json"), "utf8"));
  const expected = process.env.FLICK_EXPECTED_VERSION;
  if (expected && expected !== manifest.version) {
    throw new Error(
      `editors/vscode/package.json is ${manifest.version}, but the release is ${expected}`,
    );
  }
  return manifest.version;
};

/**
 * Packages and returns the file's path, size and version. The zip's entry
 * names sit uncompressed in its headers, so the required ones are found
 * without unpacking it.
 */
export const pack = (out) => {
  const wanted = version();
  build();
  mkdirSync(path.dirname(out), { recursive: true });
  run(
    "npx",
    ["--yes", `@vscode/vsce@${VSCE}`, "package", "--no-dependencies", "--out", out],
    { cwd: root },
  );
  const bytes = readFileSync(out);
  if (bytes.subarray(0, 4).toString("binary") !== "PK\x03\x04") {
    throw new Error(`${out} is not a zip`);
  }
  const text = bytes.toString("binary");
  const missing = REQUIRED.filter((name) => !text.includes(name));
  if (missing.length > 0) throw new Error(`${out} lacks ${missing.join(", ")}`);
  const size = statSync(out).size;
  if (size < MIN_BYTES) throw new Error(`${out} is ${size} bytes: the bundle is missing`);
  return { out, size, version: wanted };
};

/** Whether this file is the one node was asked to run. */
const invoked =
  process.argv[1] !== undefined &&
  path.resolve(process.argv[1]) === fileURLToPath(new URL("./package.mjs", import.meta.url));

if (invoked) {
  try {
    const { out, size, version: shipped } = pack(output(process.argv.slice(2)));
    console.log(`packaged flick ${shipped}: ${path.relative(process.cwd(), out)}, ${size} bytes`);
  } catch (error) {
    console.error(`flick: ${error instanceof Error ? error.message : String(error)}`);
    process.exit(1);
  }
}
