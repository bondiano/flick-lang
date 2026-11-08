// The built-in modules Flick's own runtime carries without Effect: each of
// their foreign bindings has an export on the kernel side, where the compiler's
// `rt_specifier` sends it.
import assert from "node:assert/strict";
import { existsSync, readFileSync } from "node:fs";
import { test } from "node:test";

const repo = new URL("../../", import.meta.url);
const read = (path: string): string => readFileSync(new URL(path, repo), "utf8");

/** The modules of the kernel's first tier, but `Ui`, `Atom` and `Test`, which come later. */
const MODULES = [
  "Cause",
  "Clock",
  "Failure",
  "Fiber",
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
];

/** A list of module names in `flick-core/src/build/effects.rs`. */
const listed = (name: string): Array<string> => {
  const list = new RegExp(`const ${name}: &\\[&str\\] = &\\[([^\\]]*)\\]`).exec(read("flick-core/src/build/effects.rs"));
  assert.ok(list, `\`${name}\` in build/effects.rs`);
  return [...(list[1] ?? "").matchAll(/"([\w.]+)"/g)].map((m) => m[1] ?? "");
};

/** The source of a built-in module: of the core package, else of std. */
const source = (module: string): string => {
  const file = `${module.replaceAll(".", "/")}.fl`;
  const core = new URL(`packages/core/src/${file}`, repo);
  return existsSync(core) ? readFileSync(core, "utf8") : read(`packages/std/src/${file}`);
};

/** The kernel module `rt_specifier` makes of a runtime specifier, if it has one. */
const kernelPath = (spec: string, inRt: ReadonlyArray<string>, inCore: ReadonlyArray<string>): string | undefined => {
  if (spec === "@flick-lang/runtime") return "../src/rt/index.ts";
  const module = /^@flick-lang\/runtime\/effect\/(\w+)$/.exec(spec)?.[1];
  if (module === undefined) return undefined;
  if (inRt.includes(module)) return `../src/rt/effect/${module}.ts`;
  return inCore.includes(module) ? `../src/core/${module}.ts` : undefined;
};

test("the kernel has every binding of its first tier of built-in modules", async () => {
  const inRt = listed("IN_RT");
  const inCore = listed("IN_CORE");
  const bindings = MODULES.flatMap((module) =>
    [...source(module).matchAll(/^foreign (?:\w+ )?\w+ from "(@flick-lang\/runtime[^"]*)" "(\w+)"/gm)].map(
      ([, spec = "", name = ""]) => ({ module, spec, name }),
    ),
  );
  assert.ok(bindings.length > 200, `${bindings.length} bindings`);
  const exported = new Map<string, Set<string>>();
  for (const { module, spec, name } of bindings) {
    const path = kernelPath(spec, inRt, inCore);
    assert.ok(path, `${module} binds \`${name}\` of ${spec}, a module the kernel does not have`);
    if (!exported.has(path)) {
      // oxlint-disable-next-line no-await-in-loop -- each module once, as it first comes up
      const loaded: unknown = await import(path);
      exported.set(path, new Set(typeof loaded === "object" && loaded !== null ? Object.keys(loaded) : []));
    }
    assert.ok(exported.get(path)?.has(name), `${module} binds \`${name}\` of ${spec}`);
  }
});
