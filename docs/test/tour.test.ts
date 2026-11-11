// The tour: its index, its files and its pages agree, and each lesson's program does what the index says.
import assert from "node:assert/strict";
import { existsSync, readdirSync } from "node:fs";
import path from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";
import { lessons } from "../tour/load.ts";
import { verifyExample } from "./helpers/verify-example.ts";

const here = path.dirname(fileURLToPath(import.meta.url));
const docs = path.join(here, "..");
const all = lessons();
// a lesson that shows an example of the gallery is checked with the gallery
const own = all.filter((entry) => entry.file?.startsWith(path.join(docs, "tour")));

test("the index, the lesson files and the pages match one to one", () => {
  const ids = all.map((entry) => entry.id);
  assert.equal(new Set(ids).size, ids.length, "a lesson id is listed twice");
  const files = readdirSync(path.join(docs, "tour"), { recursive: true, encoding: "utf8" })
    .filter((name) => name.endsWith(".fl") || name.endsWith(".stdout"))
    .map((name) => path.join(docs, "tour", name))
    .sort();
  assert.deepEqual(files, own.flatMap((entry) => [entry.file ?? "", ...(entry.stdout ? [entry.stdout] : [])]).sort());
  for (const entry of all) {
    if (entry.file !== undefined) assert.ok(existsSync(entry.file), `${entry.id}: ${entry.file}`);
    if (entry.stdout !== undefined) assert.ok(existsSync(entry.stdout), `${entry.id}: ${entry.stdout}`);
    if (entry.exercise !== undefined) assert.ok(existsSync(path.join(docs, "exercises", entry.exercise, "check.toml")), entry.id);
  }
  for (const locale of ["", "ru/"]) {
    const pages = readdirSync(path.join(docs, "src/content/docs", locale, "tour")).map((name) => name.replace(/\.mdx?$/, ""));
    assert.deepEqual(pages.filter((name) => name !== "index").sort(), [...ids].sort(), `pages of ${locale || "en/"}tour`);
  }
});

for (const entry of own) {
  test(`lesson ${entry.id}: ${entry.expect}`, () => {
    verifyExample(`docs-tour/${entry.id}`, entry.file ?? "", entry.expect, entry.stdout);
  });
}
