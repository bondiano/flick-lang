// The text for language models, as the built site serves it.
import { expect, test } from "@playwright/test";
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
  expect(text.split("```flick\n").length - 1).toBe(lessons().length);
  expect(text).not.toContain("<TourLesson");
});
