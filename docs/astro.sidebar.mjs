// The sections of the documentation; the sidebar draws each one as a line of its pages.
import { readFileSync } from "node:fs";
import path from "node:path";

const reference = (name) => ({ slug: `reference/${name}` });
// The tour's order is its index's, and a course's its own; Back and Next follow the sidebar. From the
// site's root, as their loaders read them: `llms.txt` bundles this module where its own URL means nothing.
const tour = JSON.parse(readFileSync(path.join(process.cwd(), "tour/index.json"), "utf8"));
// A course's exercises, a page each, by lesson; `values/01` is `course/values-01`.
const course = JSON.parse(readFileSync(path.join(process.cwd(), "course/index.json"), "utf8")).courses[0];

export const sidebar = [
  {
    label: "Tour",
    translations: { ru: "Тур" },
    items: [
      { slug: "tour" },
      ...tour.chapters.map((chapter) => ({
        label: chapter.en,
        translations: { ru: chapter.ru },
        items: chapter.lessons.map((lesson) => ({ slug: `tour/${lesson.id}` })),
      })),
    ],
  },
  {
    label: "Course",
    translations: { ru: "Курс" },
    items: [
      { slug: "course" },
      ...course.lessons.map((lesson) => ({
        label: lesson.en,
        translations: { ru: lesson.ru },
        items: lesson.exercises.map((exercise) => ({ slug: `course/${exercise.replace("/", "-")}` })),
      })),
    ],
  },
  {
    label: "Get started",
    translations: { ru: "Начало работы" },
    items: [
      { slug: "start" },
      { slug: "start/getting-started" },
      { slug: "start/why" },
      { slug: "principles" },
    ],
  },
  {
    label: "Handbook",
    translations: { ru: "Руководство" },
    items: [{ slug: "handbook/designing-with-effects" }, { slug: "handbook/typed-dsl" }, { slug: "handbook/upgrading" }],
  },
  {
    label: "Reference",
    translations: { ru: "Справочник" },
    items: [
      {
        label: "Language",
        translations: { ru: "Язык" },
        items: ["types", "gadts", "effects", "classes", "regions", "modules", "grammar", "keywords"].map(reference),
      },
      { slug: "reference/errors" },
      { label: "Package API", translations: { ru: "API пакета" }, items: [{ slug: "api" }] },
      {
        label: "Runtime",
        translations: { ru: "Среда выполнения" },
        items: ["stdlib", "effect-surface", "runtime"].map(reference),
      },
      {
        label: "Specification",
        translations: { ru: "Спецификация" },
        items: [{ autogenerate: { directory: "spec" } }],
      },
    ],
  },
  {
    label: "Guides",
    translations: { ru: "Задачи" },
    items: ["ui", "cli-dsl", "http-api", "rpc", "server-fn", "route", "form", "observability", "schema", "sql", "ffi"].map(reference),
  },
  {
    label: "Tools",
    translations: { ru: "Инструменты" },
    items: [
      ...["cli", "project", "plugins", "editor", "codegen", "iface"].map(reference),
      { slug: "contributing/testing" },
      { slug: "contributing/implementation-decisions" },
    ],
  },
];
