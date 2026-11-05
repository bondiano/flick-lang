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
    "3624ea93dee4ebe914232b5fdb94ab3169cf212829289c548c2b62108172fde4",
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

/** The surface of `tests/ffi/ts-holes`: the TypeScript holes of `docs/formal/ts-holes.md`. */
const holes = (t) => {
  const projectRoot = mkdtempSync(path.join(tmpdir(), "flick-import-holes-"));
  t.after(() => rmSync(projectRoot, { recursive: true, force: true }));
  mkdirSync(path.join(projectRoot, "node_modules"));
  writeFileSync(path.join(projectRoot, "package.json"), '{"type":"module"}\n');
  cpSync(path.join(root, "../../tests/ffi/ts-holes"), path.join(projectRoot, "node_modules/ts-holes"), {
    recursive: true,
  });
  return JSON.parse(read("ts-holes", projectRoot));
};
const exported = (surface, name) => surface.exports.find((e) => e.name === name);

test("import mode reads no members of a class: brand, accessors and index signature stay behind its name", (t) => {
  const surface = holes(t);
  assert.equal(exported(surface, "Point").kind, "class");
  assert.deepEqual(exported(surface, "makePoint").signatures[0].result, { k: "handle", name: "Point" });
});

test("import mode reduces a type predicate to its boolean", (t) => {
  assert.deepEqual(exported(holes(t), "isPoint").signatures[0].result, { k: "boolean" });
});

test("import mode gives a readonly array the shape of a mutable one", (t) => {
  assert.deepEqual(exported(holes(t), "first").signatures[0].params[0].type, { k: "array", element: { k: "number" } });
});

test("import mode leaves an index signature outside the supported subset", (t) => {
  assert.deepEqual(exported(holes(t), "scores").signatures[0].result, { k: "other", text: "Record<string, number>" });
});

test("import mode keeps the static side of a class apart from its instances", (t) => {
  assert.deepEqual(exported(holes(t), "register").signatures[0].params[0].type, { k: "other", text: "typeof Point" });
});

test("import mode names the object types whose one name stands for two declarations", (t) => {
  const surface = holes(t);
  assert.deepEqual(surface.ambiguous, ["Options"]);
  // where each is declared tells them apart but never reaches the shapes
  assert.doesNotMatch(JSON.stringify(surface.exports), /"site"/);
});

/** The surface of a package `enum-lib` whose declarations are `dts`. */
const declared = (t, dts) => {
  const projectRoot = mkdtempSync(path.join(tmpdir(), "flick-import-enum-"));
  t.after(() => rmSync(projectRoot, { recursive: true, force: true }));
  const pkg = path.join(projectRoot, "node_modules/enum-lib");
  mkdirSync(pkg, { recursive: true });
  writeFileSync(path.join(projectRoot, "package.json"), '{"type":"module"}\n');
  writeFileSync(path.join(pkg, "package.json"), '{"name": "enum-lib", "version": "1.0.0", "types": "./index.d.ts"}\n');
  writeFileSync(path.join(pkg, "index.d.ts"), dts);
  return JSON.parse(read("enum-lib", projectRoot));
};

test("import mode names a union by its alias, which the fingerprint leaves out", (t) => {
  const named = declared(t, 'export type Method = "GET" | "POST";\nexport declare function send(method: Method): void;\n');
  const inline = declared(t, 'export declare function send(method: "GET" | "POST"): void;\n');
  const param = (surface) => surface.exports[0].signatures[0].params[0].type;
  const types = [
    { k: "literal", text: '"GET"' },
    { k: "literal", text: '"POST"' },
  ];
  assert.deepEqual(param(named), { k: "union", types, alias: "Method" });
  assert.deepEqual(param(inline), { k: "union", types });
  // naming the union changes no binding a review vouches for
  assert.equal(named.exports[0].fingerprint, inline.exports[0].fingerprint);
});
