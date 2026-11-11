import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";

const root = new URL("../../../", import.meta.url);
const read = (path: string) => readFileSync(new URL(path, root), "utf8");
import policy from "../compiler-bindings.json" with { type: "json" };
import familyPolicy from "../compiler-families.json" with { type: "json" };

test("binding routes are unique and preserve native and pure boundaries", () => {
  const routes = new Map(policy.routes.map(([from, core, native]) => [from, { core, native }]));
  assert.equal(routes.size, policy.routes.length);
  assert.deepEqual(routes.get("flick:Option"), {
    core: "@flick-lang/runtime/core/Option", native: "@flick-lang/runtime/rt/effect/Option",
  });
  assert.equal(routes.has("flick:Stream"), false);
  assert.equal(routes.get("flick:ui/react")?.core, null);
  assert.equal(routes.get("flick:ui/react")?.native, "@flick-lang/runtime/rt/ui/react");
  assert.ok(familyPolicy.families.includes("@flick-lang/runtime/rt/ui uiView"));
  assert.ok(familyPolicy.families.includes("@flick-lang/runtime/rt/effect/reactivity/Atom flickRead"));
  assert.ok(!familyPolicy.families.includes("@flick-lang/runtime/rt/ui uiText"));
});

test("family metadata covers every context-bound UI and Atom export", () => {
  const shared = new Set(read("packages/effect-ts/shared_ui_exports.txt").trim().split("\n"));
  const modules = new Set([
    "@flick-lang/runtime/rt/effect/reactivity/Atom",
    "@flick-lang/runtime/rt/ui",
    "@flick-lang/runtime/rt/ui/html",
    "@flick-lang/runtime/rt/ui/react",
    "@flick-lang/runtime/rt/ui/server",
  ]);
  const expected = read("runtime/rt_exports.txt").trim().split("\n").filter((line) => {
    const [module = "", name = ""] = line.split(" ");
    if (shared.has(name)) return false;
    return modules.has(module) ||
      (module === "@flick-lang/runtime/rt" && (name === "label" || /^(atom|form|route)[A-Z]/.test(name)));
  });
  assert.deepEqual(familyPolicy.families.toSorted(), expected.toSorted());
});

test("the compiler interprets package metadata without module-specific tables", () => {
  const source = read("flick-core/src/effects.rs").split("#[cfg(test)]")[0] ?? "";
  assert.ok(!source.includes('"@flick-lang/'));
  for (const table of ["IN_RT", "IN_CORE", "UI_IN_RT", "RT_EXPORTS"]) {
    assert.ok(!source.includes(`const ${table}`));
  }
  assert.ok(!read("flick-core/Cargo.toml").includes("effect_surface"));
});
