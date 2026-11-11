// The text for language models: links in the order of the sidebar, and the tour without its components.
import assert from "node:assert/strict";
import { test } from "node:test";
import { lessons } from "../tour/load.ts";
import { index, type Page, tour } from "../src/llms/text.ts";

const url = (path: string): string => `https://example.org/docs/${path}`;

test("llms.txt links each page of a section in the sidebar's order, and only pages that exist", () => {
  const pages = new Map<string, Page>([
    ["start", { slug: "start", title: "Start here", description: "A short path." }],
    ["reference/types", { slug: "reference/types", title: "Kinds and types" }],
  ]);
  const text = index([{ label: "Get started", items: [{ slug: "start" }, { slug: "missing" }] }, { label: "Reference", items: [{ label: "Language", items: [{ slug: "reference/types" }] }] }], pages, url);
  assert.ok(text.startsWith("# Flick\n\n> "));
  assert.match(text, /## Get started\n\n- \[Start here\]\(https:\/\/example\.org\/docs\/start\/\): A short path\.\n\n## Reference\n\n- \[Kinds and types\]\(https:\/\/example\.org\/docs\/reference\/types\/\)\n/);
  assert.ok(!text.includes("missing"));
  assert.ok(text.includes("(https://example.org/docs/llms-full.txt)"));
});

test("llms-full.txt has every lesson with its text and program, without imports or component tags", () => {
  const all = lessons();
  const pages = new Map<string, Page>(all.map((lesson) => [`tour/${lesson.id}`, {
    slug: `tour/${lesson.id}`,
    title: `Title ${lesson.id}`,
    body: `import TourLesson from "../../../components/TourLesson.astro";\n\n<TourLesson locale="en" id="${lesson.id}">\n\nText of ${lesson.id}.\n\n</TourLesson>\n`,
  }]));
  const text = tour(all, pages);
  for (const lesson of all) assert.ok(text.includes(`### Title ${lesson.id}\n\nText of ${lesson.id}.\n\n`), lesson.id);
  assert.equal(text.split("```flick\n").length - 1, all.length);
  assert.ok(!text.includes("import TourLesson") && !text.includes("<TourLesson"));
});
