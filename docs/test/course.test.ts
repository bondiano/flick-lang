// The courses: their index, their exercises and their pages agree.
import assert from "node:assert/strict";
import { existsSync, readdirSync } from "node:fs";
import path from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";
import { courses, pageOf } from "../course/load.ts";
import { loadExercise } from "../exercises/load.ts";

const docs = path.join(path.dirname(fileURLToPath(import.meta.url)), "..");

test("every exercise of a course exists, once, with a page in each locale and nothing else", () => {
  const exercises = courses().flatMap((course) => course.lessons.flatMap((lesson) => lesson.exercises));
  assert.equal(new Set(exercises).size, exercises.length, "an exercise is listed twice");
  for (const id of exercises) assert.ok(loadExercise(id).title !== "", id);
  for (const locale of ["", "ru/"]) {
    const pages = readdirSync(path.join(docs, "src/content/docs", locale, "course")).map((name) => name.replace(/\.mdx?$/, ""));
    assert.deepEqual(pages.filter((name) => name !== "index").sort(), exercises.map(pageOf).sort(), `pages of ${locale || "en/"}course`);
  }
});

test("a lesson that names a lesson of the tour names one that exists, and the last lesson has no hints", () => {
  for (const course of courses()) {
    for (const lesson of course.lessons) {
      if (lesson.tour !== undefined) assert.ok(existsSync(path.join(docs, "src/content/docs/tour", `${lesson.tour}.mdx`)), lesson.tour);
    }
    const last = course.lessons.at(-1);
    for (const id of last?.exercises ?? []) assert.deepEqual(loadExercise(id).hints, [], id);
  }
});
