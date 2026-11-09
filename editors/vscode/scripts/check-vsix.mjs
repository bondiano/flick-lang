// Checks an unpacked VSIX before the release attaches it: the files VS Code
// loads, a bundle that carries the language client instead of requiring it,
// and the manifest's version. `bundle.mjs` guards the bundler's own output;
// this guards what the archive holds, and exits 1 with a `::error::` line, so
// the workflow step fails on it.
//
//   node editors/vscode/scripts/check-vsix.mjs vsix --version 0.1.0

import { existsSync, readFileSync, statSync } from "node:fs";
import path from "node:path";
import vm from "node:vm";
import { fileURLToPath } from "node:url";

import { REQUIRED } from "./package.mjs";

/** The client is inlined: requiring it would need `node_modules` in the editor. */
const REQUIRING = /require\((["'])vscode-languageclient/;

/** A name only the bundled client carries, not the grammar. */
const INLINED = "LanguageClient";

/** The `--version` argument: the release's version, when one is given. */
export const expected = (args) => {
  const at = args.indexOf("--version");
  return at === -1 ? undefined : args[at + 1];
};

/**
 * Checks the unpacked extension in `dir` and returns its version and the
 * bundle's size. `version` is the release's, when the caller knows it.
 */
export const check = (dir, version) => {
  for (const name of REQUIRED) {
    const file = path.join(dir, name);
    if (!existsSync(file) || !statSync(file).isFile()) {
      throw new Error(`the VSIX holds no ${name}`);
    }
    if (statSync(file).size === 0) throw new Error(`the VSIX's ${name} is empty`);
  }
  const bundle = readFileSync(path.join(dir, "extension", "dist", "extension.js"), "utf8");
  if (REQUIRING.test(bundle)) {
    throw new Error(
      "the bundle requires vscode-languageclient: it needs `node_modules` in the editor",
    );
  }
  if (!bundle.includes(INLINED)) {
    throw new Error("the bundle has no LanguageClient: the language client was not bundled");
  }
  // it compiles: a syntax error here would only show when VS Code activates it
  new vm.Script(bundle, { filename: "extension/dist/extension.js" });
  const manifest = JSON.parse(readFileSync(path.join(dir, "extension", "package.json"), "utf8"));
  if (version !== undefined && manifest.version !== version) {
    throw new Error(`the VSIX is ${manifest.version}, but the release is ${version}`);
  }
  return { version: manifest.version, bytes: Buffer.byteLength(bundle) };
};

/** Whether this file is the one node was asked to run. */
const invoked =
  process.argv[1] !== undefined &&
  path.resolve(process.argv[1]) === fileURLToPath(new URL("./check-vsix.mjs", import.meta.url));

if (invoked) {
  const [dir, ...rest] = process.argv.slice(2);
  try {
    if (dir === undefined) throw new Error("usage: check-vsix.mjs DIR [--version X.Y.Z]");
    const { version, bytes } = check(dir, expected(rest));
    console.log(`the VSIX holds flick ${version}: a ${bytes}-byte bundle with the client`);
  } catch (error) {
    console.error(`::error::${error instanceof Error ? error.message : String(error)}`);
    process.exit(1);
  }
}
