// The built-in modules Flick's own runtime carries without Effect: each of
// their foreign bindings has an export on the kernel side, where the compiler's
// `rt_specifier` sends it.
import assert from "node:assert/strict";
import { existsSync, readdirSync, readFileSync } from "node:fs";
import { test } from "node:test";

import policy from "../../packages/effect-ts/compiler-bindings.json" with { type: "json" };

const repo = new URL("../../", import.meta.url);
const sources = readdirSync(new URL("packages/", repo), { withFileTypes: true })
  .filter((entry) => entry.isDirectory())
  .map((entry) => new URL(`packages/${entry.name}/src/`, repo));

/** The modules of the kernel's first tier, but `Ui` and `Atom`, which come later. */
const MODULES = [
  "Cause",
  "Clock",
  "Failure",
  "Fiber",
  "Gen",
  "Instances",
  "Layer",
  "List.Core",
  "Platform",
  "Prelude",
  "Queue",
  "RawCause",
  "Ref",
  "Schedule",
  "Scope",
  "Test",
];

/** A built-in module has exactly one source owner among the source packages. */
const source = (module: string): string => {
  const file = `${module.replaceAll(".", "/")}.fl`;
  const paths = sources.map((dir) => new URL(file, dir)).filter((path) => existsSync(path));
  assert.equal(paths.length, 1, `${module}: expected one packaged source, found ${paths.length}`);
  const [path] = paths;
  assert.ok(path);
  return readFileSync(path, "utf8");
};

test("source lookup includes the split standard-library packages", () => {
  for (const module of ["Stream.Core", "ChildProcess", "Ui", "HttpApi", "Rpc", "Otlp", "Cli"]) {
    assert.ok(source(module).includes(`module ${module} exposing (`), module);
  }
});

/** The native implementation declared by the packages. */
const kernelPath = (spec: string): string | undefined => {
  const target = policy.routes.find(([from]) => from === spec)?.[1];
  if (!target) return undefined;
  const path = target.replace("@flick-lang/runtime/", "../src/");
  return `${path === "../src/rt" ? `${path}/index` : path}.ts`;
};

test("the kernel has every binding of its first tier of built-in modules", async () => {
  const bindings = MODULES.flatMap((module) =>
    [...source(module).matchAll(/^foreign (?:\w+ )?\w+ from "(flick:[^"]*)" "(\w+)"/gm)].map(
      ([, spec = "", name = ""]) => ({ module, spec, name }),
    ),
  );
  assert.ok(bindings.length > 200, `${bindings.length} bindings`);
  const exported = new Map<string, Set<string>>();
  for (const { module, spec, name } of bindings) {
    const path = kernelPath(spec);
    assert.ok(path, `${module} binds \`${name}\` of ${spec}, a module the kernel does not have`);
    if (!exported.has(path)) {
      // oxlint-disable-next-line no-await-in-loop -- each module once, as it first comes up
      const loaded: unknown = await import(path);
      exported.set(path, new Set(typeof loaded === "object" && loaded !== null ? Object.keys(loaded) : []));
    }
    assert.ok(exported.get(path)?.has(name), `${module} binds \`${name}\` of ${spec}`);
  }
});
