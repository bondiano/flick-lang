// The sections of the documentation; the sidebar draws each one as a line of its pages.
import { readFileSync } from "node:fs";

const reference = (name) => ({ slug: `reference/${name}` });
// The tour's order is its index's; Back and Next follow the sidebar.
const tour = JSON.parse(readFileSync(new URL("./tour/index.json", import.meta.url), "utf8"));

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
    items: [{ slug: "handbook/designing-with-effects" }, { slug: "handbook/upgrading" }],
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
