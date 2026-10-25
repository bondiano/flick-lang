import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import { cpSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

const here = path.dirname(fileURLToPath(import.meta.url));
const root = path.join(here, "..");

/** What `flick ffi import` reads for the package installed in this directory. */
const read = (pkg, projectRoot = root) =>
  execFileSync("node", [path.join(root, "index.mjs")], {
    input: JSON.stringify({ root: projectRoot, package: pkg }),
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

for (const [moduleSpecifier, packageName, declaration, version] of [
  ["subpath-package/feature", "subpath-package", "feature.d.ts", "1.0.0"],
  ["package-alias", "original-package", "index.d.ts", "2.0.0"],
]) {
  test(`import mode preserves ${moduleSpecifier} independently of package identity`, (t) => {
    const projectRoot = mkdtempSync(path.join(tmpdir(), "flick-import-identity-"));
    t.after(() => rmSync(projectRoot, { recursive: true, force: true }));
    mkdirSync(path.join(projectRoot, "node_modules"));
    writeFileSync(path.join(projectRoot, "package.json"), '{"type":"module"}\n');
    // An npm alias installs the original package under the requested alias directory.
    cpSync(
      path.join(root, "../../tests/ffi/import-identity", packageName),
      path.join(projectRoot, "node_modules", moduleSpecifier.split("/")[0]),
      { recursive: true },
    );
    const surface = JSON.parse(read(moduleSpecifier, projectRoot));
    assert.equal(surface.package, moduleSpecifier);
    assert.equal(surface.version, version);
    assert.equal(surface.declarations, `${packageName}/${declaration}`);
    assert.equal(surface.exports.length, 1);
    assert.equal(surface.exports[0].name, "answer");
    assert.equal(surface.exports[0].signatures[0].result.k, "number");
    assert.match(surface.exports[0].fingerprint, /^sha256:[0-9a-f]{64}$/);
  });
}
