// `core/` is all a program built with `effect = "none"` imports: it has every
// name the built-in modules of such a build bind and every helper compiled
// code calls, and none of its modules imports Effect.
import assert from "node:assert/strict";
import { existsSync, readFileSync, readdirSync } from "node:fs";
import { test } from "node:test";
import { bigintDivide, bigintRemainder } from "../src/core/text.ts";

test("checked BigInt division keeps a zero divisor as an arithmetic error", () => {
  assert.deepEqual(bigintDivide(7n, 2n), { _tag: "Success", success: 3n });
  assert.deepEqual(bigintRemainder(-7n, 2n), { _tag: "Success", success: -1n });
  const result = bigintDivide(5n, 0n);
  assert.equal(result._tag, "Failure");
  if (result._tag === "Failure") assert.equal(result.failure._0, "division by zero");
});

const repo = new URL("../../", import.meta.url);
const read = (path: string): string => readFileSync(new URL(path, repo), "utf8");

/** The built-in modules a build without effects may use, as the compiler lists them. */
const builtins = (): Array<string> => {
  const list = /const BUILTINS: &\[&str\] = &\[([^\]]*)\]/.exec(read("flick-core/src/build/effects.rs"));
  assert.ok(list, "`BUILTINS` in build/effects.rs");
  return [...(list[1] ?? "").matchAll(/"([\w.]+)"/g)].map((m) => m[1] ?? "");
};

/** The source of a built-in module: of the core package, else of std. */
const source = (module: string): string => {
  const file = `${module.replaceAll(".", "/")}.fl`;
  const core = new URL(`packages/core/src/${file}`, repo);
  return existsSync(core) ? readFileSync(core, "utf8") : read(`packages/std/src/${file}`);
};

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
  // `kernel.rs` emits only for Flick's own runtime, whose helpers are its own
  const codegen = (kernel: boolean): string =>
    readdirSync(dir)
      .filter((f) => f.endsWith(".rs") && (f === "kernel.rs") === kernel)
      .map((f) => readFileSync(new URL(f, dir), "utf8"))
      .join("\n");
  const code = codegen(false);
  const helpers = [...code.matchAll(/rt\("(\w+)"\)/g)].map((m) => m[1] ?? "");
  assert.ok(helpers.length > 10);
  for (const helper of helpers) assert.ok(core.has(helper), `\`$rt.${helper}\``);
  const kernel = await exportsOf("../src/rt/index.ts");
  for (const [, helper] of codegen(true).matchAll(/rt\("(\w+)"\)/g)) assert.ok(helper && kernel.has(helper), `kernel \`$rt.${helper}\``);
  const { Effect } = await import("../src/core/index.ts");
  for (const [, method] of code.matchAll(/effect\("(\w+)"\)/g)) assert.ok(method && method in Effect, `Effect.${method}`);
});

test("no module of core imports Effect", () => {
  const dir = new URL("../src/core/", import.meta.url);
  for (const file of readdirSync(dir)) {
    assert.doesNotMatch(readFileSync(new URL(file, dir), "utf8"), /from "effect/, file);
  }
});
