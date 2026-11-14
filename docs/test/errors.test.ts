import { strict as assert } from "node:assert";
import { test } from "node:test";
import { areaInventoryProblems, diagnosticAreaProblems, expectedPages, problems } from "../scripts/errors.mjs";

test("every registered compiler error has current English and Russian pages", () => {
  assert.ok(expectedPages().size > 400);
  assert.deepEqual(problems(), []);
});

test("diagnostic areas match Area::ALL and both localized indexes", () => {
  assert.deepEqual(diagnosticAreaProblems(), []);
});

const paired = {
  compiler: ["Syntax", "Resolve"],
  areas: ["Syntax", "Resolve"],
  areaRu: ["Синтаксис", "Имена и модули"],
  headingsEn: ["Syntax", "Resolve"],
  headingsRu: ["Синтаксис", "Имена и модули"],
  codedAreas: new Set(["Syntax", "Resolve"]),
};

test("a missing, duplicate or unknown diagnostic area fails the index contract", () => {
  const missing = areaInventoryProblems({
    ...paired,
    areas: ["Syntax"],
    areaRu: ["Синтаксис"],
    headingsEn: ["Syntax"],
    headingsRu: ["Синтаксис"],
  });
  assert.ok(missing.includes("missing diagnostic area Resolve"), missing.join("\n"));

  const duplicate = areaInventoryProblems({
    ...paired,
    areas: ["Syntax", "Syntax"],
    areaRu: ["Синтаксис", "Синтаксис"],
    headingsEn: ["Syntax", "Syntax"],
    headingsRu: ["Синтаксис", "Синтаксис"],
  });
  assert.ok(duplicate.includes("duplicate diagnostic area Syntax"), duplicate.join("\n"));

  const unknown = areaInventoryProblems({
    ...paired,
    areas: ["Syntax", "Resolve", "Plugin"],
    areaRu: ["Синтаксис", "Имена и модули", "Плагин"],
    headingsEn: ["Syntax", "Resolve", "Plugin"],
    headingsRu: ["Синтаксис", "Имена и модули", "Плагин"],
    codedAreas: new Set(["Syntax", "Resolve", "Plugin"]),
  });
  assert.ok(unknown.includes("unknown diagnostic area Plugin"), unknown.join("\n"));
});

test("an area with no codes is omitted from both indexes", () => {
  const omitted = areaInventoryProblems({
    ...paired,
    headingsEn: ["Syntax"],
    headingsRu: ["Синтаксис"],
    codedAreas: new Set(["Syntax"]),
  });
  assert.deepEqual(omitted, []);

  const shown = areaInventoryProblems({
    ...paired,
    codedAreas: new Set(["Syntax"]),
  });
  assert.ok(shown.some((problem) => problem.startsWith("English index headings")), shown.join("\n"));
  assert.ok(shown.some((problem) => problem.startsWith("Russian index headings")), shown.join("\n"));
});
