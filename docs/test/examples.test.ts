// The playground's gallery: each example does what its entry in the index says, with the real compiler.
import assert from "node:assert/strict";
import { existsSync, readdirSync, readFileSync } from "node:fs";
import path from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";
import { verifyExample } from "./helpers/verify-example.ts";

const here = path.dirname(fileURLToPath(import.meta.url));
const examples = path.join(here, "../src/playground/examples");

interface Entry {
  readonly id: string;
  readonly en: string;
  readonly ru: string;
  /** `run`, `ui`, or `fail` and the one code its errors have. */
  readonly expect: string;
}

const index = JSON.parse(readFileSync(path.join(examples, "index.json"), "utf8")) as {
  groups: Array<{ id: string; en: string; ru: string; examples: Array<Entry> }>;
};
const entries = index.groups.flatMap((group) => group.examples);

test("the index and the example files match one to one", () => {
  const files = readdirSync(examples).filter((file) => file.endsWith(".fl")).map((file) => file.slice(0, -3));
  assert.deepEqual(entries.map((entry) => entry.id).sort(), files.sort());
  for (const entry of entries) {
    assert.equal(existsSync(path.join(examples, `${entry.id}.stdout`)), entry.expect === "run", entry.id);
    assert.ok(entry.en !== "" && entry.ru !== "", entry.id);
  }
});

for (const entry of entries) {
  test(`example ${entry.id}: ${entry.expect}`, () => {
    verifyExample(`docs-examples/${entry.id}`, path.join(examples, `${entry.id}.fl`), entry.expect, path.join(examples, `${entry.id}.stdout`));
  });
}
