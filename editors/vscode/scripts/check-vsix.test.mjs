// The unpacked-VSIX check's own tests: a bundle that requires the language
// client must fail the step, and one that carries it must pass. The exit
// codes come from the script as the workflow runs it, not from `check` alone,
// because a check that throws inside a step is only useful if the step fails.
//
//   node --test editors/vscode/scripts/check-vsix.test.mjs

import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { mkdirSync, mkdtempSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

import { check } from "./check-vsix.mjs";

const script = fileURLToPath(new URL("./check-vsix.mjs", import.meta.url));

/** A bundle as esbuild writes it: the client's code, inlined. */
const bundled = `"use strict";\nclass LanguageClient { start() {} }\nmodule.exports = { LanguageClient };\n`;

/** The same bundle as a loose install would need it: a require of the package. */
const requiring = `"use strict";\nconst { LanguageClient } = require("vscode-languageclient/node");\nmodule.exports = { LanguageClient };\n`;

/** An unpacked VSIX holding what VS Code loads, with `bundle` in place of its script. */
const vsix = ({ bundle = bundled, version = "0.1.0", grammar = "{}" } = {}) => {
  const dir = mkdtempSync(path.join(tmpdir(), "flick-vsix-"));
  const extension = path.join(dir, "extension");
  mkdirSync(path.join(extension, "dist"), { recursive: true });
  mkdirSync(path.join(extension, "syntaxes"), { recursive: true });
  writeFileSync(
    path.join(extension, "package.json"),
    JSON.stringify({ name: "flick", version, main: "./dist/extension.js" }),
  );
  writeFileSync(path.join(extension, "dist", "extension.js"), bundle);
  writeFileSync(path.join(extension, "syntaxes", "flick.tmLanguage.json"), grammar);
  writeFileSync(path.join(extension, "language-configuration.json"), "{}");
  return dir;
};

/** The script's status and stderr, as the workflow step sees them. */
const runs = (dir, ...args) => {
  try {
    const stdout = execFileSync(process.execPath, [script, dir, ...args], {
      encoding: "utf8",
      // the child's stderr is what the step shows: capture it, do not echo it
      stdio: ["ignore", "pipe", "pipe"],
    });
    return { status: 0, stdout, stderr: "" };
  } catch (error) {
    return { status: error.status, stdout: error.stdout ?? "", stderr: error.stderr ?? "" };
  }
};

test("a bundle that carries the client passes, and the step exits 0", () => {
  assert.equal(check(vsix(), "0.1.0").version, "0.1.0");
  const done = runs(vsix(), "--version", "0.1.0");
  assert.equal(done.status, 0, done.stderr);
  const bytes = Buffer.byteLength(bundled);
  assert.match(done.stdout, new RegExp(`the VSIX holds flick 0\\.1\\.0: a ${bytes}-byte bundle`));
});

test("a bundle that requires the client fails, and the step exits 1", () => {
  assert.throws(() => check(vsix({ bundle: requiring }), "0.1.0"), /requires vscode-languageclient/);
  const failed = runs(vsix({ bundle: requiring }), "--version", "0.1.0");
  assert.equal(failed.status, 1, failed.stdout);
  assert.match(failed.stderr, /^::error::the bundle requires vscode-languageclient/m);
});

test("a bundle without the client fails, whatever else it holds", () => {
  const grammarOnly = `"use strict";\nmodule.exports = { activate() {} };\n`;
  assert.throws(() => check(vsix({ bundle: grammarOnly })), /has no LanguageClient/);
  assert.equal(runs(vsix({ bundle: grammarOnly })).status, 1);
});

test("a missing or empty file VS Code loads fails", () => {
  assert.throws(() => check(vsix({ grammar: "" })), /is empty/);
  assert.equal(runs(vsix({ grammar: "" })).status, 1);
});

test("another version than the release's fails", () => {
  assert.throws(() => check(vsix({ version: "0.1.1" }), "0.1.0"), /the VSIX is 0\.1\.1/);
  assert.equal(runs(vsix({ version: "0.1.1" }), "--version", "0.1.0").status, 1);
});
