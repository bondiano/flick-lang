// `core/` is all a program built with `effect = "none"` imports: it has every
// name the built-in modules of such a build bind and every helper compiled
// code calls, and none of its modules imports Effect.
import assert from "node:assert/strict";
import { readFileSync, readdirSync } from "node:fs";
import { test } from "node:test";

const repo = new URL("../../", import.meta.url);
const read = (path: string): string => readFileSync(new URL(path, repo), "utf8");

/** The built-in modules a build without effects may use, as the compiler lists them. */
const builtins = (): Array<string> => {
  const list = /const BUILTINS: &\[&str\] = &\[([^\]]*)\]/.exec(read("flick-core/src/build/effects.rs"));
  assert.ok(list, "`BUILTINS` in build/effects.rs");
  return [...(list[1] ?? "").matchAll(/"([\w.]+)"/g)].map((m) => m[1] ?? "");
};

const source = (module: string): string => read(`flick-core/src/types/builtins/${module.replaceAll(".", "/")}.fl`);

const exportsOf = async (path: string): Promise<Set<string>> => {
  const module: unknown = await import(path);
  return new Set(typeof module === "object" && module !== null ? Object.keys(module) : []);
};

/** The `core` module standing for a runtime module a built-in binds. */
const corePath = (spec: string): string =>
  `${spec.replace(/^@flick-lang\/runtime(?:\/effect)?/, "../src/core").replace(/core$/, "core/index")}.ts`;

test("core has every binding of the built-in modules without effects", async () => {
  const modules = builtins();
  assert.ok(modules.includes("Prelude") && modules.includes("List"), modules.join(", "));
  const bindings = modules.flatMap((module) =>
    [...source(module).matchAll(/^foreign \w+ \w+ from "(@flick-lang\/runtime[^"]*)" "(\w+)"/gm)].map(
      ([, spec = "", name = ""]) => ({ module, spec, name }),
    ),
  );
  const paths = [...new Set(bindings.map((b) => corePath(b.spec)))];
  const exported = new Map(await Promise.all(paths.map(async (p) => [p, await exportsOf(p)] as const)));
  for (const { module, spec, name } of bindings) {
    assert.ok(exported.get(corePath(spec))?.has(name), `${module} binds \`${name}\` of ${spec}`);
  }
});

test("core has every runtime helper compiled code calls", async () => {
  const core = await exportsOf("../src/core/index.ts");
  const dir = new URL("flick-core/src/codegen/js/", repo);
  const code = readdirSync(dir)
    .filter((f) => f.endsWith(".rs"))
    .map((f) => readFileSync(new URL(f, dir), "utf8"))
    .join("\n");
  const helpers = [...code.matchAll(/rt\("(\w+)"\)/g)].map((m) => m[1] ?? "");
  assert.ok(helpers.length > 10);
  for (const helper of helpers) assert.ok(core.has(helper), `\`$rt.${helper}\``);
  const { Effect } = await import("../src/core/index.ts");
  for (const [, method] of code.matchAll(/effect\("(\w+)"\)/g)) assert.ok(method && method in Effect, `Effect.${method}`);
});

test("no module of core imports Effect", () => {
  const dir = new URL("../src/core/", import.meta.url);
  for (const file of readdirSync(dir)) {
    assert.doesNotMatch(readFileSync(new URL(file, dir), "utf8"), /from "effect/, file);
  }
});
