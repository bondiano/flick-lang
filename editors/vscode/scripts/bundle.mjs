// Bundles the extension and the language client it needs into
// `dist/extension.js`, so the VSIX holds one script and no `node_modules`:
// pnpm's node_modules keeps the client's own dependencies beside it, which a
// VS Code installation of a loose extension directory would not resolve.
// `vscode` stays external, as the editor provides it.
//
//   node editors/vscode/scripts/bundle.mjs
//
// esbuild is pinned and fetched by npx: the repository keeps no bundler among
// its dependencies, and the release job builds the same bytes from the same
// version.

import { spawnSync } from "node:child_process";
import { readFileSync, statSync } from "node:fs";
import path from "node:path";
import vm from "node:vm";
import { fileURLToPath } from "node:url";

/** The bundler, as the release job and this script both name it. */
export const ESBUILD = "0.28.2";

/** The extension's directory, whose `package.json` names this bundle. */
export const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");

/** What the packaged extension runs. */
export const bundle = path.join(root, "dist", "extension.js");

/** A name only the bundled client carries, not the grammar. */
const INLINED = "LanguageClient";

/** Runs `command` and fails with its own message when it does. */
export const run = (command, args, options = {}) => {
  const done = spawnSync(command, args, {
    stdio: "inherit",
    // npx on Windows is a `.cmd`, which spawn needs a shell for
    shell: process.platform === "win32",
    ...options,
  });
  if (done.error) throw new Error(`cannot run ${command}: ${done.error.message}`);
  if (done.status !== 0) throw new Error(`${command} ${args.join(" ")} exited ${done.status}`);
};

/** Bundles; returns the path and the size of the script written. */
export const build = () => {
  run(
    "npx",
    [
      "--yes",
      `esbuild@${ESBUILD}`,
      "extension.js",
      "--bundle",
      "--platform=node",
      "--format=cjs",
      "--target=node20",
      "--external:vscode",
      `--outfile=${bundle}`,
    ],
    { cwd: root },
  );
  const text = readFileSync(bundle, "utf8");
  if (!text.includes(INLINED)) {
    throw new Error(`the bundle has no ${INLINED}: the language client was not bundled`);
  }
  if (/require\("vscode-languageclient/.test(text)) {
    throw new Error("the bundle still requires vscode-languageclient: it needs node_modules");
  }
  const bytes = statSync(bundle).size;
  if (bytes < 100_000) {
    throw new Error(`the bundle is ${bytes} bytes: the client's code is missing`);
  }
  // it compiles: a syntax error here would only show when VS Code activates it
  new vm.Script(text, { filename: bundle });
  return { bundle, bytes };
};

/** Whether this file is the one node was asked to run. */
const invoked =
  process.argv[1] !== undefined && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url);

if (invoked) {
  const { bundle: written, bytes } = build();
  console.log(`bundled ${path.relative(process.cwd(), written)}: ${bytes} bytes`);
}
