// The text for language models, as the built site serves it.
import { expect, test } from "@playwright/test";
import { readFileSync } from "node:fs";
import { loadExercise } from "../exercises/load.ts";
import { lessons } from "../tour/load.ts";

test("llms.txt links the pages of the site and the whole tour", async ({ request }) => {
  const response = await request.get("/flick-lang/llms.txt");
  expect(response.status()).toBe(200);
  const text = await response.text();
  expect(text.startsWith("# Flick\n")).toBe(true);
  expect(text).toContain("](https://bondiano.github.io/flick-lang/tour/program/)");
  expect(text).toContain("](https://bondiano.github.io/flick-lang/reference/types/)");
  expect(text).toContain("](https://bondiano.github.io/flick-lang/llms-full.txt)");
});

test("llms-full.txt holds every lesson of the tour with its program", async ({ request }) => {
  const response = await request.get("/flick-lang/llms-full.txt");
  expect(response.status()).toBe(200);
  const text = await response.text();
  expect(text.split("\n### ").length - 1).toBe(lessons().length);
  const sections = text.split("\n### ").slice(1);
  for (const [index, lesson] of lessons().entries()) {
    const program = lesson.file === undefined ? loadExercise(lesson.exercise ?? "").starter : readFileSync(lesson.file, "utf8");
    const kind = lesson.exercise === undefined ? "The program" : "The exercise starts from";
    expect(sections[index], lesson.id).toContain(`${kind}:\n\n\`\`\`flick\n${program.trimEnd()}\n\`\`\``);
  }
  expect(text).not.toContain("<TourLesson");
});
