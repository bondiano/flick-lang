// The built-in modules Flick's own runtime carries without Effect: each of
// their foreign bindings has an export on the kernel side, where the compiler's
// `rt_specifier` sends it.
import assert from "node:assert/strict";
import { existsSync, readFileSync } from "node:fs";
import { test } from "node:test";

import policy from "../../packages/effect-ts/compiler-bindings.json" with { type: "json" };

const repo = new URL("../../", import.meta.url);
const read = (path: string): string => readFileSync(new URL(path, repo), "utf8");

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

/** The source of a built-in module: of the core package, else of std. */
const source = (module: string): string => {
  const file = `${module.replaceAll(".", "/")}.fl`;
  const core = new URL(`packages/core/src/${file}`, repo);
  return existsSync(core) ? readFileSync(core, "utf8") : read(`packages/std/src/${file}`);
};

/** The native implementation declared by the packages. */
const kernelPath = (spec: string): string | undefined => {
  const target = policy.routes.find(([from]) => from === spec)?.[2];
  if (!target) return undefined;
  const path = target.replace("@flick-lang/runtime/", "../src/");
  return `${path === "../src/rt" ? `${path}/index` : path}.ts`;
};

test("the kernel has every binding of its first tier of built-in modules", async () => {
  const bindings = MODULES.flatMap((module) =>
    [...source(module).matchAll(/^foreign (?:\w+ )?\w+ from "(@flick-lang\/effect-ts[^"]*)" "(\w+)"/gm)].map(
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
