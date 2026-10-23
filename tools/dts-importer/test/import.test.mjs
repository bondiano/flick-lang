import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";
import path from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

const here = path.dirname(fileURLToPath(import.meta.url));
const root = path.join(here, "..");

/** What `flick ffi import` reads for the package installed in this directory. */
const read = (pkg) =>
  execFileSync("node", [path.join(root, "index.mjs")], {
    input: JSON.stringify({ root, package: pkg }),
    encoding: "utf8",
    stdio: ["pipe", "pipe", "pipe"],
  });

for (const pkg of ["delay", "get-port", "p-map"]) {
  test(`import mode prints the shapes of ${pkg}`, () => {
    assert.equal(read(pkg), readFileSync(path.join(here, "fixtures/import", `${pkg}.json`), "utf8"));
  });
}

test("import mode prints the shapes of date-fns", () => {
  // the output is 136 kB: its digest stands in for a fixture
  const out = read("date-fns");
  assert.equal(
    createHash("sha256").update(out).digest("hex"),
    "99b55b69e38b631486e7d55d2a9e62af496c4e55b12007395a7a73d1fa92782e",
  );
});

test("import mode refuses a package it cannot resolve", () => {
  assert.throws(() => read("no-such-package"), /cannot resolve the declarations of `no-such-package`/);
});
