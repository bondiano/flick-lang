import assert from "node:assert/strict";
import { mkdirSync, readFileSync } from "node:fs";
import path from "node:path";
import { test } from "node:test";
import { fileURLToPath, pathToFileURL } from "node:url";
import { parseExerciseMetadata } from "../exercises/load.ts";
import { rolldown } from "rolldown";

const repo = fileURLToPath(new URL("../../", import.meta.url));
test("exercise files survive bundling into a prerender server chunk", async () => {
  const dir = path.join(repo, "target/tmp/exercise-prerender");
  mkdirSync(dir, { recursive: true });
  const file = path.join(dir, "loader.mjs");
  const bundle = await rolldown({ input: fileURLToPath(new URL("../exercises/load.ts", import.meta.url)), platform: "node", external: (id) => id.startsWith("node:") });
  try { await bundle.write({ file, format: "esm" }); } finally { await bundle.close(); }
  const { loadExercise } = await import(pathToFileURL(file).href);
  assert.equal(loadExercise("signatures/02").check.output, "6");
  assert.throws(() => loadExercise("../other"), /Invalid exercise ID/);
});

test("empty expected stdout is a valid contract", () => {
  const source = readFileSync(new URL("../exercises/signatures/01/check.toml", import.meta.url), "utf8");
  assert.equal(parseExerciseMetadata(source.replace('output = "5"', 'output = ""'), "test").check.output, "");
  assert.throws(() => parseExerciseMetadata(source.replace('output = "5"', ''), "test"), /output/);
  assert.throws(() => parseExerciseMetadata(source.replace('output = "5"', 'output = 5'), "test"), /output/);
});

test("a starter fails with T003 unless its exercise names another code", () => {
  const source = readFileSync(new URL("../exercises/signatures/01/check.toml", import.meta.url), "utf8");
  assert.equal(parseExerciseMetadata(source, "test").starter_error, "FLICK-T003");
  assert.equal(parseExerciseMetadata(`${source}starter_error = "FLICK-T009"\n`, "test").starter_error, "FLICK-T009");
  assert.throws(() => parseExerciseMetadata(`${source}starter_error = "T009"\n`, "test"), /starter_error/);
});

test("an exercise whose export runs says so with a Boolean", () => {
  const source = readFileSync(new URL("../exercises/signatures/01/check.toml", import.meta.url), "utf8");
  assert.equal(parseExerciseMetadata(source, "test").check.runs, undefined);
  assert.equal(parseExerciseMetadata(`${source}runs = true\n`, "test").check.runs, true);
  assert.throws(() => parseExerciseMetadata(`${source}runs = "yes"\n`, "test"), /runs/);
});

test("hints come one or several, and a question names its right choice", () => {
  const base = readFileSync(new URL("../exercises/signatures/01/check.toml", import.meta.url), "utf8");
  assert.deepEqual(parseExerciseMetadata(base, "test").hints.length, 1);
  const without = base.replace(/^hint(_ru)? = .*\n/gm, "");
  const several = `${without}hints = ["first", "second"]\nhints_ru = ["первая", "вторая"]\n`;
  assert.deepEqual(parseExerciseMetadata(several, "test").hints, ["first", "second"]);
  assert.deepEqual(parseExerciseMetadata(`${without}hints = []\nhints_ru = []\n`, "test").hints, []);
  assert.throws(() => parseExerciseMetadata(`${without}hints = ["a"]\nhints_ru = []\n`, "test"), /differ/);
  const asked = `${several}question = "Which?"\nquestion_ru = "Какой?"\nchoices = ["Int", "String"]\nchoices_ru = ["Int", "String"]\nanswer = 1\n`;
  assert.equal(parseExerciseMetadata(asked, "test").question?.answer, 1);
  assert.throws(() => parseExerciseMetadata(asked.replace("answer = 1", "answer = 2"), "test"), /answer/);
  assert.throws(() => parseExerciseMetadata(asked.replace('choices_ru = ["Int", "String"]', 'choices_ru = ["Int"]'), "test"), /choices/);
});
