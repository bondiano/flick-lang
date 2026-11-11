/** Build/CI-only reading of the courses' index: their lessons and the exercises of each, in order. */
import { readFileSync } from "node:fs";
import path from "node:path";

export interface CourseLesson {
  readonly id: string;
  readonly en: string;
  readonly ru: string;
  /** The lesson of the tour that shows what the exercises ask for. */
  readonly tour?: string;
  readonly exercises: ReadonlyArray<string>;
}
export interface Course {
  readonly id: string;
  readonly en: string;
  readonly ru: string;
  readonly lessons: ReadonlyArray<CourseLesson>;
}

/** Every course, from the site's root, as the tour's and the exercises' loaders read theirs. */
export const courses = (): ReadonlyArray<Course> =>
  // oxlint-disable-next-line typescript/no-unsafe-type-assertion -- `test/course.test.ts` checks the index against the exercises and pages
  (JSON.parse(readFileSync(path.join(process.cwd(), "course/index.json"), "utf8")) as { courses: ReadonlyArray<Course> }).courses;

/** The page of an exercise under `course/`: `values/01` is `values-01`. */
export const pageOf = (exercise: string): string => exercise.replace("/", "-");
